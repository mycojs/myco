use std::time::{Duration, Instant};

use crate::run::state::MycoState;
use crate::run::errors::get_exception_message_with_stack;
use crate::AnyError;

// Macro for inspector debug logging
#[cfg(feature = "inspector-debug")]
macro_rules! inspector_debug {
    ($($arg:tt)*) => {
        println!($($arg)*)
    };
}

#[cfg(not(feature = "inspector-debug"))]
macro_rules! inspector_debug {
    ($($arg:tt)*) => {
        ()
    };
}

pub async fn run_event_loop(scope: &mut v8::ContextScope<'_, v8::HandleScope<'_>>) -> Result<(), AnyError> {
    let mut consecutive_empty_rounds = 0;
    let max_empty_rounds = 10;
    let max_total_rounds = 1000;
    let mut total_rounds = 0;
    
    loop {
        total_rounds += 1;
        
        if total_rounds > max_total_rounds {
            eprintln!("Warning: Event loop hit maximum iteration limit");
            break;
        }
        
        // Check for unhandled errors that were caught by promise rejection handlers
        let global = scope.get_current_context().global(scope);
        let error_key = v8::String::new(scope, "__MYCO_UNHANDLED_ERROR__").unwrap();
        if let Some(error_value) = global.get(scope, error_key.into()) {
            if !error_value.is_undefined() && !error_value.is_null() {
                let error_message = get_exception_message_with_stack(scope, error_value);
                return Err(anyhow::anyhow!("{}", error_message));
            }
        }
        
        // Check for and execute ready timers
        let now = Instant::now();
        let mut executed_any_timer = false;
        
        // Get the state from the isolate
        let state_ptr = scope.get_data(0) as *mut MycoState;
        if !state_ptr.is_null() {
            let state = unsafe { &mut *state_ptr };

            // Poll inspector sessions if we have one
            if let Some(inspector_rc) = &state.inspector {
                let mut inspector = inspector_rc.borrow_mut();
                match inspector.poll_sessions() {
                    Ok(()) => {
                        // Inspector processing completed normally
                    }
                    Err(_e) => {
                        inspector_debug!("Inspector error: {:?}", _e);
                    }
                }
            }
            
            // Find ready timers (execute_at <= now)
            let mut ready_timers = Vec::new();
            let mut remaining_timers = Vec::new();
            
            for timer in state.timers.drain(..) {
                if timer.execute_at <= now {
                    ready_timers.push(timer);
                } else {
                    remaining_timers.push(timer);
                }
            }
            
            // Put back the remaining timers
            state.timers = remaining_timers;
            
            // Execute ready timers
            for timer in ready_timers {
                executed_any_timer = true;
                
                let callback_local = v8::Local::new(scope, &timer.callback);
                let global = scope.get_current_context().global(scope);
                
                if callback_local.call(scope, global.into(), &[]).is_none() {
                    eprintln!("Timer {} callback execution failed", timer.id);
                }
            }
        }
        
        // Process microtasks
        scope.perform_microtask_checkpoint();
        
        // If we executed timers or processed microtasks, reset the empty counter
        if executed_any_timer {
            consecutive_empty_rounds = 0;
        } else {
            consecutive_empty_rounds += 1;
        }
        
        // If we're in early rounds, assume we're still processing
        if total_rounds < 50 {
            consecutive_empty_rounds = 0;
        }
        
        // Check if we should continue
        let has_pending_timers = unsafe {
            let state_ptr = scope.get_data(0) as *mut MycoState;
            if !state_ptr.is_null() {
                let state = &*state_ptr;
                !state.timers.is_empty()
            } else {
                false
            }
        };
        
        if consecutive_empty_rounds >= max_empty_rounds && !has_pending_timers {
            break;
        }
        
        // Small yield to allow other tasks to run
        tokio::task::yield_now().await;
        
        // If we have pending timers, sleep until the next one is ready
        if has_pending_timers {
            let next_timer_delay = unsafe {
                let state_ptr = scope.get_data(0) as *mut MycoState;
                if !state_ptr.is_null() {
                    let state = &*state_ptr;
                    state.timers.iter()
                        .map(|t| t.execute_at.saturating_duration_since(now))
                        .min()
                        .unwrap_or(Duration::from_millis(1))
                } else {
                    Duration::from_millis(1)
                }
            };
            
            // Limit sleep time to avoid hanging
            let sleep_time = next_timer_delay.min(Duration::from_millis(10));
            if sleep_time > Duration::from_millis(0) {
                tokio::time::sleep(sleep_time).await;
            }
        }
    }
    
    Ok(())
} 