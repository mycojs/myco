use crate::deps::write_new_package_version;
use crate::errors::MycoError;
use crate::integrity::calculate_integrity;
use crate::manifest::{MycoToml, PackageDefinition, PackageVersion};
use clap::ArgMatches;
use log::{debug, info};
use std::path::PathBuf;
use std::str::FromStr;
use util::zip::{zip_directory, ZipOptions};

pub fn pack(package: &PackageDefinition) -> Result<String, MycoError> {
    info!(
        "Packing package '{}' version {}",
        package.name, package.version
    );

    debug!("Creating dist directory");
    std::fs::create_dir_all("./dist").map_err(|e| MycoError::DirectoryCreation {
        path: "./dist".to_string(),
        source: e,
    })?;

    let zip_path = format!("./dist/{}.zip", package.version);
    let toml_path = format!("./dist/{}.toml", package.version);
    debug!("Archive will be created at: {}", zip_path);
    debug!("Manifest will be copied to: {}", toml_path);

    let output_dir = PathBuf::from("./dist/".to_string());
    std::fs::create_dir_all(output_dir).map_err(|e| MycoError::DirectoryCreation {
        path: "./dist".to_string(),
        source: e,
    })?;

    info!("Creating package archive from ./src directory");
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
    debug!("Successfully created package archive");

    debug!("Reading myco.toml for distribution");
    let raw_toml = std::fs::read_to_string("./myco.toml").map_err(|e| MycoError::ReadFile {
        path: "./myco.toml".to_string(),
        source: e,
    })?;
    debug!("Writing manifest to distribution directory");
    std::fs::write(toml_path, raw_toml).map_err(|e| MycoError::FileWrite {
        path: format!("./dist/{}.toml", package.version),
        source: e,
    })?;

    info!("Calculating package integrity");
    let zip_bytes = std::fs::read(&zip_path).map_err(|e| MycoError::ReadFile {
        path: zip_path,
        source: e,
    })?;
    let integrity = calculate_integrity(&zip_bytes);
    info!("Package packed successfully with integrity: {}", integrity);
    Ok(integrity)
}

pub fn bump_version(
    myco_dir: &PathBuf,
    myco_toml: &mut MycoToml,
    matches: &ArgMatches,
) -> Result<(String, PackageVersion), MycoError> {
    debug!("Processing version bump options");
    let should_bump_major = matches.get_flag("next_major");
    let should_bump_minor = matches.get_flag("next_minor");
    let should_bump_patch = matches.get_flag("next_patch");

    debug!(
        "Version bump flags - major: {}, minor: {}, patch: {}",
        should_bump_major, should_bump_minor, should_bump_patch
    );

    let name = myco_toml
        .package
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or("<unnamed project>".to_string());
    debug!("Package name: {}", name);

    let mut version = myco_toml
        .package
        .as_ref()
        .map(|p| p.version.clone())
        .unwrap_or_else(|| {
            debug!("No version found in package, defaulting to 0.0.0");
            PackageVersion::from_str("0.0.0")
                .map_err(|_| MycoError::PackageVersionDetermination)
                .unwrap_or(PackageVersion {
                    major: 0,
                    minor: 0,
                    patch: 0,
                    prerelease: None,
                })
        });

    debug!("Current version: {}", version);

    if should_bump_patch || should_bump_minor || should_bump_major {
        let old_version = version.clone();
        if should_bump_major {
            version = version.next_major();
            info!("Bumping major version: {} -> {}", old_version, version);
        } else if should_bump_minor {
            version = version.next_minor();
            info!("Bumping minor version: {} -> {}", old_version, version);
        } else if should_bump_patch {
            version = version.next_patch();
            info!("Bumping patch version: {} -> {}", old_version, version);
        }
    } else {
        debug!(
            "No version bump requested, keeping current version: {}",
            version
        );
    }

    debug!("Writing new version to myco.toml");
    write_new_package_version(&version, &myco_dir.join("myco.toml"))?;
    myco_toml
        .package
        .as_mut()
        .map(|p| p.version = version.clone());

    info!("Version bump completed: {} v{}", name, version);
    Ok((name, version))
}
