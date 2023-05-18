use std::cmp::Ordering;

use anyhow::anyhow;
use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};

use crate::deps::resolver::{RegistryPackage, ResolveError};
use crate::manifest::{PackageName, PackageVersion};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Registry {
    pub namespace: Vec<RegistryNamespaceEntry>,
}

impl Registry {
    pub async fn resolve_package(&self, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        for namespace in &self.namespace {
            if let Some(package) = namespace.resolve_package(package_name).await? {
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

    pub async fn resolve(&self) -> Result<RegistryNamespace, ResolveError> {
        match self {
            RegistryNamespaceEntry::Inline(inner) => Ok(inner.clone()),
            RegistryNamespaceEntry::URL { index_url, .. } => {
                let contents = fetch_url_contents(&index_url).await?;
                Ok(contents)
            }
        }
    }

    pub async fn resolve_package(&self, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        if self.name().starts_with(&package_name.namespaces_to_string()) {
            let namespace = self.resolve().await?;
            namespace.resolve_package(package_name).await
        } else {
            Ok(None)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryNamespace {
    pub name: String,
    pub package: Vec<RegistryPackageEntry>,
    pub namespace: Vec<RegistryNamespaceEntry>,
}

impl RegistryNamespace {
    #[async_recursion]
    pub async fn resolve_package(&self, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        for package in &self.package {
            if package.name() == package_name {
                return Ok(Some(package.resolve().await?));
            }
        }
        for namespace in &self.namespace {
            if let Some(package) = namespace.resolve_package(package_name).await? {
                return Ok(Some(package));
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

    pub async fn resolve(&self) -> Result<RegistryPackage, ResolveError> {
        match self {
            RegistryPackageEntry::Inline(inner) => Ok(inner.clone()),
            RegistryPackageEntry::URL { index_url, .. } => {
                let contents = fetch_url_contents(&index_url).await?;
                Ok(contents)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct RegistryPackageVersion {
    pub version: PackageVersion,
    pub pack_url: String,
    pub toml_url: String,
}

impl Ord for RegistryPackageVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.version.cmp(&other.version)
    }
}

impl PartialOrd for RegistryPackageVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub async fn fetch_url_contents<T, S: AsRef<str>>(url: S) -> Result<T, ResolveError>
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
