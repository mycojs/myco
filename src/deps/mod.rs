use changes::DepsChange;
pub use changes::write_deps_changes;

use crate::myco_toml::MycoToml;

mod resolver;
mod version;
mod changes;

pub fn fetch(myco_toml: MycoToml) {
    if let Some(registries) = myco_toml.registries.clone() {
        let mut resolver = resolver::Resolver::new(registries.into_values().collect());
        let resolved_deps = resolver.resolve_all_blocking(&myco_toml);
        match resolved_deps {
            Ok(deps) => {
                for dep in deps {
                    let zip_file = if dep.pack_url.scheme() == "file" {
                        std::fs::read(dep.pack_url.path()).unwrap()
                    } else {
                        reqwest::blocking::get(dep.pack_url).unwrap().bytes().unwrap().to_vec()
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

pub fn add<T: AsRef<str>>(myco_toml: &MycoToml, package: T) -> Vec<DepsChange> {
    if let Some(registries) = myco_toml.registries.clone() {
        let mut resolver = resolver::Resolver::new(registries.into_values().collect());
        let resolved_package = resolver.resolve_package_blocking(package);
        match resolved_package {
            Ok(package) => {
                let max_version = package.package.version.iter().max().unwrap();
                vec![
                    DepsChange::Set(package.package.name, max_version.version.clone())
                ]
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

pub fn remove<T: AsRef<str>>(myco_toml: &MycoToml, package: T) -> Vec<DepsChange> {
    let myco_toml = myco_toml;
    let deps = myco_toml.clone_deps();
    let had_package = deps.contains_key(&package.as_ref().to_string());
    if had_package {
        vec![
            DepsChange::Remove(package.as_ref().to_string())
        ]
    } else {
        eprintln!("Package {} not found in myco.toml", package.as_ref());
        vec![]
    }
}

pub fn update<T: AsRef<str>>(myco_toml: &MycoToml, package: Option<T>) -> Vec<DepsChange> {
    if let Some(package) = package {
        add(myco_toml, &package)
    } else {
        let deps = myco_toml.clone_deps();
        let mut changes = vec![];
        for dep in deps.keys() {
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
