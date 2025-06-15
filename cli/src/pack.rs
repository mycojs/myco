use crate::deps::write_new_package_version;
use crate::errors::MycoError;
use crate::integrity::calculate_integrity;
use crate::manifest::{MycoToml, PackageDefinition, PackageVersion};
use clap::ArgMatches;
use std::path::PathBuf;
use std::str::FromStr;
use util::zip::{zip_directory, ZipOptions};

pub fn pack(package: &PackageDefinition) -> Result<String, MycoError> {
    std::fs::create_dir_all("./dist").map_err(|e| MycoError::DirectoryCreation {
        path: "./dist".to_string(),
        source: e,
    })?;

    let zip_path = format!("./dist/{}.zip", package.version);
    let toml_path = format!("./dist/{}.toml", package.version);

    let output_dir = PathBuf::from("./dist/".to_string());
    std::fs::create_dir_all(output_dir).map_err(|e| MycoError::DirectoryCreation {
        path: "./dist".to_string(),
        source: e,
    })?;

    zip_directory(
        "./src",
        &zip_path,
        ZipOptions {
            strip_prefix: Some("./src".to_string()),
            apply_prefix: Some(package.name.to_string()),
            ..ZipOptions::default()
        },
    )
    .map_err(|e| MycoError::Internal {
        message: format!("Failed to create archive: {}", e),
    })?;

    let raw_toml = std::fs::read_to_string("./myco.toml").map_err(|e| MycoError::ReadFile {
        path: "./myco.toml".to_string(),
        source: e,
    })?;
    std::fs::write(toml_path, raw_toml).map_err(|e| MycoError::FileWrite {
        path: format!("./dist/{}.toml", package.version),
        source: e,
    })?;

    let zip_bytes = std::fs::read(&zip_path).map_err(|e| MycoError::ReadFile {
        path: zip_path,
        source: e,
    })?;
    Ok(calculate_integrity(&zip_bytes))
}

pub fn bump_version(
    myco_dir: &PathBuf,
    myco_toml: &mut MycoToml,
    matches: &ArgMatches,
) -> Result<(String, PackageVersion), MycoError> {
    let should_bump_major = matches.get_flag("next_major");
    let should_bump_minor = matches.get_flag("next_minor");
    let should_bump_patch = matches.get_flag("next_patch");

    let name = myco_toml
        .package
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or("<unnamed project>".to_string());
    let mut version = myco_toml
        .package
        .as_ref()
        .map(|p| p.version.clone())
        .unwrap_or_else(|| {
            PackageVersion::from_str("0.0.0")
                .map_err(|_| MycoError::PackageVersionDetermination)
                .unwrap_or(PackageVersion {
                    major: 0,
                    minor: 0,
                    patch: 0,
                    prerelease: None,
                })
        });

    if should_bump_patch || should_bump_minor || should_bump_major {
        if should_bump_major {
            version = version.next_major();
        } else if should_bump_minor {
            version = version.next_minor();
        } else if should_bump_patch {
            version = version.next_patch();
        }
    }

    write_new_package_version(&version, &myco_dir.join("myco.toml"))?;
    myco_toml
        .package
        .as_mut()
        .map(|p| p.version = version.clone());
    Ok((name, version))
}
