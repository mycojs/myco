use std::fs::Metadata;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use v8;

use crate::errors::MycoError;
use crate::run::ops::macros::{
    async_op, create_rejected_promise, create_resolved_promise_void, get_state, get_string_arg,
    sync_op,
};
use crate::run::state::{MycoState, OpResult};
use crate::Capability;
use crate::{register_async_op, register_sync_op};

#[derive(Deserialize)]
struct TokenOptionalPathArg {
    token: String,
    path: Option<String>,
}

#[derive(Deserialize)]
struct TokenPathArg {
    token: String,
    path: String,
}

#[derive(Deserialize)]
struct WriteFileArg {
    token: String,
    contents: serde_v8::JsBuffer,
    path: Option<String>,
}

#[derive(Deserialize)]
struct ExecFileArg {
    token: String,
    path: Option<String>,
    args: Vec<String>,
}

#[derive(Deserialize)]
struct PathArg {
    path: String,
}

#[derive(Deserialize)]
struct EmptyArg;

pub fn register_filesystem_ops(
    scope: &mut v8::ContextScope<v8::HandleScope>,
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

fn async_op_request_read_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let path_buf = std::path::Path::new(&path);

            // Convert relative paths to absolute paths using current working directory
            let path_buf = if path_buf.is_relative() {
                match std::env::current_dir() {
                    Ok(cwd) => cwd.join(path_buf),
                    Err(e) => {
                        return OpResult::Capability(Err(format!(
                            "Failed to get current working directory: {}",
                            e
                        )))
                    }
                }
            } else {
                path_buf.to_path_buf()
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

            OpResult::Capability(Ok(Capability::ReadFile(path)))
        },
    );
}

fn async_op_request_write_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let path_buf = std::path::Path::new(&path);

            // Convert relative paths to absolute paths using current working directory
            let path_buf = if path_buf.is_relative() {
                match std::env::current_dir() {
                    Ok(cwd) => cwd.join(path_buf),
                    Err(e) => {
                        return OpResult::Capability(Err(format!(
                            "Failed to get current working directory: {}",
                            e
                        )))
                    }
                }
            } else {
                path_buf.to_path_buf()
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

            OpResult::Capability(Ok(Capability::WriteFile(path)))
        },
    );
}

fn async_op_request_exec_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    async_op(
        scope,
        rv,
        &args,
        |_scope, input: PathArg| Ok(input),
        |input| async move {
            let path = input.path;
            // Validate ExecFile: file must exist and be executable
            let path_buf = std::path::Path::new(&path);
            if !path_buf.exists() {
                return OpResult::Capability(Err(format!("File does not exist: {}", path)));
            }
            if !path_buf.is_file() {
                return OpResult::Capability(Err(format!("Path is not a file: {}", path)));
            }
            if let Err(e) = tokio::fs::metadata(path_buf).await {
                return OpResult::Capability(Err(format!("Cannot access file '{}': {}", path, e)));
            }
            // On Unix-like systems, check if executable bit is set
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = tokio::fs::metadata(path_buf).await {
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 == 0 {
                        return OpResult::Capability(Err(format!(
                            "File is not executable: {}",
                            path
                        )));
                    }
                }
            }

            OpResult::Capability(Ok(Capability::ExecFile(path)))
        },
    );
}

fn async_op_request_read_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let path_buf = std::path::Path::new(&path);
            if !path_buf.exists() {
                return OpResult::Capability(Err(format!("Directory does not exist: {}", path)));
            }
            if !path_buf.is_dir() {
                return OpResult::Capability(Err(format!("Path is not a directory: {}", path)));
            }
            if let Err(e) = tokio::fs::read_dir(path_buf).await {
                return OpResult::Capability(Err(format!(
                    "Cannot read directory '{}': {}",
                    path, e
                )));
            }

            OpResult::Capability(Ok(Capability::ReadDir(path)))
        },
    );
}

