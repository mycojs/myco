use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::cell::RefCell;
use std::rc::Rc;

pub use token::*;

use crate::AnyError;
use crate::manifest::MycoToml;
use util;
use tokio::sync::mpsc;

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

#[derive(Debug, Clone)]
pub struct DebugOptions {
    pub port: u16,
    pub break_on_start: bool,
    pub wait_for_connection: bool,
}

#[macro_use]
mod token;
mod ops;
mod inspector;

// Thread-local storage for tracking the current module resolution context
thread_local! {
    static MODULE_RESOLUTION_STACK: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
}

// Timer structure to track pending timeouts
pub struct Timer {
    pub id: u32,
    pub callback: v8::Global<v8::Function>,
    pub execute_at: Instant,
}

impl Timer {
    pub fn new(id: u32, callback: v8::Global<v8::Function>, execute_at: Instant) -> Self {
        Self {
            id,
            callback,
            execute_at,
        }
    }
}

// State that gets stored in the V8 isolate
pub struct MycoState {
    pub capabilities: CapabilityRegistry,
    pub module_cache: HashMap<String, v8::Global<v8::Module>>,
    pub timers: Vec<Timer>,
    pub next_timer_id: u32,
    pub module_url_to_path: HashMap<String, PathBuf>,
    pub inspector: Option<Rc<RefCell<inspector::MycoInspector>>>,
}

impl MycoState {
    pub fn new() -> Self {
        Self {
            capabilities: CapabilityRegistry::new(),
            module_cache: HashMap::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            module_url_to_path: HashMap::new(),
            inspector: None,
        }
    }
}

// File type detection for module loading
#[derive(Debug, PartialEq)]
enum FileType {
    Unknown,
    TypeScript,
    JavaScript,
    Json,
}

impl FileType {
    pub fn from_path(path: &Path) -> Self {
        match path.extension() {
            None => Self::Unknown,
            Some(os_str) => {
                let lowercase_str = os_str.to_str().map(|s| s.to_lowercase());
                match lowercase_str.as_deref() {
                    | Some("ts")
                    | Some("mts")
                    | Some("cts")
                    | Some("tsx") => Self::TypeScript,
                    | Some("js")
                    | Some("jsx")
                    | Some("mjs")
                    | Some("cjs") => Self::JavaScript,
                    Some("json") => Self::Json,
                    _ => Self::Unknown,
                }
            }
        }
    }
}

// Runtime snapshot is empty for now since we're not using snapshots yet
static RUNTIME_SNAPSHOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/MYCO_SNAPSHOT.bin"));

const MAIN_JS: &str = "\
const Myco = globalThis.Myco;

// Delete the global scope that we don't want access to
delete globalThis.Myco;

const {default: userModule} = await import('{{USER_MODULE}}');

// Call the user module and capture the result
const result = await userModule(Myco);

// Store the exit code in a global variable that Rust can access
globalThis.__MYCO_EXIT_CODE__ = typeof result === 'number' ? result : 0;
";

pub fn run(myco_toml: &MycoToml, script: &String, debug_options: Option<DebugOptions>) {
    if let Some(run) = &myco_toml.run {
        if let Some(script) = run.get(script) {
            run_file(script, debug_options);
        } else {
            run_file(script, debug_options);
        }
    } else {
        run_file(script, debug_options);
    };
}

pub fn run_file(file_path: &str, debug_options: Option<DebugOptions>) {
    // Convert to absolute path for better error reporting
    let absolute_path = match std::fs::canonicalize(file_path) {
        Ok(path) => path,
        Err(_) => {
            // If canonicalize fails, try to construct absolute path manually
            let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            current_dir.join(file_path)
        }
    };
    
    // Check if file exists
    if !absolute_path.exists() {
        eprintln!("Myco error: File not found: {}", absolute_path.display());
        std::process::exit(1);
    }
    
    // Check if it's actually a file (not a directory)
    if !absolute_path.is_file() {
        eprintln!("Myco error: Path is not a file: {}", absolute_path.display());
        std::process::exit(1);
    }
    
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    
    match runtime.block_on(run_js(file_path, debug_options)) {
        Ok(exit_code) => {
            std::process::exit(exit_code);
        }
        Err(error) => {
            eprintln!("Error running script: {error}");
            std::process::exit(1);
        }
    }
}

#[repr(C, align(16))]
struct IcuData<T: ?Sized>(T);
static ICU_DATA: &'static IcuData<[u8]> = &IcuData(*include_bytes!("icudtl.dat"));

