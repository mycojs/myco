use std::path::PathBuf;
use util::zip::{zip_directory, ZipOptions};

use crate::manifest::MycoToml;
use crate::run;

pub fn pack(myco_toml: &MycoToml) {
    if let Some(package) = myco_toml.package.as_ref() {
        if let Some(pre_pack) = &package.pre_pack {
            run::run(myco_toml, pre_pack);
        }
        std::fs::create_dir_all("./dist").expect("Failed to create dist directory");

        let version_number = myco_toml.package.as_ref().unwrap().version.clone();
        let zip_name = format!("{}-{}.zip", package.name, version_number);
        let mut output_dir = PathBuf::from(format!("./dist/{}", zip_name));
        output_dir.pop();
        std::fs::create_dir_all(output_dir).expect("Failed to create parent directory");

        zip_directory("./src", format!("./dist/{}", zip_name), ZipOptions {
            strip_prefix: Some("./src".to_string()),
            apply_prefix: Some(format!("{}", package.name)),
            ..ZipOptions::default()
        }).expect("Failed to zip directory");

        let raw_toml = std::fs::read_to_string("./myco.toml").expect("Failed to read myco.toml");
        let toml_name = format!("{}-{}.toml", package.name, version_number);
        std::fs::write(format!("./dist/{}", toml_name), raw_toml).expect("Failed to write myco.toml");
    } else {
        panic!("No package definition found");
    }
}
