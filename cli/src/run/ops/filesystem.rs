use std::fs::Metadata;
use std::path::{Path, PathBuf};

use serde::Serialize;
use v8;

use crate::errors::MycoError;
use crate::run::capabilities::FileGrant;
use crate::run::confine::{confine, resolve_through_links, FinalLink, Refusal};
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

// Make paths absolute without collapsing `..` before following symlinks.
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

    Ok(absolute_path)
}

// Resolve aliases once at issuance; retain native paths without lossy encoding.
fn pin_dir(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("Cannot resolve directory '{}': {}", path.display(), e))
}

// Keep the leaf for unlinking, and remember its target for operations that follow it.
fn pin_file(path: &Path) -> Result<FileGrant, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid file path: no parent directory: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("Invalid file path: no file name: {}", path.display()))?;
    // An empty PATH entry yields a bare filename whose parent is `""`.
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let parent = parent
        .canonicalize()
        .map_err(|e| format!("Cannot resolve directory '{}': {}", parent.display(), e))?;
    let path = parent.join(name);
    let target = resolve_through_links(&path)
        .ok_or_else(|| format!("Cannot resolve file '{}'", path.display()))?;
    Ok(FileGrant { path, target })
}

fn resolve_file(grant: &FileGrant, final_link: FinalLink) -> Result<PathBuf, MycoError> {
    let (probe, expected) = match final_link {
        FinalLink::Followed => (grant.path.as_path(), grant.target.as_path()),
        FinalLink::Unlinked => {
            let parent = grant.path.parent().expect("file grants have a parent");
            (parent, parent)
        }
    };
    match resolve_through_links(probe) {
        Some(target) if target == expected => Ok(grant.path.clone()),
        Some(_) => Err(MycoError::PathOutsideToken {
            path: grant.path.display().to_string(),
        }),
        None => Err(MycoError::PathUnresolvableLink {
            path: grant.path.display().to_string(),
        }),
    }
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

            let pinned = match pin_file(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::ReadFile(pinned)))
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

            let pinned = match pin_file(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::WriteFile(pinned)))
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

            let pinned = match pin_file(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::ExecFile(pinned)))
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

            let pinned = match pin_dir(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::ReadDir(pinned)))
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

            let pinned = match pin_dir(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::WriteDir(pinned)))
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

            let pinned = match pin_dir(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::ExecDir(pinned)))
        },
    );
}

// Path resolution helpers

// Report the caller's path, never a destination outside the grant.
fn canonical(dir: &Path, path: String, final_link: FinalLink) -> Result<PathBuf, MycoError> {
    confine(dir, &path, final_link).map_err(|refusal| match refusal {
        Refusal::Outside => MycoError::PathOutsideToken { path },
        Refusal::Unresolvable => MycoError::PathUnresolvableLink { path },
    })
}

/// Stat is separate from reading bytes so exec grants can expose metadata only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Stat,
    List,
    Write,
    Exec,
}

impl Access {
    fn describe(self) -> &'static str {
        match self {
            Access::Read => "read",
            Access::Stat => "stat",
            Access::List => "list",
            Access::Write => "write",
            Access::Exec => "exec",
        }
    }
}

