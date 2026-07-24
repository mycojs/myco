use std::fs::Metadata;
use std::path::PathBuf;

use serde::Serialize;
use v8;

use crate::errors::MycoError;
use crate::run::ops::convert::{JsBuffer, ToJsBuffer};
use crate::run::ops::macros::{
    async_op, create_rejected_promise, create_resolved_promise_void, get_state, get_string_arg,
    sync_op,
};
use crate::run::state::{MycoState, OpResult};
use crate::Capability;
use crate::{impl_from_v8_struct, impl_to_v8_struct, register_async_op, register_sync_op};

struct TokenOptionalPathArg {
    token: String,
    path: Option<String>,
}

impl_from_v8_struct!(TokenOptionalPathArg {
    token: String,
    path: Option<String>,
});

struct TokenPathArg {
    token: String,
    path: String,
}

impl_from_v8_struct!(TokenPathArg {
    token: String,
    path: String,
});

struct WriteFileArg {
    token: String,
    contents: JsBuffer,
    path: Option<String>,
}

impl_from_v8_struct!(WriteFileArg {
    token: String,
    contents: JsBuffer,
    path: Option<String>,
});

struct ExecFileArg {
    token: String,
    path: Option<String>,
    args: Vec<String>,
}

impl_from_v8_struct!(ExecFileArg {
    token: String,
    path: Option<String>,
    args: Vec<String>,
});

struct PathArg {
    path: String,
}

impl_from_v8_struct!(PathArg { path: String });

pub fn register_filesystem_ops(
    scope: &mut v8::PinScope<'_, '_>,
    myco_ops: &v8::Object,
) -> Result<(), MycoError> {
    register_async_op!(
        scope,
        myco_ops,
        "request_read_file",
        async_op_request_read_file
    );
    register_async_op!(
        scope,
        myco_ops,
        "request_write_file",
        async_op_request_write_file
    );
    register_async_op!(
        scope,
        myco_ops,
        "request_exec_file",
        async_op_request_exec_file
    );
    register_async_op!(
        scope,
        myco_ops,
        "request_read_dir",
        async_op_request_read_dir
    );
    register_async_op!(
        scope,
        myco_ops,
        "request_write_dir",
        async_op_request_write_dir
    );
    register_async_op!(
        scope,
        myco_ops,
        "request_exec_dir",
        async_op_request_exec_dir
    );
    register_async_op!(scope, myco_ops, "read_file", async_op_read_file);
    register_async_op!(scope, myco_ops, "write_file", async_op_write_file);
    register_async_op!(scope, myco_ops, "remove_file", async_op_remove_file);
    register_async_op!(scope, myco_ops, "stat_file", async_op_stat_file);
    register_async_op!(scope, myco_ops, "list_dir", async_op_list_dir);
    register_async_op!(scope, myco_ops, "mkdirp", async_op_mkdirp);
    register_async_op!(scope, myco_ops, "rmdir", async_op_rmdir);
    register_async_op!(scope, myco_ops, "rmdir_recursive", async_op_rmdir_recursive);
    register_async_op!(scope, myco_ops, "exec_file", async_op_exec_file);
    register_sync_op!(scope, myco_ops, "read_file", sync_op_read_file);
    register_sync_op!(scope, myco_ops, "write_file", sync_op_write_file);
    register_sync_op!(scope, myco_ops, "remove_file", sync_op_remove_file);
    register_sync_op!(scope, myco_ops, "stat_file", sync_op_stat_file);
    register_sync_op!(scope, myco_ops, "list_dir", sync_op_list_dir);
    register_sync_op!(scope, myco_ops, "mkdirp", sync_op_mkdirp);
    register_sync_op!(scope, myco_ops, "rmdir", sync_op_rmdir);
    register_sync_op!(scope, myco_ops, "exec_file", sync_op_exec_file);
    register_sync_op!(scope, myco_ops, "cwd", sync_op_cwd);
    register_sync_op!(scope, myco_ops, "chdir", sync_op_chdir);
    Ok(())
}

