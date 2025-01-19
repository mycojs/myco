use serde::{Deserialize, Serialize};

use super::resolver::ResolvedVersion;

#[derive(Serialize, Deserialize)]
pub struct LockFile {
    pub package: Vec<ResolvedVersion>,
}

impl LockFile {
    pub fn save(&self) -> Result<(), std::io::Error> {
        std::fs::write("myco-lock.toml", toml::to_string_pretty(self).unwrap())
    }

    pub fn load() -> Self {
        let contents = std::fs::read_to_string("myco-lock.toml").unwrap();
        toml::from_str(&contents).unwrap()
    }

    pub fn new() -> Self {
        Self {
            package: Vec::new(),
        }
    }
}
