use crate::run::stack_trace;

pub fn get_exception_message_with_stack(
    scope: &mut v8::HandleScope,
    exception: v8::Local<v8::Value>,
) -> String {
    // Try to get the message property if this is an Error object
    let message = if let Ok(exception_obj) = v8::Local::<v8::Object>::try_from(exception) {
        let message_key = match v8::String::new(scope, "message") {
            Some(key) => key,
            None => return "Failed to create V8 string for 'message' key".to_string(),
        };
        if let Some(message_val) = exception_obj.get(scope, message_key.into()) {
            if message_val.is_string() {
                message_val.to_rust_string_lossy(scope)
            } else {
                exception.to_rust_string_lossy(scope)
            }
        } else {
            exception.to_rust_string_lossy(scope)
        }
    } else {
        exception.to_rust_string_lossy(scope)
    };

    // Try to get the stack property if this is an Error object
    let stack = if let Ok(exception_obj) = v8::Local::<v8::Object>::try_from(exception) {
        let stack_key = match v8::String::new(scope, "stack") {
            Some(key) => key,
            None => return format!("{} (failed to create V8 string for 'stack' key)", message),
        };
        if let Some(stack_val) = exception_obj.get(scope, stack_key.into()) {
            if stack_val.is_string() {
                Some(stack_val.to_rust_string_lossy(scope))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Apply source map transformations to the stack trace
    let mapped_stack = if let Some(stack_trace) = stack {
        stack_trace::format_stack_trace_with_source_maps(scope, &stack_trace)
    } else {
        stack_trace::capture_call_site_stack(scope, 0)
    };

    mapped_stack
}
