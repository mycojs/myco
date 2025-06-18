use log::{debug, info, trace};
use std::path::PathBuf;
use std::sync::Once;
use tokio::sync::mpsc;

use crate::errors::MycoError;
use crate::manifest::myco_local::MycoLocalToml;
use crate::run::constants::{ICU_DATA, RUNTIME_SNAPSHOT};
use crate::run::event_loop::run_event_loop;
use crate::run::inspector;
use crate::run::modules::{host_import_module_dynamically_callback, load_and_run_module, FileType};
use crate::run::ops;
use crate::run::state::{DebugOptions, MycoState};

static V8_INIT: Once = Once::new();

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

pub async fn run_js(
    file_path: &PathBuf,
    myco_local: Option<MycoLocalToml>,
    debug_options: Option<DebugOptions>,
) -> Result<i32, MycoError> {
    info!("Starting JavaScript execution for: {}", file_path.display());
    debug!("Myco local configuration: {:?}", myco_local.is_some());
    debug!("Debug options: {:?}", debug_options);

    // Initialize V8 (only once per process)
    V8_INIT.call_once(|| {
        info!("Initializing V8 engine (first run)");
        debug!("Setting ICU data ({} bytes)", ICU_DATA.0.len());
        // Include 10MB ICU data file.
        v8::icu::set_common_data_74(&ICU_DATA.0).expect("Failed to set ICU data");

        debug!("Creating V8 platform");
        // Initialize V8 platform
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        info!("V8 engine initialized successfully");
    });

    debug!("Creating V8 isolate");
    let mut isolate = v8::Isolate::new(Default::default());

    // Set up inspector if debugging is enabled
    let inspector_rx = if let Some(debug_opts) = debug_options.as_ref() {
        info!("Setting up debug inspector on port {}", debug_opts.port);
        debug!(
            "Inspector options - break_on_start: {}, wait_for_connection: {}",
            debug_opts.break_on_start, debug_opts.wait_for_connection
        );

        let (session_tx, session_rx) = mpsc::channel(1);
        let inspector_server = inspector::Inspector::new(debug_opts, session_tx);
        inspector_server.start();

        info!("Inspector server started on port {}", debug_opts.port);

        if debug_opts.break_on_start {
            info!("Waiting for debugger to connect before execution...");
        } else {
            debug!("Inspector ready - debugger can connect at any time");
        }

        Some(session_rx)
    } else {
        debug!("No debug options provided, running without inspector");
        None
    };

    // Set up the host import module dynamically callback for dynamic imports
    debug!("Setting up dynamic import callback");
    isolate.set_host_import_module_dynamically_callback(host_import_module_dynamically_callback);

    // Get the current runtime handle to pass to MycoState
    debug!("Getting current Tokio runtime handle");
    let runtime_handle = tokio::runtime::Handle::current();

    // Store state in isolate data
    debug!("Creating Myco runtime state");
    let mut state = MycoState::new(myco_local, runtime_handle);

    // Create inspector first, before any scopes, to avoid borrow conflicts
    let inspector =
        if let (Some(session_rx), Some(debug_opts)) = (inspector_rx, debug_options.as_ref()) {
            debug!("Creating inspector with V8 context");
            // Create a temporary scope just to create the context
            let mut temp_scope = v8::HandleScope::new(&mut isolate);
            let context = v8::Context::new(&mut temp_scope, Default::default());
            let global_context = v8::Global::new(&mut temp_scope, context);
            drop(temp_scope); // Drop the scope to release the borrow

            debug!("Initializing Myco inspector");
            // Now create the inspector with the isolate outside of any scope
            Some(inspector::MycoInspector::new(
                &mut isolate,
                global_context,
                session_rx,
                debug_opts.break_on_start,
                debug_opts.wait_for_connection,
            ))
        } else {
            debug!("No inspector needed, running without debugging");
            None
        };

    state.inspector = inspector;
    debug!("Storing state in V8 isolate");
    isolate.set_data(0, Box::into_raw(Box::new(state)) as *mut std::ffi::c_void);

    // Now create the main scopes for execution
    debug!("Creating V8 handle scope");
    let mut handle_scope = v8::HandleScope::new(&mut isolate);
    let scope = &mut handle_scope;

    // Get the context from the inspector or create a new one
    debug!("Setting up V8 execution context");
    let context = if let Some(inspector_rc) =
        unsafe { &(*(scope.get_data(0) as *const MycoState)).inspector }
    {
        debug!("Using inspector context for debugging");
        let inspector = inspector_rc.borrow();
        if let Some(global_context) = inspector.get_context() {
            v8::Local::new(scope, global_context)
        } else {
            debug!("Creating new context (inspector context not available)");
            v8::Context::new(scope, Default::default())
        }
    } else {
        debug!("Creating new context (no inspector)");
        v8::Context::new(scope, Default::default())
    };

    let mut context_scope = v8::ContextScope::new(scope, context);
    let scope = &mut context_scope;

    // Handle break-on-start if needed
    debug!("Checking for debug break-on-start options");
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if !state_ptr.is_null() {
        let state = unsafe { &mut *state_ptr };
        if let Some(inspector_rc) = &state.inspector {
            let mut inspector = inspector_rc.borrow_mut();

            if inspector.should_wait_for_connection() {
                info!("Waiting for debugger connection before starting execution");
                inspector.wait_for_session();
                info!("Debugger connected, continuing execution");
            } else if inspector.should_break_on_start() {
                debug!("Setting breakpoint on next statement");
                inspector.break_on_next_statement();
            }
        }
    }

    // Create global object and register ops
    debug!("Setting up global object and registering ops");
    let global = scope.get_current_context().global(scope);
    ops::register_ops(scope, &global)?;
    info!("JavaScript runtime operations registered");

    // Since we're not using snapshots yet, execute the runtime code manually
    if RUNTIME_SNAPSHOT.is_empty() {
        debug!("Executing runtime code (no snapshot available)");
        execute_runtime_code(scope)?;
        debug!("Runtime code executed successfully");
    } else {
        debug!("Using runtime snapshot");
    }

    // Check if the file is a TypeScript/JavaScript module or a simple script
    debug!("Determining file type for: {}", file_path.display());
    let file_type = FileType::from_path(file_path);
    info!("File type detected: {:?}", file_type);

    let is_module = match file_type {
        FileType::TypeScript | FileType::JavaScript => {
            info!("Loading file as ES module");
            // Load as ES module using the MAIN_JS template
            load_and_run_module(scope, file_path).await?;
            debug!("ES module loaded and executed successfully");
            true
        }
        _ => {
            info!("Loading file as simple script");
            // Load as simple script
            debug!("Reading script file: {}", file_path.display());
            let user_script =
                std::fs::read_to_string(file_path).map_err(|e| MycoError::ReadFile {
                    path: file_path.to_string_lossy().to_string(),
                    source: e,
                })?;

            debug!("Script size: {} characters", user_script.len());
            debug!("Compiling script");
            let source = v8::String::new(scope, &user_script).ok_or(MycoError::V8StringCreation)?;
            let script = v8::Script::compile(scope, source, None).ok_or_else(|| {
                MycoError::ScriptCompilation {
                    message: "Failed to compile user script".to_string(),
                }
            })?;

            debug!("Executing compiled script");
            script
                .run(scope)
                .ok_or_else(|| MycoError::ScriptExecution {
                    message: "Failed to run user script".to_string(),
                })?;

            info!("Script executed successfully");
            false
        }
    };

    // Run the event loop
    debug!("Starting event loop");
    run_event_loop(scope).await?;
    debug!("Event loop completed");

    // Extract the exit code from the global variable (only for modules)
    let exit_code = if is_module {
        debug!("Extracting exit code from module");
        let global = scope.get_current_context().global(scope);
        let exit_code_key =
            v8::String::new(scope, "__MYCO_EXIT_CODE__").ok_or(MycoError::V8StringCreation)?;
        let exit_code_value = global.get(scope, exit_code_key.into());

        if let Some(value) = exit_code_value {
            if value.is_number() {
                let code = value.number_value(scope).unwrap_or(0.0) as i32;
                debug!("Module exit code: {}", code);
                code
            } else {
                debug!("Exit code is not a number, defaulting to 0");
                0
            }
        } else {
            debug!("No exit code found, defaulting to 0");
            0
        }
    } else {
        debug!("Simple script execution complete (no exit code)");
        0 // Simple scripts don't return exit codes
    };

    info!(
        "JavaScript execution completed with exit code: {}",
        exit_code
    );
    Ok(exit_code)
}

fn execute_runtime_code(
    scope: &mut v8::ContextScope<'_, v8::HandleScope>,
) -> Result<(), MycoError> {
    // Read the transpiled runtime code
    debug!("Loading runtime JavaScript code");
    let runtime_code = include_str!(concat!(env!("OUT_DIR"), "/runtime.js"));
    debug!("Runtime code size: {} characters", runtime_code.len());

    trace!("Compiling runtime code");
    let source = v8::String::new(scope, runtime_code).ok_or(MycoError::V8StringCreation)?;
    let script = v8::Script::compile(scope, source, None).ok_or(MycoError::RuntimeCompilation)?;

    trace!("Executing runtime code");
    script.run(scope).ok_or(MycoError::RuntimeExecution)?;

    debug!("Runtime code execution completed");
    Ok(())
}
