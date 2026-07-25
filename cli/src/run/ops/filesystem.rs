use std::fs::Metadata;
use std::os::fd::{AsFd, BorrowedFd};
use std::path::{Path, PathBuf};

use serde::Serialize;
use v8;

use rustix::fs::{AtFlags, Mode, OFlags};

use crate::errors::MycoError;
use crate::run::confine::{FileGrant, OpenError, Root};
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

// Open the granted directory and keep it open. The descriptor is the grant: it
// names the directory that was there when the request was made, so replacing or
// renaming it afterwards cannot move the token's scope (#63), and every later
// path is resolved by the kernel beneath it rather than by us.
fn grant_dir(path: &Path) -> Result<Root, String> {
    Root::open(path).map_err(|e| format!("Cannot open directory '{}': {}", path.display(), e))
}

// The same for a single file: the directory it lives in is held open, and its
// own name is left unresolved, since a file grant may name something that does
// not exist yet and one for a symlink should go on meaning the symlink.
fn grant_file(path: &Path) -> Result<FileGrant, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid file path: no parent directory: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("Invalid file path: no file name: {}", path.display()))?;
    Ok(FileGrant {
        dir: grant_dir(parent)?,
        name: name.to_string_lossy().to_string(),
    })
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

            let pinned = match grant_file(&path_buf) {
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

            let pinned = match grant_file(&path_buf) {
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

            let pinned = match grant_file(&path_buf) {
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

            // Held open now, so the capability is this directory rather than a
            // name that may come to mean another one.
            let pinned = match grant_dir(&path_buf) {
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

            // Held open now, so the capability is this directory rather than a
            // name that may come to mean another one.
            let pinned = match grant_dir(&path_buf) {
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

            // Held open now, so the capability is this directory rather than a
            // name that may come to mean another one.
            let pinned = match grant_dir(&path_buf) {
                Ok(pinned) => pinned,
                Err(e) => return OpResult::Capability(Err(e)),
            };
            OpResult::Capability(Ok(Capability::ExecDir(pinned)))
        },
    );
}

// Path resolution helpers

// What an operation does with the path it is given.
//
// Split finer than the three names the ops used to pass as strings, because
// `read` covered both reading a file's contents and stat'ing it. Those are
// different authorities -- being allowed to run a binary does not imply being
// allowed to read it -- and conflating them is what left #62 with no clean fix.
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

// What a capability is scoped to: one named file, or a directory that paths
// resolve under.
enum Scope<'a> {
    File(&'a FileGrant),
    Dir(&'a Root),
}

// What each capability grants. Total over `Capability` on purpose -- a new
// variant fails to compile here until its grants are stated, rather than
// silently joining a catch-all and being reported as a bad token (#66).
fn grant(capability: &Capability) -> Option<(Scope<'_>, &'static [Access])> {
    match capability {
        Capability::ReadFile(file) => Some((Scope::File(file), &[Access::Read, Access::Stat])),
        Capability::WriteFile(file) => Some((Scope::File(file), &[Access::Write])),
        // Stat but not read: size and mtime of something you may execute is not
        // extra authority (#62), the bytes are.
        Capability::ExecFile(file) => Some((Scope::File(file), &[Access::Exec, Access::Stat])),
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

// A checked grant, ready to be used off the JS thread.
//
// The root is a second descriptor for the granted directory and `rel` is the
// path to resolve beneath it. Nothing here has touched the filesystem yet: the
// resolution happens at the open, which is also the operation, so there is no
// window in which what was checked and what is used can differ.
struct Grant {
    root: Root,
    rel: String,
}

impl Grant {
    fn open(
        &self,
        action: &str,
        flags: OFlags,
        mode: Mode,
    ) -> Result<std::os::fd::OwnedFd, MycoError> {
        self.root
            .open_at(&self.rel, flags, mode)
            .map_err(|e| self.error(action, e))
    }

    fn parent(&self, action: &str) -> Result<(std::os::fd::OwnedFd, String), MycoError> {
        self.root
            .parent_at(&self.rel)
            .map_err(|e| self.error(action, e))
    }

    /// The path as the caller named it, under the directory they were granted.
    /// Never where a symlink led: that is outside the grant, and there is no
    /// readlink op, so reporting it would be an oracle for what lies beyond.
    fn display(&self) -> String {
        self.root.display().join(&self.rel).display().to_string()
    }

    fn error(&self, action: &str, e: OpenError) -> MycoError {
        match e {
            OpenError::Outside => MycoError::PathOutsideToken {
                path: self.rel.clone(),
            },
            OpenError::Io(source) => MycoError::FileOperation {
                action: action.to_string(),
                path: self.display(),
                source,
            },
        }
    }
}

fn resolve_path(
    state: &MycoState,
    token: &str,
    path: Option<String>,
    access: Access,
) -> Result<Grant, MycoError> {
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

    let (root, rel) = match (scope, path) {
        (Scope::File(file), None) => (&file.dir, file.name.clone()),
        (Scope::File(_), Some(_)) => return Err(MycoError::PathNotAllowedForFileToken),
        (Scope::Dir(dir), Some(path)) => (dir, path),
        (Scope::Dir(_), None) => return Err(MycoError::PathRequiredForDirToken),
    };

    let root = root.try_clone().map_err(|e| MycoError::Internal {
        message: format!("Failed to duplicate the granted directory: {}", e),
    })?;
    Ok(Grant { root, rel })
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
// The work each operation does, once the grant has been checked.
//
// Written once and shared by the sync and async entry points below, which used
// to be hand-duplicated and had drifted apart.

fn io_error(grant: &Grant, what: &str, e: std::io::Error) -> MycoError {
    MycoError::FileOperation {
        action: what.to_string(),
        path: grant.display(),
        source: e,
    }
}

fn errno_error(grant: &Grant, what: &str, e: rustix::io::Errno) -> MycoError {
    io_error(grant, what, e.into())
}

fn read_bytes(grant: &Grant) -> Result<Vec<u8>, MycoError> {
    use std::io::Read;
    let fd = grant.open("read file", OFlags::RDONLY, Mode::empty())?;
    let mut bytes = Vec::new();
    std::fs::File::from(fd)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(grant, "read file", e))?;
    Ok(bytes)
}

fn write_bytes(grant: &Grant, contents: &[u8]) -> Result<(), MycoError> {
    use std::io::Write;
    let fd = grant.open(
        "write file",
        OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC,
        Mode::from_raw_mode(0o644),
    )?;
    std::fs::File::from(fd)
        .write_all(contents)
        .map_err(|e| io_error(grant, "write file", e))
}

// `unlinkat` acts on the name, never on what it points at, so removing a
// symlink that leads out of the grant removes the link and leaves its target
// alone. That used to be a flag the caller passed and could get wrong.
fn remove_entry(grant: &Grant) -> Result<(), MycoError> {
    let (parent, name) = grant.parent("remove file")?;
    match rustix::fs::unlinkat(parent.as_fd(), name.as_str(), AtFlags::empty()) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
        Err(e) => Err(errno_error(grant, "remove file", e)),
    }
}

fn remove_directory(grant: &Grant) -> Result<(), MycoError> {
    let (parent, name) = grant.parent("remove directory")?;
    match rustix::fs::unlinkat(parent.as_fd(), name.as_str(), AtFlags::REMOVEDIR) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
        Err(e) => Err(errno_error(grant, "remove directory", e)),
    }
}

// Each component is created through the root, so the kernel resolves what comes
// before it and a component that is a symlink out of the grant is refused
// rather than followed.
fn make_directories(grant: &Grant) -> Result<(), MycoError> {
    let mut prefix = String::new();
    for component in grant.rel.split('/').filter(|c| !c.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);

        let step = Grant {
            root: grant.root.try_clone().map_err(|e| MycoError::Internal {
                message: format!("Failed to duplicate the granted directory: {}", e),
            })?,
            rel: prefix.clone(),
        };
        // Already there, and a directory: nothing to do. A component that
        // leaves the grant is refused as such rather than being reported as a
        // name that cannot be created.
        match step.open(
            "create directory",
            OFlags::RDONLY | OFlags::DIRECTORY,
            Mode::empty(),
        ) {
            Ok(_) => continue,
            Err(e @ MycoError::PathOutsideToken { .. }) => return Err(e),
            Err(_) => {}
        }
        let (parent, name) = step.parent("create directory")?;
        match rustix::fs::mkdirat(parent.as_fd(), name.as_str(), Mode::from_raw_mode(0o777)) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(e) => return Err(errno_error(&step, "create directory", e)),
        }
    }
    Ok(())
}

// Descends only through real directories: every step is opened `O_NOFOLLOW`, so
// a symlink inside the tree is unlinked rather than followed into.
fn remove_tree(dir: BorrowedFd<'_>) -> std::io::Result<()> {
    let mut entries = Vec::new();
    for entry in rustix::fs::Dir::read_from(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        entries.push(name);
    }

    for name in entries {
        let stat = rustix::fs::statat(dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)?;
        let is_dir = (stat.st_mode as u32 & rustix::fs::FileType::Directory.as_raw_mode())
            == rustix::fs::FileType::Directory.as_raw_mode();
        if is_dir {
            let sub = rustix::fs::openat(
                dir,
                name.as_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            remove_tree(sub.as_fd())?;
            rustix::fs::unlinkat(dir, name.as_str(), AtFlags::REMOVEDIR)?;
        } else {
            rustix::fs::unlinkat(dir, name.as_str(), AtFlags::empty())?;
        }
    }
    Ok(())
}

fn remove_tree_at(grant: &Grant) -> Result<(), MycoError> {
    let (parent, name) = grant.parent("remove directory tree")?;
    let stat = match rustix::fs::statat(parent.as_fd(), name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(rustix::io::Errno::NOENT) => return Ok(()),
        Err(e) => return Err(errno_error(grant, "remove directory tree", e)),
    };
    let is_dir = (stat.st_mode as u32 & rustix::fs::FileType::Directory.as_raw_mode())
        == rustix::fs::FileType::Directory.as_raw_mode();

    if !is_dir {
        // A symlink where a directory was named: unlink it, as the old
        // `remove_dir_all` did, rather than following it anywhere.
        return match rustix::fs::unlinkat(parent.as_fd(), name.as_str(), AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
            Err(e) => Err(errno_error(grant, "remove directory tree", e)),
        };
    }

    let dir = rustix::fs::openat(
        parent.as_fd(),
        name.as_str(),
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| errno_error(grant, "remove directory tree", e))?;
    remove_tree(dir.as_fd()).map_err(|e| io_error(grant, "remove directory tree", e))?;
    match rustix::fs::unlinkat(parent.as_fd(), name.as_str(), AtFlags::REMOVEDIR) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
        Err(e) => Err(errno_error(grant, "remove directory tree", e)),
    }
}

fn stat_of(grant: &Grant) -> Result<Metadata, MycoError> {
    let fd = grant.open("get file metadata for", OFlags::PATH, Mode::empty())?;
    std::fs::File::from(fd)
        .metadata()
        .map_err(|e| io_error(grant, "get file metadata for", e))
}

fn list_entries(grant: &Grant) -> Result<Vec<File>, MycoError> {
    let fd = grant.open(
        "list directory",
        OFlags::RDONLY | OFlags::DIRECTORY,
        Mode::empty(),
    )?;

    let mut names = Vec::new();
    for entry in rustix::fs::Dir::read_from(fd.as_fd())
        .map_err(|e| errno_error(grant, "list directory", e))?
    {
        let entry = entry.map_err(|e| errno_error(grant, "list directory", e))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        names.push(name);
    }
    names.sort();

    let mut result = Vec::new();
    for name in names {
        // Not followed, so a symlink is reported as one -- which is what the
        // `include_symlinks` filter in the runtime reads.
        let entry = rustix::fs::openat(
            fd.as_fd(),
            name.as_str(),
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| errno_error(grant, "list directory", e))?;
        let metadata = std::fs::File::from(entry)
            .metadata()
            .map_err(|e| io_error(grant, "list directory", e))?;
        result.push(File {
            name,
            stats: Stats::from_metadata(metadata),
        });
    }
    Ok(result)
}

// The resolved file is reached through `/proc/self/fd`, so what is executed is
// the descriptor the check opened rather than a path resolved a second time.
// The descriptor is deliberately not close-on-exec: the child has to be able to
// open it.
fn exec_at(grant: &Grant, args: Vec<String>) -> Result<ExecResult, MycoError> {
    use std::os::fd::AsRawFd;

    let fd = grant
        .root
        .open_at_exact(&grant.rel, OFlags::PATH, Mode::empty())
        .map_err(|e| grant.error("execute", e))?;

    // `argv[0]` is set back to the path the caller named. Otherwise every
    // program launched this way would see `/proc/self/fd/N` as its own name,
    // which is an implementation detail of how it was reached.
    use std::os::unix::process::CommandExt;
    let program = format!("/proc/self/fd/{}", fd.as_raw_fd());
    let output = std::process::Command::new(&program)
        .arg0(grant.display())
        .args(args)
        .output()
        .map_err(|e| io_error(grant, "execute", e))?;

    Ok(ExecResult {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_code: output.status.code().unwrap_or(-1),
    })
}

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
            let grant = resolve_path(state, &input.token, input.path.clone(), Access::Read)?;
            read_bytes(&grant).map(ToJsBuffer::from)
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
            let grant = resolve_path(state, &input.token, input.path.clone(), Access::Write)?;
            write_bytes(&grant, &input.contents)
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
            let grant = resolve_path(state, &input.token, input.path.clone(), Access::Write)?;
            remove_entry(&grant)
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
            let grant = resolve_path(state, &input.token, Some(input.path.clone()), Access::Write)?;
            make_directories(&grant)
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
            let grant = resolve_path(state, &input.token, Some(input.path.clone()), Access::Write)?;
            remove_directory(&grant)
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
            let grant = resolve_path(state, &input.token, input.path.clone(), Access::Stat)?;
            // Absence is reported as `null` here, unlike the async op, which
            // rejects. Long-standing, and the suite pins both.
            Ok(stat_of(&grant).ok().map(Stats::from_metadata))
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
            let grant = resolve_path(state, &input.token, Some(input.path.clone()), Access::List)?;
            list_entries(&grant)
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
            let grant = resolve_path(state, &input.token, input.path.clone(), Access::Exec)?;
            exec_at(&grant, input.args)
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
            resolve_path(state, &input.token, input.path.clone(), Access::Read)
        },
        |grant| async move { OpResult::Binary(read_bytes(&grant).map_err(|e| e.to_string())) },
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
            let grant = resolve_path(state, &input.token, input.path.clone(), Access::Write)?;
            Ok((grant, input.contents))
        },
        |(grant, contents)| async move {
            OpResult::Void(write_bytes(&grant, &contents).map_err(|e| e.to_string()))
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
            resolve_path(state, &input.token, input.path.clone(), Access::Write)
        },
        |grant| async move { OpResult::Void(remove_entry(&grant).map_err(|e| e.to_string())) },
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
            resolve_path(state, &input.token, Some(input.path.clone()), Access::Write)
        },
        |grant| async move { OpResult::Void(make_directories(&grant).map_err(|e| e.to_string())) },
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
            resolve_path(state, &input.token, Some(input.path.clone()), Access::Write)
        },
        |grant| async move { OpResult::Void(remove_directory(&grant).map_err(|e| e.to_string())) },
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
            resolve_path(state, &input.token, Some(input.path.clone()), Access::Write)
        },
        |grant| async move { OpResult::Void(remove_tree_at(&grant).map_err(|e| e.to_string())) },
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
            resolve_path(state, &input.token, input.path.clone(), Access::Stat)
        },
        |grant| async move {
            OpResult::Json(
                stat_of(&grant)
                    .map(|metadata| {
                        serde_json::to_string(&FileStats::from_metadata(metadata)).unwrap()
                    })
                    .map_err(|e| e.to_string()),
            )
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
            resolve_path(state, &input.token, Some(input.path.clone()), Access::List)
        },
        |grant| async move {
            OpResult::Json(
                list_entries(&grant)
                    .map(|entries| {
                        let infos: Vec<FileInfo> = entries
                            .into_iter()
                            .map(|file| FileInfo {
                                name: file.name,
                                stats: FileStats {
                                    is_file: file.stats.is_file,
                                    is_dir: file.stats.is_dir,
                                    is_symlink: file.stats.is_symlink,
                                    size: file.stats.size,
                                    readonly: file.stats.readonly,
                                    modified: file.stats.modified,
                                    accessed: file.stats.accessed,
                                    created: file.stats.created,
                                },
                            })
                            .collect();
                        serde_json::to_string(&infos).unwrap()
                    })
                    .map_err(|e| e.to_string()),
            )
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
            let grant = resolve_path(state, &input.token, input.path.clone(), Access::Exec)?;
            Ok((grant, input.args))
        },
        |(grant, args)| async move {
            OpResult::Json(
                exec_at(&grant, args)
                    .map(|result| serde_json::to_string(&result).unwrap())
                    .map_err(|e| e.to_string()),
            )
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

    // Path confinement itself is tested in `crate::run::confine`, which owns
    // it. What is specific here is which capability grants which access.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let path =
                std::env::temp_dir().join(format!("myco-grant-{}-{}", std::process::id(), name));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Scratch {
                path: path.canonicalize().unwrap(),
            }
        }

        fn dir(&self) -> Root {
            Root::open(&self.path).unwrap()
        }

        fn file(&self) -> FileGrant {
            FileGrant {
                dir: self.dir(),
                name: "f".to_string(),
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    // Every capability crossed with every access, so a gap is a failing test
    // rather than a token that reads as invalid at runtime (#62, #66).
    #[test]
    fn each_capability_grants_exactly_what_it_should() {
        use Access::*;
        let scratch = Scratch::new("table");
        let expected: Vec<(Capability, &[Access])> = vec![
            (Capability::ReadFile(scratch.file()), &[Read, Stat]),
            (Capability::WriteFile(scratch.file()), &[Write]),
            (Capability::ExecFile(scratch.file()), &[Exec, Stat]),
            (Capability::ReadDir(scratch.dir()), &[Read, Stat, List]),
            (Capability::WriteDir(scratch.dir()), &[Write]),
            (Capability::ExecDir(scratch.dir()), &[Exec, Stat]),
        ];

        for (capability, granted) in &expected {
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
        let scratch = Scratch::new("exec");
        for capability in [
            Capability::ExecFile(scratch.file()),
            Capability::ExecDir(scratch.dir()),
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
}
