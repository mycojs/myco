use crate::run::stack_trace;
use log::{debug, trace, warn};

pub fn get_exception_message_with_stack(
    scope: &mut v8::HandleScope,
    exception: v8::Local<v8::Value>,
) -> String {
    debug!("Processing JavaScript exception for error reporting");

    // Try to get the message property if this is an Error object
    let message = if let Ok(exception_obj) = v8::Local::<v8::Object>::try_from(exception) {
        trace!("Exception is an object, extracting message property");
        let message_key = match v8::String::new(scope, "message") {
            Some(key) => key,
            None => {
                warn!("Failed to create V8 string for 'message' key");
                return "Failed to create V8 string for 'message' key".to_string();
            }
        };
        if let Some(message_val) = exception_obj.get(scope, message_key.into()) {
            if message_val.is_string() {
                let msg = message_val.to_rust_string_lossy(scope);
                trace!("Extracted error message: {}", msg);
                msg
            } else {
                trace!("Message property is not a string, using exception string representation");
                exception.to_rust_string_lossy(scope)
            }
        } else {
            trace!("No message property found, using exception string representation");
            exception.to_rust_string_lossy(scope)
        }
    } else {
        trace!("Exception is not an object, using string representation");
        exception.to_rust_string_lossy(scope)
    };

    // Try to get the stack property if this is an Error object
    trace!("Extracting stack trace from exception");
    let stack = if let Ok(exception_obj) = v8::Local::<v8::Object>::try_from(exception) {
        let stack_key = match v8::String::new(scope, "stack") {
            Some(key) => key,
            None => {
                warn!("Failed to create V8 string for 'stack' key");
                return format!("{} (failed to create V8 string for 'stack' key)", message);
            }
        };
        if let Some(stack_val) = exception_obj.get(scope, stack_key.into()) {
            if stack_val.is_string() {
                let stack_str = stack_val.to_rust_string_lossy(scope);
                trace!(
                    "Found stack trace property ({} characters)",
                    stack_str.len()
                );
                Some(stack_str)
            } else {
                trace!("Stack property exists but is not a string");
                None
            }
        } else {
            trace!("No stack property found on exception object");
            None
        }
    } else {
        trace!("Exception is not an object, cannot extract stack property");
        None
    };

    // Apply source map transformations to the stack trace
    debug!("Formatting stack trace with source map transformations");
    if let Some(stack_trace) = stack {
        debug!("Using exception stack trace");
        stack_trace::format_stack_trace_with_source_maps(scope, &stack_trace)
    } else {
        debug!("No stack trace available, capturing current call site");
        stack_trace::capture_call_site_stack(scope, 0)
    }
}
