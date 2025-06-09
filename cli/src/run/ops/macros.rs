use v8;
use crate::{AnyError, MycoState};
use anyhow::anyhow;

// Helper functions
pub fn get_state<'a>(scope: &'a mut v8::HandleScope) -> Result<&'a mut MycoState, AnyError> {
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if state_ptr.is_null() {
        return Err(anyhow!("Failed to get isolate state"));
    }
    Ok(unsafe { &mut *state_ptr })
}

pub fn get_string_arg(scope: &mut v8::HandleScope, args: &v8::FunctionCallbackArguments, index: i32, name: &str) -> Result<String, ()> {
    if args.length() <= index {
        let error = v8::String::new(scope, &format!("Missing required argument: {}", name)).unwrap();
        scope.throw_exception(error.into());
        return Err(());
    }
    let arg = args.get(index);
    if !arg.is_string() {
        let error = v8::String::new(scope, &format!("{} must be a string", name)).unwrap();
        scope.throw_exception(error.into());
        return Err(());
    }
    Ok(arg.to_rust_string_lossy(scope))
}

// Helper function to create a resolved promise
pub fn create_resolved_promise<'a>(scope: &'a mut v8::HandleScope, value: v8::Local<'a, v8::Value>) -> v8::Local<'a, v8::Value> {
    let promise_resolver = v8::PromiseResolver::new(scope).unwrap();
    let promise = promise_resolver.get_promise(scope);
    promise_resolver.resolve(scope, value);
    promise.into()
}

pub fn create_resolved_promise_void<'a>(scope: &'a mut v8::HandleScope) -> v8::Local<'a, v8::Value> {
    let promise_resolver = v8::PromiseResolver::new(scope).unwrap();
    let promise = promise_resolver.get_promise(scope);
    let undefined_value = v8::undefined(scope).into();
    promise_resolver.resolve(scope, undefined_value);
    promise.into()
}

pub fn create_rejected_promise<'a>(scope: &'a mut v8::HandleScope, error_msg: &str) -> v8::Local<'a, v8::Value> {
    let promise_resolver = v8::PromiseResolver::new(scope).unwrap();
    let promise = promise_resolver.get_promise(scope);
    let error = v8::String::new(scope, error_msg).unwrap();
    promise_resolver.reject(scope, error.into());
    promise.into()
}

#[macro_export]
macro_rules! request_op {
    ($name:ident, $capability:ident) => {
        fn $name(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
            let url = match $crate::run::ops::macros::get_string_arg(scope, &args, 0, "url") {
                Ok(u) => u,
                Err(_) => return,
            };
            
            match $crate::run::ops::macros::get_state(scope) {
                Ok(state) => {
                    let token = state.capabilities.register(Capability::$capability(url));
                    let token_string = v8::String::new(scope, &token).unwrap();
                    rv.set(token_string.into());
                }
                Err(e) => {
                    let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
                    scope.throw_exception(error.into());
                }
            }
        }
    };
}

#[macro_export]
macro_rules! request_file_op {
    ($name:ident, $capability:ident) => {
        fn $name(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
            let path = match $crate::run::ops::macros::get_string_arg(scope, &args, 0, "path") {
                Ok(p) => p,
                Err(_) => return,
            };
            
            match $crate::run::ops::macros::get_state(scope) {
                Ok(state) => {
                    let token = state.capabilities.register(Capability::$capability(path));
                    let token_string = v8::String::new(scope, &token).unwrap();
                    rv.set(token_string.into());
                }
                Err(e) => {
                    let error = v8::String::new(scope, &format!("Failed to get state: {}", e)).unwrap();
                    scope.throw_exception(error.into());
                }
            }
        }
    };
} 