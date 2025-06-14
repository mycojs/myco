use std::fs::Metadata;
use std::path::PathBuf;

use v8;
use serde::{Deserialize, Serialize};

use crate::errors::MycoError;
use crate::Capability;
use crate::run::state::MycoState;
use crate::run::ops::macros::{get_state, get_string_arg, create_resolved_promise, create_rejected_promise, create_resolved_promise_void, throw_js_error, sync_op};
use crate::{register_sync_op, register_async_op};

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
struct EmptyArg;

pub fn register_filesystem_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), MycoError> {
    register_async_op!(scope, myco_ops, "request_read_file", async_op_request_read_file);
    register_async_op!(scope, myco_ops, "request_write_file", async_op_request_write_file);
    register_async_op!(scope, myco_ops, "request_exec_file", async_op_request_exec_file);
    register_async_op!(scope, myco_ops, "request_read_dir", async_op_request_read_dir);
    register_async_op!(scope, myco_ops, "request_write_dir", async_op_request_write_dir);
    register_async_op!(scope, myco_ops, "request_exec_dir", async_op_request_exec_dir);
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

fn async_op_request_read_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => return,
    };
    
    // Validate ReadFile: for read-write compatibility, allow files that don't exist yet 
    // if we have permission to create them (similar to write validation)
    let path_buf = std::path::Path::new(&path);
    
    // Convert relative paths to absolute paths using current working directory
    let path_buf = if path_buf.is_relative() {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path_buf),
            Err(e) => {
                throw_js_error(scope, &format!("Failed to get current working directory: {}", e));
                return;
            }
        }
    } else {
        path_buf.to_path_buf()
    };
    
    if path_buf.exists() {
        // If file exists, it must be a readable file
        if !path_buf.is_file() {
            throw_js_error(scope, &format!("Path is not a file: {}", path));
            return;
        }
        if let Err(e) = std::fs::metadata(&path_buf) {
            throw_js_error(scope, &format!("Cannot access file '{}': {}", path, e));
            return;
        }
    } else {
        // If file doesn't exist, check if we can create it (for read-write compatibility)
        if let Some(parent) = path_buf.parent() {
            if !parent.exists() {
                throw_js_error(scope, &format!("Parent directory does not exist: {}", parent.display()));
                return;
            }
            if !parent.is_dir() {
                throw_js_error(scope, &format!("Parent path is not a directory: {}", parent.display()));
                return;
            }
            if let Err(e) = std::fs::metadata(parent) {
                throw_js_error(scope, &format!("Cannot access parent directory '{}': {}", parent.display(), e));
                return;
            }
        } else {
            throw_js_error(scope, "Invalid file path: no parent directory");
            return;
        }
    }
    
    match get_state(scope) {
        Ok(state) => {
            let token = state.capabilities.register(Capability::ReadFile(path));
            let token_string = v8::String::new(scope, &token).unwrap();
            rv.set(token_string.into());
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to get state: {}", e));
        }
    }
}

fn async_op_request_write_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => return,
    };
    
    // Validate WriteFile: parent directory must exist if file doesn't exist
    let path_buf = std::path::Path::new(&path);
    
    // Convert relative paths to absolute paths using current working directory
    let path_buf = if path_buf.is_relative() {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path_buf),
            Err(e) => {
                throw_js_error(scope, &format!("Failed to get current working directory: {}", e));
                return;
            }
        }
    } else {
        path_buf.to_path_buf()
    };
    
    if path_buf.exists() {
        if path_buf.is_dir() {
            throw_js_error(scope, &format!("Path is a directory, not a file: {}", path));
            return;
        }
        if let Err(e) = std::fs::metadata(&path_buf) {
            throw_js_error(scope, &format!("Cannot access file '{}': {}", path, e));
            return;
        }
    } else if let Some(parent) = path_buf.parent() {
        if !parent.exists() {
            throw_js_error(scope, &format!("Parent directory does not exist: {}", parent.display()));
            return;
        }
        if !parent.is_dir() {
            throw_js_error(scope, &format!("Parent path is not a directory: {}", parent.display()));
            return;
        }
        if let Err(e) = std::fs::metadata(parent) {
            throw_js_error(scope, &format!("Cannot access parent directory '{}': {}", parent.display(), e));
            return;
        }
    } else {
        throw_js_error(scope, "Invalid file path: no parent directory");
        return;
    }
    
    match get_state(scope) {
        Ok(state) => {
            let token = state.capabilities.register(Capability::WriteFile(path));
            let token_string = v8::String::new(scope, &token).unwrap();
            rv.set(token_string.into());
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to get state: {}", e));
        }
    }
}

