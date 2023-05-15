use crate::myco_toml::MycoToml;

mod resolver;
mod version;

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
