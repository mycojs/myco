pub use capabilities::*;
use crate::errors::MycoError;

use crate::manifest::MycoToml;

// Module declarations
mod capabilities;
mod ops;
mod inspector;
mod stack_trace;
mod engine;
mod state;
mod modules;
mod event_loop;
mod errors;
mod constants;

// Re-export public types from state module
pub use state::DebugOptions;

pub fn run(myco_toml: &MycoToml, script: &String, debug_options: Option<DebugOptions>) -> Result<i32, MycoError> {
    if let Some(run) = &myco_toml.run {
        if let Some(script) = run.get(script) {
            run_file(script, debug_options)
        } else {
            run_file(script, debug_options)
        }
    } else {
        run_file(script, debug_options)
    }
}

pub fn run_file(file_path: &str, debug_options: Option<DebugOptions>) -> Result<i32, MycoError> {
    // Convert to absolute path for better error reporting
    let absolute_path = match std::fs::canonicalize(file_path) {
        Ok(path) => path,
        Err(_e) => {
            // If canonicalize fails, try to construct absolute path manually
            let current_dir = std::env::current_dir()
                .map_err(|e| MycoError::CurrentDirectory { source: e })?;
            current_dir.join(file_path)
        }
    };

    // The working directory is the nearest myco.toml to the executable
    let working_dir = match MycoToml::load_nearest(absolute_path.clone()) {
        Ok((dir, _)) => dir,
        Err(_) => absolute_path.clone()
    };

    std::env::set_current_dir(&working_dir)
        .map_err(|e| MycoError::CurrentDirectory { source: e })?;
    
    // Check if file exists
    if !absolute_path.exists() {
        return Err(MycoError::FileNotFound { 
            path: absolute_path.display().to_string() 
        });
    }
    
    // Check if it's actually a file (not a directory)
    if !absolute_path.is_file() {
        return Err(MycoError::NotAFile { 
            path: absolute_path.display().to_string() 
        });
    }
    
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| MycoError::TokioRuntime { source: e })?;
    
    match runtime.block_on(engine::run_js(&absolute_path, debug_options)) {
        Ok(exit_code) => Ok(exit_code),
        Err(error) => Err(error),
    }
}