fn async_op_request_exec_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => return,
    };
    
    // Validate ExecFile: file must exist and be executable
    let path_buf = std::path::Path::new(&path);
    if !path_buf.exists() {
        throw_js_error(scope, &format!("File does not exist: {}", path));
        return;
    }
    if !path_buf.is_file() {
        throw_js_error(scope, &format!("Path is not a file: {}", path));
        return;
    }
    if let Err(e) = std::fs::metadata(path_buf) {
        throw_js_error(scope, &format!("Cannot access file '{}': {}", path, e));
        return;
    }
    // On Unix-like systems, check if executable bit is set
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path_buf) {
            let permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                throw_js_error(scope, &format!("File is not executable: {}", path));
                return;
            }
        }
    }
    
    match get_state(scope) {
        Ok(state) => {
            let token = state.capabilities.register(Capability::ExecFile(path));
            let token_string = v8::String::new(scope, &token).unwrap();
            rv.set(token_string.into());
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to get state: {}", e));
        }
    }
}

fn async_op_request_read_dir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => return,
    };
    
    // Validate ReadDir: directory must exist and be readable
    let path_buf = std::path::Path::new(&path);
    if !path_buf.exists() {
        throw_js_error(scope, &format!("Directory does not exist: {}", path));
        return;
    }
    if !path_buf.is_dir() {
        throw_js_error(scope, &format!("Path is not a directory: {}", path));
        return;
    }
    if let Err(e) = std::fs::read_dir(path_buf) {
        throw_js_error(scope, &format!("Cannot read directory '{}': {}", path, e));
        return;
    }
    
    match get_state(scope) {
        Ok(state) => {
            let token = state.capabilities.register(Capability::ReadDir(path));
            let token_string = v8::String::new(scope, &token).unwrap();
            rv.set(token_string.into());
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to get state: {}", e));
        }
    }
}

fn async_op_request_write_dir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => return,
    };
    
    // Validate WriteDir: directory must exist and be writable
    let path_buf = std::path::Path::new(&path);
    if !path_buf.exists() {
        throw_js_error(scope, &format!("Directory does not exist: {}", path));
        return;
    }
    if !path_buf.is_dir() {
        throw_js_error(scope, &format!("Path is not a directory: {}", path));
        return;
    }
    if let Err(e) = std::fs::metadata(path_buf) {
        throw_js_error(scope, &format!("Cannot access directory '{}': {}", path, e));
        return;
    }
    
    match get_state(scope) {
        Ok(state) => {
            let token = state.capabilities.register(Capability::WriteDir(path));
            let token_string = v8::String::new(scope, &token).unwrap();
            rv.set(token_string.into());
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to get state: {}", e));
        }
    }
}

fn async_op_request_exec_dir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => return,
    };
    
    // Validate ExecDir: directory must exist and be accessible
    let path_buf = std::path::Path::new(&path);
    if !path_buf.exists() {
        throw_js_error(scope, &format!("Directory does not exist: {}", path));
        return;
    }
    if !path_buf.is_dir() {
        throw_js_error(scope, &format!("Path is not a directory: {}", path));
        return;
    }
    if let Err(e) = std::fs::metadata(path_buf) {
        throw_js_error(scope, &format!("Cannot access directory '{}': {}", path, e));
        return;
    }
    
    match get_state(scope) {
        Ok(state) => {
            let token = state.capabilities.register(Capability::ExecDir(path));
            let token_string = v8::String::new(scope, &token).unwrap();
            rv.set(token_string.into());
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to get state: {}", e));
        }
    }
}

