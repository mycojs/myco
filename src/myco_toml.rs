use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::anyhow;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use crate::AnyError;

#[derive(Serialize, Deserialize)]
pub struct MycoToml {
    pub package: Option<PackageDefinition>,
    pub run: Option<HashMap<String, String>>,
    pub registries: Option<HashMap<String, Url>>,
    pub deps: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize)]
pub struct PackageDefinition {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
}

impl MycoToml {
    fn from_str(contents: &str) -> Result<Self, AnyError> {
        Ok(toml::from_str(&contents)?)
    }

    pub fn load_nearest(start_dir: PathBuf) -> Result<(PathBuf, Self), AnyError> {
        let mut current_dir = start_dir;
        loop {
            let mut file_path = current_dir.join("myco.toml");
            if file_path.exists() {
                let contents = std::fs::read_to_string(&file_path)?;
                file_path.pop();
                return Ok((file_path, Self::from_str(&contents)?));
            }
            if !current_dir.pop() {
                return Err(anyhow!("No myco.toml found"));
            }
        }
    }

    pub fn to_string(&self) -> String {
        toml::to_string(self).unwrap()
    }
}
