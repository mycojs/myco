use std::path::{Path, PathBuf};
use std::cell::RefCell;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sourcemap::SourceMap;

use crate::run::errors::get_exception_message_with_stack;
use crate::run::state::MycoState;
use crate::run::constants::MAIN_JS;
use util;
use crate::errors::MycoError;

// Thread-local storage for tracking the current module resolution context
thread_local! {
    static MODULE_RESOLUTION_STACK: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
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

pub async fn load_and_run_module(scope: &mut v8::ContextScope<'_, v8::HandleScope<'_>>, file_path: &PathBuf) -> Result<(), MycoError> {
    // Create the main module contents using the MAIN_JS template
    let user_module_path = file_path.clone();
    let user_module_absolute_path = user_module_path.canonicalize()
        .map_err(|e| MycoError::PathCanonicalization { 
            path: file_path.to_string_lossy().to_string(), 
            source: e 
        })?;
    let user_module_url = format!("file://{}", user_module_absolute_path.to_string_lossy());
    
    // Set the current module path context for the main module to the user module's path
    MODULE_RESOLUTION_STACK.with(|current| {
        *current.borrow_mut() = vec![user_module_absolute_path.clone()];
    });

    let main_module_contents = MAIN_JS.replace("{{USER_MODULE}}", &user_module_url);
    
    // Compile the main module as an ES module
    let main_source = v8::String::new(scope, &main_module_contents)
        .ok_or(MycoError::V8StringCreation)?;
    let main_origin = create_module_origin(scope, "myco:main")?;
    let mut main_source_obj = v8::script_compiler::Source::new(main_source, Some(&main_origin));
    
    let main_module = v8::script_compiler::compile_module(scope, &mut main_source_obj)
        .ok_or_else(|| MycoError::MainModuleCompilation { 
            message: "Failed to compile main module".to_string() 
        })?;

    // Instantiate the module - this will trigger module resolution for the import
    let instantiate_result = main_module.instantiate_module(scope, module_resolve_callback);
    if instantiate_result.is_none() {
        return Err(MycoError::MainModuleInstantiation { 
            message: "Failed to instantiate main module - likely due to import resolution failure".to_string() 
        });
    }

    // Use TryCatch to capture exceptions during module evaluation
    let mut try_catch = v8::TryCatch::new(scope);
    let scope = &mut try_catch;

    // Evaluate the module - this may return a promise for async modules
    let result = main_module.evaluate(scope);
    if result.is_none() {
        // Check if there was an exception during evaluation
        if scope.has_caught() {
            if let Some(exception) = scope.exception() {
                let error_message = get_exception_message_with_stack(scope, exception);
                return Err(MycoError::ModuleEvaluation { message: error_message });
            }
        }
        return Err(MycoError::ModuleEvaluation { 
            message: "Module evaluation failed".to_string() 
        });
    }

    let result_value = result.ok_or_else(|| MycoError::ModuleEvaluation { 
        message: "Module evaluation returned None".to_string() 
    })?;

    // Check for any caught exceptions after evaluation
    if scope.has_caught() {
        if let Some(exception) = scope.exception() {
            let error_message = get_exception_message_with_stack(scope, exception);
            return Err(MycoError::ModuleEvaluation { message: error_message });
        }
    }

    // If the result is a promise, we need to handle its potential rejection
    if result_value.is_promise() {
        let promise = v8::Local::<v8::Promise>::try_from(result_value)
            .map_err(|_| MycoError::ModuleEvaluation { 
                message: "Failed to cast result to Promise".to_string() 
            })?;
        
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
        
        let handler_source = v8::String::new(scope, promise_handler_code)
            .ok_or(MycoError::V8StringCreation)?;
        let handler_script = v8::Script::compile(scope, handler_source, None)
            .ok_or(MycoError::PromiseHandler)?;
        
        let handler_result = handler_script.run(scope)
            .ok_or(MycoError::PromiseHandlerExecution)?;
        
        if let Ok(handler_fn) = v8::Local::<v8::Function>::try_from(handler_result) {
            let args = [promise.into()];
            let _wrapped_promise = handler_fn.call(scope, global.into(), &args);
        }
    }

    Ok(())
}

fn create_module_origin<'s>(scope: &mut v8::ContextScope<'s, v8::HandleScope>, url: &str) -> Result<v8::ScriptOrigin<'s>, MycoError> {
    let name = v8::String::new(scope, url)
        .ok_or(MycoError::V8StringCreation)?;
    Ok(v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,  // line_offset
        0,  // column_offset
        false,  // is_cross_origin
        -1,  // script_id
        None,  // source_map_url - no source map for main template
        false,  // is_opaque
        false,  // is_wasm
        true,  // is_module
        None,  // host_defined_options
    ))
}

