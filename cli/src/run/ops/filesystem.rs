use std::fs::Metadata;
use std::path::PathBuf;

use anyhow::anyhow;
use v8;

use crate::{AnyError, Capability, MycoState, request_file_op};
use crate::run::ops::macros::{get_state, get_string_arg, create_resolved_promise, create_rejected_promise, create_resolved_promise_void};

pub fn register_filesystem_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), anyhow::Error> {
    macro_rules! register_op {
        ($name:literal, $fn:ident) => {
            let func = v8::Function::new(scope, $fn).unwrap();
            let key = v8::String::new(scope, $name).unwrap();
            myco_ops.set(scope, key.into(), func.into());
        };
    }
    
    register_op!("request_read_file", request_read_file_op);
    register_op!("request_write_file", request_write_file_op);
    register_op!("request_exec_file", request_exec_file_op);
    register_op!("request_read_dir", request_read_dir_op);
    register_op!("request_write_dir", request_write_dir_op);
    register_op!("request_exec_dir", request_exec_dir_op);
    register_op!("read_file", read_file_op);
    register_op!("write_file", write_file_op);
    register_op!("remove_file", remove_file_op);
    register_op!("stat_file", stat_file_op);
    register_op!("list_dir", list_dir_op);
    register_op!("mkdirp", mkdirp_op);
    register_op!("rmdir", rmdir_op);
    register_op!("exec_file", exec_file_op);
    register_op!("read_file_sync", read_file_sync_op);
    register_op!("write_file_sync", write_file_sync_op);
    register_op!("remove_file_sync", remove_file_sync_op);
    register_op!("stat_file_sync", stat_file_sync_op);
    register_op!("list_dir_sync", list_dir_sync_op);
    register_op!("mkdirp_sync", mkdirp_sync_op);
    register_op!("rmdir_sync", rmdir_sync_op);
    register_op!("exec_file_sync", exec_file_sync_op);
    register_op!("cwd", cwd_op);
    Ok(())
}

request_file_op!(request_read_file_op, ReadFile);
request_file_op!(request_write_file_op, WriteFile);
request_file_op!(request_exec_file_op, ExecFile);
request_file_op!(request_read_dir_op, ReadDir);
request_file_op!(request_write_dir_op, WriteDir);
request_file_op!(request_exec_dir_op, ExecDir);

// Path resolution helpers
fn canonical(dir: String, path: String) -> Result<PathBuf, AnyError> {
    let dir = PathBuf::from(dir).canonicalize()?;
    let path = if path != "/" {
        dir.clone().join(path.trim_start_matches("/"))
    } else {
        dir.clone()
    };
    if !path.starts_with(&dir) {
        Err(anyhow!("Attempted to access a path outside of the token's scope: {}", path.display()))
    } else {
        Ok(path)
    }
}

fn resolve_path(state: &MycoState, token: &str, path: Option<String>, access_type: &str) -> Result<PathBuf, AnyError> {
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
            Err(anyhow!("Invalid token for {} access", access_type))
        }
    }
}

// Data structures
#[derive(serde::Serialize)]
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

#[derive(serde::Serialize)]
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

#[derive(serde::Serialize)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

// Sync operations
fn read_file_sync_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => return,
    };
    let path = if args.length() > 1 && !args.get(1).is_null_or_undefined() {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
            scope.throw_exception(error.into());
            return;
        }
    };
    
    let path_buf = match resolve_path(state, &token, path.clone(), "read") {
        Ok(p) => p,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to resolve path for read operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)).unwrap();
            scope.throw_exception(error.into());
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
            rv.set(uint8_array.into());
        }
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to read file '{}': {}", path_buf.display(), e)).unwrap();
            scope.throw_exception(error.into());
        }
    }
}

fn write_file_sync_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut _rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => return,
    };
    
    let contents = if let Ok(uint8_array) = v8::Local::<v8::Uint8Array>::try_from(args.get(1)) {
        let mut data = vec![0u8; uint8_array.byte_length()];
        if uint8_array.copy_contents(&mut data) != data.len() {
            let error = v8::String::new(scope, "Failed to copy Uint8Array contents").unwrap();
            scope.throw_exception(error.into());
            return;
        }
        data
    } else {
        let error = v8::String::new(scope, "contents must be Uint8Array").unwrap();
        scope.throw_exception(error.into());
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
            let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
            scope.throw_exception(error.into());
            return;
        }
    };
    
    let path_buf = match resolve_path(state, &token, path.clone(), "write") {
        Ok(p) => p,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to resolve path for write operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)).unwrap();
            scope.throw_exception(error.into());
            return;
        }
    };
    
    if let Err(e) = std::fs::write(&path_buf, contents) {
        let error = v8::String::new(scope, &format!("Failed to write file '{}': {}", path_buf.display(), e)).unwrap();
        scope.throw_exception(error.into());
    }
}

macro_rules! simple_file_op {
    ($name:ident, $op:expr, $access:literal, $op_name:literal) => {
        fn $name(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut _rv: v8::ReturnValue) {
            let token = match get_string_arg(scope, &args, 0, "token") {
                Ok(t) => t,
                Err(_) => return,
            };
            let path = if args.length() > 1 && !args.get(1).is_null_or_undefined() {
                Some(args.get(1).to_rust_string_lossy(scope))
            } else {
                None
            };
            
            let state = match get_state(scope) {
                Ok(s) => s,
                Err(e) => {
                    let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
                    scope.throw_exception(error.into());
                    return;
                }
            };
            
            let path_buf = match resolve_path(state, &token, path.clone(), $access) {
                Ok(p) => p,
                Err(e) => {
                    let error = v8::String::new(scope, &format!("Failed to resolve path for {} operation with token '{}'{}: {}", 
                        $op_name,
                        token, 
                        path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                        e)).unwrap();
                    scope.throw_exception(error.into());
                    return;
                }
            };
            
            if let Err(e) = $op(&path_buf) {
                let error = v8::String::new(scope, &format!("Failed to {} '{}': {}", $op_name, path_buf.display(), e)).unwrap();
                scope.throw_exception(error.into());
            }
        }
    };
}

