use changes::DepsChange;
pub use changes::{write_deps_changes, write_new_package_version};

use crate::manifest::{Location, MycoToml, PackageName};
use crate::integrity::calculate_integrity;

mod resolver;
mod changes;
mod registry;
mod lockfile;

pub fn install(myco_toml: MycoToml, save: bool) {
    if let Some(registries) = myco_toml.registries.clone() {
        let mut resolver = resolver::Resolver::new(registries.into_values().collect());
        let resolved_deps = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(resolver.resolve_all(&myco_toml));

        let mut new_lockfile = lockfile::LockFile::new();
        match resolved_deps {
            Ok(deps) => {
                // TODO: Make this more efficient by only downloading the files we don't have yet
                std::fs::remove_dir_all("vendor").unwrap_or(());

                let mut sorted_deps: Vec<_> = deps.into_iter().collect();
                sorted_deps.sort_by(|(name1, ver1), (name2, ver2)| {
                    name1.cmp(name2).then(ver1.version.cmp(&ver2.version))
                });

                for (name, version) in sorted_deps {
                    let zip_file = match &version.pack_url {
                        Location::Url(url) => {
                            if url.scheme() == "file" {
                                std::fs::read(url.path()).unwrap()
                            } else {
                                reqwest::blocking::get(url.clone()).unwrap().bytes().unwrap().to_vec()
                            }
                        }
                        Location::Path { path } => {
                            std::fs::read(path).unwrap()
                        }
                    };

                    // Validate integrity
                    let calculated_integrity = calculate_integrity(&zip_file);
                    if calculated_integrity != version.integrity {
                        eprintln!("Integrity check failed for package: {}", name);
                        eprintln!("Expected: {}", version.integrity);
                        eprintln!("Got: {}", calculated_integrity);
                        std::process::exit(1);
                    }

                    new_lockfile.package.push(version.clone());

                    let mut zip_archive = zip::ZipArchive::new(std::io::Cursor::new(zip_file)).unwrap();

                    // Iterate through the entries in the ZIP archive
                    for i in 0..zip_archive.len() {
                        let mut entry = zip_archive.by_index(i).unwrap();
                        let out_path = std::path::PathBuf::from("./vendor").join(entry.name());

                        if entry.is_dir() {
                            // Create a new directory if the entry is a directory
                            std::fs::create_dir_all(&out_path).unwrap();
                        } else {
                            // Create a new file and write the entry's contents to it
                            let mut out_file = std::fs::File::create(&out_path).unwrap();
                            std::io::copy(&mut entry, &mut out_file).unwrap();
                        }
                    }
                }

                if save {
                    new_lockfile.save().unwrap();
                } else {
                    let existing_lockfile = lockfile::LockFile::load();
                    match existing_lockfile {
                        Ok(existing_lockfile) => {
                            let lockfiles_match = existing_lockfile.package == new_lockfile.package;
                            if !lockfiles_match {
                                eprintln!("Lockfile mismatch. Please run `myco install --save` to update the lockfile.");
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error loading lockfile: {:?}.\n\nHave you run `myco install --save`?", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error resolving dependencies: {:?}", e);
            }
        }
    } else {
        eprintln!("No registries found in myco.toml");
    }
}

pub fn add(myco_toml: &MycoToml, package: PackageName) -> Vec<DepsChange> {
    if let Some(registries) = myco_toml.registries.clone() {
        let mut resolver = resolver::Resolver::new(registries.into_values().collect());
        let resolved_package = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(resolver.resolve_package(&package));

        match resolved_package {
            Ok(Some(package)) => {
                let max_version = package.versions.iter().max().unwrap();
                vec![
                    DepsChange::Set(package.name, max_version.version.clone())
                ]
            }
            Ok(None) => {
                eprintln!("Package {} not found in any registries", package);
                vec![]
            }
            Err(e) => {
                eprintln!("Error resolving dependencies: {:?}", e);
                vec![]
            }
        }
    } else {
        eprintln!("No registries found in myco.toml");
        vec![]
    }
}

pub fn remove(myco_toml: &MycoToml, package: PackageName) -> Vec<DepsChange> {
    let myco_toml = myco_toml;
    let deps = myco_toml.clone_deps();
    let had_package = deps.contains_key(&package);
    if had_package {
        vec![
            DepsChange::Remove(package)
        ]
    } else {
        eprintln!("Package {} not found in myco.toml", package);
        vec![]
    }
}

pub fn update(myco_toml: &MycoToml, package: Option<PackageName>) -> Vec<DepsChange> {
    if let Some(package) = package {
        add(myco_toml, package)
    } else {
        let deps = myco_toml.clone_deps();
        let mut changes = vec![];
        for dep in deps.into_keys() {
            changes.append(&mut add(myco_toml, dep));
        }
        changes
    }
}

pub fn list(myco_toml: MycoToml) {
    let deps = myco_toml.into_deps();
    for (name, version) in deps {
        println!("{} = \"{}\"", name, version);
    }
}
