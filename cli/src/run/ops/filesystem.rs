use std::fs::Metadata;
use std::path::{Path, PathBuf};

use serde::Serialize;
use v8;

use crate::errors::MycoError;
use crate::run::confine::{confine, FinalLink, Refusal};
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

// Pin a directory grant to the directory it named, so the capability refers to
// a fixed place rather than to whatever the name resolves to at the time of a
// later call. Replacing an ancestor with a symlink after the grant is made
// would otherwise move the token's scope silently (#63). It also takes a
// `canonicalize` out of every filesystem operation.
fn pin_dir(path: &Path) -> Result<String, String> {
    path.canonicalize()
        .map(|pinned| pinned.to_string_lossy().to_string())
        .map_err(|e| format!("Cannot resolve directory '{}': {}", path.display(), e))
}

// The same for a single file, whose own last component is left unresolved: a
// file grant may name something that does not exist yet, so there is nothing to
// resolve. Pinning the parent is what stops the grant moving.
//
// That last component is the residue of #63 for file grants. A file token is
// not re-checked on use -- there is no path to confine -- so if the name is
// later replaced by a symlink, an operation through the token follows it. The
// directory the grant was made in is fixed; what sits at that name is not.
fn pin_file(path: &Path) -> Result<String, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid file path: no parent directory: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("Invalid file path: no file name: {}", path.display()))?;
    let parent = parent
        .canonicalize()
        .map_err(|e| format!("Cannot resolve directory '{}': {}", parent.display(), e))?;
    Ok(parent.join(name).to_string_lossy().to_string())
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

            // Pinned now, so the capability names this directory rather than
            // whatever the path resolves to when it is next used.
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

            // Pinned now, so the capability names this directory rather than
            // whatever the path resolves to when it is next used.
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

            // Pinned now, so the capability names this directory rather than
            // whatever the path resolves to when it is next used.
            let pinned = match pin_dir(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::ExecDir(pinned)))
        },
    );
}

// Path resolution helpers

// Resolve a path relative to a directory token, confining it to that directory.
//
// The containment decision lives in `crate::run::confine`, which the module
// graph uses too -- see the module docs there for what the approach does and
// does not cover (hardlinks, bind mounts, and the window between this check and
// the syscall that follows it are all outside it).
//
// `dir` is the directory the grant was pinned to when it was made, already
// canonical, so nothing is re-resolved per call and no ancestor swapped for a
// symlink afterwards can move where the token points.
fn canonical(dir: &str, path: String, final_link: FinalLink) -> Result<PathBuf, MycoError> {
    // Both name the path the caller asked for, never where it led. Where a
    // symlink points is not something a token holder can otherwise find out --
    // there is no readlink op -- and a refusal that reports the landing hands
    // back a path outside the grant, which is an oracle for the shape of the
    // filesystem beyond it. What the caller passed in is also the more useful
    // half for fixing the call.
    confine(Path::new(dir), &path, final_link).map_err(|refusal| match refusal {
        Refusal::Outside(_) => MycoError::PathOutsideToken { path },
        // Distinguished from an escape on purpose: the holder already has this
        // directory, so naming which of the two happened tells them nothing
        // they could not find out, and calling a dangling link inside their own
        // directory "outside the token's scope" sends them hunting (#61).
        Refusal::Unresolvable(_) => MycoError::PathUnresolvableLink { path },
    })
}

