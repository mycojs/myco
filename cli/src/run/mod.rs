use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use token::*;

use crate::AnyError;
use crate::manifest::MycoToml;
use util;

#[macro_use]
mod token;
mod ops;

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
}

impl MycoState {
    pub fn new() -> Self {
        Self {
            capabilities: CapabilityRegistry::new(),
            module_cache: HashMap::new(),
            timers: Vec::new(),
            next_timer_id: 1,
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

userModule(Myco);
";

pub fn run(myco_toml: &MycoToml, script: &String) {
    if let Some(run) = &myco_toml.run {
        if let Some(script) = run.get(script) {
            run_file(script);
        } else {
            run_file(script);
        }
    } else {
        run_file(script);
    };
}

pub fn run_file(file_path: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_js(file_path)) {
            eprintln!("error: {error}");
            eprintln!("{}", error.backtrace());
    }
}

async fn run_js(file_name: &str) -> Result<(), AnyError> {
    // Initialize V8 platform (only once per process)
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    // Create a V8 isolate
    let mut isolate = if RUNTIME_SNAPSHOT.is_empty() {
        v8::Isolate::new(Default::default())
    } else {
        let startup_data = v8::StartupData::from(RUNTIME_SNAPSHOT);
        let params = v8::Isolate::create_params().snapshot_blob(startup_data);
        v8::Isolate::new(params)
    };

    // Set up the host import module dynamically callback for dynamic imports
    isolate.set_host_import_module_dynamically_callback(host_import_module_dynamically_callback);

    // Store state in isolate data
    let state = MycoState::new();
    isolate.set_data(0, Box::into_raw(Box::new(state)) as *mut std::ffi::c_void);

    let mut handle_scope = v8::HandleScope::new(&mut isolate);
    let scope = &mut handle_scope;

    // Create context first
    let context = v8::Context::new(scope, Default::default());
    let mut context_scope = v8::ContextScope::new(scope, context);
    let scope = &mut context_scope;

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
    
    match file_type {
        FileType::TypeScript | FileType::JavaScript => {
            // Load as ES module using the MAIN_JS template
            load_and_run_module(scope, file_name).await?;
        }
        _ => {
            // Load as simple script
            let user_script = std::fs::read_to_string(file_name)
                .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_name, e))?;
            
            let source = v8::String::new(scope, &user_script).unwrap();
            let script = v8::Script::compile(scope, source, None)
                .ok_or_else(|| anyhow::anyhow!("Failed to compile user script"))?;
            
            script.run(scope)
                .ok_or_else(|| anyhow::anyhow!("Failed to run user script"))?;
        }
    }

    // Run the event loop
    run_event_loop(scope).await?;

    Ok(())
}