// Helper function to convert relative paths to normalized absolute paths
fn normalize_path(path: &str) -> Result<PathBuf, String> {
    let path_buf = std::path::Path::new(path);

    let absolute_path = if path_buf.is_relative() {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path_buf),
            Err(e) => {
                return Err(format!("Failed to get current working directory: {}", e));
            }
        }
    } else {
        path_buf.to_path_buf()
    };

    // Normalize the path to remove . and .. components
    let mut components = Vec::new();
    for component in absolute_path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip current directory components
            }
            std::path::Component::ParentDir => {
                // Remove the last component for parent directory
                components.pop();
            }
            _ => {
                components.push(component);
            }
        }
    }

    // Reconstruct the path from components
    let mut normalized = PathBuf::new();
    for component in components {
        normalized.push(component);
    }

    Ok(normalized)
}

// Helper function to find a binary in PATH
async fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_env = std::env::var("PATH").ok()?;

    for path_dir in std::env::split_paths(&path_env) {
        let candidate = path_dir.join(binary_name);

        // Check if file exists and is executable
        if candidate.exists() && candidate.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = tokio::fs::metadata(&candidate).await {
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 != 0 {
                        return Some(candidate);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                // On non-Unix systems, just check if file exists
                return Some(candidate);
            }
        }
    }

    None
}

fn async_op_request_read_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |_scope, input: PathArg| Ok(input),
        |input| async move {
            let path = input.path;
            // Validate ReadFile: for read-write compatibility, allow files that don't exist yet
            // if we have permission to create them (similar to write validation)
            let path_buf = match normalize_path(&path) {
                Ok(p) => p,
                Err(e) => return OpResult::Capability(Err(e)),
            };

            if path_buf.exists() {
                // If file exists, it must be a readable file
                if !path_buf.is_file() {
                    return OpResult::Capability(Err(format!("Path is not a file: {}", path)));
                }
                if let Err(e) = tokio::fs::metadata(&path_buf).await {
                    return OpResult::Capability(Err(format!(
                        "Cannot access file '{}': {}",
                        path, e
                    )));
                }
            } else {
                // If file doesn't exist, check if we can create it (for read-write compatibility)
                if let Some(parent) = path_buf.parent() {
                    if !parent.exists() {
                        return OpResult::Capability(Err(format!(
                            "Parent directory does not exist: {}",
                            parent.display()
                        )));
                    }
                    if !parent.is_dir() {
                        return OpResult::Capability(Err(format!(
                            "Parent path is not a directory: {}",
                            parent.display()
                        )));
                    }
                    if let Err(e) = tokio::fs::metadata(parent).await {
                        return OpResult::Capability(Err(format!(
                            "Cannot access parent directory '{}': {}",
                            parent.display(),
                            e
                        )));
                    }
                } else {
                    return OpResult::Capability(Err(
                        "Invalid file path: no parent directory".to_string()
                    ));
                }
            }

            // Use the absolute path for the capability
            let absolute_path = path_buf.to_string_lossy().to_string();
            OpResult::Capability(Ok(Capability::ReadFile(absolute_path)))
        },
    );
}

fn async_op_request_write_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |_scope, input: PathArg| Ok(input),
        |input| async move {
            let path = input.path;
            // Validate WriteFile: parent directory must exist if file doesn't exist
            let path_buf = match normalize_path(&path) {
                Ok(p) => p,
                Err(e) => return OpResult::Capability(Err(e)),
            };

            if path_buf.exists() {
                if path_buf.is_dir() {
                    return OpResult::Capability(Err(format!(
                        "Path is a directory, not a file: {}",
                        path
                    )));
                }
                if let Err(e) = tokio::fs::metadata(&path_buf).await {
                    return OpResult::Capability(Err(format!(
                        "Cannot access file '{}': {}",
                        path, e
                    )));
                }
            } else if let Some(parent) = path_buf.parent() {
                if !parent.exists() {
                    return OpResult::Capability(Err(format!(
                        "Parent directory does not exist: {}",
                        parent.display()
                    )));
                }
                if !parent.is_dir() {
                    return OpResult::Capability(Err(format!(
                        "Parent path is not a directory: {}",
                        parent.display()
                    )));
                }
                if let Err(e) = tokio::fs::metadata(parent).await {
                    return OpResult::Capability(Err(format!(
                        "Cannot access parent directory '{}': {}",
                        parent.display(),
                        e
                    )));
                }
            } else {
                return OpResult::Capability(Err(
                    "Invalid file path: no parent directory".to_string()
                ));
            }

            // Use the absolute path for the capability
            let absolute_path = path_buf.to_string_lossy().to_string();
            OpResult::Capability(Ok(Capability::WriteFile(absolute_path)))
        },
    );
}

