use changes::DepsChange;
pub use changes::{write_deps_changes, write_new_package_version};

pub use lockfile::LockFileDiff;

use crate::errors::MycoError;
use crate::integrity::calculate_integrity;
use crate::manifest::{Location, MycoToml, PackageName};

mod changes;
mod lockfile;
mod registry;
mod resolver;
mod tsconfig;

const MYCO_DTS: &str = include_str!("../../../runtime/.myco/myco.d.ts");

pub fn install(myco_toml: MycoToml, save: bool) -> Result<(), MycoError> {
    let registries = if let Some(registries) = myco_toml.registries.clone() {
        registries.into_values().collect()
    } else {
        vec![]
    };
    let mut resolver = resolver::Resolver::new(registries);
    let resolved_deps = tokio::runtime::Runtime::new()
        .map_err(|e| MycoError::TokioRuntime { source: e })?
        .block_on(resolver.generate_lockfile(&myco_toml));

    match resolved_deps {
        Ok(new_lockfile) => {
            if save {
                new_lockfile.save()?;
                install_from_lockfile(&new_lockfile, &myco_toml)?;
            } else {
                // Verify existing lockfile matches
                let existing_lockfile = lockfile::LockFile::load();
                match existing_lockfile {
                    Ok(existing_lockfile) => {
                        let lockfiles_match = existing_lockfile.package == new_lockfile.package;
                        if !lockfiles_match {
                            return Err(MycoError::LockfileMismatch {
                                diff: existing_lockfile.diff(&new_lockfile),
                            });
                        }
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

    Ok(())
}

fn install_from_lockfile(
    lockfile: &lockfile::LockFile,
    myco_toml: &MycoToml,
) -> Result<(), MycoError> {
    // TODO: Make this more efficient by only downloading the files we don't have yet
    std::fs::remove_dir_all("vendor").unwrap_or(());

    for version in &lockfile.package {
        let zip_file = match &version.pack_url {
            Location::Url(url) => {
                if url.scheme() == "file" {
                    std::fs::read(url.path()).map_err(|e| MycoError::PackageDownload {
                        url: url.to_string(),
                        source: Box::new(e),
                    })?
                } else {
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
                std::fs::read(path).map_err(|e| MycoError::PackageDownload {
                    url: path.display().to_string(),
                    source: Box::new(e),
                })?
            }
        };

        // Validate integrity
        let calculated_integrity = calculate_integrity(&zip_file);
        if calculated_integrity != version.integrity {
            return Err(MycoError::IntegrityMismatch {
                package: version.name.to_string(),
                expected: version.integrity.clone(),
                actual: calculated_integrity,
            });
        }

        let mut zip_archive = zip::ZipArchive::new(std::io::Cursor::new(zip_file))
            .map_err(|e| MycoError::PackageExtraction { source: e })?;

        // Iterate through the entries in the ZIP archive
        for i in 0..zip_archive.len() {
            let mut entry = zip_archive
                .by_index(i)
                .map_err(|e| MycoError::PackageExtraction { source: e })?;
            let out_path = std::path::PathBuf::from("./vendor").join(entry.name());

            if entry.is_dir() {
                // Create a new directory if the entry is a directory
                std::fs::create_dir_all(&out_path)
                    .map_err(|e| MycoError::VendorDirCreation { source: e })?;
            } else {
                // Create a new file and write the entry's contents to it
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
    }

    // Create .myco directory and myco.d.ts file
    std::fs::create_dir_all(".myco").map_err(|e| MycoError::DirectoryCreation {
        path: ".myco".to_string(),
        source: e,
    })?;

    std::fs::write(".myco/myco.d.ts", MYCO_DTS).map_err(|e| MycoError::FileWrite {
        path: ".myco/myco.d.ts".to_string(),
        source: e,
    })?;

    // Create tsconfig.json dynamically based on myco.toml configuration
    let tsconfig_content = tsconfig::generate_tsconfig_json(&myco_toml)?;

    std::fs::write("tsconfig.json", tsconfig_content).map_err(|e| MycoError::FileWrite {
        path: "tsconfig.json".to_string(),
        source: e,
    })?;

    Ok(())
}

pub fn add(myco_toml: &MycoToml, package: PackageName) -> Result<Vec<DepsChange>, MycoError> {
    if let Some(registries) = myco_toml.registries.clone() {
        let mut resolver = resolver::Resolver::new(registries.into_values().collect());
        let resolved_package = tokio::runtime::Runtime::new()
            .map_err(|e| MycoError::TokioRuntime { source: e })?
            .block_on(resolver.resolve_package(&package));

        match resolved_package {
            Ok(Some(package)) => {
                let max_version =
                    package
                        .versions
                        .iter()
                        .max()
                        .ok_or_else(|| MycoError::PackageNotFound {
                            package: package.name.to_string(),
                        })?;
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
    let deps = myco_toml.clone_deps();
    let had_package = deps.contains_key(&package);
    if had_package {
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
        add(myco_toml, package)
    } else {
        let deps = myco_toml.clone_deps();
        let mut changes = vec![];
        for dep in deps.into_keys() {
            changes.append(&mut add(myco_toml, dep)?);
        }
        Ok(changes)
    }
}

pub fn list(myco_toml: MycoToml) {
    let deps = myco_toml.into_deps();
    for (name, version) in deps {
        println!("{} = \"{}\"", name, version);
    }
}