async fn load_and_run_module(scope: &mut v8::ContextScope<'_, v8::HandleScope<'_>>, file_name: &str) -> Result<(), AnyError> {
    // Create the main module contents using the MAIN_JS template
    let user_module_path = std::path::PathBuf::from(file_name);
    let user_module_url = format!("file://{}", user_module_path.canonicalize()?.to_string_lossy());
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

    // Evaluate the module - this returns a promise for async modules
    let result = main_module.evaluate(scope);
    if result.is_none() {
        return Err(anyhow::anyhow!("Module evaluation failed"));
    }

    // The result might be a promise for modules with top-level await
    let evaluation_result = result.unwrap();
    if evaluation_result.is_promise() {
        // We have a promise, we need to handle it properly
        let promise = v8::Local::<v8::Promise>::try_from(evaluation_result)
            .map_err(|_| anyhow::anyhow!("Failed to cast evaluation result to promise"))?;
        
        // Check promise state
        match promise.state() {
            v8::PromiseState::Pending => {
                // Promise is still pending, we need to process the event loop
                // For now, let's just process microtasks and see if it resolves
                for _ in 0..10 {  // Try a few times
                    scope.perform_microtask_checkpoint();
                    if promise.state() != v8::PromiseState::Pending {
                        break;
                    }
                }
                
                // Check the final state
                match promise.state() {
                    v8::PromiseState::Fulfilled => {
                        // Success - promise resolved
                    },
                    v8::PromiseState::Rejected => {
                        let reason = promise.result(scope);
                        let reason_string = reason.to_rust_string_lossy(scope);
                        return Err(anyhow::anyhow!("Module evaluation promise rejected: {}", reason_string));
                    },
                    v8::PromiseState::Pending => {
                        return Err(anyhow::anyhow!("Module evaluation promise is still pending after processing microtasks"));
                    }
                }
            },
            v8::PromiseState::Fulfilled => {
                // Promise already resolved, we're good
            },
            v8::PromiseState::Rejected => {
                let reason = promise.result(scope);
                let reason_string = reason.to_rust_string_lossy(scope);
                return Err(anyhow::anyhow!("Module evaluation promise rejected: {}", reason_string));
            }
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
        
        // Check for and execute ready timers
        let now = Instant::now();
        let mut executed_any_timer = false;
        
        // Get the state from the isolate
        let state_ptr = scope.get_data(0) as *mut MycoState;
        if !state_ptr.is_null() {
            let state = unsafe { &mut *state_ptr };
            
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

    // Register time operations
    ops::time::register_time_ops(scope, &myco_ops)?;

    // Register filesystem operations
    ops::filesystem::register_filesystem_ops(scope, &myco_ops)?;

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

    // Load and compile the module
    match load_and_compile_module(scope, &specifier_str) {
        Ok(module) => {
            // Instantiate the module recursively
            match module.instantiate_module(scope, module_resolve_callback) {
                Some(_) => Some(module),
                None => {
                    eprintln!("Failed to instantiate module: {}", specifier_str);
                    None
                }
            }
        },
        Err(e) => {
            eprintln!("Failed to load and compile module '{}': {}", specifier_str, e);
            None
        }
    }
}

fn load_and_compile_module<'s>(scope: &mut v8::HandleScope<'s>, specifier: &str) -> Result<v8::Local<'s, v8::Module>, AnyError> {
    // Convert file:// URL to path
    let path = if specifier.starts_with("file://") {
        PathBuf::from(&specifier[7..])  // Remove "file://" prefix
    } else {
        PathBuf::from(specifier)
    };
    
    let file_type = FileType::from_path(&path);
    
    // Determine if we need to transpile
    let should_transpile = matches!(file_type, FileType::TypeScript);
    
    let final_code = if should_transpile {
        // Use the existing transpilation logic
        match util::transpile::parse_and_gen_path(&path) {
            Ok(transpiled) => transpiled.source,
            Err(_) => return Err(anyhow::anyhow!("Failed to transpile {}", path.display())),
        }
    } else {
        std::fs::read_to_string(&path)?
    };

    // Create V8 module
    let source_text = v8::String::new(scope, &final_code).unwrap();
    let origin = create_module_origin_for_scope(scope, specifier);
    let mut source = v8::script_compiler::Source::new(source_text, Some(&origin));

    v8::script_compiler::compile_module(scope, &mut source)
        .ok_or_else(|| anyhow::anyhow!("Failed to compile module: {}", specifier))
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
    
    // Try to load and compile the module
    match load_and_compile_module(scope, &specifier_str) {
        Ok(module) => {
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
                            let error_msg = v8::String::new(scope, "Failed to evaluate dynamically imported module").unwrap();
                            resolver.reject(scope, error_msg.into());
                        }
                    }
                },
                None => {
                    let error_msg = v8::String::new(scope, "Failed to instantiate dynamically imported module").unwrap();
                    resolver.reject(scope, error_msg.into());
                }
            }
        },
        Err(e) => {
            let error_msg = v8::String::new(scope, &format!("Failed to load module: {}", e)).unwrap();
            resolver.reject(scope, error_msg.into());
        }
    }
    
    Some(promise)
}