fn async_op_request_exec_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |_scope, input: PathArg| Ok(input),
        |input| async move {
            let path = input.path;

            // Check if this is a PATH binary request (no path separators)
            let path_buf = if !path.contains('/') {
                // Try to find the binary in PATH
                match find_in_path(&path).await {
                    Some(found_path) => found_path,
                    None => {
                        return OpResult::Capability(Err(format!(
                            "Binary '{}' not found in PATH",
                            path
                        )))
                    }
                }
            } else {
                // Traditional path resolution
                match normalize_path(&path) {
                    Ok(p) => p,
                    Err(e) => return OpResult::Capability(Err(e)),
                }
            };

            if !path_buf.exists() {
                return OpResult::Capability(Err(format!(
                    "File does not exist: {}",
                    path_buf.display()
                )));
            }
            if !path_buf.is_file() {
                return OpResult::Capability(Err(format!(
                    "Path is not a file: {}",
                    path_buf.display()
                )));
            }
            if let Err(e) = tokio::fs::metadata(&path_buf).await {
                return OpResult::Capability(Err(format!(
                    "Cannot access file '{}': {}",
                    path_buf.display(),
                    e
                )));
            }
            // On Unix-like systems, check if executable bit is set
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = tokio::fs::metadata(&path_buf).await {
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 == 0 {
                        return OpResult::Capability(Err(format!(
                            "File is not executable: {}",
                            path_buf.display()
                        )));
                    }
                }
            }

            // Use the absolute path for the capability
            let absolute_path = path_buf.to_string_lossy().to_string();
            OpResult::Capability(Ok(Capability::ExecFile(absolute_path)))
        },
    );
}

fn async_op_request_read_dir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |_scope, input: PathArg| Ok(input),
        |input| async move {
            let path = input.path;
            // Validate ReadDir: directory must exist and be readable
            let path_buf = match normalize_path(&path) {
                Ok(p) => p,
                Err(e) => return OpResult::Capability(Err(e)),
            };

            if !path_buf.exists() {
                return OpResult::Capability(Err(format!("Directory does not exist: {}", path)));
            }
            if !path_buf.is_dir() {
                return OpResult::Capability(Err(format!("Path is not a directory: {}", path)));
            }
            if let Err(e) = tokio::fs::read_dir(&path_buf).await {
                return OpResult::Capability(Err(format!(
                    "Cannot read directory '{}': {}",
                    path, e
                )));
            }

            // Use the absolute path for the capability
            let absolute_path = path_buf.to_string_lossy().to_string();
            OpResult::Capability(Ok(Capability::ReadDir(absolute_path)))
        },
    );
}

fn async_op_request_write_dir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |_scope, input: PathArg| Ok(input),
        |input| async move {
            let path = input.path;
            // Validate WriteDir: directory must exist and be writable
            let path_buf = match normalize_path(&path) {
                Ok(p) => p,
                Err(e) => return OpResult::Capability(Err(e)),
            };

            if !path_buf.exists() {
                return OpResult::Capability(Err(format!("Directory does not exist: {}", path)));
            }
            if !path_buf.is_dir() {
                return OpResult::Capability(Err(format!("Path is not a directory: {}", path)));
            }
            if let Err(e) = tokio::fs::metadata(&path_buf).await {
                return OpResult::Capability(Err(format!(
                    "Cannot access directory '{}': {}",
                    path, e
                )));
            }

            // Use the absolute path for the capability
            let absolute_path = path_buf.to_string_lossy().to_string();
            OpResult::Capability(Ok(Capability::WriteDir(absolute_path)))
        },
    );
}

