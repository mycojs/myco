use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::PathBuf;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::manifest::{PackageName, PackageVersion};
use crate::AnyError;

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
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

    pub fn join(self: &Location, url: &str) -> Result<Location, AnyError> {
        Ok(match self {
            Location::Url(base_url) => Location::Url(if url.matches("^[a-zA-Z]+://").count() > 0 {
                Url::parse(url).map_err(|e| anyhow!(e))?
            } else {
                base_url.join(url).map_err(|e| anyhow!(e))?
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
}

#[derive(Serialize, Deserialize)]
pub struct PackageDefinition {
    pub name: String,
    pub version: PackageVersion,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub pre_pack: Option<String>,
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

    pub fn save_blocking(&self) -> Result<(), AnyError> {
        std::fs::write("myco.toml", toml::to_string(&self).unwrap())?;
        Ok(())
    }

    pub fn to_string(&self) -> String {
        toml::to_string(self).unwrap()
    }

    pub fn clone_deps(&self) -> BTreeMap<PackageName, PackageVersion> {
        self.deps.as_ref().cloned().unwrap_or(BTreeMap::new())
    }

    pub fn into_deps(self) -> BTreeMap<PackageName, PackageVersion> {
        self.deps.unwrap_or(BTreeMap::new())
    }
}
