use std::env;

use clap::{arg, command, Command};

mod loader;
mod capabilities;
mod init;
mod run;
mod myco_toml;

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
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        if let Some(file) = matches.get_one::<String>("file") {
            run::run_file(file);
        } else {
            run::run();
        }
    }

    if let Some(matches) = matches.subcommand_matches("init") {
        if let Some(dir) = matches.get_one::<String>("dir") {
            init::init(dir.to_string());
        }
    }
}