fn async_op_request_exec_dir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |_scope, input: PathArg| Ok(input),
        |input| async move {
            let path = input.path;
            // Validate ExecDir: directory must exist and be accessible
            let path_buf = match normalize_path(&path) {
                Ok(p) => p,
                Err(e) => return OpResult::Capability(Err(e)),
            };

            if !path_buf.exists() {
                return OpResult::Capability(Err(format!("Directory does not exist: {}", path)));
            }
            if !path_buf.is_dir() {
                return OpResult::Capability(Err(format!("Path is not a directory: {}", path)));
            }
            if let Err(e) = tokio::fs::metadata(&path_buf).await {
                return OpResult::Capability(Err(format!(
                    "Cannot access directory '{}': {}",
                    path, e
                )));
            }

            // Use the absolute path for the capability
            let absolute_path = path_buf.to_string_lossy().to_string();
            OpResult::Capability(Ok(Capability::ExecDir(absolute_path)))
        },
    );
}

// Path resolution helpers

// Resolve a path relative to a directory token, confining it to that directory.
//
// The path a directory token is used with is untrusted, so every way it could
// name something outside the token's directory has to be closed here:
//
//   - `..` is applied to the path we are building rather than left in it. A
//     `..` at the directory root has nowhere to go and is rejected, so no
//     amount of them walks out. Leaving them in the path would defeat the
//     containment check entirely: `Path::starts_with` compares components and
//     does not interpret `..`, so `<dir>/../secret` is "inside" `<dir>` as far
//     as it is concerned, and the OS resolves the `..` only at open time.
//   - A symlink escapes just as effectively as a `..` does, so each component
//     is checked as it is appended: if it is a symlink, its target must
//     canonicalize to something inside the directory.
//
// The returned path is the lexical one, with symlinks left unresolved, so that
// operations keep acting on what they were asked for -- notably `remove` on a
// symlink unlinks the symlink rather than deleting what it points at. Every
// prefix of it has been checked, so the OS resolving it lands inside the
// directory.
fn canonical(dir: String, path: String) -> Result<PathBuf, MycoError> {
    let dir_path = PathBuf::from(&dir);
    let dir = dir_path
        .canonicalize()
        .map_err(|e| MycoError::PathCanonicalization {
            path: dir,
            source: e,
        })?;

    let outside = |path: &std::path::Path| MycoError::PathOutsideToken {
        path: path.display().to_string(),
    };

    // A leading `/` names the root of the token's directory, not the root of
    // the filesystem.
    let relative = PathBuf::from(path.trim_start_matches('/'));

    let mut resolved = dir.clone();
    for component in relative.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if resolved == dir {
                    return Err(outside(&resolved.join("..")));
                }
                resolved.pop();
            }
            // Unreachable on Unix now that the leading `/` is trimmed, but a
            // Windows prefix (`C:\`) would otherwise replace the directory
            // wholesale when joined.
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(outside(std::path::Path::new(&path)));
            }
            std::path::Component::Normal(name) => {
                resolved.push(name);
                let metadata = match std::fs::symlink_metadata(&resolved) {
                    Ok(metadata) => metadata,
                    // Either nothing is there -- the usual case, since writes
                    // name files that do not exist yet -- or we cannot see it,
                    // in which case the operation itself cannot reach it
                    // either.
                    Err(_) => continue,
                };
                if metadata.file_type().is_symlink() {
                    // A symlink that does not resolve is rejected rather than
                    // followed: we cannot show its target stays in scope, and
                    // writing through it would follow it wherever it points.
                    let target = resolved
                        .canonicalize()
                        .map_err(|_| outside(resolved.as_path()))?;
                    if !target.starts_with(&dir) {
                        return Err(outside(target.as_path()));
                    }
                }
            }
        }
    }

    Ok(resolved)
}

