use log::{debug, error, info};
use std::fs;
use std::path::PathBuf;

use crate::errors::MycoError;
use crate::manifest::{Location, MycoToml, PackageDefinition, PackageName};
use crate::pack::pack;

pub fn publish(myco_toml: &MycoToml, registry_name: &str) -> Result<(), MycoError> {
    info!("Publishing package to registry: {}", registry_name);

    // Get the package definition
    let package = myco_toml
        .package
        .as_ref()
        .ok_or_else(|| MycoError::NoPackageDefinition)?;

    info!("Publishing package: {} v{}", package.name, package.version);

    // Get the registry location
    let registries = myco_toml
        .registries
        .as_ref()
        .ok_or_else(|| MycoError::NoRegistries)?;

    debug!(
        "Available registries: {:?}",
        registries.keys().collect::<Vec<_>>()
    );
    let registry_location =
        registries
            .get(registry_name)
            .ok_or_else(|| MycoError::RegistryNotFound {
                name: registry_name.to_string(),
            })?;

    // Load and parse the registry toml
    match registry_location {
        Location::Path { path } => {
            info!("Publishing to local registry at: {}", path.display());

            // Pack the package first
            info!("Packing package for distribution");
            let integrity = pack(package)?;

            // Find or create the namespace section
            info!("Updating registry index");
            update_registry(path, package, &integrity)?;

            // Copy the package files
            let package_location = registry_location
                .join(&format!("{}/", package.name))?
                .as_path()
                .ok_or_else(|| MycoError::InvalidRegistryFormat {
                    message: "Registry location is not a path".to_string(),
                })?;
            info!("Copying package files to: {}", package_location.display());
            copy_package_files(&package_location)?;

            info!(
                "Successfully published {} v{}",
                package.name, package.version
            );
            println!("Published {} v{}", package.name, package.version);
            Ok(())
        }
        Location::Url(_) => Err(MycoError::UrlRegistryNotSupported),
    }
}

