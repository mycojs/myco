use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::MycoError;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[derive(Default)]
pub struct MycoLocalToml {
    pub resolve: Option<BTreeMap<String, Vec<String>>>,
}

impl MycoLocalToml {
    fn from_str(contents: &str) -> Result<Self, MycoError> {
        toml::from_str(contents).map_err(|e| MycoError::ManifestParse { source: e })
    }

    pub fn load_from_myco_toml_path(myco_toml_dir: PathBuf) -> Result<Self, MycoError> {
        let file_path = myco_toml_dir.join(".myco").join("myco-local.toml");
        if !file_path.exists() {
            return Err(MycoError::LocalManifestNotFound {
                myco_toml_dir: myco_toml_dir.display().to_string(),
            });
        }
        let contents = std::fs::read_to_string(&file_path).map_err(|e| MycoError::ReadFile {
            path: file_path.display().to_string(),
            source: e,
        })?;
        Self::from_str(&contents)
    }

    pub fn load_from_path(file_path: PathBuf) -> Result<Self, MycoError> {
        let contents = std::fs::read_to_string(&file_path).map_err(|e| MycoError::ReadFile {
            path: file_path.display().to_string(),
            source: e,
        })?;
        Self::from_str(&contents)
    }

    pub fn save_blocking(&self, dir_path: PathBuf) -> Result<(), MycoError> {
        let contents =
            toml::to_string(&self).map_err(|e| MycoError::ManifestSerialize { source: e })?;

        let myco_dir = dir_path.join(".myco");
        std::fs::create_dir_all(&myco_dir).map_err(|e| MycoError::FileWrite {
            path: myco_dir.display().to_string(),
            source: e,
        })?;

        let file_path = myco_dir.join("myco-local.toml");
        std::fs::write(&file_path, contents).map_err(|e| MycoError::FileWrite {
            path: file_path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }

    pub fn to_string(&self) -> Result<String, MycoError> {
        toml::to_string(self).map_err(|e| MycoError::ManifestSerialize { source: e })
    }

    pub fn to_string_lossy(&self) -> String {
        self.to_string()
            .unwrap_or_else(|_| "<invalid myco-local manifest>".to_string())
    }

    pub fn clone_resolve(&self) -> BTreeMap<String, Vec<String>> {
        self.resolve.as_ref().cloned().unwrap_or(BTreeMap::new())
    }

    pub fn into_resolve(self) -> BTreeMap<String, Vec<String>> {
        self.resolve.unwrap_or_default()
    }

    pub fn get_resolve_paths(&self, package_name: &str) -> Option<&Vec<String>> {
        self.resolve.as_ref()?.get(package_name)
    }

    /// Get the first path for a package (for backward compatibility)
    pub fn get_resolve_path(&self, package_name: &str) -> Option<&String> {
        self.resolve.as_ref()?.get(package_name)?.first()
    }

    pub fn add_resolve(&mut self, package_name: String, path: String) {
        if self.resolve.is_none() {
            self.resolve = Some(BTreeMap::new());
        }
        if let Some(resolve) = &mut self.resolve {
            resolve
                .entry(package_name)
                .or_insert_with(Vec::new)
                .push(path);
        }
    }

    pub fn add_resolve_paths(&mut self, package_name: String, paths: Vec<String>) {
        if self.resolve.is_none() {
            self.resolve = Some(BTreeMap::new());
        }
        if let Some(resolve) = &mut self.resolve {
            resolve.insert(package_name, paths);
        }
    }

    pub fn remove_resolve(&mut self, package_name: &str) -> Option<Vec<String>> {
        self.resolve.as_mut()?.remove(package_name)
    }
}

