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
            deps::install(myco_toml);
        }
    } else if let Some(_) = matches.subcommand_matches("install") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(myco_dir).unwrap();
        deps::install(myco_toml);
    } else if let Some(matches) = matches.subcommand_matches("add") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(&myco_dir).unwrap();
        let package = matches.get_one::<String>("package").unwrap();
        let changes = deps::add(&myco_toml, PackageName::from_str(package).unwrap());
        write_deps_changes(&changes, &myco_dir.join("myco.toml"));

        // Sync changes
        let (_, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        deps::install(myco_toml);
    } else if let Some(matches) = matches.subcommand_matches("remove") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(&myco_dir).unwrap();
        let package = matches.get_one::<String>("package").unwrap();
        let changes = deps::remove(&myco_toml, PackageName::from_str(package).unwrap());
        write_deps_changes(&changes, &myco_dir.join("myco.toml"));

        // Sync changes
        let (_, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        deps::install(myco_toml);
    } else if let Some(matches) = matches.subcommand_matches("update") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(&myco_dir).unwrap();
        let package = matches.get_one::<String>("package");
        let changes = deps::update(&myco_toml, package.map(|s| PackageName::from_str(s).unwrap()));
        write_deps_changes(&changes, &myco_dir.join("myco.toml"));

        // Sync changes
        let (_, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        deps::install(myco_toml);
    } else if let Some(_) = matches.subcommand_matches("list") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(myco_dir).unwrap();
        deps::list(myco_toml);
    } else if let Some(_) = matches.subcommand_matches("pack") {
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(myco_dir).unwrap();
        pack::pack(&myco_toml);
    }
}
