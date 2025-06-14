use v8;
use crate::run::stack_trace::capture_call_site_stack;
use crate::errors::MycoError;
use crate::register_op;

pub fn register_console_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), MycoError> {
    register_op!(scope, myco_ops, "print_sync", print_op);
    register_op!(scope, myco_ops, "eprint_sync", eprint_op);
    register_op!(scope, myco_ops, "trace_sync", trace_op);

    Ok(())
}

fn print_op(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let message_arg = args.get(0);
    
    let message = if message_arg.is_string() {
        message_arg.to_rust_string_lossy(scope)
    } else {
        format_value(scope, message_arg)
    };

    print!("{}", message);
}

fn eprint_op(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let message_arg = args.get(0);
    
    let message = if message_arg.is_string() {
        message_arg.to_rust_string_lossy(scope)
    } else {
        format_value(scope, message_arg)
    };

    eprint!("{}", message);
}

fn trace_op(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    // Get stack trace
    let result = capture_call_site_stack(scope, 1);
    rv.set(v8::String::new(scope, &result).unwrap().into());
}

fn format_value(scope: &mut v8::HandleScope, arg: v8::Local<'_, v8::Value>) -> String {
    if arg.is_string() {
        arg.to_rust_string_lossy(scope)
    } else if arg.is_number() {
        arg.number_value(scope).unwrap_or(0.0).to_string()
    } else if arg.is_boolean() {
        arg.boolean_value(scope).to_string()
    } else if arg.is_null() {
        "null".to_string()
    } else if arg.is_undefined() {
        "undefined".to_string()
    } else {
        // Try to get a better representation for objects
        try_object_representation(scope, arg)
    }
}

fn try_object_representation(scope: &mut v8::HandleScope, arg: v8::Local<'_, v8::Value>) -> String {
    // First check if it's an array and use JSON serialization for arrays
    if let Ok(object) = v8::Local::<v8::Object>::try_from(arg) {
        // Check if it's an array using Array.isArray equivalent
        if object.is_array() {
            match v8::json::stringify(scope, arg) {
                Some(json_string) => return json_string.to_rust_string_lossy(scope),
                None => {} // Fall through to other methods
            }
        }
        
        // For non-arrays, try to call toString() method if it exists and is callable
        let to_string_key = v8::String::new(scope, "toString").unwrap();
        
        if let Some(to_string_value) = object.get(scope, to_string_key.into()) {
            if let Ok(to_string_fn) = v8::Local::<v8::Function>::try_from(to_string_value) {
                // Call toString() method
                if let Some(result) = to_string_fn.call(scope, object.into(), &[]) {
                    if result.is_string() {
                        let string_result = result.to_rust_string_lossy(scope);
                        // Avoid infinite recursion by checking if it's not just "[object Object]"
                        if string_result != "[object Object]" {
                            return string_result;
                        }
                    }
                }
            }
        }
    }
    
    // If toString didn't work, try JSON serialization
    match v8::json::stringify(scope, arg) {
        Some(json_string) => json_string.to_rust_string_lossy(scope),
        None => {
            // Fall back to default representation
            "[object Object]".to_string()
        }
    }
} 