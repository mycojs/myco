use std::{env, fs};

pub use anyhow::Error as AnyError;
use clap::{arg, command, Command, crate_description, crate_version};

pub use run::*;

use crate::myco_toml::MycoToml;

mod init;
mod run;
mod myco_toml;

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
        .arg_required_else_help(true)
        .args_conflicts_with_subcommands(true)
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        let default = &"default".to_string();
        let script = matches.get_one::<String>("script").unwrap_or(default);
        let myco_toml = fs::read_to_string("myco.toml").unwrap();
        let myco_toml = MycoToml::from_str(&myco_toml).unwrap();
        run::run(myco_toml, script);
    } else if let Some(matches) = matches.subcommand_matches("init") {
        if let Some(dir) = matches.get_one::<String>("dir") {
            init::init(dir.to_string());
        }
    }
}