fn update_registry(
    path: &PathBuf,
    package: &PackageDefinition,
    integrity: &str,
) -> Result<(), MycoError> {
    debug!("Updating registry at: {}", path.display());
    debug!(
        "Package: {} v{}, integrity: {}",
        package.name, package.version, integrity
    );

    let registry_content = fs::read_to_string(path).map_err(|e| MycoError::ReadFile {
        path: path.display().to_string(),
        source: e,
    })?;
    debug!("Successfully read registry file");

    let mut registry_doc: toml_edit::Document = registry_content
        .parse()
        .map_err(|e| MycoError::RegistryParse { source: e })?;
    debug!("Successfully parsed registry TOML");

    // Parse the package name to get namespace information
    let package_name = PackageName::from_str(&package.name)?;
    debug!("Parsed package name: {:?}", package_name);

    // Get or create the root namespace array
    let namespace =
        registry_doc["namespace"].or_insert(toml_edit::Item::ArrayOfTables(Default::default()));
    let namespace =
        namespace
            .as_array_of_tables_mut()
            .ok_or_else(|| MycoError::InvalidRegistryFormat {
                message: "namespace should be an array of tables".to_string(),
            })?;

    // Find or create the correct namespace entry
    let ns_name = package_name.namespaces_to_string();
    debug!("Looking for namespace: {}", ns_name);
    let mut ns_entry = namespace
        .iter_mut()
        .find(|ns| ns["name"].as_str() == Some(&ns_name));

    if ns_entry.is_none() {
        debug!("Creating new namespace: {}", ns_name);
        let mut new_ns = toml_edit::Table::new();
        new_ns.insert("name", toml_edit::value(ns_name));
        namespace.push(new_ns);
        ns_entry = namespace.get_mut(namespace.len() - 1);
    } else {
        debug!("Found existing namespace: {}", ns_name);
    }

    let ns_entry = ns_entry.unwrap();

    // Get or create the package array
    let packages =
        ns_entry["package"].or_insert(toml_edit::Item::ArrayOfTables(Default::default()));
    let packages =
        packages
            .as_array_of_tables_mut()
            .ok_or_else(|| MycoError::InvalidRegistryFormat {
                message: "package should be an array of tables".to_string(),
            })?;

    // Find or create the package entry
    debug!("Looking for package: {}", package.name);
    let mut pkg_entry = packages
        .iter_mut()
        .find(|p| p["name"].as_str() == Some(&package.name));

    if pkg_entry.is_none() {
        debug!("Creating new package entry: {}", package.name);
        let mut new_pkg = toml_edit::Table::new();
        new_pkg.insert("name", toml_edit::value(&package.name));
        new_pkg.insert(
            "versions",
            toml_edit::Item::ArrayOfTables(Default::default()),
        );
        packages.push(new_pkg);
        pkg_entry = packages.get_mut(packages.len() - 1);
    } else {
        debug!("Found existing package: {}", package.name);
    }

    let pkg_entry = pkg_entry.unwrap();

    // Get or update the versions array
    let versions = pkg_entry["versions"]
        .as_value_mut()
        .ok_or_else(|| MycoError::InvalidRegistryFormat {
            message: "versions should be an inline array".to_string(),
        })?
        .as_array_mut()
        .ok_or_else(|| MycoError::InvalidRegistryFormat {
            message: "versions should be an inline array".to_string(),
        })?;
    let version_exists = versions
        .iter_mut()
        .map(|v| v.as_inline_table_mut())
        .filter(|v| v.is_some())
        .any(|v| v.unwrap()["version"].as_str() == Some(&package.version.to_string()));

    // Check if version already exists
    if version_exists {
        return Err(MycoError::VersionExists {
            version: package.version.to_string(),
        });
    }

    // Add the new version entry
    debug!("Adding new version entry: {}", package.version);
    let mut version_entry = toml_edit::InlineTable::new();
    version_entry.insert("version", package.version.to_string().into());
    version_entry.insert("integrity", integrity.into());
    versions.push(version_entry);
    decorate_inline_tables_array(versions);

    // Write the updated registry back to disk
    debug!("Writing updated registry to disk");
    fs::write(path, registry_doc.to_string()).map_err(|e| MycoError::FileWrite {
        path: path.display().to_string(),
        source: e,
    })?;

    info!(
        "Successfully updated registry with {} v{}",
        package.name, package.version
    );
    Ok(())
}

fn copy_package_files(path: &PathBuf) -> Result<(), MycoError> {
    info!("Copying package files to: {}", path.display());
    println!("Copying package files to {}", path.display());

    // Create the target directory if it doesn't exist
    debug!("Creating target directory: {}", path.display());
    fs::create_dir_all(path).map_err(|e| MycoError::DirectoryCreation {
        path: path.display().to_string(),
        source: e,
    })?;

    // Copy the package's dist folder contents to the target path
    let dist_path = PathBuf::from("dist");
    if !dist_path.exists() {
        return Err(MycoError::DistDirectoryNotFound);
    }

    debug!("Reading distribution directory: {}", dist_path.display());
    let entries: Vec<_> = fs::read_dir(&dist_path)
        .map_err(|e| MycoError::ReadFile {
            path: dist_path.display().to_string(),
            source: e,
        })?
        .collect();

    info!(
        "Copying {} files from distribution directory",
        entries.len()
    );
    for entry_result in entries {
        let entry = entry_result.map_err(|e| MycoError::ReadFile {
            path: dist_path.display().to_string(),
            source: e,
        })?;
        let target_path = path.join(entry.file_name());
        debug!(
            "Copying {} to {}",
            entry.path().display(),
            target_path.display()
        );
        fs::copy(entry.path(), target_path).map_err(|e| MycoError::FileWrite {
            path: path.display().to_string(),
            source: e,
        })?;
    }

    info!("Successfully copied all package files");
    Ok(())
}

fn decorate_inline_tables_array(array: &mut toml_edit::Array) {
    for value in array.iter_mut() {
        let decor = value.decor_mut();
        *decor = toml_edit::Decor::new("\n    ", "");
    }
    array.set_trailing_comma(true);
    array.set_trailing("\n");
}