simple_file_op!(remove_file_sync_op, std::fs::remove_file, "write", "remove file");
simple_file_op!(mkdirp_sync_op, std::fs::create_dir_all, "write", "create directory");
simple_file_op!(rmdir_sync_op, std::fs::remove_dir, "write", "remove directory");

fn stat_file_sync_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => return,
    };
    let path = if args.length() > 1 && !args.get(1).is_null_or_undefined() {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
            scope.throw_exception(error.into());
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
                    rv.set(parsed);
                }
                Err(e) => {
                    let error = v8::String::new(scope, &format!("Failed to get file metadata for '{}': {}", path_buf.display(), e)).unwrap();
                    scope.throw_exception(error.into());
                }
            }
        }
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to resolve path for stat operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)).unwrap();
            scope.throw_exception(error.into());
        }
    }
}

fn list_dir_sync_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => return,
    };
    let path = match get_string_arg(scope, &args, 1, "path") {
        Ok(p) => p,
        Err(_) => return,
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
            scope.throw_exception(error.into());
            return;
        }
    };
    
    let path_buf = match resolve_path(state, &token, Some(path.clone()), "read") {
        Ok(p) => p,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to resolve path for list directory operation with token '{}' and path '{}': {}", 
                token, path, e)).unwrap();
            scope.throw_exception(error.into());
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
                                let error = v8::String::new(scope, &format!("Failed to get metadata for directory entry in '{}': {}", path_buf.display(), e)).unwrap();
                                scope.throw_exception(error.into());
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let error = v8::String::new(scope, &format!("Failed to read directory entry in '{}': {}", path_buf.display(), e)).unwrap();
                        scope.throw_exception(error.into());
                        return;
                    }
                }
            }
            let result_json = serde_json::to_string(&result).unwrap();
            let json_value = v8::String::new(scope, &result_json).unwrap();
            let parsed = v8::json::parse(scope, json_value).unwrap();
            rv.set(parsed);
        }
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to list directory '{}': {}", path_buf.display(), e)).unwrap();
            scope.throw_exception(error.into());
        }
    }
}

fn exec_file_sync_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => return,
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
        let error = v8::String::new(scope, "args must be an array").unwrap();
        scope.throw_exception(error.into());
        return;
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
            scope.throw_exception(error.into());
            return;
        }
    };
    
    let path_buf = match resolve_path(state, &token, path.clone(), "exec") {
        Ok(p) => p,
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to resolve path for exec operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)).unwrap();
            scope.throw_exception(error.into());
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
            rv.set(parsed);
        }
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to execute command '{}': {}", path_buf.display(), e)).unwrap();
            scope.throw_exception(error.into());
        }
    }
}

// Async operations (promise-returning wrappers around sync operations)
fn read_file_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
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
            let error = v8::String::new(scope, &format!("Failed to resolve path for read operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)).unwrap();
            scope.throw_exception(error.into());
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

fn write_file_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
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

async_simple_file_op!(remove_file_op, std::fs::remove_file, "write", "remove file");
async_simple_file_op!(mkdirp_op, std::fs::create_dir_all, "write", "create directory");
async_simple_file_op!(rmdir_op, std::fs::remove_dir, "write", "remove directory");

fn stat_file_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
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
                    rv.set(create_rejected_promise(scope, &format!("Failed to get file metadata for '{}': {}", path_buf.display(), e)));
                }
            }
        }
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to resolve path for stat operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)));
        }
    }
}

fn list_dir_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
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
            let error = v8::String::new(scope, &format!("Failed to resolve path for list directory operation with token '{}' and path '{}': {}", 
                token, path, e)).unwrap();
            scope.throw_exception(error.into());
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
                                rv.set(create_rejected_promise(scope, &format!("Failed to get metadata for directory entry in '{}': {}", path_buf.display(), e)));
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        rv.set(create_rejected_promise(scope, &format!("Failed to read directory entry in '{}': {}", path_buf.display(), e)));
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
            rv.set(create_rejected_promise(scope, &format!("Failed to list directory '{}': {}", path_buf.display(), e)));
        }
    }
}

fn exec_file_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
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
        rv.set(create_rejected_promise(scope, "args must be an array"));
        return;
    };
    
    let state = match get_state(scope) {
        Ok(s) => s,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to get state: {}", e)));
            return;
        }
    };

    let path_buf = match resolve_path(state, &token, path.clone(), "exec") {
        Ok(p) => p,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to resolve path for exec operation with token '{}'{}: {}", 
                token, 
                path.map(|p| format!(" and path '{}'", p)).unwrap_or_default(),
                e)));
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
            rv.set(create_rejected_promise(scope, &format!("Failed to execute command: {}", e)));
        }
    }
}

fn cwd_op(scope: &mut v8::HandleScope, _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    match std::env::current_dir() {
        Ok(path) => {
            let cwd_string = v8::String::new(scope, &path.to_string_lossy()).unwrap();
            rv.set(cwd_string.into());
        }
        Err(e) => {
            let error = v8::String::new(scope, &format!("Failed to get current working directory: {}", e)).unwrap();
            scope.throw_exception(error.into());
        }
    }
}
