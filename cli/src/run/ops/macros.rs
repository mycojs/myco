use crate::errors::MycoError;
use crate::run::state::MycoState;
use v8;

// Helper functions
pub fn get_state<'a>(scope: &'a mut v8::HandleScope) -> Result<&'a mut MycoState, MycoError> {
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if state_ptr.is_null() {
        return Err(MycoError::Internal {
            message: "Failed to get isolate state".to_string(),
        });
    }
    Ok(unsafe { &mut *state_ptr })
}

pub fn sync_op<T, R, F>(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
    f: F,
) where
    T: for<'de> serde::Deserialize<'de>,
    R: serde::Serialize,
    F: FnOnce(&mut v8::HandleScope, T) -> Result<R, MycoError>,
{
    let arg = get_arg::<T>(scope, args);
    match arg {
        Ok(value) => match f(scope, value) {
            Ok(input) => {
                let result = serde_v8::to_v8(scope, input).unwrap();
                rv.set(result);
            }
            Err(e) => {
                let js_error = create_js_error(scope, &format!("{}", e));
                scope.throw_exception(js_error);
            }
        },
        Err(e) => {
            let js_error = create_js_error(scope, &format!("{}", e));
            scope.throw_exception(js_error);
        }
    }
}

pub fn get_arg<T: for<'de> serde::Deserialize<'de>>(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
) -> Result<T, MycoError> {
    let arg = args.get(0);
    match serde_v8::from_v8::<T>(scope, arg) {
        Ok(result) => Ok(result),
        Err(e) => {
            let error = create_js_error(scope, &format!("Failed to deserialize arg: {}", e));
            scope.throw_exception(error);
            Err(MycoError::Internal {
                message: format!("Failed to deserialize arg: {}", e),
            })
        }
    }
}

pub fn get_string_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
    name: &str,
) -> Result<String, ()> {
    if args.length() <= index {
        let error = create_js_error(scope, &format!("Missing required argument: {}", name));
        scope.throw_exception(error);
        return Err(());
    }
    let arg = args.get(index);
    if !arg.is_string() {
        let error = create_js_error(scope, &format!("{} must be a string", name));
        scope.throw_exception(error);
        return Err(());
    }
    Ok(arg.to_rust_string_lossy(scope))
}

// Helper function to create a proper JavaScript Error object with stack trace
pub fn create_js_error<'a>(
    scope: &mut v8::HandleScope<'a>,
    message: &str,
) -> v8::Local<'a, v8::Value> {
    let message_str = v8::String::new(scope, message).unwrap();
    let error_key = v8::String::new(scope, "Error").unwrap();
    let error_constructor = scope
        .get_current_context()
        .global(scope)
        .get(scope, error_key.into())
        .unwrap();

    if let Ok(error_constructor) = v8::Local::<v8::Function>::try_from(error_constructor) {
        let args = [message_str.into()];
        if let Some(error_obj) = error_constructor.new_instance(scope, &args) {
            return error_obj.into();
        }
    }

    // Fallback to plain string if Error constructor fails
    message_str.into()
}

// Helper function to throw a proper JavaScript Error
pub fn throw_js_error(scope: &mut v8::HandleScope, message: &str) {
    let error = create_js_error(scope, message);
    scope.throw_exception(error);
}

pub fn create_resolved_promise_void<'a>(
    scope: &'a mut v8::HandleScope,
) -> v8::Local<'a, v8::Value> {
    let promise_resolver = v8::PromiseResolver::new(scope).unwrap();
    let promise = promise_resolver.get_promise(scope);
    let undefined_value = v8::undefined(scope).into();
    promise_resolver.resolve(scope, undefined_value);
    promise.into()
}

pub fn create_rejected_promise<'a>(
    scope: &'a mut v8::HandleScope,
    error_msg: &str,
) -> v8::Local<'a, v8::Value> {
    let promise_resolver = v8::PromiseResolver::new(scope).unwrap();
    let promise = promise_resolver.get_promise(scope);
    let error = create_js_error(scope, error_msg);
    promise_resolver.reject(scope, error);
    promise.into()
}

pub fn async_op<Prep, Input, PrepFn, DispatchFn, Fut>(
    scope: &mut v8::HandleScope,
    mut rv: v8::ReturnValue,
    args: &v8::FunctionCallbackArguments,
    prep_fn: PrepFn,
    dispatch_fn: DispatchFn,
) where
    PrepFn: FnOnce(&mut v8::HandleScope, Input) -> Result<Prep, MycoError>,
    DispatchFn: FnOnce(Prep) -> Fut + Send + 'static,
    Input: for<'de> serde::Deserialize<'de>,
    Fut: std::future::Future<Output = crate::run::state::OpResult> + Send + 'static,
    Prep: Send + 'static,
{
    let arg = match get_arg::<Input>(scope, args) {
        Ok(value) => value,
        Err(e) => {
            rv.set(create_rejected_promise(
                scope,
                &format!("Failed to deserialize arguments: {}", e),
            ));
            return;
        }
    };

    let prep = match prep_fn(scope, arg) {
        Ok(prep) => prep,
        Err(e) => {
            rv.set(create_rejected_promise(scope, &format!("{}", e)));
            return;
        }
    };

    // Get state
    let state_ptr = scope.get_data(0) as *mut crate::run::state::MycoState;
    if state_ptr.is_null() {
        rv.set(create_rejected_promise(
            scope,
            "Failed to get isolate state",
        ));
        return;
    }
    let state = unsafe { &mut *state_ptr };

    // Create promise resolver
    match v8::PromiseResolver::new(scope) {
        Some(resolver) => {
            let promise = resolver.get_promise(scope);

            // Register pending operation
            let op_id = state.get_next_op_id();
            let resolver_global = v8::Global::new(scope, resolver);
            state.register_pending_op(op_id, resolver_global);

            // Get handles for async task
            let runtime_handle = state.runtime_handle.clone();
            let op_sender = state.op_sender.clone();

            // Spawn the task
            runtime_handle.spawn(async move {
                let result = dispatch_fn(prep).await;
                let _ = op_sender.send(result.to_final_op_result(op_id));
            });

            rv.set(promise.into());
        }
        None => {
            rv.set(create_rejected_promise(
                scope,
                "Failed to create promise resolver",
            ));
        }
    }
}

#[macro_export]
macro_rules! request_op {
    ($name:ident, $capability:ident) => {
        fn $name(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) {
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
                    $crate::run::ops::macros::throw_js_error(
                        scope,
                        &format!("Failed to get state: {}", e),
                    );
                }
            }
        }
    };
}

#[macro_export]
macro_rules! register_sync_op {
    ($scope:ident, $myco_ops:ident, $name:literal, $fn:ident) => {
        let func = v8::Function::new($scope, $fn).unwrap();
        let key = v8::String::new($scope, $name).unwrap();
        let sync_key = v8::String::new($scope, "sync").unwrap();
        let sync_obj = $myco_ops
            .get($scope, sync_key.into())
            .unwrap()
            .to_object($scope)
            .unwrap();
        sync_obj.set($scope, key.into(), func.into());
    };
}

#[macro_export]
macro_rules! register_async_op {
    ($scope:ident, $myco_ops:ident, $name:literal, $fn:ident) => {
        let func = v8::Function::new($scope, $fn).unwrap();
        let key = v8::String::new($scope, $name).unwrap();
        let async_key = v8::String::new($scope, "async").unwrap();
        let async_obj = $myco_ops
            .get($scope, async_key.into())
            .unwrap()
            .to_object($scope)
            .unwrap();
        async_obj.set($scope, key.into(), func.into());
    };
}
