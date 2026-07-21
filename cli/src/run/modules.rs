use base64::{engine::general_purpose::STANDARD, Engine as _};
use log::{debug, info, trace};
use sourcemap::SourceMap;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::errors::MycoError;
use crate::run::errors::get_exception_message_with_stack;
use crate::run::state::MycoState;

// Thread-local storage for tracking the current module resolution context
thread_local! {
    static MODULE_RESOLUTION_STACK: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

// File type detection for module loading
#[derive(Debug, PartialEq)]
pub enum FileType {
    Unknown,
    TypeScript,
    JavaScript,
    Json,
}

impl FileType {
    pub fn from_path(path: &Path) -> Self {
        let file_type = match path.extension() {
            None => {
                trace!("No extension found for path: {}", path.display());
                Self::Unknown
            }
            Some(os_str) => {
                let lowercase_str = os_str.to_str().map(|s| s.to_lowercase());
                match lowercase_str.as_deref() {
                    Some("ts") | Some("mts") | Some("cts") | Some("tsx") => {
                        trace!("Detected TypeScript file: {}", path.display());
                        Self::TypeScript
                    }
                    Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => {
                        trace!("Detected JavaScript file: {}", path.display());
                        Self::JavaScript
                    }
                    Some("json") => {
                        trace!("Detected JSON file: {}", path.display());
                        Self::Json
                    }
                    _ => {
                        trace!("Unknown file type for path: {}", path.display());
                        Self::Unknown
                    }
                }
            }
        };

        debug!("File type for {}: {:?}", path.display(), file_type);
        file_type
    }
}

/// Loads the user's entry module, evaluates it, and invokes its default export with
/// the powerbox.
///
/// The user module is compiled and evaluated directly - there is no generated wrapper
/// module. The powerbox is passed as a plain argument, so it is never reachable from
/// `globalThis` at any point.
pub fn load_and_run_module(
    scope: &mut v8::PinScope<'_, '_>,
    file_path: &PathBuf,
    myco_powerbox: &v8::Global<v8::Value>,
) -> Result<(), MycoError> {
    info!("Loading and running module: {}", file_path.display());

    debug!("Canonicalizing user module path");
    let user_module_path = file_path.clone();
    let user_module_absolute_path =
        user_module_path
            .canonicalize()
            .map_err(|e| MycoError::PathCanonicalization {
                path: file_path.to_string_lossy().to_string(),
                source: e,
            })?;
    debug!(
        "User module path: {}",
        user_module_absolute_path.to_string_lossy()
    );

    // Set the current module path context so relative imports from the entry module
    // resolve against the entry module's directory.
    debug!("Setting module resolution stack");
    MODULE_RESOLUTION_STACK.with(|current| {
        *current.borrow_mut() = vec![user_module_absolute_path.clone()];
    });

    let base_path = user_module_absolute_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    debug!("Compiling user entry module");
    let main_module = load_and_compile_module(
        scope,
        &user_module_absolute_path.to_string_lossy(),
        &base_path,
    )
    .map_err(|e| MycoError::MainModuleCompilation {
        message: e.to_string(),
    })?;
    debug!("Entry module compiled successfully");

    // Instantiate the module - this will trigger module resolution for its imports
    debug!("Instantiating entry module (will trigger module resolution)");
    let instantiate_result = main_module.instantiate_module(scope, module_resolve_callback);
    if instantiate_result.is_none() {
        return Err(MycoError::MainModuleInstantiation {
            message: "Failed to instantiate main module - likely due to import resolution failure"
                .to_string(),
        });
    }
    debug!("Entry module instantiated successfully");

    // Use TryCatch to capture exceptions during module evaluation
    debug!("Setting up exception handler for module evaluation");
    v8::tc_scope!(let scope, scope);

    // Evaluate the module - this returns a promise, which for a module with top-level
    // await does not settle until the event loop has driven it to completion.
    debug!("Evaluating entry module");
    let result = main_module.evaluate(scope);
    if result.is_none() {
        // Check if there was an exception during evaluation
        if scope.has_caught() {
            if let Some(exception) = scope.exception() {
                let error_message = get_exception_message_with_stack(scope, exception);
                return Err(MycoError::ModuleEvaluation {
                    message: error_message,
                });
            }
        }
        return Err(MycoError::ModuleEvaluation {
            message: "Module evaluation failed".to_string(),
        });
    }

    let result_value = result.ok_or_else(|| MycoError::ModuleEvaluation {
        message: "Module evaluation returned None".to_string(),
    })?;

    // Check for any caught exceptions after evaluation
    if scope.has_caught() {
        if let Some(exception) = scope.exception() {
            let error_message = get_exception_message_with_stack(scope, exception);
            return Err(MycoError::ModuleEvaluation {
                message: error_message,
            });
        }
    }

    let evaluation_promise = v8::Local::<v8::Promise>::try_from(result_value).map_err(|_| {
        MycoError::ModuleEvaluation {
            message: "Module evaluation did not return a promise".to_string(),
        }
    })?;

    // Once evaluation settles, call the module's default export with the powerbox,
    // then record the exit code (or the unhandled error) into isolate state.
    trace!("Wiring entry-point invocation onto the module evaluation promise");
    let namespace = main_module.get_module_namespace();
    let powerbox = v8::Local::new(scope, myco_powerbox);

    let callback_data = v8::Array::new(scope, 2);
    callback_data.set_index(scope, 0, namespace);
    callback_data.set_index(scope, 1, powerbox);

    let call_entry_point = v8::Function::builder(call_default_export)
        .data(callback_data.into())
        .build(scope)
        .ok_or(MycoError::PromiseHandler)?;
    let entry_point_result = evaluation_promise
        .then(scope, call_entry_point)
        .ok_or(MycoError::PromiseHandlerExecution)?;

    let record_exit_code = v8::Function::builder(record_exit_code)
        .build(scope)
        .ok_or(MycoError::PromiseHandler)?;
    let settled = entry_point_result
        .then(scope, record_exit_code)
        .ok_or(MycoError::PromiseHandlerExecution)?;

    let record_error = v8::Function::builder(record_unhandled_error)
        .build(scope)
        .ok_or(MycoError::PromiseHandler)?;
    settled
        .catch(scope, record_error)
        .ok_or(MycoError::PromiseHandlerExecution)?;

    Ok(())
}

/// Promise callback: reads `default` off the entry module's namespace and calls it
/// with the powerbox. Data is `[namespace, powerbox]`.
fn call_default_export<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s>,
) {
    let data = match v8::Local::<v8::Array>::try_from(args.data()) {
        Ok(data) => data,
        Err(_) => return,
    };
    let namespace = match data.get_index(scope, 0) {
        Some(value) => value,
        None => return,
    };
    let powerbox = match data.get_index(scope, 1) {
        Some(value) => value,
        None => return,
    };

    let namespace = match v8::Local::<v8::Object>::try_from(namespace) {
        Ok(namespace) => namespace,
        Err(_) => return,
    };

    let default_key = match v8::String::new(scope, "default") {
        Some(key) => key,
        None => return,
    };
    let default_export = namespace.get(scope, default_key.into());

    let entry_point = default_export.and_then(|v| v8::Local::<v8::Function>::try_from(v).ok());
    match entry_point {
        Some(entry_point) => {
            let undefined = v8::undefined(scope);
            // If this returns None an exception is pending; leaving it pending rejects
            // the derived promise, which is exactly the old `await userModule(Myco)`
            // behaviour.
            if let Some(result) = entry_point.call(scope, undefined.into(), &[powerbox]) {
                rv.set(result);
            }
        }
        None => {
            if let Some(message) = v8::String::new(
                scope,
                "The entry module's default export is not a function. \
                 Expected `export default function (myco) { ... }`",
            ) {
                let exception = v8::Exception::type_error(scope, message);
                scope.throw_exception(exception);
            }
        }
    }
}

