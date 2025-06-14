use v8;
use std::time::{Duration, Instant};
use serde::Deserialize;
use crate::run::state::Timer;
use crate::errors::MycoError;
use crate::run::ops::macros::{sync_op, get_state};
use crate::register_sync_op;

#[derive(Deserialize)]
struct DelayArg {
    delay: f64,
}

#[derive(Deserialize)]
struct TimerIdArg {
    timer_id: f64,
}

pub fn register_time_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "set_timeout", sync_op_set_timeout);
    register_sync_op!(scope, myco_ops, "clear_timeout", sync_op_clear_timeout);
    
    Ok(())
}

fn sync_op_set_timeout(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: DelayArg| -> Result<u32, MycoError> {
        let delay_ms = input.delay.max(0.0) as u64;
        let delay = Duration::from_millis(delay_ms);
        let execute_at = Instant::now() + delay;
        
        // Get timer_id first while we can borrow state mutably
        let timer_id = {
            let state = get_state(scope)?;
            let timer_id = state.next_timer_id;
            state.next_timer_id += 1;
            timer_id
        };
        
        let callback_code = format!("(function() {{ globalThis.__mycoTimerComplete({}) }})", timer_id);
        let callback_source = v8::String::new(scope, &callback_code).unwrap();
        let callback_script = v8::Script::compile(scope, callback_source, None);
        
        if let Some(script) = callback_script {
            if let Some(callback_fn_val) = script.run(scope) {
                if let Ok(callback_fn) = v8::Local::<v8::Function>::try_from(callback_fn_val) {
                    let global_callback = v8::Global::new(scope, callback_fn);
                    
                    let timer = Timer::new(timer_id, global_callback, execute_at);
                    let state = get_state(scope)?;
                    state.timers.push(timer);
                    
                    return Ok(timer_id);
                }
            }
        }
        
        Err(MycoError::Internal {
            message: "Failed to create timer callback".to_string()
        })
    });
}

fn sync_op_clear_timeout(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |scope, input: TimerIdArg| -> Result<(), MycoError> {
        let timer_id = input.timer_id as u32;
        let state = get_state(scope)?;
        state.timers.retain(|timer| timer.id != timer_id);
        Ok(())
    });
} 