async fn run_js(file_name: &str, debug_options: Option<DebugOptions>) -> Result<i32, AnyError> {
    // Include 10MB ICU data file.
    v8::icu::set_common_data_74(&ICU_DATA.0).unwrap();

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
    let mut state = MycoState::new();
    
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
    register_ops(scope, &global)?;

    // Since we're not using snapshots yet, execute the runtime code manually
    if RUNTIME_SNAPSHOT.is_empty() {
        execute_runtime_code(scope)?;
    }

    // Check if the file is a TypeScript/JavaScript module or a simple script
    let path = PathBuf::from(file_name);
    let file_type = FileType::from_path(&path);
    
    let is_module = match file_type {
        FileType::TypeScript | FileType::JavaScript => {
            // Load as ES module using the MAIN_JS template
            load_and_run_module(scope, file_name).await?;
            true
        }
        _ => {
            // Load as simple script
            let user_script = std::fs::read_to_string(file_name)
                .map_err(|e| anyhow::anyhow!("Failed to read script file '{}': {}", file_name, e))?;
            
            let source = v8::String::new(scope, &user_script).unwrap();
            let script = v8::Script::compile(scope, source, None)
                .ok_or_else(|| anyhow::anyhow!("Failed to compile user script"))?;
            
            script.run(scope)
                .ok_or_else(|| anyhow::anyhow!("Failed to run user script"))?;
            
            false
        }
    };

    // Run the event loop
    run_event_loop(scope).await?;

    // Extract the exit code from the global variable (only for modules)
    let exit_code = if is_module {
        let global = scope.get_current_context().global(scope);
        let exit_code_key = v8::String::new(scope, "__MYCO_EXIT_CODE__").unwrap();
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

async fn load_and_run_module(scope: &mut v8::ContextScope<'_, v8::HandleScope<'_>>, file_name: &str) -> Result<(), AnyError> {
    // Create the main module contents using the MAIN_JS template
    let user_module_path = std::path::PathBuf::from(file_name);
    let user_module_absolute_path = user_module_path.canonicalize()?;
    let user_module_url = format!("file://{}", user_module_absolute_path.to_string_lossy());
    
    // Set the current module path context for the main module to the user module's path
    MODULE_RESOLUTION_STACK.with(|current| {
        *current.borrow_mut() = vec![user_module_absolute_path.clone()];
    });

    let main_module_contents = MAIN_JS.replace("{{USER_MODULE}}", &user_module_url);
    
    // Compile the main module as an ES module
    let main_source = v8::String::new(scope, &main_module_contents).unwrap();
    let main_origin = create_module_origin(scope, "myco:main");
    let mut main_source_obj = v8::script_compiler::Source::new(main_source, Some(&main_origin));
    
    let main_module = v8::script_compiler::compile_module(scope, &mut main_source_obj)
        .ok_or_else(|| anyhow::anyhow!("Failed to compile main module"))?;

    // Instantiate the module - this will trigger module resolution for the import
    let instantiate_result = main_module.instantiate_module(scope, module_resolve_callback);
    if instantiate_result.is_none() {
        return Err(anyhow::anyhow!("Failed to instantiate main module - likely due to import resolution failure"));
    }

    // Use TryCatch to capture exceptions during module evaluation
    let mut try_catch = v8::TryCatch::new(scope);
    let scope = &mut try_catch;

    // Evaluate the module - this may return a promise for async modules
    let result = main_module.evaluate(scope);
    if result.is_none() {
        // Check if there was an exception during evaluation
        if scope.has_caught() {
            let exception = scope.exception().unwrap();
            let error_message = get_exception_message_with_stack(scope, exception);
            return Err(anyhow::anyhow!("{}", error_message));
        }
        return Err(anyhow::anyhow!("Module evaluation failed"));
    }

    let result_value = result.unwrap();

    // Check for any caught exceptions after evaluation
    if scope.has_caught() {
        let exception = scope.exception().unwrap();
        let error_message = get_exception_message_with_stack(scope, exception);
        return Err(anyhow::anyhow!("{}", error_message));
    }

    // If the result is a promise, we need to handle its potential rejection
    if result_value.is_promise() {
        let promise = v8::Local::<v8::Promise>::try_from(result_value).unwrap();
        
        // Set up a handler for promise rejection
        let global = scope.get_current_context().global(scope);
        let promise_handler_code = r#"
        (function(promise) {
            return promise.catch(function(error) {
                // Store the error globally so Rust can access it
                globalThis.__MYCO_UNHANDLED_ERROR__ = error;
                throw error; // Re-throw to maintain the rejection
            });
        })
        "#;
        
        let handler_source = v8::String::new(scope, promise_handler_code).unwrap();
        let handler_script = v8::Script::compile(scope, handler_source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to compile promise handler"))?;
        
        let handler_result = handler_script.run(scope)
            .ok_or_else(|| anyhow::anyhow!("Failed to run promise handler"))?;
        
        if let Ok(handler_fn) = v8::Local::<v8::Function>::try_from(handler_result) {
            let args = [promise.into()];
            let _wrapped_promise = handler_fn.call(scope, global.into(), &args);
        }
    }

    Ok(())
}

fn execute_runtime_code(scope: &mut v8::ContextScope<'_, v8::HandleScope>) -> Result<(), AnyError> {
    // Read the transpiled runtime code
    let runtime_code = include_str!(concat!(env!("OUT_DIR"), "/runtime.js"));
    
    let source = v8::String::new(scope, runtime_code).unwrap();
    let script = v8::Script::compile(scope, source, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to compile runtime script"))?;
    
    script.run(scope)
        .ok_or_else(|| anyhow::anyhow!("Failed to run runtime script"))?;
    
    Ok(())
}

async fn run_event_loop(scope: &mut v8::ContextScope<'_, v8::HandleScope<'_>>) -> Result<(), AnyError> {
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

fn register_ops(scope: &mut v8::ContextScope<v8::HandleScope>, global: &v8::Object) -> Result<(), AnyError> {
    // Create the Myco object
    let myco_obj = v8::Object::new(scope);

    // Create MycoOps object for low-level operations
    let myco_ops = v8::Object::new(scope);
    
    // Register console operations
    ops::console::register_console_ops(scope, &myco_ops)?;
    
    // Register encoding operations
    ops::encoding::register_encoding_ops(scope, &myco_ops)?;

    // Register TOML operations
    ops::toml::register_toml_ops(scope, &myco_ops)?;

    // Register time operations
    ops::time::register_time_ops(scope, &myco_ops)?;

    // Register filesystem operations
    ops::filesystem::register_filesystem_ops(scope, &myco_ops)?;
    
    // Register HTTP operations
    ops::http::client::register_http_client_ops(scope, &myco_ops)?;

    // Set argv property on Myco object
    let argv: Vec<String> = std::env::args().collect();
    let v8_array = v8::Array::new(scope, argv.len() as i32);
    for (i, arg) in argv.iter().enumerate() {
        let v8_string = v8::String::new(scope, arg).unwrap();
        v8_array.set_index(scope, i as u32, v8_string.into());
    }
    let argv_key = v8::String::new(scope, "argv").unwrap();
    myco_obj.set(scope, argv_key.into(), v8_array.into());

    // Set Myco object on global
    let myco_key = v8::String::new(scope, "Myco").unwrap();
    global.set(scope, myco_key.into(), myco_obj.into());
    
    // Set MycoOps object on global (will be captured and deleted by runtime)
    let myco_ops_key = v8::String::new(scope, "MycoOps").unwrap();
    global.set(scope, myco_ops_key.into(), myco_ops.into());
    
    Ok(())
}

fn create_module_origin<'s>(scope: &mut v8::ContextScope<'s, v8::HandleScope>, url: &str) -> v8::ScriptOrigin<'s> {
    let name = v8::String::new(scope, url).unwrap();
    v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,  // line_offset
        0,  // column_offset
        false,  // is_cross_origin
        -1,  // script_id
        None,  // source_map_url
        false,  // is_opaque
        false,  // is_wasm
        true,  // is_module
        None,  // host_defined_options
    )
}

fn module_resolve_callback<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    _referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    let scope = &mut unsafe { v8::CallbackScope::new(context) };

    // Get specifier 
    let specifier_str = specifier.to_rust_string_lossy(scope);

    // Determine the base path from the current module context
    let base_path = MODULE_RESOLUTION_STACK.with(|stack| {
        if let Some(current_path) = stack.borrow().last() {
            if let Some(parent) = current_path.parent() {
                parent.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        }
    });

    // Load and compile the module
    match load_and_compile_module(scope, &specifier_str, &base_path) {
        Ok(module) => {
            // Get the module path for the stack
            let module_url = format!("file://{}", 
                if specifier_str.starts_with("file://") {
                    PathBuf::from(&specifier_str[7..])
                } else {
                    let path = PathBuf::from(&specifier_str);
                    if path.is_absolute() {
                        path
                    } else {
                        let normalized_path = if let Ok(stripped) = path.strip_prefix("./") {
                            stripped.to_path_buf()
                        } else {
                            path
                        };
                        base_path.join(normalized_path)
                    }
                }.to_string_lossy()
            );
            
            // Get the absolute path from the state mapping
            let module_path = {
                let state_ptr = scope.get_data(0) as *mut MycoState;
                if !state_ptr.is_null() {
                    let state = unsafe { &*state_ptr };
                    state.module_url_to_path.get(&module_url).cloned()
                } else {
                    None
                }
            };
            
            // Push the module path onto the resolution stack before instantiation
            if let Some(abs_path) = module_path {
                MODULE_RESOLUTION_STACK.with(|stack| {
                    stack.borrow_mut().push(abs_path);
                });
            }
            
            // Use TryCatch to capture exceptions during instantiation
            let mut try_catch = v8::TryCatch::new(scope);
            let scope = &mut try_catch;
            
            // Instantiate the module recursively
            let result = match module.instantiate_module(scope, module_resolve_callback) {
                Some(_) => Some(module),
                None => {
                    // Check if there was an exception during instantiation
                    let error_detail = if scope.has_caught() {
                        let exception = scope.exception().unwrap();
                        get_exception_message_with_stack(scope, exception)
                    } else {
                        "Unknown instantiation error".to_string()
                    };
                    eprintln!("Failed to instantiate module '{}': {}", specifier_str, error_detail);
                    None
                }
            };
            
            // Pop the module path from the resolution stack after instantiation
            MODULE_RESOLUTION_STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
            
            result
        },
        Err(e) => {
            eprintln!("Failed to load and compile module '{}': {}", specifier_str, e);
            None
        }
    }
}

fn load_and_compile_module<'s>(scope: &mut v8::HandleScope<'s>, specifier: &str, base_path: &Path) -> Result<v8::Local<'s, v8::Module>, AnyError> {
    // Convert file:// URL to path
    let path = if specifier.starts_with("file://") {
        PathBuf::from(&specifier[7..])  // Remove "file://" prefix
    } else {
        PathBuf::from(specifier)
    };
    
    let absolute_path = if path.is_absolute() {
        path.clone()
    } else {
        // Normalize relative paths to remove "./" prefix and other path inconsistencies
        let normalized_path = if let Ok(stripped) = path.strip_prefix("./") {
            stripped.to_path_buf()
        } else {
            path.clone()
        };
        
        base_path.join(normalized_path)
    };
    
    if !absolute_path.exists() {
        return Err(anyhow::anyhow!(
            "Module file not found: {} (resolved to: {} from base: {})", 
            specifier, 
            absolute_path.display(),
            base_path.display()
        ));
    }
    
    let file_type = FileType::from_path(&path);
    
    // Determine if we need to transpile
    let should_transpile = matches!(file_type, FileType::TypeScript);
    
    let final_code = if should_transpile {
        // Use the existing transpilation logic
        match util::transpile::parse_and_gen_path(&absolute_path) {
            Ok(transpiled) => transpiled.source,
            Err(e) => return Err(anyhow::anyhow!(
                "Failed to transpile {} (resolved to: {}): {}", 
                specifier, 
                absolute_path.display(),
                e
            )),
        }
    } else {
        match std::fs::read_to_string(&absolute_path) {
            Ok(content) => content,
            Err(e) => return Err(anyhow::anyhow!(
                "Failed to read module file '{}' (resolved from specifier '{}'): {}", 
                absolute_path.display(),
                specifier, 
                e
            )),
        }
    };

    // Create V8 module using the absolute path as the URL for proper referrer resolution
    let module_url = format!("file://{}", absolute_path.to_string_lossy());
    let source_text = v8::String::new(scope, &final_code).unwrap();
    let origin = create_module_origin_for_scope(scope, &module_url);
    let mut source = v8::script_compiler::Source::new(source_text, Some(&origin));

    let module = match v8::script_compiler::compile_module(scope, &mut source) {
        Some(module) => module,
        None => return Err(anyhow::anyhow!("Failed to compile module: {} (resolved to: {})", specifier, absolute_path.display())),
    };

    // Store the module URL to path mapping in the isolate state
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if !state_ptr.is_null() {
        let state = unsafe { &mut *state_ptr };
        state.module_url_to_path.insert(module_url, absolute_path.clone());
    }

    Ok(module)
}