// Path resolution helpers
fn canonical(dir: String, path: String) -> Result<PathBuf, MycoError> {
    let dir_path = PathBuf::from(&dir);
    let dir = dir_path.canonicalize()
        .map_err(|e| MycoError::PathCanonicalization { path: dir, source: e })?;
    let path = if path != "/" {
        dir.clone().join(path.trim_start_matches("/"))
    } else {
        dir.clone()
    };
    if !path.starts_with(&dir) {
        Err(MycoError::Internal { 
            message: format!("Attempted to access a path outside of the token's scope: {}", path.display()) 
        })
    } else {
        Ok(path)
    }
}

fn resolve_path(state: &MycoState, token: &str, path: Option<String>, access_type: &str) -> Result<PathBuf, MycoError> {
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
        _ => {
            Err(MycoError::Internal { 
                message: format!("Invalid token for {} access", access_type) 
            })
        }
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
            modified: metadata.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
            accessed: metadata.accessed().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
            created: metadata.created().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
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

#[derive(Serialize)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

// Sync operations
fn sync_op_read_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: TokenOptionalPathArg| -> Result<serde_v8::ToJsBuffer, MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, input.path.clone(), "read")?;
        std::fs::read(&path_buf)
            .map(|bytes| serde_v8::ToJsBuffer::from(bytes))
            .map_err(|e| MycoError::Internal {
                message: format!("Failed to read file '{}': {}", path_buf.display(), e)
            })
    });
}

fn sync_op_write_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: WriteFileArg| -> Result<(), MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, input.path.clone(), "write")?;
        std::fs::write(&path_buf, input.contents).map_err(|e| MycoError::Internal {
            message: format!("Failed to write file '{}': {}", path_buf.display(), e)
        })
    });
}

fn sync_op_remove_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: TokenOptionalPathArg| -> Result<(), MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, input.path.clone(), "write")?;
        std::fs::remove_file(&path_buf).map_err(|e| MycoError::Internal {
            message: format!("Failed to remove file '{}': {}", path_buf.display(), e)
        })
    });
}

fn sync_op_mkdirp(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: TokenPathArg| -> Result<(), MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
        std::fs::create_dir_all(&path_buf).map_err(|e| MycoError::Internal {
            message: format!("Failed to create directory '{}': {}", path_buf.display(), e)
        })
    });
}

fn sync_op_rmdir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: TokenPathArg| -> Result<(), MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "write")?;
        std::fs::remove_dir(&path_buf).map_err(|e| MycoError::Internal {
            message: format!("Failed to remove directory '{}': {}", path_buf.display(), e)
        })
    });
}

fn sync_op_stat_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: TokenOptionalPathArg| -> Result<Option<Stats>, MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, input.path.clone(), "read")?;
        match std::fs::metadata(&path_buf) {
            Ok(metadata) => Ok(Some(Stats::from_metadata(metadata))),
            Err(_) => Ok(None),
        }
    });
}

fn sync_op_list_dir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: TokenPathArg| -> Result<Vec<File>, MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, Some(input.path.clone()), "read")?;
        
        let entries = std::fs::read_dir(&path_buf).map_err(|e| MycoError::Internal {
            message: format!("Failed to list directory '{}': {}", path_buf.display(), e)
        })?;
        
        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| MycoError::Internal {
                message: format!("Failed to read directory entry in '{}': {}", path_buf.display(), e)
            })?;
            let metadata = entry.metadata().map_err(|e| MycoError::Internal {
                message: format!("Failed to get metadata for directory entry in '{}': {}", path_buf.display(), e)
            })?;
            result.push(File::from(entry.path(), metadata));
        }
        Ok(result)
    });
}

fn sync_op_exec_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: ExecFileArg| -> Result<ExecResult, MycoError> {
        let state = get_state(scope)?;
        let path_buf = resolve_path(state, &input.token, input.path.clone(), "exec")?;
        
        let output = std::process::Command::new(&path_buf)
            .args(input.args)
            .output()
            .map_err(|e| MycoError::Internal {
                message: format!("Failed to execute command '{}': {}", path_buf.display(), e)
            })?;
            
        Ok(ExecResult {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    });
}

