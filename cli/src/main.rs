use std::env;

pub use anyhow::Error as AnyError;
use clap::{arg, command, Command};

pub use run::*;

use crate::deps::write_deps_changes;
use crate::manifest::{MycoToml, PackageName};

mod init;
mod run;
mod manifest;
mod deps;
mod pack;
mod integrity;
mod publish;

fn main() {
    let matches = command!()
        .subcommand(
            Command::new("run")
                .about("Run a JS/TS file in Myco")
                .arg(arg!([script] "The name of the script to run, either a name from myco.toml's [run] block or a relative path. Defaults to 'default'."))
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
        .arg_required_else_help(true)
        .args_conflicts_with_subcommands(true)
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        let default = &"default".to_string();
        let script = matches.get_one::<String>("script").unwrap_or(default);
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(myco_dir).unwrap();
        run::run(&myco_toml, script);
    } else if let Some(matches) = matches.subcommand_matches("init") {
        if let Some(dir) = matches.get_one::<String>("dir") {
            init::init(dir.to_string());

            // Sync changes
            let (_, myco_toml) = MycoToml::load_nearest(std::path::PathBuf::from(dir)).unwrap();
            deps::install(myco_toml, true);
            println!("Initialized {}", dir);
        }
    } else if let Some(matches) = matches.subcommand_matches("install") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(myco_dir).unwrap();
        let save = matches.get_flag("save");
        deps::install(myco_toml, save);
        println!("Installed dependencies");
    } else if let Some(matches) = matches.subcommand_matches("add") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(&myco_dir).unwrap();
        let package = matches.get_one::<String>("package").unwrap();
        let changes = deps::add(&myco_toml, PackageName::from_str(package).unwrap());
        write_deps_changes(&changes, &myco_dir.join("myco.toml"));

        // Sync changes
        let (_, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        deps::install(myco_toml, true);
        println!("Added {}", package);
    } else if let Some(matches) = matches.subcommand_matches("remove") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(&myco_dir).unwrap();
        let package = matches.get_one::<String>("package").unwrap();
        let changes = deps::remove(&myco_toml, PackageName::from_str(package).unwrap());
        write_deps_changes(&changes, &myco_dir.join("myco.toml"));

        // Sync changes
        let (_, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        deps::install(myco_toml, true);
        println!("Removed {}", package);
    } else if let Some(matches) = matches.subcommand_matches("update") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(&myco_dir).unwrap();
        let package = matches.get_one::<String>("package");
        let changes = deps::update(&myco_toml, package.map(|s| PackageName::from_str(s).unwrap()));
        write_deps_changes(&changes, &myco_dir.join("myco.toml"));

        // Sync changes
        let (_, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        deps::install(myco_toml, true);
        println!("Updated {}", package.unwrap_or(&"all dependencies".to_string()));
    } else if let Some(_) = matches.subcommand_matches("list") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(myco_dir).unwrap();
        deps::list(myco_toml);
    } else if let Some(matches) = matches.subcommand_matches("pack") {
        let (myco_dir, mut myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();

        let (name, version) = pack::bump_version(&myco_dir, &mut myco_toml, matches);

        if let Some(package) = myco_toml.package.as_ref() {
            if let Some(pre_pack) = &package.pre_pack {
                run::run(&myco_toml, pre_pack);
            }

            env::set_current_dir(&myco_dir).unwrap();
            let integrity = pack::pack(package);
            println!("Integrity: {}", integrity);
            println!("Packed {} v{}", name, version);
        }
    } else if let Some(matches) = matches.subcommand_matches("publish") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(&myco_dir).unwrap();
        let registry = matches.get_one::<String>("registry").unwrap();
        if let Err(e) = publish::publish(&myco_toml, registry) {
            eprintln!("Failed to publish: {}", e);
            std::process::exit(1);
        }
    }
}