fn resolve_path(
    state: &MycoState,
    token: &str,
    path: Option<String>,
    access_type: &str,
) -> Result<PathBuf, MycoError> {
    let capability = state.capabilities.get(token);

    match capability {
        Some(Capability::ReadFile(file_path)) if access_type == "read" && path.is_none() => {
            Ok(PathBuf::from(file_path.clone()))
        }
        Some(Capability::ReadDir(dir)) if access_type == "read" && path.is_some() => {
            canonical(dir.clone(), path.unwrap())
        }
        Some(Capability::WriteFile(file_path)) if access_type == "write" && path.is_none() => {
            Ok(PathBuf::from(file_path.clone()))
        }
        Some(Capability::WriteDir(dir)) if access_type == "write" && path.is_some() => {
            canonical(dir.clone(), path.unwrap())
        }
        Some(Capability::ExecFile(file_path)) if access_type == "exec" && path.is_none() => {
            Ok(PathBuf::from(file_path.clone()))
        }
        Some(Capability::ExecDir(dir)) if access_type == "exec" && path.is_some() => {
            canonical(dir.clone(), path.unwrap())
        }
        _ => Err(MycoError::Internal {
            message: format!("Invalid token for {} access", access_type),
        }),
    }
}

// Data structures
pub struct Stats {
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub readonly: bool,
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
    pub created: Option<u64>,
}

impl_to_v8_struct!(Stats {
    is_file,
    is_dir,
    is_symlink,
    size,
    readonly,
    modified,
    accessed,
    created,
});

impl Stats {
    fn from_metadata(metadata: Metadata) -> Self {
        Self {
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.file_type().is_symlink(),
            size: metadata.len(),
            readonly: metadata.permissions().readonly(),
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            accessed: metadata
                .accessed()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            created: metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        }
    }
}

pub struct File {
    pub name: String,
    pub stats: Stats,
}

impl_to_v8_struct!(File { name, stats });

impl File {
    fn from(path: PathBuf, metadata: Metadata) -> Self {
        Self {
            name: path.file_name().unwrap().to_str().unwrap().to_owned(),
            stats: Stats::from_metadata(metadata),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

impl_to_v8_struct!(ExecResult {
    stdout,
    stderr,
    exit_code,
});

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileStats {
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub readonly: bool,
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
    pub created: Option<u64>,
}

impl FileStats {
    // Helper functions for async operations
    fn from_metadata(metadata: Metadata) -> FileStats {
        FileStats {
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.file_type().is_symlink(),
            size: metadata.len(),
            readonly: metadata.permissions().readonly(),
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            accessed: metadata
                .accessed()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            created: metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileInfo {
    pub name: String,
    pub stats: FileStats,
}

impl FileInfo {
    fn from_path_and_metadata(path: PathBuf, metadata: Metadata) -> FileInfo {
        FileInfo {
            name: path.file_name().unwrap().to_str().unwrap().to_owned(),
            stats: FileStats::from_metadata(metadata),
        }
    }
}

// Sync operations
fn sync_op_read_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenOptionalPathArg| -> Result<ToJsBuffer, MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "read")?;
            std::fs::read(&path_buf)
                .map(ToJsBuffer::from)
                .map_err(|e| MycoError::Internal {
                    message: format!("Failed to read file '{}': {}", path_buf.display(), e),
                })
        },
    );
}

fn sync_op_write_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: WriteFileArg| -> Result<(), MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "write")?;
            std::fs::write(&path_buf, input.contents).map_err(|e| MycoError::Internal {
                message: format!("Failed to write file '{}': {}", path_buf.display(), e),
            })
        },
    );
}

fn sync_op_remove_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenOptionalPathArg| -> Result<(), MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "write")?;
            match std::fs::remove_file(&path_buf) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(MycoError::Internal {
                    message: format!("Failed to remove file '{}': {}", path_buf.display(), e),
                }),
            }
        },
    );
}

fn sync_op_mkdirp<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenPathArg| -> Result<(), MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
            std::fs::create_dir_all(&path_buf).map_err(|e| MycoError::Internal {
                message: format!("Failed to create directory '{}': {}", path_buf.display(), e),
            })
        },
    );
}

