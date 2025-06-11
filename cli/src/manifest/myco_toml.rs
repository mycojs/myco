use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;
use serde_json;

use crate::manifest::{PackageName, PackageVersion};
use crate::errors::MycoError;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(untagged)]
pub enum Location {
    Url(Url),
    Path { path: PathBuf },
}

impl Location {
    pub fn to_string(&self) -> String {
        match self {
            Location::Url(url) => url.to_string(),
            Location::Path { path } => path.to_string_lossy().to_string(),
        }
    }

    pub fn join(self: &Location, url: &str) -> Result<Location, MycoError> {
        Ok(match self {
            Location::Url(base_url) => Location::Url(if url.matches("^[a-zA-Z]+://").count() > 0 {
                Url::parse(url).map_err(|_e| MycoError::InvalidUrl { 
                    url: url.to_string() 
                })?
            } else {
                base_url.join(url).map_err(|_e| MycoError::InvalidUrl { 
                    url: url.to_string() 
                })?
            }),
            Location::Path { path } => {
                let mut path = PathBuf::from(path);
                if path.exists() && !path.is_dir() {
                    path.pop(); // Get rid of the filename
                }
                Location::Path {
                    path: path.join(url),
                }
            }
        })
    }

    pub fn as_path(&self) -> Option<PathBuf> {
        match self {
            Location::Url(_) => None,
            Location::Path { path } => Some(path.clone()),
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[derive(Serialize, Deserialize)]
pub struct MycoToml {
    pub package: Option<PackageDefinition>,
    pub run: Option<BTreeMap<String, String>>,
    pub registries: Option<BTreeMap<String, Location>>,
    pub deps: Option<BTreeMap<PackageName, PackageVersion>>,
    pub tsconfig: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Serialize, Deserialize)]
pub struct PackageDefinition {
    pub name: String,
    pub version: PackageVersion,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub pre_pack: Option<String>,
    pub include: Option<Vec<String>>,
}

impl MycoToml {
    fn from_str(contents: &str) -> Result<Self, MycoError> {
        toml::from_str(&contents)
            .map_err(|e| MycoError::ManifestParse { source: e })
    }

    pub fn load_nearest(start_dir: PathBuf) -> Result<(PathBuf, Self), MycoError> {
        let original_start_dir = start_dir.clone();
        let mut current_dir = start_dir;
        loop {
            let mut file_path = current_dir.join("myco.toml");
            if file_path.exists() {
                let contents = std::fs::read_to_string(&file_path)
                    .map_err(|e| MycoError::ReadFile { 
                        path: file_path.display().to_string(), 
                        source: e 
                    })?;
                file_path.pop();
                return Ok((file_path, Self::from_str(&contents)?));
            }
            if !current_dir.pop() {
                return Err(MycoError::ManifestNotFound { 
                    start_dir: original_start_dir.display().to_string() 
                });
            }
        }
    }

    pub fn save_blocking(&self) -> Result<(), MycoError> {
        let contents = toml::to_string(&self)
            .map_err(|e| MycoError::ManifestSerialize { source: e })?;
        std::fs::write("myco.toml", contents)
            .map_err(|e| MycoError::FileWrite { 
                path: "myco.toml".to_string(), 
                source: e 
            })?;
        Ok(())
    }

    pub fn to_string(&self) -> Result<String, MycoError> {
        toml::to_string(self)
            .map_err(|e| MycoError::ManifestSerialize { source: e })
    }

    pub fn to_string_lossy(&self) -> String {
        self.to_string().unwrap_or_else(|_| "<invalid manifest>".to_string())
    }

    pub fn clone_deps(&self) -> BTreeMap<PackageName, PackageVersion> {
        self.deps.as_ref().cloned().unwrap_or(BTreeMap::new())
    }

    pub fn into_deps(self) -> BTreeMap<PackageName, PackageVersion> {
        self.deps.unwrap_or(BTreeMap::new())
    }
}