pub fn module_resolve_callback<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    _referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    let scope = &mut unsafe { v8::CallbackScope::new(context) };

    // Get specifier 
    let specifier_str = specifier.to_rust_string_lossy(scope);

    // Check if this specifier should be resolved using myco-local.toml
    let resolved_specifier = {
        let state_ptr = scope.get_data(0) as *mut MycoState;
        if !state_ptr.is_null() {
            let state = unsafe { &*state_ptr };
            if let Some(myco_local) = &state.myco_local {
                // Check for exact match first
                if let Some(resolved_path) = myco_local.get_resolve_path(&specifier_str) {
                    resolved_path.clone()
                } else {
                    // Check for prefix matches
                    let mut best_match: Option<(String, String)> = None;
                    
                    for (alias, path) in myco_local.clone_resolve() {
                        if specifier_str.starts_with(&alias) {
                            // Check if this is a proper prefix match (either exact or followed by '/')
                            if specifier_str == alias || specifier_str.chars().nth(alias.len()) == Some('/') {
                                // Found a prefix match, take the longest one
                                if best_match.is_none() || alias.len() > best_match.as_ref().unwrap().0.len() {
                                    best_match = Some((alias, path));
                                }
                            }
                        }
                    }
                    
                    if let Some((alias, resolved_path)) = best_match {
                        if specifier_str == alias {
                            // Exact match
                            resolved_path
                        } else {
                            // Prefix match, append the remaining path
                            let remaining = &specifier_str[alias.len()..];
                            let final_path = format!("{}{}", resolved_path, remaining);
                            final_path
                        }
                    } else {
                        specifier_str.clone()
                    }
                }
            } else {
                specifier_str.clone()
            }
        } else {
            specifier_str.clone()
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

    // Load and compile the module using the resolved specifier
    match load_and_compile_module(scope, &resolved_specifier, &base_path) {
        Ok(module) => {
            // Get the module path for the stack
            let module_url = format!("file://{}", 
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
                        if let Some(exception) = scope.exception() {
                            get_exception_message_with_stack(scope, exception)
                        } else {
                            "Unknown exception occurred".to_string()
                        }
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
            eprintln!("Failed to load and compile module '{}': {}", resolved_specifier, e);
            None
        }
    }
}

pub fn load_and_compile_module<'s>(scope: &mut v8::HandleScope<'s>, specifier: &str, base_path: &Path) -> Result<v8::Local<'s, v8::Module>, MycoError> {
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
                    resolved_path: format!("{} (directory with no index file)", absolute_path.display()),
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
            },
            Err(e) => {
                // Convert UtilError to MycoError with the correct path
                let myco_error = match e {
                    util::UtilError::Transpilation { message } => {
                        MycoError::Transpilation { 
                            path: final_absolute_path.display().to_string(), 
                            message 
                        }
                    }
                    util::UtilError::TypeScriptParsing { message } => {
                        MycoError::Transpilation { 
                            path: final_absolute_path.display().to_string(), 
                            message 
                        }
                    }
                    util::UtilError::CodeGeneration { message } => {
                        MycoError::Transpilation { 
                            path: final_absolute_path.display().to_string(), 
                            message 
                        }
                    }
                    util::UtilError::SourceMapGeneration { message } => {
                        MycoError::Transpilation { 
                            path: final_absolute_path.display().to_string(), 
                            message 
                        }
                    }
                    _ => e.into(), // Use the From trait for other errors
                };
                return Err(myco_error);
            }
        }
    } else {
        let content = std::fs::read_to_string(&final_absolute_path)
            .map_err(|e| MycoError::ReadFile {
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
        let data_url = format!("data:application/json;charset=utf-8;base64,{}", source_map_base64);
        
        // Parse and store the source map in the state for later use in stack trace mapping
        if let Ok(parsed_source_map) = SourceMap::from_slice(source_map.as_bytes()) {
            let state_ptr = scope.get_data(0) as *mut MycoState;
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                state.source_maps.insert(module_url.clone(), parsed_source_map);
            }
        }
        
        Some(data_url)
    } else {
        None
    };
    
    let source_text = v8::String::new(scope, &final_code)
        .ok_or(MycoError::V8StringCreation)?;
    let origin = create_module_origin_for_scope(scope, &module_url, source_map_url.as_deref())?;
    let mut source = v8::script_compiler::Source::new(source_text, Some(&origin));

    let module = v8::script_compiler::compile_module(scope, &mut source)
        .ok_or_else(|| MycoError::ModuleCompilation {
            specifier: specifier.to_string(),
            resolved_path: final_absolute_path.display().to_string(),
        })?;

    // Store the module URL to path mapping in the isolate state
    let state_ptr = scope.get_data(0) as *mut MycoState;
    if !state_ptr.is_null() {
        let state = unsafe { &mut *state_ptr };
        state.module_url_to_path.insert(module_url, final_absolute_path.clone());
    }

    Ok(module)
}

