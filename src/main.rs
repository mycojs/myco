use std::{env};

pub use anyhow::Error as AnyError;
use clap::{arg, command, Command};

pub use run::*;

use crate::myco_toml::MycoToml;

mod init;
mod run;
mod myco_toml;
mod deps;

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
            Command::new("deps")
                .about("Manage dependencies in the project")
                .subcommand(
                    Command::new("fetch")
                        .about("Fetch dependencies")
                )
        )
        .arg_required_else_help(true)
        .args_conflicts_with_subcommands(true)
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        let default = &"default".to_string();
        let script = matches.get_one::<String>("script").unwrap_or(default);
        let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
        env::set_current_dir(myco_dir).unwrap();
        run::run(myco_toml, script);
    } else if let Some(matches) = matches.subcommand_matches("init") {
        if let Some(dir) = matches.get_one::<String>("dir") {
            init::init(dir.to_string());
        }
    } else if let Some(matches) = matches.subcommand_matches("deps") {
        if let Some(_) = matches.subcommand_matches("fetch") {
            let (myco_dir, myco_toml) = MycoToml::load_nearest(env::current_dir().unwrap()).unwrap();
            env::set_current_dir(myco_dir).unwrap();
            deps::fetch(myco_toml);
        }
    }
}