// Async operations (promise-returning wrappers around sync operations)
fn async_op_read_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid token"));
            return;
        }
    };
    let path = if args.length() > 1 && !args.get(1).is_null_or_undefined() {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to get state: {}", e)));
            return;
        }
    };
    
    let path_buf = match resolve_path(state, &token, path.clone(), "read") {
        Ok(p) => p,
        Err(e) => {
            throw_js_error(scope, &format!("Failed to resolve path for read operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e));
            return;
        }
    };
    
    match std::fs::read(&path_buf) {
        Ok(contents) => {
            let array_buffer = v8::ArrayBuffer::new(scope, contents.len());
            let backing_store = array_buffer.get_backing_store();
            unsafe {
                let data = backing_store.data().unwrap().as_ptr() as *mut u8;
                std::ptr::copy_nonoverlapping(contents.as_ptr(), data, contents.len());
            }
            let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, contents.len()).unwrap();
            rv.set(create_resolved_promise(scope, uint8_array.into()));
        }
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to read file '{}': {}", path_buf.display(), e)));
        }
    }
}

fn async_op_write_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid token"));
            return;
        }
    };
    
    let contents = if let Ok(uint8_array) = v8::Local::<v8::Uint8Array>::try_from(args.get(1)) {
        let mut data = vec![0u8; uint8_array.byte_length()];
        if uint8_array.copy_contents(&mut data) != data.len() {
            rv.set(create_rejected_promise(scope, "Failed to copy Uint8Array contents"));
            return;
        }
        data
    } else {
        rv.set(create_rejected_promise(scope, "contents must be Uint8Array"));
        return;
    };
    
    let path = if args.length() > 2 && !args.get(2).is_null_or_undefined() {
        Some(args.get(2).to_rust_string_lossy(scope))
    } else {
        None
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to get state: {}", e)));
            return;
        }
    };
    
    let path_buf = match resolve_path(state, &token, path.clone(), "write") {
        Ok(p) => p,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to resolve path for write operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)));
            return;
        }
    };
    
    match std::fs::write(&path_buf, contents) {
        Ok(_) => rv.set(create_resolved_promise_void(scope)),
        Err(e) => rv.set(create_rejected_promise(scope, &format!("Failed to write file '{}': {}", path_buf.display(), e))),
    }
}

macro_rules! async_simple_file_op {
    ($name:ident, $op:expr, $access:literal, $op_name:literal) => {
        fn $name(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
            let token = match get_string_arg(scope, &args, 0, "token") {
                Ok(t) => t,
                Err(_) => {
                    rv.set(create_rejected_promise(scope, "Missing or invalid token"));
                    return;
                }
            };
            let path = if args.length() > 1 && !args.get(1).is_null_or_undefined() {
                Some(args.get(1).to_rust_string_lossy(scope))
            } else {
                None
            };
            
            let state = match get_state(scope) {
                Ok(s) => s,
                Err(e) => {
                    rv.set(create_rejected_promise(scope, &format!("Failed to get state: {}", e)));
                    return;
                }
            };
            
            let path_buf = match resolve_path(state, &token, path.clone(), $access) {
                Ok(p) => p,
                Err(e) => {
                    rv.set(create_rejected_promise(scope, &format!("Failed to resolve path for {} operation with token '{}'{}: {}", 
                        $op_name,
                        token, 
                        path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                        e)));
                    return;
                }
            };
            
            match $op(&path_buf) {
                Ok(_) => rv.set(create_resolved_promise_void(scope)),
                Err(e) => rv.set(create_rejected_promise(scope, &format!("Failed to {} '{}': {}", $op_name, path_buf.display(), e))),
            }
        }
    };
}

async_simple_file_op!(async_op_remove_file, std::fs::remove_file, "write", "remove file");
async_simple_file_op!(async_op_mkdirp, std::fs::create_dir_all, "write", "create directory");
async_simple_file_op!(async_op_rmdir, std::fs::remove_dir, "write", "remove directory");
async_simple_file_op!(async_op_rmdir_recursive, std::fs::remove_dir_all, "write", "remove directory recursively");

