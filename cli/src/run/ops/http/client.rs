use v8;

use crate::{Capability, request_op, register_op};
use crate::run::ops::macros::{get_state, get_string_arg, create_resolved_promise, create_rejected_promise};
use crate::errors::MycoError;

pub fn register_http_client_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), MycoError> {
    register_op!(scope, myco_ops, "request_fetch_url", request_fetch_url_op);
    register_op!(scope, myco_ops, "request_fetch_prefix", request_fetch_prefix_op);
    register_op!(scope, myco_ops, "fetch_url", fetch_url_op);
    
    Ok(())
}

// Token request operations
request_op!(request_fetch_url_op, FetchUrl);
request_op!(request_fetch_prefix_op, FetchPrefix);

// Fetch operation
fn fetch_url_op(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let token = match get_string_arg(scope, &args, 0, "token") {
        Ok(t) => t,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "Missing or invalid token"));
            return;
        }
    };
    
    // Optional path parameter for prefix tokens
    let optional_path = if args.length() > 1 && !args.get(1).is_null_or_undefined() {
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
    
    // Determine the final URL to fetch
    let url = match state.capabilities.get(&token) {
        Some(Capability::FetchUrl(allowed_url)) => {
            if optional_path.is_some() {
                rv.set(create_rejected_promise(scope, "Path parameter not allowed for specific URL tokens"));
                return;
            }
            allowed_url.clone()
        }
        Some(Capability::FetchPrefix(base_url)) => {
            match optional_path {
                Some(path) => {
                    // Security checks to prevent path traversal
                    if path.contains("..") {
                        rv.set(create_rejected_promise(scope, "Path traversal not allowed (contains '..')"));
                        return;
                    }
                    
                    if path.contains("://") {
                        rv.set(create_rejected_promise(scope, "Full URLs not allowed in path parameter"));
                        return;
                    }
                    
                    // Combine base URL with path (no automatic slash addition)
                    let full_url = format!("{}{}", base_url, path);
                    
                    full_url
                }
                None => {
                    rv.set(create_rejected_promise(scope, "Path parameter required for prefix tokens"));
                    return;
                }
            }
        }
        _ => {
            rv.set(create_rejected_promise(scope, "Invalid token for URL access"));
            return;
        }
    };
    
    // Perform the HTTP request synchronously using blocking reqwest
    let result = std::thread::spawn(move || {
        let client = reqwest::blocking::Client::new();
        match client.get(&url).send() {
            Ok(response) => {
                match response.bytes() {
                    Ok(bytes) => Ok(bytes.to_vec()),
                    Err(e) => Err(format!("Failed to read response body: {}", e)),
                }
            }
            Err(e) => Err(format!("HTTP request failed: {}", e)),
        }
    }).join();
    
    let result = match result {
        Ok(inner_result) => inner_result,
        Err(_) => {
            rv.set(create_rejected_promise(scope, "HTTP request thread panicked"));
            return;
        }
    };
    
    match result {
        Ok(bytes) => {
            let array_buffer = v8::ArrayBuffer::new(scope, bytes.len());
            let backing_store = array_buffer.get_backing_store();
            unsafe {
                let data = backing_store.data().unwrap().as_ptr() as *mut u8;
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            }
            let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, bytes.len()).unwrap();
            rv.set(create_resolved_promise(scope, uint8_array.into()));
        }
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("Failed to fetch URL: {}", e)));
        }
    }
} 