/// Promise callback: records the entry point's resolved value as the process exit code.
fn record_exit_code<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'s>,
) {
    let value = args.get(0);
    let exit_code = if value.is_number() {
        value.number_value(scope).unwrap_or(0.0) as i32
    } else {
        0
    };
    debug!("Entry point completed with exit code: {}", exit_code);

    let state_ptr = scope.get_data(0) as *mut MycoState;
    if !state_ptr.is_null() {
        let state = unsafe { &mut *state_ptr };
        state.exit_code = exit_code;
    }
}

/// Promise callback: records a rejection so the event loop can report it and bail out.
fn record_unhandled_error<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'s>,
) {
    let error_value = args.get(0);
    debug!("Entry point rejected with an unhandled error");

    let global_error = v8::Global::new(scope, error_value);
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if !state_ptr.is_null() {
        let state = unsafe { &mut *state_ptr };
        state.unhandled_error = Some(global_error);
    }
}

pub fn module_resolve_callback<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    _referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    v8::callback_scope!(unsafe let scope, context);

    // Get specifier
    let specifier_str = specifier.to_rust_string_lossy(scope);

    // Check if this specifier should be resolved using myco-local.toml
    let resolved_specifiers = {
        let state_ptr = scope.get_data(0) as *mut MycoState;
        if !state_ptr.is_null() {
            let state = unsafe { &*state_ptr };
            if let Some(myco_local) = &state.myco_local {
                // Check for exact match first
                if let Some(resolved_paths) = myco_local.get_resolve_paths(&specifier_str) {
                    resolved_paths.clone()
                } else {
                    // Check for prefix matches
                    let mut best_match: Option<(String, Vec<String>)> = None;

                    for (alias, paths) in myco_local.clone_resolve() {
                        if specifier_str.starts_with(&alias) {
                            // Check if this is a proper prefix match (either exact or followed by '/')
                            if specifier_str == alias
                                || specifier_str.chars().nth(alias.len()) == Some('/')
                            {
                                // Found a prefix match, take the longest one
                                if best_match.is_none()
                                    || alias.len() > best_match.as_ref().unwrap().0.len()
                                {
                                    best_match = Some((alias, paths));
                                }
                            }
                        }
                    }

                    if let Some((alias, resolved_paths)) = best_match {
                        if specifier_str == alias {
                            // Exact match
                            resolved_paths
                        } else {
                            // Prefix match, append the remaining path to each resolved path
                            let remaining = &specifier_str[alias.len()..];
                            resolved_paths
                                .into_iter()
                                .map(|path| format!("{}{}", path, remaining))
                                .collect()
                        }
                    } else {
                        vec![specifier_str.clone()]
                    }
                }
            } else {
                vec![specifier_str.clone()]
            }
        } else {
            vec![specifier_str.clone()]
        }
    };

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

    // Try each resolved specifier until one works
    for resolved_specifier in resolved_specifiers {
        // Load and compile the module using the resolved specifier
        match load_and_compile_module(scope, &resolved_specifier, &base_path) {
            Ok(module) => {
                // Get the module path for the stack
                let module_url = format!(
                    "file://{}",
                    if resolved_specifier.starts_with("file://") {
                        PathBuf::from(&resolved_specifier[7..])
                    } else {
                        let path = PathBuf::from(&resolved_specifier);
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
                    }
                    .to_string_lossy()
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
                v8::tc_scope!(let scope, scope);

                // Instantiate the module recursively
                let result = match module.instantiate_module(scope, module_resolve_callback) {
                    Some(_) => Some(module),
                    None => {
                        // Check if there was an exception during instantiation
                        let error_detail = if scope.has_caught() {
                            if let Some(exception) = scope.exception() {
                                get_exception_message_with_stack(scope, exception)
                            } else {
                                "Unknown exception occurred".to_string()
                            }
                        } else {
                            "Unknown instantiation error".to_string()
                        };
                        eprintln!(
                            "Failed to instantiate module '{}': {}",
                            specifier_str, error_detail
                        );
                        None
                    }
                };

                // Pop the module path from the resolution stack after instantiation
                MODULE_RESOLUTION_STACK.with(|stack| {
                    stack.borrow_mut().pop();
                });

                return result;
            }
            Err(_e) => {
                // Try the next resolved specifier if this one failed
                continue;
            }
        }
    }

    // If we get here, none of the resolved specifiers worked
    eprintln!(
        "Failed to load and compile module '{}' from any resolved paths",
        specifier_str
    );
    None
}

