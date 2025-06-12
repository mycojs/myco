use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::errors::MycoError;
use crate::run::state::{MycoState, DebugOptions};
use crate::run::constants::{ICU_DATA, RUNTIME_SNAPSHOT};
use crate::run::modules::{FileType, load_and_run_module, host_import_module_dynamically_callback};
use crate::run::event_loop::run_event_loop;
use crate::run::ops;
use crate::run::inspector;
use crate::manifest::myco_local::MycoLocalToml;

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

pub async fn run_js(file_path: &PathBuf, myco_local: Option<MycoLocalToml>, debug_options: Option<DebugOptions>) -> Result<i32, MycoError> {
    // Include 10MB ICU data file.
    v8::icu::set_common_data_74(&ICU_DATA.0)
        .map_err(|_| MycoError::IcuDataInit)?;

    // Initialize V8 platform (only once per process)
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    let mut isolate = v8::Isolate::new(Default::default());

    // Set up inspector if debugging is enabled
    let inspector_rx = if let Some(debug_opts) = debug_options.as_ref() {
        let (session_tx, session_rx) = mpsc::channel(1);
        let inspector_server = inspector::Inspector::new(debug_opts, session_tx);
        inspector_server.start();

        if debug_opts.break_on_start {
            inspector_debug!("Waiting for debugger to connect...");
        } else {
            inspector_debug!("Inspector server started. Debugger can connect at any time.");
        }
        
        Some(session_rx)
    } else {
        None
    };

    // Set up the host import module dynamically callback for dynamic imports
    isolate.set_host_import_module_dynamically_callback(host_import_module_dynamically_callback);

    // Store state in isolate data
    let mut state = MycoState::new(myco_local);
    
    // Create inspector first, before any scopes, to avoid borrow conflicts
    let inspector = if let (Some(session_rx), Some(debug_opts)) = (inspector_rx, debug_options.as_ref()) {
        // Create a temporary scope just to create the context
        let mut temp_scope = v8::HandleScope::new(&mut isolate);
        let context = v8::Context::new(&mut temp_scope, Default::default());
        let global_context = v8::Global::new(&mut temp_scope, context);
        drop(temp_scope); // Drop the scope to release the borrow
        
        // Now create the inspector with the isolate outside of any scope
        Some(inspector::MycoInspector::new(
            &mut isolate,
            global_context,
            session_rx,
            debug_opts.break_on_start,
            debug_opts.wait_for_connection,
        ))
    } else {
        None
    };
    
    state.inspector = inspector;
    isolate.set_data(0, Box::into_raw(Box::new(state)) as *mut std::ffi::c_void);

    // Now create the main scopes for execution
    let mut handle_scope = v8::HandleScope::new(&mut isolate);
    let scope = &mut handle_scope;
    
    // Get the context from the inspector or create a new one
    let context = if let Some(inspector_rc) = unsafe { &(*(scope.get_data(0) as *const MycoState)).inspector } {
        let inspector = inspector_rc.borrow();
        if let Some(global_context) = inspector.get_context() {
            v8::Local::new(scope, global_context)
        } else {
            v8::Context::new(scope, Default::default())
        }
    } else {
        v8::Context::new(scope, Default::default())
    };

    let mut context_scope = v8::ContextScope::new(scope, context);
    let scope = &mut context_scope;

    // Handle break-on-start if needed
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if !state_ptr.is_null() {
        let state = unsafe { &mut *state_ptr };
        if let Some(inspector_rc) = &state.inspector {
            let mut inspector = inspector_rc.borrow_mut();
            
            if inspector.should_wait_for_connection() {
                inspector.wait_for_session();
            }
            else if inspector.should_break_on_start() {
                inspector.break_on_next_statement();
            }
        }
    }

    // Create global object and register ops
    let global = scope.get_current_context().global(scope);
    ops::register_ops(scope, &global)?;

    // Since we're not using snapshots yet, execute the runtime code manually
    if RUNTIME_SNAPSHOT.is_empty() {
        execute_runtime_code(scope)?;
    }

    // Check if the file is a TypeScript/JavaScript module or a simple script
    let file_type = FileType::from_path(&file_path);
    
    let is_module = match file_type {
        FileType::TypeScript | FileType::JavaScript => {
            // Load as ES module using the MAIN_JS template
            load_and_run_module(scope, file_path).await?;
            true
        }
        _ => {
            // Load as simple script
            let user_script = std::fs::read_to_string(file_path)
                .map_err(|e| MycoError::ReadFile { 
                    path: file_path.to_string_lossy().to_string(), 
                    source: e 
                })?;
            
            let source = v8::String::new(scope, &user_script)
                .ok_or(MycoError::V8StringCreation)?;
            let script = v8::Script::compile(scope, source, None)
                .ok_or_else(|| MycoError::ScriptCompilation { 
                    message: "Failed to compile user script".to_string() 
                })?;
            
            script.run(scope)
                .ok_or_else(|| MycoError::ScriptExecution { 
                    message: "Failed to run user script".to_string() 
                })?;
            
            false
        }
    };

    // Run the event loop
    run_event_loop(scope).await?;

    // Extract the exit code from the global variable (only for modules)
    let exit_code = if is_module {
        let global = scope.get_current_context().global(scope);
        let exit_code_key = v8::String::new(scope, "__MYCO_EXIT_CODE__")
            .ok_or(MycoError::V8StringCreation)?;
        let exit_code_value = global.get(scope, exit_code_key.into());
        
        if let Some(value) = exit_code_value {
            if value.is_number() {
                value.number_value(scope).unwrap_or(0.0) as i32
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0 // Simple scripts don't return exit codes
    };

    Ok(exit_code)
}

fn execute_runtime_code(scope: &mut v8::ContextScope<'_, v8::HandleScope>) -> Result<(), MycoError> {
    // Read the transpiled runtime code
    let runtime_code = include_str!(concat!(env!("OUT_DIR"), "/runtime.js"));
    
    let source = v8::String::new(scope, runtime_code)
        .ok_or(MycoError::V8StringCreation)?;
    let script = v8::Script::compile(scope, source, None)
        .ok_or(MycoError::RuntimeCompilation)?;
    
    script.run(scope)
        .ok_or(MycoError::RuntimeExecution)?;
    
    Ok(())
} 