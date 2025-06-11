use crate::run::stack_trace;

pub fn get_exception_message_with_stack(scope: &mut v8::HandleScope, exception: v8::Local<v8::Value>) -> String {
    // Try to get the message property if this is an Error object
    let message = if let Ok(exception_obj) = v8::Local::<v8::Object>::try_from(exception) {
        let message_key = v8::String::new(scope, "message").unwrap();
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
        let stack_key = v8::String::new(scope, "stack").unwrap();
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
        // Fallback: try to get current stack trace and map it
        if let Some(stack_trace) = v8::StackTrace::current_stack_trace(scope, 10) {
            let mut trace_lines = vec![format!("Error: {}", message)];
            let formatted_trace = stack_trace::format_v8_stack_trace_with_source_maps(scope, stack_trace, 0);
            if !formatted_trace.is_empty() {
                trace_lines.push(formatted_trace);
            }
            trace_lines.join("\n")
        } else {
            format!("Error: {}", message)
        }
    };
    
    mapped_stack
} 