/// What a capability is scoped to: one file, or a directory paths resolve under.
enum Scope<'a> {
    File(&'a FileGrant),
    Dir(&'a Path),
}

/// Exhaustive so new capability variants require an explicit access decision.
fn grant(capability: &Capability) -> Option<(Scope<'_>, &'static [Access])> {
    match capability {
        Capability::ReadFile(path) => Some((Scope::File(path), &[Access::Read, Access::Stat])),
        Capability::WriteFile(path) => Some((Scope::File(path), &[Access::Write])),
        Capability::ExecFile(path) => Some((Scope::File(path), &[Access::Exec, Access::Stat])),
        Capability::ReadDir(dir) => {
            Some((Scope::Dir(dir), &[Access::Read, Access::Stat, Access::List]))
        }
        Capability::WriteDir(dir) => Some((Scope::Dir(dir), &[Access::Write])),
        Capability::ExecDir(dir) => Some((Scope::Dir(dir), &[Access::Exec, Access::Stat])),
        Capability::FetchUrl(_)
        | Capability::FetchPrefix(_)
        | Capability::TcpListener(_)
        | Capability::TcpStream(_) => None,
    }
}

fn resolve_path(
    state: &MycoState,
    token: &str,
    path: Option<String>,
    access: Access,
    final_link: FinalLink,
) -> Result<PathBuf, MycoError> {
    let capability = state
        .capabilities
        .get(token)
        .ok_or(MycoError::UnknownToken)?;

    let (scope, granted) = grant(capability).ok_or(MycoError::TokenGrantsNoFileAccess)?;

    if !granted.contains(&access) {
        return Err(MycoError::CapabilityLacksAccess {
            access: access.describe().to_string(),
        });
    }

    match (scope, path) {
        (Scope::File(file), None) => resolve_file(file, final_link),
        (Scope::File(_), Some(_)) => Err(MycoError::PathNotAllowedForFileToken),
        (Scope::Dir(dir), Some(path)) => canonical(dir, path, final_link),
        (Scope::Dir(_), None) => Err(MycoError::PathRequiredForDirToken),
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Read,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Write,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Write,
                FinalLink::Unlinked,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                Some(input.path.clone()),
                Access::Write,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                Some(input.path.clone()),
                Access::Write,
                FinalLink::Unlinked,
            )?;
            #[cfg(unix)]
            let unlink = std::fs::symlink_metadata(&path_buf)
                .is_ok_and(|metadata| metadata.file_type().is_symlink());
            #[cfg(not(unix))]
            let unlink = false;
            let result = if unlink {
                std::fs::remove_file(&path_buf)
            } else {
                std::fs::remove_dir(&path_buf)
            };
            match result {
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Stat,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                Some(input.path.clone()),
                Access::List,
                FinalLink::Followed,
            )?;

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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Exec,
                FinalLink::Followed,
            )?;

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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Read,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Write,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Write,
                FinalLink::Unlinked,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                Some(input.path.clone()),
                Access::Write,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                Some(input.path.clone()),
                Access::Write,
                FinalLink::Unlinked,
            )?;
            Ok(path_buf)
        },
        |path_buf| async move {
            #[cfg(unix)]
            let unlink = tokio::fs::symlink_metadata(&path_buf)
                .await
                .is_ok_and(|metadata| metadata.file_type().is_symlink());
            #[cfg(not(unix))]
            let unlink = false;
            let result = if unlink {
                tokio::fs::remove_file(&path_buf).await
            } else {
                tokio::fs::remove_dir(&path_buf).await
            };

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
            let path_buf = resolve_path(
                state,
                &input.token,
                Some(input.path.clone()),
                Access::Write,
                FinalLink::Unlinked,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Stat,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                Some(input.path.clone()),
                Access::List,
                FinalLink::Followed,
            )?;
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
            let path_buf = resolve_path(
                state,
                &input.token,
                input.path.clone(),
                Access::Exec,
                FinalLink::Followed,
            )?;
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

    #[cfg(unix)]
    #[test]
    fn a_grant_is_pinned_to_the_directory_it_named() {
        let scratch = ScratchDir::new("pinning");
        let dir = scratch.dir();
        fs::create_dir(dir.join("real")).unwrap();
        fs::create_dir(dir.join("other")).unwrap();
        std::os::unix::fs::symlink("real", dir.join("link")).unwrap();

        let pinned = pin_dir(&dir.join("link")).unwrap();
        assert_eq!(pinned, dir.join("real"));

        fs::remove_file(dir.join("link")).unwrap();
        std::os::unix::fs::symlink("other", dir.join("link")).unwrap();
        assert_eq!(
            canonical(&pinned, "file.txt".to_string(), FinalLink::Followed).unwrap(),
            dir.join("real/file.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_grant_pins_the_directory_and_keeps_the_name() {
        let scratch = ScratchDir::new("pinning-file");
        let dir = scratch.dir();
        fs::create_dir(dir.join("real")).unwrap();
        std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
        fs::write(dir.join("real/target.txt"), "contents").unwrap();
        std::os::unix::fs::symlink("target.txt", dir.join("real/alias.txt")).unwrap();

        let alias = pin_file(&dir.join("link/alias.txt")).unwrap();
        assert_eq!(alias.path, dir.join("real/alias.txt"));
        assert_eq!(alias.target, dir.join("real/target.txt"));
        assert_eq!(
            resolve_file(&alias, FinalLink::Followed).unwrap(),
            alias.path
        );
        let absent = pin_file(&dir.join("link/absent.txt")).unwrap();
        assert_eq!(absent.path, dir.join("real/absent.txt"));
        assert_eq!(absent.target, absent.path);
        assert_eq!(
            resolve_file(&absent, FinalLink::Followed).unwrap(),
            absent.path
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_empty_path_entry_can_grant_a_current_directory_binary() {
        const CHILD: &str = "MYCO_TEST_EMPTY_PATH";
        const TEST: &str =
            "run::ops::filesystem::tests::an_empty_path_entry_can_grant_a_current_directory_binary";
        if std::env::var_os(CHILD).is_some() {
            let candidate = find_in_path("binary")
                .await
                .expect("binary in current directory");
            let grant = pin_file(&candidate).unwrap();
            assert!(grant.path.is_absolute());
            assert!(std::process::Command::new(grant.path)
                .arg("--list")
                .output()
                .unwrap()
                .status
                .success());
            return;
        }

        // Isolate cwd and PATH changes from the other, concurrently running tests.
        let scratch = ScratchDir::new("empty-path");
        let executable = std::env::current_exe().unwrap();
        std::os::unix::fs::symlink(&executable, scratch.dir().join("binary")).unwrap();
        let child = std::process::Command::new(executable)
            .args(["--exact", TEST, "--nocapture"])
            .current_dir(scratch.dir())
            .env("PATH", "")
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_grant_refuses_repointing_but_can_unlink_the_leaf() {
        let scratch = ScratchDir::new("file-repoint");
        let dir = scratch.dir();
        fs::create_dir(dir.join("root")).unwrap();
        fs::write(dir.join("outside"), "secret").unwrap();
        let leaf = dir.join("root/file");
        let grant = pin_file(&leaf).unwrap();
        fs::write(&leaf, "created after grant").unwrap();
        assert_eq!(resolve_file(&grant, FinalLink::Followed).unwrap(), leaf);
        fs::remove_file(&leaf).unwrap();
        std::os::unix::fs::symlink("../outside", &leaf).unwrap();
        assert!(matches!(
            resolve_file(&grant, FinalLink::Followed),
            Err(MycoError::PathOutsideToken { .. })
        ));
        fs::remove_file(resolve_file(&grant, FinalLink::Unlinked).unwrap()).unwrap();
        assert_eq!(fs::read_to_string(dir.join("outside")).unwrap(), "secret");

        // An explicitly granted outward symlink is allowed until its target changes.
        std::os::unix::fs::symlink("../outside", &leaf).unwrap();
        let alias = pin_file(&leaf).unwrap();
        assert_eq!(
            fs::read_to_string(resolve_file(&alias, FinalLink::Followed).unwrap()).unwrap(),
            "secret"
        );
        fs::remove_file(&leaf).unwrap();
        std::os::unix::fs::symlink("../other", &leaf).unwrap();
        assert!(resolve_file(&alias, FinalLink::Followed).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn grants_retain_non_utf8_canonical_paths() {
        use std::os::unix::ffi::OsStrExt;
        let scratch = ScratchDir::new("native-path");
        let dir = scratch.dir();
        let native = dir.join(std::ffi::OsStr::from_bytes(b"native-\xff"));
        fs::create_dir(&native).unwrap();
        fs::write(native.join("file"), "contents").unwrap();
        std::os::unix::fs::symlink(&native, dir.join("alias")).unwrap();
        let grant = pin_file(&dir.join("alias/file")).unwrap();
        assert_eq!(
            resolve_file(&grant, FinalLink::Followed).unwrap(),
            native.join("file")
        );
        assert_eq!(
            canonical(
                &pin_dir(&dir.join("alias")).unwrap(),
                "file".into(),
                FinalLink::Followed
            )
            .unwrap(),
            native.join("file")
        );
    }

    fn file(path: &str) -> FileGrant {
        FileGrant {
            path: path.into(),
            target: path.into(),
        }
    }

    #[test]
    fn each_capability_grants_exactly_what_it_should() {
        use Access::*;
        let expected: &[(Capability, &[Access])] = &[
            (Capability::ReadFile(file("/f")), &[Read, Stat]),
            (Capability::WriteFile(file("/f")), &[Write]),
            (Capability::ExecFile(file("/f")), &[Exec, Stat]),
            (Capability::ReadDir("/d".into()), &[Read, Stat, List]),
            (Capability::WriteDir("/d".into()), &[Write]),
            (Capability::ExecDir("/d".into()), &[Exec, Stat]),
        ];

        for (capability, granted) in expected {
            let (_, actual) = grant(capability).expect("a filesystem capability");
            for access in [Read, Stat, List, Write, Exec] {
                assert_eq!(
                    actual.contains(&access),
                    granted.contains(&access),
                    "{:?} and {:?}",
                    capability,
                    access
                );
            }
        }
    }

    #[test]
    fn capabilities_that_are_not_filesystem_grant_no_file_access() {
        assert!(grant(&Capability::FetchUrl("https://example.com".to_string())).is_none());
        assert!(grant(&Capability::FetchPrefix(
            url::Url::parse("https://example.com/").unwrap()
        ))
        .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn refusals_report_the_requested_path_not_the_target() {
        let scratch = ScratchDir::new("refusal");
        let dir = scratch.dir();
        std::os::unix::fs::symlink("../outside/secret", dir.join("escape")).unwrap();
        std::os::unix::fs::symlink("cycle", dir.join("cycle")).unwrap();
        assert!(matches!(
            canonical(dir, "escape".into(), FinalLink::Followed),
            Err(MycoError::PathOutsideToken { path }) if path == "escape"
        ));
        assert!(matches!(
            canonical(dir, "cycle".into(), FinalLink::Followed),
            Err(MycoError::PathUnresolvableLink { path }) if path == "cycle"
        ));
    }
}
