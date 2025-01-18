use crate::manifest::{Location, PackageName, PackageVersion};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct LockFile {
    pub package: Vec<LockFileEntry>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct LockFileEntry {
    pub name: PackageName,
    pub version: PackageVersion,
    pub pack_url: Location,
    pub toml_url: Location,
    pub integrity: String,
}

impl LockFile {
    pub fn save(&self) -> Result<(), std::io::Error> {
        std::fs::write("myco-lock.toml", toml::to_string_pretty(self).unwrap())
    }

    pub fn load() -> Result<Self, std::io::Error> {
        let contents = std::fs::read_to_string("myco-lock.toml")?;
        Ok(toml::from_str(&contents).unwrap())
    }

    pub fn new() -> Self {
        Self {
            package: Vec::new(),
        }
    }
}
