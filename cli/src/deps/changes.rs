use std::path::PathBuf;

use toml_edit::{value, Document, Item, Table};

use crate::errors::MycoError;
use crate::manifest::{PackageName, PackageVersion};

pub enum DepsChange {
    Set(PackageName, PackageVersion),
    Remove(PackageName),
}

fn apply_deps_changes<T: AsRef<str>>(
    changes: &Vec<DepsChange>,
    toml: T,
) -> Result<String, MycoError> {
    let mut doc = toml
        .as_ref()
        .parse::<Document>()
        .map_err(|e| MycoError::Internal {
            message: format!("Invalid TOML: {}", e),
        })?;
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
                doc["deps"]
                    .as_table_mut()
                    .ok_or_else(|| MycoError::Internal {
                        message: "deps should be a table".to_string(),
                    })?
                    .remove(&name.to_string());
            }
        }
    }
    if doc["deps"]
        .as_table()
        .ok_or_else(|| MycoError::Internal {
            message: "deps should be a table".to_string(),
        })?
        .is_empty()
    {
        doc.remove("deps");
    }
    Ok(doc.to_string())
}

pub fn write_deps_changes(changes: &Vec<DepsChange>, path: &PathBuf) -> Result<(), MycoError> {
    let contents = std::fs::read_to_string(path).map_err(|e| MycoError::ReadFile {
        path: path.display().to_string(),
        source: e,
    })?;
    let new_contents = apply_deps_changes(changes, contents)?;
    std::fs::write(path, &new_contents).map_err(|e| MycoError::FileWrite {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(())
}

pub fn write_new_package_version(
    version: &PackageVersion,
    path: &PathBuf,
) -> Result<(), MycoError> {
    let contents = std::fs::read_to_string(path).map_err(|e| MycoError::ReadFile {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut doc = contents
        .parse::<Document>()
        .map_err(|e| MycoError::Internal {
            message: format!("Invalid TOML: {}", e),
        })?;
    if let None = doc["package"].as_table() {
        let table = Table::new();
        doc["package"] = Item::Table(table);
    }
    doc["package"]["version"] = value(version.to_string());
    std::fs::write(path, doc.to_string()).map_err(|e| MycoError::FileWrite {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(())
}
