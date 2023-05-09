use serde::{Serialize, Deserialize};
use toml::from_str;

#[derive(Serialize, Deserialize)]
pub struct MycoToml {
    pub package: MycoTomlPackage,
}

#[derive(Serialize, Deserialize)]
pub struct MycoTomlPackage {
    pub name: String,
    pub version: String,
    pub description: String,
    pub main: String,
}

#[derive(Debug)]
pub struct TomlError {
    message: String,
}

impl std::fmt::Display for TomlError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TomlError: {}", self.message)
    }
}

impl MycoToml {
    pub fn from_string(contents: &str) -> Result<Self, TomlError> {
        from_str(&contents).map_err(|e| TomlError {
            message: e.to_string(),
        })
    }

    pub fn to_string(&self) -> String {
        toml::to_string(self).unwrap()
    }
}