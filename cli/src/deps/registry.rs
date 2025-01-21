use std::cmp::{Ord, Ordering};
use std::path::PathBuf;

use crate::AnyError;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::deps::resolver::ResolveError;
use crate::manifest::{Location, PackageName, PackageVersion};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryPackage {
    pub name: PackageName,
    pub versions: Vec<VersionEntry>,
    pub base_path: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Hash)]
pub struct VersionEntry {
    pub version: PackageVersion,
    pub integrity: String,
}

impl Ord for VersionEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.version.cmp(&other.version)
    }
}

impl PartialOrd for VersionEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ResolvedVersion {
    pub name: PackageName,
    pub version: PackageVersion,
    pub pack_url: Location,
    pub toml_url: Location,
    pub integrity: String,
}

impl ResolvedVersion {
    pub fn new(name: PackageName, location: &Location, version_entry: &VersionEntry) -> Result<Self, AnyError> {
        let pack_url = join(location, &format!("{}.zip", &version_entry.version)).map_err(|e| e.into_cause())?;
        let toml_url = join(location, &format!("{}.toml", &version_entry.version)).map_err(|e| e.into_cause())?;
        Ok(Self {
            name,
            version: version_entry.version.clone(),
            pack_url,
            toml_url,
            integrity: version_entry.integrity.clone(),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Registry {
    pub namespace: Vec<RegistryNamespace>,
}

impl Registry {
    pub fn resolve_package(&self, location: &Location, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        for namespace in &self.namespace {
            if let Some(package) = namespace.resolve_package(location, package_name)? {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }

    pub fn resolve_version(&self, location: &Location, package_name: &PackageName, version: &PackageVersion) -> Result<Option<ResolvedVersion>, ResolveError> {
        let resolved = self.resolve_package(&location, &package_name)?;
        if let Some(package) = resolved {
            let version = package.versions.into_iter().find(|v| v.version == *version);
            if let Some(version) = version {
                let package_location = join(&location, &package.base_path)?;
                let version =
                    ResolvedVersion::new(package.name.clone(), &package_location, &version)
                        .map_err(|e| ResolveError::UrlError(location.to_string(), e))?;
                return Ok(Some(version));
            }
        }
        Ok(None)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryNamespace {
    pub name: String,
    pub package: Option<Vec<RegistryPackage>>,
    pub namespace: Option<Vec<RegistryNamespace>>,
}

impl RegistryNamespace {
    pub fn resolve_package(&self, location: &Location, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        if let Some(packages) = &self.package {
            for package in packages {
                if &package.name == package_name {
                    return Ok(Some(package.clone()));
                }
            }
        }
        if let Some(namespaces) = &self.namespace {
            for namespace in namespaces {
                if let Some(package) = namespace.resolve_package(&location, package_name)? {
                    return Ok(Some(package));
                }
            }
        }
        Ok(None)
    }
}

async fn fetch_url_contents<T, S: AsRef<str>>(url: S) -> Result<T, ResolveError>
    where
        T: serde::de::DeserializeOwned
{
    let url = url.as_ref();
    let text = if url.starts_with("http://") || url.starts_with("https://") {
        let resp = reqwest::get(url).await
            .map_err(|e| ResolveError::UrlError(url.to_string(), e.into()))?;
        resp.text().await.map_err(|e| ResolveError::UrlError(url.to_string(), e.into()))
    } else if url.starts_with("file://") {
        let url = url.trim_start_matches("file://");
        std::fs::read_to_string(&url).map_err(|e| ResolveError::UrlError(url.to_string(), e.into()))
    } else {
        Err(ResolveError::UrlError(url.to_string(), anyhow!("Unknown URL scheme")))
    }?;
    toml::from_str(&text)
        .map_err(|e| ResolveError::ParseError(url.to_string(), e.into()))
}

pub async fn fetch_contents<T>(location: &Location) -> Result<T, ResolveError>
    where
        T: serde::de::DeserializeOwned
{
    Ok(match location {
        Location::Url(url) => fetch_url_contents(url.as_str()).await?,
        Location::Path { path } => {
            tokio::fs::read_to_string(path)
                .await
                .map_err(|e| ResolveError::UrlError(path.to_string_lossy().to_string(), e.into()))
                .and_then(|text| toml::from_str(&text)
                    .map_err(|e| ResolveError::ParseError(path.to_string_lossy().to_string(), e.into())))?
        }
    })
}

pub fn join(location: &Location, url: &str) -> Result<Location, ResolveError> {
    Ok(match location {
        Location::Url(base_url) => {
            Location::Url(if url.matches("^[a-zA-Z]+://").count() > 0 {
                Url::parse(url)
                    .map_err(|e| ResolveError::UrlError(url.to_string(), e.into()))?
            } else {
                base_url.join(url)
                    .map_err(|e| ResolveError::UrlError(url.to_string(), e.into()))?
            })
        }
        Location::Path { path } => {
            let mut path = PathBuf::from(path);
            if path.exists() && !path.is_dir() {
                path.pop(); // Get rid of the filename
            }
            Location::Path { path: path.join(url) }
        }
    })
}