pub fn load_and_compile_module<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    specifier: &str,
    base_path: &Path,
) -> Result<v8::Local<'s, v8::Module>, MycoError> {
    // Convert file:// URL to path
    let path = if specifier.starts_with("file://") {
        PathBuf::from(&specifier[7..]) // Remove "file://" prefix
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

    // Handle directory imports by looking for index files
    let final_absolute_path = if absolute_path.exists() {
        if absolute_path.is_dir() {
            // Look for index files in order of preference
            let index_candidates = ["index.ts", "index.tsx", "index.js", "index.jsx"];
            let mut found_index = None;

            for candidate in &index_candidates {
                let index_path = absolute_path.join(candidate);
                if index_path.exists() {
                    found_index = Some(index_path);
                    break;
                }
            }

            if let Some(index_path) = found_index {
                index_path
            } else {
                return Err(MycoError::ModuleNotFound {
                    specifier: specifier.to_string(),
                    resolved_path: format!(
                        "{} (directory with no index file)",
                        absolute_path.display()
                    ),
                });
            }
        } else {
            absolute_path
        }
    } else {
        return Err(MycoError::ModuleNotFound {
            specifier: specifier.to_string(),
            resolved_path: absolute_path.display().to_string(),
        });
    };

    let file_type = FileType::from_path(&final_absolute_path);

    // Determine if we need to transpile
    let should_transpile = matches!(file_type, FileType::TypeScript);

    let (final_code, source_map_content) = if should_transpile {
        // Use the existing transpilation logic and capture source map
        match util::transpile::parse_and_gen_path(&final_absolute_path) {
            Ok(transpiled) => {
                let source_map_content = String::from_utf8(transpiled.source_map)
                    .map_err(|e| MycoError::InvalidSourceMapUtf8 { source: e })?;
                (transpiled.source, Some(source_map_content))
            }
            Err(e) => {
                // Convert UtilError to MycoError with the correct path
                let myco_error = match e {
                    util::UtilError::Transpilation { message } => MycoError::Transpilation {
                        path: final_absolute_path.display().to_string(),
                        message,
                    },
                    util::UtilError::TypeScriptParsing { message } => MycoError::Transpilation {
                        path: final_absolute_path.display().to_string(),
                        message,
                    },
                    util::UtilError::CodeGeneration { message } => MycoError::Transpilation {
                        path: final_absolute_path.display().to_string(),
                        message,
                    },
                    util::UtilError::SourceMapGeneration { message } => MycoError::Transpilation {
                        path: final_absolute_path.display().to_string(),
                        message,
                    },
                    _ => e.into(), // Use the From trait for other errors
                };
                return Err(myco_error);
            }
        }
    } else {
        let content =
            std::fs::read_to_string(&final_absolute_path).map_err(|e| MycoError::ReadFile {
                path: final_absolute_path.display().to_string(),
                source: e,
            })?;
        (content, None)
    };

    // Create V8 module using the absolute path as the URL for proper referrer resolution
    let module_url = format!("file://{}", final_absolute_path.to_string_lossy());

    // Store source map if we have one
    let source_map_url = if let Some(ref source_map) = source_map_content {
        // Create a data URL for the source map so V8 can access it synchronously
        let source_map_base64 = STANDARD.encode(source_map.as_bytes());
        let data_url = format!(
            "data:application/json;charset=utf-8;base64,{}",
            source_map_base64
        );

        // Parse and store the source map in the state for later use in stack trace mapping
        if let Ok(parsed_source_map) = SourceMap::from_slice(source_map.as_bytes()) {
            let state_ptr = scope.get_data(0) as *mut MycoState;
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                state
                    .source_maps
                    .insert(module_url.clone(), parsed_source_map);
            }
        }

        Some(data_url)
    } else {
        None
    };

    let source_text = v8::String::new(scope, &final_code).ok_or(MycoError::V8StringCreation)?;
    let origin = create_module_origin_for_scope(scope, &module_url, source_map_url.as_deref())?;
    let mut source = v8::script_compiler::Source::new(source_text, Some(&origin));

    let module = v8::script_compiler::compile_module(scope, &mut source).ok_or_else(|| {
        MycoError::ModuleCompilation {
            specifier: specifier.to_string(),
            resolved_path: final_absolute_path.display().to_string(),
        }
    })?;

    // Store the module URL to path mapping in the isolate state
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if !state_ptr.is_null() {
        let state = unsafe { &mut *state_ptr };
        state
            .module_url_to_path
            .insert(module_url, final_absolute_path.clone());
    }

    Ok(module)
}

