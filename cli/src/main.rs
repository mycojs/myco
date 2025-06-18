use std::env;

use clap::{arg, command, ArgAction, Command};
use log::{debug, error, info, warn, LevelFilter};

pub use run::*;

use crate::deps::write_deps_changes;
use crate::errors::MycoError;
use crate::manifest::{MycoToml, PackageName};

mod deps;
mod errors;
mod init;
mod integrity;
mod logger;
mod manifest;
mod pack;
mod publish;
mod run;
mod workspace;

fn main() {
    if let Err(e) = run_main() {
        error!("{}", e);
        std::process::exit(1);
    }
}

fn run_main() -> Result<(), MycoError> {
    let matches = command!()
        .arg(arg!(--"log-level" <LEVEL> "Set log level").value_parser(["off", "error", "warn", "info", "debug", "trace"]).global(true))
        .arg(arg!(--"no-color" "Disable colored output").action(ArgAction::SetTrue).global(true))
        .subcommand(
            Command::new("run")
                .about("Run a JS/TS file in Myco")
                .arg(arg!([script] "The name of the script to run, either a name from myco.toml's [run] block or a relative path. Defaults to 'default'."))
                .arg(arg!([args] ... "Arguments to pass to the script").trailing_var_arg(true).allow_hyphen_values(true))
                .arg(arg!(--inspect "Enable V8 inspector for debugging").action(clap::ArgAction::SetTrue))
                .arg(arg!(--"inspect-port" <PORT> "Port for V8 inspector to listen on").value_parser(clap::value_parser!(u16)).default_value("9229"))
                .arg(arg!(--"inspect-brk" "Enable V8 inspector and break on start").action(clap::ArgAction::SetTrue))
                .arg(arg!(--"inspect-wait" "Enable V8 inspector and wait for connection").action(clap::ArgAction::SetTrue))
        )
        .subcommand(
            Command::new("init")
                .about("Initialize a new Myco project")
                .arg(arg!(<dir> "The directory to initialize"))
        )
        .subcommand(
            Command::new("install")
                .about("Install dependencies from myco.toml")
                .arg(arg!(--save "Write the lockfile after installing"))
        )
        .subcommand(
            Command::new("add")
                .about("Add a dependency to myco.toml")
                .arg(arg!(<package> "The package to add"))
        )
        .subcommand(
            Command::new("remove")
                .about("Remove a dependency from myco.toml")
                .arg(arg!(<package> "The package to remove"))
        )
        .subcommand(
            Command::new("update")
                .about("Update a dependency")
                .arg(arg!([package] "The package to update. Defaults to all."))
        )
        .subcommand(
            Command::new("list")
                .about("List dependencies")
        )
        .subcommand(
            Command::new("pack")
                .about("Pack the project for release")
                .arg(arg!(--next_major "Bump the major version").conflicts_with("next_minor").conflicts_with("next_patch"))
                .arg(arg!(--next_minor "Bump the minor version").conflicts_with("next_major").conflicts_with("next_patch"))
                .arg(arg!(--next_patch "Bump the patch version").conflicts_with("next_major").conflicts_with("next_minor"))
        )
        .subcommand(
            Command::new("publish")
                .about("Publish the current package to a registry")
                .arg(arg!(<registry> "The registry to publish to"))
        )
        .subcommand(
            Command::new("workspace")
                .alias("ws")
                .about("Workspace commands")
                .subcommand(
                    Command::new("list")
                        .about("List all workspace members")
                )
                .subcommand(
                    Command::new("install")
                        .about("Install dependencies for all workspace members")
                        .arg(arg!(--save "Write the lockfile after installing"))
                )
                .subcommand(
                    Command::new("run")
                        .about("Run a script in all workspace members that define it")
                        .arg(arg!(<script> "The script to run"))
                        .arg(arg!(-p --package <PACKAGE> "Run only in specified packages (can be used multiple times)").action(ArgAction::Append))
                )
        )
        .arg_required_else_help(true)
        .args_conflicts_with_subcommands(true)
        .get_matches();

    // Initialize logger based on command line flags
    let no_color = matches.get_flag("no-color");
    let log_level = matches.get_one::<String>("log-level");

    let level = if let Some(level_str) = log_level {
        logger::level_from_str(level_str).unwrap_or(LevelFilter::Info)
    } else {
        LevelFilter::Error
    };

    let use_colors = !no_color;
    if let Err(e) = logger::init_logger(level, use_colors) {
        eprintln!("Failed to initialize logger: {}", e);
        std::process::exit(1);
    }

    info!("Myco CLI starting with log level: {:?}", level);

    if let Some(matches) = matches.subcommand_matches("run") {
        info!("Running 'run' subcommand");
        let default = &"default".to_string();
        let script = matches.get_one::<String>("script").unwrap_or(default);
        let inspect = matches.get_flag("inspect");
        let inspect_brk = matches.get_flag("inspect-brk");
        let inspect_wait = matches.get_flag("inspect-wait");
        let inspect_port = matches
            .get_one::<u16>("inspect-port")
            .copied()
            .unwrap_or(9229);

        debug!("Script to run: {}", script);
        debug!(
            "Debug options - inspect: {}, inspect_brk: {}, inspect_wait: {}, port: {}",
            inspect, inspect_brk, inspect_wait, inspect_port
        );

        // Enable debugging if any inspect flag is set
        let debug_options = if inspect || inspect_brk || inspect_wait {
            info!("Debug mode enabled on port {}", inspect_port);
            Some(run::DebugOptions {
                port: inspect_port,
                break_on_start: inspect_brk,
                wait_for_connection: inspect_brk || inspect_wait,
            })
        } else {
            None
        };

        let current_dir =
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
        debug!("Current directory: {}", current_dir.display());

        let myco_location = match MycoToml::load_nearest(current_dir.clone()) {
            Ok((dir, toml)) => {
                info!("Found myco.toml at: {}", dir.display());
                Some((dir, toml))
            }
            Err(e) => {
                debug!("No myco.toml found: {}", e);
                None
            }
        };

        let exit_code = if let Some((working_dir, myco_toml)) = myco_location {
            info!("Running script '{}' in project mode", script);
            env::set_current_dir(&working_dir).map_err(|_e| MycoError::SetCurrentDirectory {
                dir: working_dir.display().to_string(),
            })?;
            run::run(&myco_toml, script, debug_options)?
        } else {
            info!("Running script '{}' as standalone file", script);
            run::run_file(script, debug_options)?
        };
        info!("Script execution completed with exit code: {}", exit_code);
        std::process::exit(exit_code);
    } else if let Some(matches) = matches.subcommand_matches("init") {
        info!("Running 'init' subcommand");
        if let Some(dir) = matches.get_one::<String>("dir") {
            info!("Initializing new Myco project in directory: {}", dir);
            init::init(dir.to_string())?;

            // Sync changes
            debug!("Loading myco.toml to sync dependencies");
            let (_, myco_toml) = MycoToml::load_nearest(std::path::PathBuf::from(dir))?;
            info!("Installing initial dependencies");
            deps::install(myco_toml, true)?;
            info!("Project initialized successfully in: {}", dir);
            println!("Initialized {}", dir);
        }
    } else if let Some(matches) = matches.subcommand_matches("install") {
        info!("Running 'install' subcommand");
        let (myco_dir, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        info!("Found myco.toml at: {}", myco_dir.display());
        env::set_current_dir(&myco_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: myco_dir.display().to_string(),
        })?;
        let save = matches.get_flag("save");
        debug!("Save lockfile: {}", save);
        info!("Installing dependencies");
        deps::install(myco_toml, save)?;
        info!("Dependencies installed successfully");
        println!("Installed dependencies");
    } else if let Some(matches) = matches.subcommand_matches("add") {
        info!("Running 'add' subcommand");
        let (myco_dir, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        info!("Found myco.toml at: {}", myco_dir.display());
        env::set_current_dir(&myco_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: myco_dir.display().to_string(),
        })?;
        let package = matches
            .get_one::<String>("package")
            .ok_or_else(|| MycoError::Internal {
                message: "Package argument is required".to_string(),
            })?;
        info!("Adding package: {}", package);
        let package_name =
            PackageName::from_str(package).map_err(|_| MycoError::InvalidPackageName {
                name: package.to_string(),
            })?;
        debug!("Parsed package name: {:?}", package_name);
        let changes = deps::add(&myco_toml, package_name)?;
        debug!("Generated dependency changes");
        write_deps_changes(&changes, &myco_dir.join("myco.toml"))?;
        info!("Updated myco.toml with new dependency");

        // Sync changes
        debug!("Reloading myco.toml and syncing dependencies");
        let (_, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        deps::install(myco_toml, true)?;
        info!("Package '{}' added successfully", package);
        println!("Added {}", package);
    } else if let Some(matches) = matches.subcommand_matches("remove") {
        info!("Running 'remove' subcommand");
        let (myco_dir, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        info!("Found myco.toml at: {}", myco_dir.display());
        env::set_current_dir(&myco_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: myco_dir.display().to_string(),
        })?;
        let package = matches
            .get_one::<String>("package")
            .ok_or_else(|| MycoError::Internal {
                message: "Package argument is required".to_string(),
            })?;
        info!("Removing package: {}", package);
        let package_name =
            PackageName::from_str(package).map_err(|_| MycoError::InvalidPackageName {
                name: package.to_string(),
            })?;
        debug!("Parsed package name: {:?}", package_name);
        let changes = deps::remove(&myco_toml, package_name)?;
        debug!("Generated dependency changes");
        write_deps_changes(&changes, &myco_dir.join("myco.toml"))?;
        info!("Updated myco.toml to remove dependency");

        // Sync changes
        debug!("Reloading myco.toml and syncing dependencies");
        let (_, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        deps::install(myco_toml, true)?;
        info!("Package '{}' removed successfully", package);
        println!("Removed {}", package);
    } else if let Some(matches) = matches.subcommand_matches("update") {
        info!("Running 'update' subcommand");
        let (myco_dir, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        info!("Found myco.toml at: {}", myco_dir.display());
        env::set_current_dir(&myco_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: myco_dir.display().to_string(),
        })?;
        let package = matches.get_one::<String>("package");
        let package_name = if let Some(package) = package {
            info!("Updating specific package: {}", package);
            Some(
                PackageName::from_str(package).map_err(|_| MycoError::InvalidPackageName {
                    name: package.to_string(),
                })?,
            )
        } else {
            info!("Updating all dependencies");
            None
        };
        debug!("Parsed package name: {:?}", package_name);
        let changes = deps::update(&myco_toml, package_name)?;
        debug!("Generated dependency changes");
        write_deps_changes(&changes, &myco_dir.join("myco.toml"))?;
        info!("Updated myco.toml with new versions");

        // Sync changes
        debug!("Reloading myco.toml and syncing dependencies");
        let (_, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        deps::install(myco_toml, true)?;
        let target = package.map(|s| s.as_str()).unwrap_or("all dependencies");
        info!("Updated {} successfully", target);
        println!("Updated {}", target);
    } else if matches.subcommand_matches("list").is_some() {
        info!("Running 'list' subcommand");
        let (myco_dir, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        info!("Found myco.toml at: {}", myco_dir.display());
        env::set_current_dir(&myco_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: myco_dir.display().to_string(),
        })?;
        debug!("Listing dependencies");
        deps::list(myco_toml);
    } else if let Some(matches) = matches.subcommand_matches("pack") {
        info!("Running 'pack' subcommand");
        let (myco_dir, mut myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        info!("Found myco.toml at: {}", myco_dir.display());

        debug!("Bumping version");
        let (name, version) = pack::bump_version(&myco_dir, &mut myco_toml, matches)?;
        info!("Version bumped to: {} v{}", name, version);

        if let Some(package) = myco_toml.package.as_ref() {
            if let Some(pre_pack) = &package.pre_pack {
                info!("Running pre-pack script: {}", pre_pack);
                let exit_code = run::run(&myco_toml, pre_pack, None)?;
                if exit_code != 0 {
                    return Err(MycoError::ScriptExecution {
                        message: format!("Pre-pack script exited with code {}", exit_code),
                    });
                }
                info!("Pre-pack script completed successfully");
            }

            env::set_current_dir(&myco_dir).map_err(|_e| MycoError::SetCurrentDirectory {
                dir: myco_dir.display().to_string(),
            })?;
            info!("Creating package archive");
            let integrity = pack::pack(package)?;
            info!("Package created successfully with integrity: {}", integrity);
            println!("Integrity: {}", integrity);
            println!("Packed {} v{}", name, version);
        } else {
            warn!("No package configuration found in myco.toml");
        }
    } else if let Some(matches) = matches.subcommand_matches("publish") {
        info!("Running 'publish' subcommand");
        let (myco_dir, myco_toml) = MycoToml::load_nearest(
            env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?,
        )?;
        info!("Found myco.toml at: {}", myco_dir.display());
        env::set_current_dir(&myco_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: myco_dir.display().to_string(),
        })?;
        let registry =
            matches
                .get_one::<String>("registry")
                .ok_or_else(|| MycoError::Internal {
                    message: "Registry argument is required".to_string(),
                })?;
        info!("Publishing to registry: {}", registry);
        publish::publish(&myco_toml, registry).map_err(|e| MycoError::Operation {
            message: e.to_string(),
        })?;
        info!("Package published successfully");
    } else if let Some(ws_matches) = matches.subcommand_matches("workspace") {
        info!("Running 'workspace' subcommand");
        if let Some(_list_matches) = ws_matches.subcommand_matches("list") {
            info!("Running 'workspace list' subcommand");
            let current_dir =
                env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
            debug!("Current directory: {}", current_dir.display());
            let workspace = workspace::Workspace::discover(current_dir)?;
            info!("Discovered workspace at: {}", workspace.root.display());
            info!("Found {} workspace members", workspace.members.len());

            println!("Workspace members:");
            for member in &workspace.members {
                let relative_path = member
                    .path
                    .strip_prefix(&workspace.root)
                    .unwrap_or(&member.path);
                println!("  {} ({})", member.name, relative_path.display());
            }
        } else if let Some(install_matches) = ws_matches.subcommand_matches("install") {
            info!("Running 'workspace install' subcommand");
            let current_dir =
                env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
            debug!("Current directory: {}", current_dir.display());
            let workspace = workspace::Workspace::discover(current_dir)?;
            info!("Discovered workspace at: {}", workspace.root.display());
            let save = install_matches.get_flag("save");
            debug!("Save lockfile: {}", save);
            info!(
                "Installing dependencies for {} workspace members",
                workspace.members.len()
            );
            workspace::install_workspace(&workspace, save)?;
            info!("Workspace dependencies installed successfully");
            println!("Installed workspace dependencies");
        } else if let Some(run_matches) = ws_matches.subcommand_matches("run") {
            info!("Running 'workspace run' subcommand");
            let current_dir =
                env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
            debug!("Current directory: {}", current_dir.display());
            let workspace = workspace::Workspace::discover(current_dir)?;
            info!("Discovered workspace at: {}", workspace.root.display());
            let script = run_matches.get_one::<String>("script").unwrap();
            let package_filters: Vec<String> = run_matches
                .get_many::<String>("package")
                .unwrap_or_default()
                .cloned()
                .collect();
            info!("Running script '{}' across workspace", script);
            if !package_filters.is_empty() {
                debug!("Package filters: {:?}", package_filters);
            }
            workspace::run_workspace_script(&workspace, script, &package_filters)?;
            info!("Workspace script execution completed");
        }
    }

    Ok(())
}
