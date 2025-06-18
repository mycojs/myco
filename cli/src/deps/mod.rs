use changes::DepsChange;
pub use changes::{write_deps_changes, write_new_package_version};

pub use lockfile::LockFileDiff;
use log::{debug, error, info, warn};

use crate::errors::MycoError;
use crate::integrity::calculate_integrity;
use crate::manifest::{Location, MycoToml, PackageName};

mod changes;
mod lockfile;
mod registry;
mod resolver;
pub mod tsconfig;

const MYCO_DTS: &str = include_str!("../../../runtime/.myco/myco.d.ts");

pub fn install(myco_toml: MycoToml, save: bool) -> Result<(), MycoError> {
    info!("Installing dependencies (save: {})", save);

    let registries = if let Some(registries) = myco_toml.registries.clone() {
        debug!("Found {} registries configured", registries.len());
        registries.into_values().collect()
    } else {
        debug!("No registries configured");
        vec![]
    };

    let mut resolver = resolver::Resolver::new(registries);
    info!("Resolving dependencies from myco.toml");
    let resolved_deps = tokio::runtime::Runtime::new()
        .map_err(|e| MycoError::TokioRuntime { source: e })?
        .block_on(resolver.generate_lockfile(&myco_toml));

    match resolved_deps {
        Ok(new_lockfile) => {
            info!(
                "Successfully resolved {} packages",
                new_lockfile.package.len()
            );
            if save {
                info!("Saving lockfile");
                new_lockfile.save()?;
                install_from_lockfile(&new_lockfile, &myco_toml)?;
            } else {
                // Verify existing lockfile matches
                debug!("Verifying existing lockfile matches resolved dependencies");
                let existing_lockfile = lockfile::LockFile::load();
                match existing_lockfile {
                    Ok(existing_lockfile) => {
                        let lockfiles_match = existing_lockfile.package == new_lockfile.package;
                        if !lockfiles_match {
                            warn!("Lockfile does not match resolved dependencies");
                            return Err(MycoError::LockfileMismatch {
                                diff: existing_lockfile.diff(&new_lockfile),
                            });
                        }
                        debug!("Lockfile matches, proceeding with installation");
                        install_from_lockfile(&existing_lockfile, &myco_toml)?;
                    }
                    Err(_) => {
                        return Err(MycoError::LockfileLoad);
                    }
                }
            }
        }
        Err(e) => {
            return Err(MycoError::DependencyResolution {
                message: format!("{:?}", e),
            });
        }
    }

    info!("Dependency installation completed successfully");
    Ok(())
}

fn install_from_lockfile(
    lockfile: &lockfile::LockFile,
    myco_toml: &MycoToml,
) -> Result<(), MycoError> {
    info!(
        "Installing {} packages from lockfile",
        lockfile.package.len()
    );

    // TODO: Make this more efficient by only downloading the files we don't have yet
    debug!("Removing existing vendor directory");
    std::fs::remove_dir_all("vendor").unwrap_or(());

    for version in &lockfile.package {
        info!("Installing package: {} v{}", version.name, version.version);
        debug!("Package URL: {:?}", version.pack_url);

        let zip_file = match &version.pack_url {
            Location::Url(url) => {
                if url.scheme() == "file" {
                    debug!("Reading package from local file: {}", url.path());
                    std::fs::read(url.path()).map_err(|e| MycoError::PackageDownload {
                        url: url.to_string(),
                        source: Box::new(e),
                    })?
                } else {
                    info!("Downloading package from: {}", url);
                    reqwest::blocking::get(url.clone())
                        .and_then(|response| response.bytes())
                        .map_err(|e| MycoError::PackageDownload {
                            url: url.to_string(),
                            source: Box::new(e),
                        })?
                        .to_vec()
                }
            }
            Location::Path { path } => {
                debug!("Reading package from path: {}", path.display());
                std::fs::read(path).map_err(|e| MycoError::PackageDownload {
                    url: path.display().to_string(),
                    source: Box::new(e),
                })?
            }
        };

        // Validate integrity
        debug!("Validating package integrity");
        let calculated_integrity = calculate_integrity(&zip_file);
        if calculated_integrity != version.integrity {
            return Err(MycoError::IntegrityMismatch {
                package: version.name.to_string(),
                expected: version.integrity.clone(),
                actual: calculated_integrity,
            });
        }
        debug!("Package integrity verified");

        debug!("Extracting package archive");
        let mut zip_archive = zip::ZipArchive::new(std::io::Cursor::new(zip_file))
            .map_err(|e| MycoError::PackageExtraction { source: e })?;

        // Iterate through the entries in the ZIP archive
        debug!("Archive contains {} files", zip_archive.len());
        for i in 0..zip_archive.len() {
            let mut entry = zip_archive
                .by_index(i)
                .map_err(|e| MycoError::PackageExtraction { source: e })?;
            let out_path = std::path::PathBuf::from("./vendor").join(entry.name());

            if entry.is_dir() {
                // Create a new directory if the entry is a directory
                debug!("Creating directory: {}", out_path.display());
                std::fs::create_dir_all(&out_path)
                    .map_err(|e| MycoError::VendorDirCreation { source: e })?;
            } else {
                // Create a new file and write the entry's contents to it
                debug!("Extracting file: {}", out_path.display());
                let mut out_file =
                    std::fs::File::create(&out_path).map_err(|e| MycoError::FileWrite {
                        path: out_path.display().to_string(),
                        source: e,
                    })?;
                std::io::copy(&mut entry, &mut out_file).map_err(|e| MycoError::FileWrite {
                    path: out_path.display().to_string(),
                    source: e,
                })?;
            }
        }
        debug!("Successfully extracted package: {}", version.name);
    }

    // Create .myco directory and myco.d.ts file
    info!("Setting up development environment files");
    debug!("Creating .myco directory");
    std::fs::create_dir_all(".myco").map_err(|e| MycoError::DirectoryCreation {
        path: ".myco".to_string(),
        source: e,
    })?;

    debug!("Writing .myco/myco.d.ts");
    std::fs::write(".myco/myco.d.ts", MYCO_DTS).map_err(|e| MycoError::FileWrite {
        path: ".myco/myco.d.ts".to_string(),
        source: e,
    })?;

    // Create tsconfig.json dynamically based on myco.toml configuration
    debug!("Generating tsconfig.json");
    let tsconfig_content = tsconfig::generate_tsconfig_json(myco_toml)?;

    std::fs::write("tsconfig.json", tsconfig_content).map_err(|e| MycoError::FileWrite {
        path: "tsconfig.json".to_string(),
        source: e,
    })?;

    info!("Successfully installed all packages and configured development environment");
    Ok(())
}