fn async_op_stat_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid token"));
            return;
        }
    };
    let path = if args.length() > 1 && !args.get(1).is_null_or_undefined() {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to get state: {}", e)));
            return;
        }
    };
    
    match resolve_path(state, &token, path.clone(), "read") {
        Ok(path_buf) => {
            match std::fs::metadata(&path_buf) {
                Ok(metadata) => {
                    let stats = Stats::from_metadata(metadata);
                    let stats_json = serde_json::to_string(&stats).unwrap();
                    let json_value = v8::String::new(scope, &stats_json).unwrap();
                    let parsed = v8::json::parse(scope, json_value).unwrap();
                    rv.set(create_resolved_promise(scope, parsed));
                }
                Err(e) => {
                    throw_js_error(scope, &format!("Failed to get file metadata for '{}': {}", path_buf.display(), e));
                }
            }
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to resolve path for stat operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e));
        }
    }
}

fn async_op_list_dir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid token"));
            return;
        }
    };
    let path = match get_string_arg(scope, &args, 1, "path") {
        Ok(p) => p,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid path"));
            return;
        }
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to get state: {}", e)));
            return;
        }
    };
    
    let path_buf = match resolve_path(state, &token, Some(path.clone()), "read") {
        Ok(p) => p,
        Err(e) => {
            throw_js_error(scope, &format!("Failed to resolve path for list directory operation with token '{}' and path '{}': {}", 
                token, path, e));
            return;
        }
    };
    
    match std::fs::read_dir(&path_buf) {
        Ok(entries) => {
            let mut result = Vec::new();
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        match entry.metadata() {
                            Ok(metadata) => result.push(File::from(entry.path(), metadata)),
                            Err(e) => {
                                throw_js_error(scope, &format!("Failed to get metadata for directory entry in '{}': {}", path_buf.display(), e));
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        throw_js_error(scope, &format!("Failed to read directory entry in '{}': {}", path_buf.display(), e));
                        return;
                    }
                }
            }
            let result_json = serde_json::to_string(&result).unwrap();
            let json_value = v8::String::new(scope, &result_json).unwrap();
            let parsed = v8::json::parse(scope, json_value).unwrap();
            rv.set(create_resolved_promise(scope, parsed));
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to list directory '{}': {}", path_buf.display(), e));
        }
    }
}

fn async_op_exec_file(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid token"));
            return;
        }
    };
    
    let path = if args.get(1).is_null_or_undefined() {
        None
    } else {
        Some(args.get(1).to_rust_string_lossy(scope))
    };
    
    let cmd_args = if let Ok(args_array) = v8::Local::<v8::Array>::try_from(args.get(2)) {
        let mut result = Vec::new();
        for i in 0..args_array.length() {
            if let Some(arg_value) = args_array.get_index(scope, i) {
                result.push(arg_value.to_rust_string_lossy(scope));
            }
        }
        result
    } else {
        throw_js_error(scope, "args must be an array");
        return;
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            throw_js_error(scope, &format!("Failed to get state: {}", e));
            return;
        }
    };

    let path_buf = match resolve_path(state, &token, path.clone(), "exec") {
        Ok(p) => p,
        Err(e) => {
            throw_js_error(scope, &format!("Failed to resolve path for exec operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e));
            return;
        }
    };

    match std::process::Command::new(&path_buf).args(cmd_args).output() {
        Ok(output) => {
            let exec_result = ExecResult {
                stdout: output.stdout,
                stderr: output.stderr,
                exit_code: output.status.code().unwrap_or(-1),
            };
            let result_json = serde_json::to_string(&exec_result).unwrap();
            let json_value = v8::String::new(scope, &result_json).unwrap();
            let parsed = v8::json::parse(scope, json_value).unwrap();
            rv.set(create_resolved_promise(scope, parsed));
        }
        Err(e) => {
            throw_js_error(scope, &format!("Failed to execute command: {}", e));
        }
    }
}

fn sync_op_cwd(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |_scope, _input: ()| -> Result<String, MycoError> {
        std::env::current_dir()
            .map(|path| path.to_string_lossy().to_string())
            .map_err(|e| MycoError::Internal {
                message: format!("Failed to get current working directory: {}", e)
            })
    });
}

fn sync_op_chdir(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let path = match get_string_arg(scope, &args, 0, "path") {
        Ok(p) => p,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid path"));
            return;
        }
    };

    match std::env::set_current_dir(&path) {
        Ok(_) => rv.set(create_resolved_promise_void(scope)),
        Err(e) => rv.set(create_rejected_promise(scope, &format!("Failed to change directory to '{}': {}", path, e))),
    }
}

