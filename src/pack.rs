use util::zip::{zip_directory, ZipOptions};
use crate::myco_toml::MycoToml;

pub fn pack(myco_toml: &MycoToml) {
    if let Some(package) = myco_toml.package.as_ref() {
        std::fs::create_dir_all("./dist").expect("Failed to create dist directory");

        let version_number = myco_toml.package.as_ref().unwrap().version.clone();
        let zip_name = format!("{}-{}.zip", package.name, version_number);
        zip_directory("./src", format!("./dist/{}", zip_name), ZipOptions {
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