fn create_module_origin_for_scope<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    url: &str,
    source_map_url: Option<&str>,
) -> Result<v8::ScriptOrigin<'s>, MycoError> {
    let name = v8::String::new(scope, url).ok_or(MycoError::V8StringCreation)?;
    let source_map_value = if let Some(url) = source_map_url {
        Some(
            v8::String::new(scope, url)
                .ok_or(MycoError::V8StringCreation)?
                .into(),
        )
    } else {
        None
    };
    Ok(v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,     // line_offset
        0,     // column_offset
        false, // is_cross_origin
        -1,    // script_id
        source_map_value,
        false, // is_opaque
        false, // is_wasm
        true,  // is_module
        None,  // host_defined_options
    ))
}

pub fn host_import_module_dynamically_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _host_defined_options: v8::Local<'s, v8::Data>,
    _resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let specifier_str = specifier.to_rust_string_lossy(scope);

    // Create a promise resolver
    let resolver = v8::PromiseResolver::new(scope)?;
    let promise = resolver.get_promise(scope);

    // For dynamic imports, we don't have referrer info, so use current working directory
    let base_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Try to load and compile the module
    match load_and_compile_module(scope, &specifier_str, &base_path) {
        Ok(module) => {
            // Use TryCatch to capture exceptions during instantiation
            v8::tc_scope!(let scope, scope);

            // Instantiate the module
            match module.instantiate_module(scope, module_resolve_callback) {
                Some(_) => {
                    // Evaluate the module - this returns a value for the module namespace
                    match module.evaluate(scope) {
                        Some(_result) => {
                            // For dynamic imports, we need to resolve with the module namespace object
                            let module_namespace = module.get_module_namespace();
                            resolver.resolve(scope, module_namespace);
                        }
                        None => {
                            // Check for exceptions during evaluation
                            let error_detail = if scope.has_caught() {
                                if let Some(exception) = scope.exception() {
                                    get_exception_message_with_stack(scope, exception)
                                } else {
                                    "Unknown exception occurred".to_string()
                                }
                            } else {
                                "Unknown evaluation error".to_string()
                            };
                            if let Some(error_msg) = v8::String::new(
                                scope,
                                &format!(
                                    "Failed to evaluate dynamically imported module '{}': {}",
                                    specifier_str, error_detail
                                ),
                            ) {
                                resolver.reject(scope, error_msg.into());
                            }
                        }
                    }
                }
                None => {
                    // Check for exceptions during instantiation
                    let error_detail = if scope.has_caught() {
                        if let Some(exception) = scope.exception() {
                            get_exception_message_with_stack(scope, exception)
                        } else {
                            "Unknown exception occurred".to_string()
                        }
                    } else {
                        "Unknown instantiation error".to_string()
                    };
                    if let Some(error_msg) = v8::String::new(
                        scope,
                        &format!(
                            "Failed to instantiate dynamically imported module '{}': {}",
                            specifier_str, error_detail
                        ),
                    ) {
                        resolver.reject(scope, error_msg.into());
                    }
                }
            }
        }
        Err(e) => {
            if let Some(error_msg) = v8::String::new(
                scope,
                &format!("Failed to load module '{}': {}", specifier_str, e),
            ) {
                resolver.reject(scope, error_msg.into());
            }
        }
    }

    Some(promise)
}