pub fn add(myco_toml: &MycoToml, package: PackageName) -> Result<Vec<DepsChange>, MycoError> {
    info!("Adding package: {}", package);

    if let Some(registries) = myco_toml.registries.clone() {
        debug!("Resolving package from {} registries", registries.len());
        let mut resolver = resolver::Resolver::new(registries.into_values().collect());
        let resolved_package = tokio::runtime::Runtime::new()
            .map_err(|e| MycoError::TokioRuntime { source: e })?
            .block_on(resolver.resolve_package(&package));

        match resolved_package {
            Ok(Some(package)) => {
                debug!(
                    "Found package '{}' with {} versions available",
                    package.name,
                    package.versions.len()
                );
                let max_version =
                    package
                        .versions
                        .iter()
                        .max()
                        .ok_or_else(|| MycoError::PackageNotFound {
                            package: package.name.to_string(),
                        })?;
                info!(
                    "Selected latest version: {} v{}",
                    package.name, max_version.version
                );
                Ok(vec![DepsChange::Set(
                    package.name,
                    max_version.version.clone(),
                )])
            }
            Ok(None) => Err(MycoError::PackageNotFound {
                package: package.to_string(),
            }),
            Err(e) => Err(MycoError::DependencyResolution {
                message: format!("{:?}", e),
            }),
        }
    } else {
        Err(MycoError::NoRegistries)
    }
}

pub fn remove(myco_toml: &MycoToml, package: PackageName) -> Result<Vec<DepsChange>, MycoError> {
    info!("Removing package: {}", package);

    let deps = myco_toml.clone_deps();
    debug!("Current dependencies: {} packages", deps.len());
    let had_package = deps.contains_key(&package);

    if had_package {
        info!("Package '{}' found and will be removed", package);
        Ok(vec![DepsChange::Remove(package)])
    } else {
        Err(MycoError::PackageNotFound {
            package: package.to_string(),
        })
    }
}

pub fn update(
    myco_toml: &MycoToml,
    package: Option<PackageName>,
) -> Result<Vec<DepsChange>, MycoError> {
    if let Some(package) = package {
        info!("Updating specific package: {}", package);
        add(myco_toml, package)
    } else {
        info!("Updating all dependencies");
        let deps = myco_toml.clone_deps();
        debug!("Found {} dependencies to update", deps.len());
        let mut changes = vec![];
        for dep in deps.into_keys() {
            debug!("Updating dependency: {}", dep);
            changes.append(&mut add(myco_toml, dep)?);
        }
        info!("Successfully updated {} dependencies", changes.len());
        Ok(changes)
    }
}

pub fn list(myco_toml: MycoToml) {
    let deps = myco_toml.into_deps();
    info!("Listing {} dependencies", deps.len());

    if deps.is_empty() {
        println!("No dependencies found in myco.toml");
        return;
    }

    for (name, version) in deps {
        println!("{} = \"{}\"", name, version);
    }
}
