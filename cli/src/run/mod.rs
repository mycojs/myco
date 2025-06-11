use std::path::PathBuf;

pub use capabilities::*;

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
    
    match runtime.block_on(engine::run_js(file_path, debug_options)) {
        Ok(exit_code) => {
            std::process::exit(exit_code);
        }
        Err(error) => {
            eprintln!("Error running script: {error}");
            std::process::exit(1);
        }
    }
}