fn async_op_request_write_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let path_buf = std::path::Path::new(&path);
            if !path_buf.exists() {
                return OpResult::Capability(Err(format!("Directory does not exist: {}", path)));
            }
            if !path_buf.is_dir() {
                return OpResult::Capability(Err(format!("Path is not a directory: {}", path)));
            }
            if let Err(e) = tokio::fs::metadata(path_buf).await {
                return OpResult::Capability(Err(format!(
                    "Cannot access directory '{}': {}",
                    path, e
                )));
            }

            OpResult::Capability(Ok(Capability::WriteDir(path)))
        },
    );
}

fn async_op_request_exec_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let path_buf = std::path::Path::new(&path);
            if !path_buf.exists() {
                return OpResult::Capability(Err(format!("Directory does not exist: {}", path)));
            }
            if !path_buf.is_dir() {
                return OpResult::Capability(Err(format!("Path is not a directory: {}", path)));
            }
            if let Err(e) = tokio::fs::metadata(path_buf).await {
                return OpResult::Capability(Err(format!(
                    "Cannot access directory '{}': {}",
                    path, e
                )));
            }

            OpResult::Capability(Ok(Capability::ExecDir(path)))
        },
    );
}

// Path resolution helpers
fn canonical(dir: String, path: String) -> Result<PathBuf, MycoError> {
    let dir_path = PathBuf::from(&dir);
    let dir = dir_path
        .canonicalize()
        .map_err(|e| MycoError::PathCanonicalization {
            path: dir,
            source: e,
        })?;
    let path = if path != "/" {
        dir.clone().join(path.trim_start_matches("/"))
    } else {
        dir.clone()
    };
    if !path.starts_with(&dir) {
        Err(MycoError::Internal {
            message: format!(
                "Attempted to access a path outside of the token's scope: {}",
                path.display()
            ),
        })
    } else {
        Ok(path)
    }
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
#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct File {
    pub name: String,
    pub stats: Stats,
}

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
fn sync_op_read_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenOptionalPathArg| -> Result<serde_v8::ToJsBuffer, MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "read")?;
            std::fs::read(&path_buf)
                .map(serde_v8::ToJsBuffer::from)
                .map_err(|e| MycoError::Internal {
                    message: format!("Failed to read file '{}': {}", path_buf.display(), e),
                })
        },
    );
}

fn sync_op_write_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn sync_op_remove_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenOptionalPathArg| -> Result<(), MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, input.path.clone(), "write")?;
            std::fs::remove_file(&path_buf).map_err(|e| MycoError::Internal {
                message: format!("Failed to remove file '{}': {}", path_buf.display(), e),
            })
        },
    );
}

fn sync_op_mkdirp(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn sync_op_rmdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |scope, input: TokenPathArg| -> Result<(), MycoError> {
            let state = get_state(scope)?;
            let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
            std::fs::remove_dir(&path_buf).map_err(|e| MycoError::Internal {
                message: format!("Failed to remove directory '{}': {}", path_buf.display(), e),
            })
        },
    );
}

fn sync_op_stat_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn sync_op_list_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn sync_op_exec_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn async_op_read_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn async_op_write_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn async_op_remove_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let result = tokio::fs::remove_file(&path_buf)
                .await
                .map_err(|e| format!("Failed to remove file '{}': {}", path_buf.display(), e));

            OpResult::Void(result)
        },
    );
}

fn async_op_mkdirp(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn async_op_rmdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let result = tokio::fs::remove_dir(&path_buf)
                .await
                .map_err(|e| format!("Failed to remove directory '{}': {}", path_buf.display(), e));

            OpResult::Void(result)
        },
    );
}

fn async_op_rmdir_recursive(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
            let result = tokio::fs::remove_dir_all(&path_buf).await.map_err(|e| {
                format!(
                    "Failed to remove directory recursively '{}': {}",
                    path_buf.display(),
                    e
                )
            });

            OpResult::Void(result)
        },
    );
}

fn async_op_stat_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn async_op_list_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn async_op_exec_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn sync_op_cwd(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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

fn sync_op_chdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
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
