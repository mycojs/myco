use std::{env, fs};

pub use anyhow::Error as AnyError;
use clap::{arg, command, Command};

pub use run::*;

use crate::myco_toml::MycoToml;

mod init;
mod run;
mod myco_toml;
mod transpile;
mod check;

fn main() {
    let matches = command!()
        .subcommand(
            Command::new("run")
                .about("Run a JS/TS file in Myco")
                .arg(arg!([file] "The path to the file to run. If none is provided, Myco will look for a Myco.toml file to parse."))
        )
        .subcommand(
            Command::new("init")
                .about("Initialize a new Myco project")
                .arg(arg!(<dir> "The directory to initialize"))
        )
        .subcommand(
            Command::new("check")
                .about("Check a Myco project for errors")
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        if let Some(file) = matches.get_one::<String>("file") {
            run_file(file);
        } else {
            run();
        }
    }

    if let Some(matches) = matches.subcommand_matches("init") {
        if let Some(dir) = matches.get_one::<String>("dir") {
            init::init(dir.to_string());
        }
    }

    if let Some(_) = matches.subcommand_matches("check") {
        check();
    }
}

pub fn run() {
    let myco_toml = fs::read_to_string("myco.toml").unwrap();
    let myco_toml = MycoToml::from_string(&myco_toml).unwrap();
    run_file(&myco_toml.package.main)
}

pub fn check() {
    let myco_toml = fs::read_to_string("myco.toml").unwrap();
    let myco_toml = MycoToml::from_string(&myco_toml).unwrap();
    check::check(myco_toml)
}