fn sync_op_rmdir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenPathArg| -> Result<(), MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
            match std::fs::remove_dir(&path_buf) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(MycoError::Internal {
                    message: format!("Failed to remove directory '{}': {}", path_buf.display(), e),
                }),
            }
        },
    );
}

fn sync_op_stat_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenOptionalPathArg| -> Result<Option<Stats>, MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "read")?;
            match std::fs::metadata(&path_buf) {
                Ok(metadata) => Ok(Some(Stats::from_metadata(metadata))),
                Err(_) => Ok(None),
            }
        },
    );
}

fn sync_op_list_dir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenPathArg| -> Result<Vec<File>, MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "read")?;

            let entries = std::fs::read_dir(&path_buf).map_err(|e| MycoError::Internal {
                message: format!("Failed to list directory '{}': {}", path_buf.display(), e),
            })?;

            let mut result = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| MycoError::Internal {
                    message: format!(
                        "Failed to read directory entry in '{}': {}",
                        path_buf.display(),
                        e
                    ),
                })?;
                let metadata = entry.metadata().map_err(|e| MycoError::Internal {
                    message: format!(
                        "Failed to get metadata for directory entry in '{}': {}",
                        path_buf.display(),
                        e
                    ),
                })?;
                result.push(File::from(entry.path(), metadata));
            }
            Ok(result)
        },
    );
}

fn sync_op_exec_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: ExecFileArg| -> Result<ExecResult, MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "exec")?;

            let output = std::process::Command::new(&path_buf)
                .args(input.args)
                .output()
                .map_err(|e| MycoError::Internal {
                    message: format!("Failed to execute command '{}': {}", path_buf.display(), e),
                })?;

            Ok(ExecResult {
                stdout: output.stdout,
                stderr: output.stderr,
                exit_code: output.status.code().unwrap_or(-1),
            })
        },
    );
}

fn async_op_read_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenOptionalPathArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "read")?;
            Ok(path_buf)
        },
        |path_buf: PathBuf| async move {
            let result = tokio::fs::read(&path_buf)
                .await
                .map_err(|e| format!("Failed to read file '{}': {}", path_buf.display(), e));

            OpResult::Binary(result)
        },
    );
}

fn async_op_write_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: WriteFileArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "write")?;
            Ok((input, path_buf))
        },
        |(input, path_buf)| async move {
            let result = tokio::fs::write(&path_buf, input.contents)
                .await
                .map_err(|e| format!("Failed to write file '{}': {}", path_buf.display(), e));

            OpResult::Void(result)
        },
    );
}

fn async_op_remove_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenOptionalPathArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "write")?;
            Ok(path_buf)
        },
        |path_buf| async move {
            let result = tokio::fs::remove_file(&path_buf).await;

            if let Err(e) = &result {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return OpResult::Void(Ok(()));
                }
            }

            OpResult::Void(
                result
                    .map_err(|e| format!("Failed to remove file '{}': {}", path_buf.display(), e)),
            )
        },
    );
}

fn async_op_mkdirp<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenPathArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
            Ok(path_buf)
        },
        |path_buf| async move {
            let result = tokio::fs::create_dir_all(&path_buf)
                .await
                .map_err(|e| format!("Failed to create directory '{}': {}", path_buf.display(), e));

            OpResult::Void(result)
        },
    );
}

fn async_op_rmdir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenPathArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
            Ok(path_buf)
        },
        |path_buf| async move {
            // Same as above, we don't care if the directory doesn't exist
            let result = tokio::fs::remove_dir(&path_buf).await;

            if let Err(e) = &result {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return OpResult::Void(Ok(()));
                }
            }

            OpResult::Void(
                result.map_err(|e| {
                    format!("Failed to remove directory '{}': {}", path_buf.display(), e)
                }),
            )
        },
    );
}

