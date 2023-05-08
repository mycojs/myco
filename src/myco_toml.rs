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

impl MycoToml {
    pub fn new() -> Self {
        Self {
            package: MycoTomlPackage {
                name: "myco".to_string(),
                version: "0.0.1".to_string(),
                description: "Myco project".to_string(),
                main: "src".to_string(),
            }
        }
    }

    pub fn from_string(contents: &str) -> Result<Self, TomlError> {
        from_str(&contents).map_err(|e| TomlError {
            message: e.to_string(),
        })
    }

    pub fn to_string(&self) -> String {
        toml::to_string(self).unwrap()
    }
}