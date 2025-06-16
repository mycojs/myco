use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::MycoError;
use crate::manifest::{DependencyVersion, Location, PackageName};

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct WorkspaceDefinition {
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub workspace: WorkspaceDefinition,
    pub run: Option<BTreeMap<String, String>>,
    pub registries: Option<BTreeMap<String, Location>>,
    pub deps: Option<BTreeMap<PackageName, DependencyVersion>>,
    pub tsconfig: Option<BTreeMap<String, serde_json::Value>>,
}

impl WorkspaceManifest {
    fn from_str(contents: &str) -> Result<Self, MycoError> {
        toml::from_str(contents).map_err(|e| MycoError::ManifestParse { source: e })
    }

    pub fn load_nearest(start_dir: PathBuf) -> Result<(PathBuf, Self), MycoError> {
        let original_start_dir = start_dir.clone();
        let mut current_dir = start_dir;
        loop {
            let mut file_path = current_dir.join("workspace.toml");
            if file_path.exists() {
                let contents =
                    std::fs::read_to_string(&file_path).map_err(|e| MycoError::ReadFile {
                        path: file_path.display().to_string(),
                        source: e,
                    })?;
                file_path.pop();
                return Ok((file_path, Self::from_str(&contents)?));
            }
            if !current_dir.pop() {
                return Err(MycoError::WorkspaceNotFound {
                    start_dir: original_start_dir.display().to_string(),
                });
            }
        }
    }

    pub fn save_blocking(&self) -> Result<(), MycoError> {
        let contents =
            toml::to_string(&self).map_err(|e| MycoError::ManifestSerialize { source: e })?;
        std::fs::write("workspace.toml", contents).map_err(|e| MycoError::FileWrite {
            path: "workspace.toml".to_string(),
            source: e,
        })?;
        Ok(())
    }

    pub fn to_string(&self) -> Result<String, MycoError> {
        toml::to_string(self).map_err(|e| MycoError::ManifestSerialize { source: e })
    }

    pub fn to_string_lossy(&self) -> String {
        self.to_string()
            .unwrap_or_else(|_| "<invalid workspace manifest>".to_string())
    }
}
