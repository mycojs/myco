use crate::errors::MycoError;
pub use capabilities::*;
use log::{debug, info, warn};

use crate::manifest::myco_local::MycoLocalToml;
use crate::manifest::MycoToml;

// Module declarations
mod capabilities;
mod constants;
mod engine;
mod errors;
mod event_loop;
mod inspector;
mod modules;
mod ops;
mod stack_trace;
mod state;

// Re-export public types from state module
pub use state::DebugOptions;

pub fn run(
    myco_toml: &MycoToml,
    script: &String,
    debug_options: Option<DebugOptions>,
) -> Result<i32, MycoError> {
    info!("Running script: {}", script);
    debug!("Debug options: {:?}", debug_options);

    if let Some(run) = &myco_toml.run {
        debug!("Found run configuration with {} scripts", run.len());
        if let Some(script_path) = run.get(script) {
            info!("Found script '{}' mapping to: {}", script, script_path);
            run_file(script_path, debug_options)
        } else {
            debug!(
                "Script '{}' not found in run configuration, treating as file path",
                script
            );
            run_file(script, debug_options)
        }
    } else {
        debug!("No run configuration found, treating script as file path");
        run_file(script, debug_options)
    }
}

pub fn run_file(file_path: &str, debug_options: Option<DebugOptions>) -> Result<i32, MycoError> {
    info!("Running file: {}", file_path);

    // Convert to absolute path for better error reporting
    debug!("Converting to absolute path");
    let absolute_path = match std::fs::canonicalize(file_path) {
        Ok(path) => {
            debug!("Canonicalized path: {}", path.display());
            path
        }
        Err(_e) => {
            debug!("Canonicalization failed, constructing absolute path manually");
            // If canonicalize fails, try to construct absolute path manually
            let current_dir = std::env::current_dir()
                .map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
            let manual_path = current_dir.join(file_path);
            debug!("Manual absolute path: {}", manual_path.display());
            manual_path
        }
    };

    // Check if the file exists
    if !absolute_path.exists() {
        warn!("File not found: {}", absolute_path.display());
        return Err(MycoError::FileNotFound {
            path: absolute_path.display().to_string(),
        });
    }

    // The working directory is the nearest myco.toml to the executable
    debug!("Finding nearest myco.toml for working directory");
    let working_dir = match MycoToml::load_nearest(absolute_path.clone()) {
        Ok((dir, _)) => {
            debug!("Found myco.toml, working directory: {}", dir.display());
            dir
        }
        Err(_) => {
            debug!("No myco.toml found, using file directory as working directory");
            absolute_path.clone()
        }
    };

    // Try to load myco-local.toml
    debug!("Loading myco-local.toml");
    let myco_local = MycoLocalToml::load_from_myco_toml_path(working_dir.clone()).ok();
    if myco_local.is_some() {
        debug!("Successfully loaded myco-local.toml");
    } else {
        debug!("No myco-local.toml found");
    }

    debug!("Setting working directory to: {}", working_dir.display());
    std::env::set_current_dir(&working_dir).map_err(|_e| MycoError::SetCurrentDirectory {
        dir: working_dir.display().to_string(),
    })?;

    // Check if file exists
    if !absolute_path.exists() {
        warn!(
            "File not found after changing directory: {}",
            absolute_path.display()
        );
        return Err(MycoError::FileNotFound {
            path: absolute_path.display().to_string(),
        });
    }

    // Check if it's actually a file (not a directory)
    if !absolute_path.is_file() {
        warn!("Path is not a file: {}", absolute_path.display());
        return Err(MycoError::NotAFile {
            path: absolute_path.display().to_string(),
        });
    }

    debug!("Creating Tokio runtime");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| MycoError::TokioRuntime { source: e })?;

    info!("Starting JavaScript execution");
    runtime.block_on(engine::run_js(&absolute_path, myco_local, debug_options))
}
