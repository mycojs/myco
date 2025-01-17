use std::path::PathBuf;

use anyhow::anyhow;
use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::deps::resolver::{RegistryPackage, ResolveError};
use crate::manifest::{PackageName, Location};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Registry {
    pub namespace: Vec<RegistryNamespaceEntry>,
}

impl Registry {
    pub async fn resolve_package(&self, location: &Location, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        for namespace in &self.namespace {
            if let Some(package) = namespace.resolve_package(location, package_name).await? {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum RegistryNamespaceEntry {
    Inline(RegistryNamespace),
    URL {
        name: String,
        index_url: String,
    },
}

impl RegistryNamespaceEntry {
    pub fn name(&self) -> &str {
        match self {
            RegistryNamespaceEntry::Inline(inner) => &inner.name,
            RegistryNamespaceEntry::URL { name, .. } => name,
        }
    }

    pub async fn resolve(&self, location: &Location) -> Result<(Location, RegistryNamespace), ResolveError> {
        match self {
            RegistryNamespaceEntry::Inline(inner) => Ok((location.clone(), inner.clone())),
            RegistryNamespaceEntry::URL { index_url, .. } => {
                let index_url = join(location, index_url)?;
                let contents = fetch_contents(&index_url).await?;
                Ok((index_url, contents))
            }
        }
    }

    pub async fn resolve_package(&self, location: &Location, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        if self.name().starts_with(&package_name.namespaces_to_string()) {
            let (location, namespace) = self.resolve(location).await?;
            namespace.resolve_package(&location, package_name).await
        } else {
            Ok(None)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryNamespace {
    pub name: String,
    pub package: Option<Vec<RegistryPackageEntry>>,
    pub namespace: Option<Vec<RegistryNamespaceEntry>>,
}

impl RegistryNamespace {
    #[async_recursion]
    pub async fn resolve_package(&self, location: &Location, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        if let Some(packages) = &self.package {
            for package in packages {
                if package.name() == package_name {
                    return Ok(Some(package.resolve(location).await?));
                }
            }
        }
        if let Some(namespaces) = &self.namespace {
            for namespace in namespaces {
                if let Some(package) = namespace.resolve_package(&location, package_name).await? {
                    return Ok(Some(package));
                }
            }
        }
        Ok(None)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum RegistryPackageEntry {
    Inline(RegistryPackage),
    URL {
        name: PackageName,
        index_url: String,
    },
}

impl RegistryPackageEntry {
    pub fn name(&self) -> &PackageName {
        match self {
            RegistryPackageEntry::Inline(inner) => &inner.name,
            RegistryPackageEntry::URL { name, .. } => name,
        }
    }

    pub async fn resolve(&self, location: &Location) -> Result<RegistryPackage, ResolveError> {
        match self {
            RegistryPackageEntry::Inline(inner) => Ok(inner.clone()),
            RegistryPackageEntry::URL { index_url, .. } => {
                let index_url = join(&location, index_url)?;
                let contents = fetch_contents(&index_url).await?;
                Ok(contents)
            }
        }
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
            path.pop(); // Get rid of the filename
            Location::Path { path: path.join(url) }
        }
    })
}