fn create_module_origin_for_scope<'s>(scope: &mut v8::HandleScope<'s>, url: &str) -> v8::ScriptOrigin<'s> {
    let name = v8::String::new(scope, url).unwrap();
    v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,  // line_offset
        0,  // column_offset
        false,  // is_cross_origin
        -1,  // script_id
        None,  // source_map_url
        false,  // is_opaque
        false,  // is_wasm
        true,  // is_module
        None,  // host_defined_options
    )
}

fn host_import_module_dynamically_callback<'s>(
    scope: &mut v8::HandleScope<'s>,
    _host_defined_options: v8::Local<'s, v8::Data>,
    _resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let specifier_str = specifier.to_rust_string_lossy(scope);
    
    // Create a promise resolver
    let resolver = match v8::PromiseResolver::new(scope) {
        Some(resolver) => resolver,
        None => return None,
    };
    let promise = resolver.get_promise(scope);
    
    // For dynamic imports, we don't have referrer info, so use current working directory
    let base_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    
    // Try to load and compile the module
    match load_and_compile_module(scope, &specifier_str, &base_path) {
        Ok(module) => {
            // Use TryCatch to capture exceptions during instantiation
            let mut try_catch = v8::TryCatch::new(scope);
            let scope = &mut try_catch;
            
            // Instantiate the module
            match module.instantiate_module(scope, module_resolve_callback) {
                Some(_) => {
                    // Evaluate the module - this returns a value for the module namespace
                    match module.evaluate(scope) {
                        Some(_result) => {
                            // For dynamic imports, we need to resolve with the module namespace object
                            let module_namespace = module.get_module_namespace();
                            resolver.resolve(scope, module_namespace);
                        },
                        None => {
                            // Check for exceptions during evaluation
                            let error_detail = if scope.has_caught() {
                                let exception = scope.exception().unwrap();
                                get_exception_message_with_stack(scope, exception)
                            } else {
                                "Unknown evaluation error".to_string()
                            };
                            let error_msg = v8::String::new(scope, &format!(
                                "Failed to evaluate dynamically imported module '{}': {}", 
                                specifier_str, 
                                error_detail
                            )).unwrap();
                            resolver.reject(scope, error_msg.into());
                        }
                    }
                },
                None => {
                    // Check for exceptions during instantiation
                    let error_detail = if scope.has_caught() {
                        let exception = scope.exception().unwrap();
                        get_exception_message_with_stack(scope, exception)
                    } else {
                        "Unknown instantiation error".to_string()
                    };
                    let error_msg = v8::String::new(scope, &format!(
                        "Failed to instantiate dynamically imported module '{}': {}", 
                        specifier_str, 
                        error_detail
                    )).unwrap();
                    resolver.reject(scope, error_msg.into());
                }
            }
        },
        Err(e) => {
            let error_msg = v8::String::new(scope, &format!("Failed to load module '{}': {}", specifier_str, e)).unwrap();
            resolver.reject(scope, error_msg.into());
        }
    }
    
    Some(promise)
}

