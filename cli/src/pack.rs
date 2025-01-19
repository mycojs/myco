use std::path::PathBuf;
use std::str::FromStr;
use clap::ArgMatches;
use util::zip::{zip_directory, ZipOptions};
use crate::deps::write_new_package_version;
use crate::manifest::{MycoToml, PackageDefinition, PackageVersion};
use crate::integrity::calculate_integrity;

pub fn pack(package: &PackageDefinition) -> String {
    std::fs::create_dir_all("./dist").expect("Failed to create dist directory");
    
    let zip_path = format!("./dist/{}.zip", package.name);
    let toml_path = format!("./dist/{}.toml", package.name);

    let output_dir = PathBuf::from("./dist/".to_string());
    std::fs::create_dir_all(output_dir).expect("Failed to create parent directory");

    zip_directory("./src", &zip_path, ZipOptions {
        strip_prefix: Some("./src".to_string()),
        apply_prefix: Some(format!("{}", package.name)),
        ..ZipOptions::default()
    }).expect("Failed to zip directory");

    let raw_toml = std::fs::read_to_string("./myco.toml").expect("Failed to read myco.toml");
    std::fs::write(toml_path, raw_toml).expect("Failed to write myco.toml");
    
    let zip_bytes = std::fs::read(&zip_path).expect("Failed to read zip file");
    calculate_integrity(&zip_bytes)
}

pub fn bump_version(myco_dir: &PathBuf, myco_toml: &mut MycoToml, matches: &ArgMatches) -> (String, PackageVersion) {
    let should_bump_major = matches.get_flag("next_major");
    let should_bump_minor = matches.get_flag("next_minor");
    let should_bump_patch = matches.get_flag("next_patch");

    let name = myco_toml.package.as_ref().map(|p| p.name.clone()).unwrap_or("<unnamed project>".to_string());
    let mut version = myco_toml.package.as_ref().map(|p| p.version.clone()).unwrap_or(PackageVersion::from_str("0.0.0").unwrap());
    if should_bump_patch || should_bump_minor || should_bump_major {
        if should_bump_major {
            version = version.next_major();
        } else if should_bump_minor {
            version = version.next_minor();
        } else if should_bump_patch {
            version = version.next_patch();
        }
    }
    write_new_package_version(&version, &myco_dir.join("myco.toml"));
    myco_toml.package.as_mut().map(|p| p.version = version.clone());
    (name, version)
}