fn async_op_rmdir_recursive<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenPathArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
            Ok(path_buf)
        },
        |path_buf| async move {
            let result = tokio::fs::remove_dir_all(&path_buf).await;

            if let Err(e) = &result {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return OpResult::Void(Ok(()));
                }
            }

            OpResult::Void(result.map_err(|e| {
                format!(
                    "Failed to remove directory recursively '{}': {}",
                    path_buf.display(),
                    e
                )
            }))
        },
    );
}

fn async_op_stat_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenOptionalPathArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "read")?;
            Ok(path_buf)
        },
        |path_buf| async move {
            let result = tokio::fs::metadata(&path_buf)
                .await
                .map(FileStats::from_metadata)
                .map(|result| serde_json::to_string(&result).unwrap())
                .map_err(|e| {
                    format!(
                        "Failed to get file metadata for '{}': {}",
                        path_buf.display(),
                        e
                    )
                });

            OpResult::Json(result)
        },
    );
}

fn async_op_list_dir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: TokenPathArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "read")?;
            Ok(path_buf)
        },
        |path_buf: PathBuf| async move {
            let result = async move {
                let mut entries = tokio::fs::read_dir(&path_buf).await.map_err(|e| {
                    format!("Failed to list directory '{}': {}", path_buf.display(), e)
                })?;

                let mut files = Vec::new();
                while let Some(entry) = entries.next_entry().await.map_err(|e| {
                    format!(
                        "Failed to read directory entry in '{}': {}",
                        path_buf.display(),
                        e
                    )
                })? {
                    let metadata = entry.metadata().await.map_err(|e| {
                        format!(
                            "Failed to get metadata for directory entry in '{}': {}",
                            path_buf.display(),
                            e
                        )
                    })?;

                    files.push(FileInfo::from_path_and_metadata(entry.path(), metadata));
                }

                Ok::<Vec<FileInfo>, String>(files)
            }
            .await;
            let result = result.map(|files| serde_json::to_string(&files).unwrap());

            OpResult::Json(result)
        },
    );
}

fn async_op_exec_file<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |scope, input: ExecFileArg| {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "exec")?;
            Ok((input, path_buf))
        },
        |(input, path_buf)| async move {
            let result = tokio::process::Command::new(&path_buf)
                .args(input.args)
                .output()
                .await
                .map(|output| ExecResult {
                    stdout: output.stdout,
                    stderr: output.stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                })
                .map(|result| serde_json::to_string(&result).unwrap())
                .map_err(|e| format!("Failed to execute command '{}': {}", path_buf.display(), e));

            OpResult::Json(result)
        },
    );
}

fn sync_op_cwd<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |_scope, _input: ()| -> Result<String, MycoError> {
            std::env::current_dir()
                .map(|path| path.to_string_lossy().to_string())
                .map_err(|e| MycoError::Internal {
                    message: format!("Failed to get current working directory: {}", e),
                })
        },
    );
}

