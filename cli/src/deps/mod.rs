use changes::DepsChange;
pub use changes::write_deps_changes;
use crate::deps::resolver::ResolvedDependency;

use crate::manifest::{MycoToml, PackageName};

mod resolver;
mod changes;
mod registry;

pub fn fetch(myco_toml: MycoToml) {
    if let Some(registries) = myco_toml.registries.clone() {
        let mut resolver = resolver::Resolver::new(registries.into_values().collect());
        let resolved_deps = resolver.resolve_all_blocking(&myco_toml);
        match resolved_deps {
            Ok(deps) => {
                for dep in deps.into_values() {
                    let zip_file = match dep {
                        ResolvedDependency::Version(version) => {
                            if version.pack_url.scheme() == "file" {
                                std::fs::read(version.pack_url.path()).unwrap()
                            } else {
                                reqwest::blocking::get(version.pack_url).unwrap().bytes().unwrap().to_vec()
                            }
                        }
                        ResolvedDependency::Url(url) => {
                            reqwest::blocking::get(url).unwrap().bytes().unwrap().to_vec()
                        }
                    };

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
        let resolved_package = resolver.resolve_package_blocking(&package);
        match resolved_package {
            Ok(Some(package)) => {
                let max_version = package.version.iter().max().unwrap();
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
