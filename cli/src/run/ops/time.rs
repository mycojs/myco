use v8;
use std::time::{Duration, Instant};
use crate::run::state::{MycoState, Timer};

pub fn register_time_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), anyhow::Error> {
    // Register the set_timeout op
    let set_timeout_fn = v8::Function::new(scope, set_timeout_op).unwrap();
    let set_timeout_key = v8::String::new(scope, "set_timeout").unwrap();
    myco_ops.set(scope, set_timeout_key.into(), set_timeout_fn.into());
    
    // Register the clear_timeout op
    let clear_timeout_fn = v8::Function::new(scope, clear_timeout_op).unwrap();
    let clear_timeout_key = v8::String::new(scope, "clear_timeout").unwrap();
    myco_ops.set(scope, clear_timeout_key.into(), clear_timeout_fn.into());
    
    Ok(())
}

fn set_timeout_op(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        let error = v8::String::new(scope, "set_timeout requires 1 argument: delay").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let delay_arg = args.get(0);
    
    // Validate delay is a number
    if !delay_arg.is_number() {
        let error = v8::String::new(scope, "set_timeout: delay must be a number").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    // Get the delay in milliseconds
    let delay_ms = delay_arg.number_value(scope).unwrap_or(0.0).max(0.0) as u64;
    let delay = Duration::from_millis(delay_ms);
    let execute_at = Instant::now() + delay;
    
    // Get the state from the isolate
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if state_ptr.is_null() {
        let error = v8::String::new(scope, "set_timeout: failed to get isolate state").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let state = unsafe { &mut *state_ptr };
    
    // Create a timer that will call a global timer completion function
    let timer_id = state.next_timer_id;
    state.next_timer_id += 1;
    
    // Create a callback that calls the global timer completion handler
    let callback_code = format!("(function() {{ globalThis.__mycoTimerComplete({}) }})", timer_id);
    let callback_source = v8::String::new(scope, &callback_code).unwrap();
    let callback_script = v8::Script::compile(scope, callback_source, None);
    
    if let Some(script) = callback_script {
        if let Some(callback_fn_val) = script.run(scope) {
            if let Ok(callback_fn) = v8::Local::<v8::Function>::try_from(callback_fn_val) {
                let global_callback = v8::Global::new(scope, callback_fn);
                
                let timer = Timer::new(timer_id, global_callback, execute_at);
                state.timers.push(timer);
                
                // Return the timer ID
                let timer_id_value = v8::Number::new(scope, timer_id as f64);
                rv.set(timer_id_value.into());
                return;
            }
        }
    }
    
    let error = v8::String::new(scope, "set_timeout: failed to create timer callback").unwrap();
    scope.throw_exception(error.into());
}

fn clear_timeout_op(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        let error = v8::String::new(scope, "clear_timeout requires 1 argument: timer_id").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let timer_id_arg = args.get(0);
    
    // Validate timer_id is a number
    if !timer_id_arg.is_number() {
        let error = v8::String::new(scope, "clear_timeout: timer_id must be a number").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let timer_id = timer_id_arg.number_value(scope).unwrap_or(0.0) as u32;
    
    // Get the state from the isolate
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if state_ptr.is_null() {
        let error = v8::String::new(scope, "clear_timeout: failed to get isolate state").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let state = unsafe { &mut *state_ptr };
    
    // Remove the timer with the matching ID
    state.timers.retain(|timer| timer.id != timer_id);
} 