fn sync_op_chdir<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid path"));
            return;
        }
    };

    match std::env::set_current_dir(&path) {
        Ok(_) => rv.set(create_resolved_promise_void(scope)),
        Err(e) => rv.set(create_rejected_promise(
            scope,
            &format!("Failed to change directory to '{}': {}", path, e),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    // A directory under the system temp dir, removed when the test ends. Named
    // per test so the suite can keep running tests in parallel.
    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new(name: &str) -> ScratchDir {
            let path = std::env::temp_dir().join(format!(
                "myco-canonical-{}-{}",
                std::process::id(),
                name
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            // The temp dir itself may be reached through a symlink (/tmp on
            // macOS), and `canonical` compares against canonicalized paths.
            ScratchDir {
                path: path.canonicalize().unwrap(),
            }
        }

        fn dir(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn resolve(dir: &Path, path: &str) -> Result<PathBuf, MycoError> {
        canonical(dir.to_string_lossy().to_string(), path.to_string())
    }

    fn is_outside(result: Result<PathBuf, MycoError>) -> bool {
        matches!(result, Err(MycoError::PathOutsideToken { .. }))
    }

    #[test]
    fn resolves_paths_inside_the_directory() {
        let scratch = ScratchDir::new("inside");
        let dir = scratch.dir();
        fs::write(dir.join("file.txt"), "contents").unwrap();

        assert_eq!(resolve(dir, "file.txt").unwrap(), dir.join("file.txt"));
        assert_eq!(resolve(dir, "./file.txt").unwrap(), dir.join("file.txt"));
        // A leading slash names the root of the token's directory.
        assert_eq!(resolve(dir, "/file.txt").unwrap(), dir.join("file.txt"));
        assert_eq!(resolve(dir, "/").unwrap(), dir.to_path_buf());
        // Write targets do not exist yet, so a path that resolves to nothing is
        // still a valid one.
        assert_eq!(resolve(dir, "new.txt").unwrap(), dir.join("new.txt"));
        assert_eq!(
            resolve(dir, "missing/deeper/new.txt").unwrap(),
            dir.join("missing/deeper/new.txt")
        );
    }

    #[test]
    fn rejects_parent_traversal_out_of_the_directory() {
        let scratch = ScratchDir::new("traversal");
        let dir = scratch.dir();
        fs::write(dir.join("file.txt"), "contents").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();

        assert!(is_outside(resolve(dir, "../secret.txt")));
        assert!(is_outside(resolve(dir, "/../secret.txt")));
        assert!(is_outside(resolve(dir, "..")));
        assert!(is_outside(resolve(dir, "sub/../../secret.txt")));
        assert!(is_outside(resolve(dir, "../../../../../../etc/passwd")));
        // Leaving the directory and coming back is still leaving it.
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        assert!(is_outside(resolve(dir, &format!("../{}/file.txt", name))));
    }

    #[test]
    fn applies_parent_traversal_that_stays_inside() {
        let scratch = ScratchDir::new("traversal-inside");
        let dir = scratch.dir();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("file.txt"), "contents").unwrap();

        // The `..` is applied here rather than passed through to the OS, so
        // what comes back is the collapsed path.
        assert_eq!(
            resolve(dir, "sub/../file.txt").unwrap(),
            dir.join("file.txt")
        );
        assert_eq!(resolve(dir, "sub/..").unwrap(), dir.to_path_buf());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_pointing_out_of_the_directory() {
        let scratch = ScratchDir::new("symlink-escape");
        let dir = scratch.dir();
        let outside = scratch.dir().parent().unwrap().join(format!(
            "myco-canonical-{}-symlink-escape-target",
            std::process::id()
        ));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();

        std::os::unix::fs::symlink(&outside, dir.join("escape")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("secret-link")).unwrap();

        // Both the symlink itself and anything reached through it.
        assert!(is_outside(resolve(dir, "secret-link")));
        assert!(is_outside(resolve(dir, "escape")));
        assert!(is_outside(resolve(dir, "escape/secret.txt")));
        // Including a file that does not exist yet, which a write would
        // otherwise create outside the directory.
        assert!(is_outside(resolve(dir, "escape/planted.txt")));

        let _ = fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn allows_symlinks_that_stay_inside_the_directory() {
        let scratch = ScratchDir::new("symlink-inside");
        let dir = scratch.dir();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/file.txt"), "contents").unwrap();
        std::os::unix::fs::symlink("sub", dir.join("link")).unwrap();

        // The symlink is left in the returned path so that operations act on
        // what they were asked for -- `remove` on a symlink unlinks it rather
        // than deleting its target.
        assert_eq!(resolve(dir, "link").unwrap(), dir.join("link"));
        assert_eq!(
            resolve(dir, "link/file.txt").unwrap(),
            dir.join("link/file.txt")
        );
        assert_eq!(
            resolve(dir, "link/new.txt").unwrap(),
            dir.join("link/new.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_do_not_resolve() {
        let scratch = ScratchDir::new("symlink-broken");
        let dir = scratch.dir();
        std::os::unix::fs::symlink("nowhere.txt", dir.join("broken")).unwrap();

        // Where a broken symlink points cannot be shown to be in scope, and
        // writing through one follows it wherever it leads.
        assert!(is_outside(resolve(dir, "broken")));
    }
}