fn create_module_origin_for_scope<'s>(scope: &mut v8::HandleScope<'s>, url: &str, source_map_url: Option<&str>) -> Result<v8::ScriptOrigin<'s>, MycoError> {
    let name = v8::String::new(scope, url)
        .ok_or(MycoError::V8StringCreation)?;
    let source_map_value = if let Some(url) = source_map_url {
        Some(v8::String::new(scope, url)
            .ok_or(MycoError::V8StringCreation)?
            .into())
    } else {
        None
    };
    Ok(v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,  // line_offset
        0,  // column_offset
        false,  // is_cross_origin
        -1,  // script_id
        source_map_value,
        false,  // is_opaque
        false,  // is_wasm
        true,  // is_module
        None,  // host_defined_options
    ))
}

pub fn host_import_module_dynamically_callback<'s>(
    scope: &mut v8::HandleScope<'s>,
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
                                if let Some(exception) = scope.exception() {
                                    get_exception_message_with_stack(scope, exception)
                                } else {
                                    "Unknown exception occurred".to_string()
                                }
                            } else {
                                "Unknown evaluation error".to_string()
                            };
                            if let Some(error_msg) = v8::String::new(scope, &format!(
                                "Failed to evaluate dynamically imported module '{}': {}", 
                                specifier_str, 
                                error_detail
                            )) {
                                resolver.reject(scope, error_msg.into());
                            }
                        }
                    }
                },
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
                    if let Some(error_msg) = v8::String::new(scope, &format!(
                        "Failed to instantiate dynamically imported module '{}': {}", 
                        specifier_str, 
                        error_detail
                    )) {
                        resolver.reject(scope, error_msg.into());
                    }
                }
            }
        },
        Err(e) => {
            if let Some(error_msg) = v8::String::new(scope, &format!("Failed to load module '{}': {}", specifier_str, e)) {
                resolver.reject(scope, error_msg.into());
            }
        }
    }
    
    Some(promise)
} 