/// What an operation does with the path it is given.
///
/// Split finer than the three names the ops used to pass as strings, because
/// `read` covered both reading a file's contents and stat'ing it. Those are
/// different authorities -- being allowed to run a binary does not imply being
/// allowed to read it -- and conflating them is what left #62 with no clean fix.
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
    File(&'a str),
    Dir(&'a str),
}

/// What each capability grants. Total over `Capability` on purpose -- a new
/// variant fails to compile here until its grants are stated, rather than
/// silently joining a catch-all and being reported as a bad token (#66).
fn grant(capability: &Capability) -> Option<(Scope<'_>, &'static [Access])> {
    match capability {
        Capability::ReadFile(path) => Some((Scope::File(path), &[Access::Read, Access::Stat])),
        Capability::WriteFile(path) => Some((Scope::File(path), &[Access::Write])),
        // Stat but not read: size and mtime of something you may execute is not
        // extra authority (#62), the bytes are.
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
        (Scope::File(file_path), None) => Ok(PathBuf::from(file_path)),
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

    // What every operation that follows a link at the end of the path gets:
    // read, write, exec, stat, list.
    fn resolve(dir: &Path, path: &str) -> Result<PathBuf, MycoError> {
        canonical(
            &dir.to_string_lossy(),
            path.to_string(),
            FinalLink::Followed,
        )
    }

    // What `remove` and `rmdir` get: the link itself is the target, so where it
    // points is never touched.
    fn unlink(dir: &Path, path: &str) -> Result<PathBuf, MycoError> {
        canonical(
            &dir.to_string_lossy(),
            path.to_string(),
            FinalLink::Unlinked,
        )
    }

    fn is_outside(result: Result<PathBuf, MycoError>) -> bool {
        matches!(result, Err(MycoError::PathOutsideToken { .. }))
    }

    fn is_unresolvable(result: Result<PathBuf, MycoError>) -> bool {
        matches!(result, Err(MycoError::PathUnresolvableLink { .. }))
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

    // The grant names a directory, not a name that can be repointed at one.
    #[cfg(unix)]
    #[test]
    fn a_grant_is_pinned_to_the_directory_it_named() {
        let scratch = ScratchDir::new("pinning");
        let dir = scratch.dir();
        fs::create_dir(dir.join("real")).unwrap();
        fs::create_dir(dir.join("other")).unwrap();
        std::os::unix::fs::symlink("real", dir.join("link")).unwrap();

        let pinned = pin_dir(&dir.join("link")).unwrap();
        assert_eq!(PathBuf::from(&pinned), dir.join("real"));

        // Repointing the name the grant was made through moves nothing: the
        // token still resolves into the directory it was issued for.
        fs::remove_file(dir.join("link")).unwrap();
        std::os::unix::fs::symlink("other", dir.join("link")).unwrap();
        assert_eq!(
            canonical(&pinned, "file.txt".to_string(), FinalLink::Followed).unwrap(),
            dir.join("real/file.txt")
        );
    }

    // A file grant keeps its own last component unresolved -- it may name
    // something that does not exist yet, and a token for a symlink should go on
    // acting on the symlink.
    #[cfg(unix)]
    #[test]
    fn a_file_grant_pins_the_directory_and_keeps_the_name() {
        let scratch = ScratchDir::new("pinning-file");
        let dir = scratch.dir();
        fs::create_dir(dir.join("real")).unwrap();
        std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
        fs::write(dir.join("real/target.txt"), "contents").unwrap();
        std::os::unix::fs::symlink("target.txt", dir.join("real/alias.txt")).unwrap();

        assert_eq!(
            PathBuf::from(pin_file(&dir.join("link/alias.txt")).unwrap()),
            dir.join("real/alias.txt")
        );
        // Including one that is not there yet.
        assert_eq!(
            PathBuf::from(pin_file(&dir.join("link/absent.txt")).unwrap()),
            dir.join("real/absent.txt")
        );
    }

    // Every capability crossed with every access, so a gap is a failing test
    // rather than a token that reads as invalid at runtime (#62, #66).
    #[test]
    fn each_capability_grants_exactly_what_it_should() {
        use Access::*;
        let expected: &[(Capability, &[Access])] = &[
            (Capability::ReadFile("/f".to_string()), &[Read, Stat]),
            (Capability::WriteFile("/f".to_string()), &[Write]),
            (Capability::ExecFile("/f".to_string()), &[Exec, Stat]),
            (Capability::ReadDir("/d".to_string()), &[Read, Stat, List]),
            (Capability::WriteDir("/d".to_string()), &[Write]),
            (Capability::ExecDir("/d".to_string()), &[Exec, Stat]),
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

    // The pair #62 is about, stated on its own because the reasoning is
    // specific: what a file is is not what it contains.
    #[test]
    fn an_exec_token_can_stat_what_it_may_run_but_not_read_it() {
        for capability in [
            Capability::ExecFile("/f".to_string()),
            Capability::ExecDir("/d".to_string()),
        ] {
            let (_, granted) = grant(&capability).expect("a filesystem capability");
            assert!(granted.contains(&Access::Stat), "{:?}", capability);
            assert!(!granted.contains(&Access::Read), "{:?}", capability);
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

    // A link is judged by where it points, not by whether it resolves. All
    // three of these point inside the directory and none of them resolves.
    #[cfg(unix)]
    #[test]
    fn admits_links_into_the_directory_that_do_not_resolve() {
        let scratch = ScratchDir::new("symlink-dangling");
        let dir = scratch.dir();
        std::os::unix::fs::symlink("nowhere.txt", dir.join("broken")).unwrap();
        std::os::unix::fs::symlink("future.txt", dir.join("pending")).unwrap();
        std::os::unix::fs::symlink("sub/deep.txt", dir.join("nested")).unwrap();

        // Writing through one creates the target it names, inside the
        // directory, which is what a caller means by it.
        assert_eq!(resolve(dir, "broken").unwrap(), dir.join("broken"));
        assert_eq!(resolve(dir, "pending").unwrap(), dir.join("pending"));
        assert_eq!(resolve(dir, "nested").unwrap(), dir.join("nested"));
    }

    // The escape is only visible at the end of the chain: a one-hop check sees
    // `b`, which is inside, while the OS follows the whole thing out.
    #[cfg(unix)]
    #[test]
    fn follows_symlink_chains_to_where_they_end() {
        let scratch = ScratchDir::new("symlink-chain");
        let dir = scratch.dir();
        let outside = dir.parent().unwrap().join(format!(
            "myco-canonical-{}-symlink-chain-target",
            std::process::id()
        ));
        fs::create_dir_all(&outside).unwrap();

        // a -> b -> outside, with a dead end so a resolving check learns nothing.
        std::os::unix::fs::symlink("b.txt", dir.join("a.txt")).unwrap();
        std::os::unix::fs::symlink(outside.join("missing.txt"), dir.join("b.txt")).unwrap();
        assert!(is_outside(resolve(dir, "a.txt")));

        // A chain that stays inside is followed just as far.
        fs::write(dir.join("end.txt"), "contents").unwrap();
        std::os::unix::fs::symlink("mid.txt", dir.join("start.txt")).unwrap();
        std::os::unix::fs::symlink("end.txt", dir.join("mid.txt")).unwrap();
        assert_eq!(resolve(dir, "start.txt").unwrap(), dir.join("start.txt"));

        // A cycle terminates rather than spinning, and is named for what it is.
        std::os::unix::fs::symlink("two.txt", dir.join("one.txt")).unwrap();
        std::os::unix::fs::symlink("one.txt", dir.join("two.txt")).unwrap();
        assert!(is_unresolvable(resolve(dir, "one.txt")));

        let _ = fs::remove_dir_all(&outside);
    }

    // `remove` unlinks the link rather than deleting what it points at, so a
    // link that leads out is still an ordinary thing to clean up (#61).
    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_directory_can_still_be_unlinked() {
        let scratch = ScratchDir::new("symlink-unlink");
        let dir = scratch.dir();
        let outside = dir.parent().unwrap().join(format!(
            "myco-canonical-{}-symlink-unlink-target",
            std::process::id()
        ));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();

        std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("escaping")).unwrap();
        std::os::unix::fs::symlink("nowhere.txt", dir.join("broken")).unwrap();

        // The link itself is inside the directory, and unlinking it touches
        // nothing at the far end.
        assert_eq!(unlink(dir, "escaping").unwrap(), dir.join("escaping"));
        assert_eq!(unlink(dir, "broken").unwrap(), dir.join("broken"));

        // Following the same link is still refused: that does touch the target.
        assert!(is_outside(resolve(dir, "escaping")));

        // What it is reached *through* still has to be in scope, so unlinking
        // is not a way around a symlinked directory.
        std::os::unix::fs::symlink(&outside, dir.join("escape")).unwrap();
        assert!(is_outside(unlink(dir, "escape/secret.txt")));
        // And `..` is applied here either way.
        assert!(is_outside(unlink(dir, "../secret.txt")));

        let _ = fs::remove_dir_all(&outside);
    }
}