fn get_exception_message_with_stack(scope: &mut v8::HandleScope, exception: v8::Local<v8::Value>) -> String {
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
    
    // If we have a stack trace, use it. Otherwise, fall back to just the message
    if let Some(stack_trace) = stack {
        // The stack trace usually includes the message, so we can just return it
        stack_trace
    } else {
        // Fallback: try to get current stack trace
        if let Some(stack_trace) = v8::StackTrace::current_stack_trace(scope, 10) {
            let mut trace_lines = vec![format!("Error: {}", message)];
            
            for i in 0..stack_trace.get_frame_count() {
                if let Some(frame) = stack_trace.get_frame(scope, i) {
                    let function_name = frame.get_function_name(scope)
                        .map(|name| name.to_rust_string_lossy(scope))
                        .unwrap_or_else(|| "<anonymous>".to_string());
                    
                    let script_name = frame.get_script_name(scope)
                        .map(|name| name.to_rust_string_lossy(scope))
                        .unwrap_or_else(|| "<unknown>".to_string());
                    
                    let line_number = frame.get_line_number();
                    let column_number = frame.get_column();
                    
                    trace_lines.push(format!("    at {} ({}:{}:{})", function_name, script_name, line_number, column_number));
                }
            }
            
            trace_lines.join("\n")
        } else {
            format!("Error: {}", message)
        }
    }
}
