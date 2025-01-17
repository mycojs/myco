use std::path::PathBuf;

use toml_edit::{Document, Item, Table, value};

use crate::manifest::{PackageName, PackageVersion};

pub enum DepsChange {
    Set(PackageName, PackageVersion),
    Remove(PackageName),
}

fn apply_deps_changes<T: AsRef<str>>(changes: &Vec<DepsChange>, toml: T) -> String {
    let mut doc = toml.as_ref().parse::<Document>().expect("Invalid TOML");
    if let None = doc["deps"].as_table() {
        let table = Table::new();
        doc["deps"] = Item::Table(table);
    }
    for change in changes {
        match change {
            DepsChange::Set(name, version) => {
                doc["deps"][&name.to_string()] = value(version.to_string());
            }
            DepsChange::Remove(name) => {
                doc["deps"].as_table_mut().unwrap().remove(&name.to_string());
            }
        }
    }
    if doc["deps"].as_table().unwrap().is_empty() {
        doc.remove("deps");
    }
    doc.to_string()
}

pub fn write_deps_changes(changes: &Vec<DepsChange>, path: &PathBuf) {
    let contents = std::fs::read_to_string(path).expect("Could not read myco.toml");
    let new_contents = apply_deps_changes(changes, contents);
    std::fs::write(path, &new_contents).expect("Could not write myco.toml");
}

pub fn write_new_package_version(version: &PackageVersion, path: &PathBuf) {
    let contents = std::fs::read_to_string(path).expect("Could not read myco.toml");
    let mut doc = contents.parse::<Document>().expect("Invalid TOML");
    if let None = doc["package"].as_table() {
        let table = Table::new();
        doc["package"] = Item::Table(table);
    }
    doc["package"]["version"] = value(version.to_string());
    std::fs::write(path, doc.to_string()).expect("Could not write myco.toml");
}
