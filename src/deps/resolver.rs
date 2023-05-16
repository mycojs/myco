use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use anyhow::anyhow;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use crate::AnyError;
use crate::myco_toml::MycoToml;

#[derive(Serialize, Deserialize, Debug)]
pub struct Registry {
    pub package: Vec<RegistryPackageEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum RegistryPackageEntry {
    Inline(RegistryPackage),
    URL {
        name: String,
        index_url: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegistryPackage {
    pub name: String,
    pub version: Vec<RegistryPackageVersion>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct RegistryPackageVersion {
    pub version: String,
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

#[derive(Debug)]
pub enum ResolveError {
    PackageNotFound(String),
    VersionNotFound(String, String),
    UrlError(String, AnyError),
    ParseError(String, AnyError),
}

pub struct Resolver {
    registries: Vec<Url>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Hash)]
pub struct ResolvedVersion {
    pub name: String,
    pub version: String,
    pub pack_url: Url,
    pub toml_url: Url,
}

impl ResolvedVersion {
    fn new(name: String, relative_to: &Url, version: RegistryPackageVersion) -> Result<Self, AnyError> {
        Ok(Self {
            name,
            version: version.version,
            pack_url: relative_to.join(&version.pack_url)?,
            toml_url: relative_to.join(&version.toml_url)?,
        })
    }
}

pub struct ResolvedPackage {
    pub registry: Url,
    pub package: RegistryPackage,
}

impl Resolver {
    pub fn new(
        registries: Vec<Url>,
    ) -> Self {
        Self {
            registries,
        }
    }

    async fn resolve_package_in_registry<T: AsRef<str>>(registry_url: &Url, package_name: T) -> Result<ResolvedPackage, ResolveError> {
        let registry: Registry = fetch_url_contents(&registry_url).await?;
        let package =
            registry.package
                .into_iter()
                .find(|entry| {
                    match entry {
                        RegistryPackageEntry::Inline(inner) => inner.name.as_str() == package_name.as_ref(),
                        RegistryPackageEntry::URL { name, .. } => name.as_str() == package_name.as_ref(),
                    }
                })
                .ok_or(ResolveError::PackageNotFound(package_name.as_ref().to_string()))?;
        let (package, registry_url) = match package {
            RegistryPackageEntry::Inline(inner) => (inner, registry_url.clone()),
            RegistryPackageEntry::URL { index_url, .. } => {
                let registry_url = registry_url.join(&index_url).map_err(|e| ResolveError::UrlError(index_url, e.into()))?;
                let contents = fetch_url_contents(&registry_url).await?;
                (contents, registry_url)
            }
        };
        Ok(ResolvedPackage {
            registry: registry_url,
            package,
        })
    }

    async fn resolve_version_in_registry<T: AsRef<str>>(registry_url: &Url, package_name: T, version: T) -> Result<ResolvedVersion, ResolveError> {
        let resolved = Self::resolve_package_in_registry(registry_url, package_name.as_ref()).await?;
        let version =
            resolved.package.version
                .into_iter()
                .find(|v| v.version.as_str() == version.as_ref())
                .ok_or(ResolveError::VersionNotFound(package_name.as_ref().to_string(), version.as_ref().to_string()))?;
        Ok(
            ResolvedVersion::new(package_name.as_ref().to_string(), &resolved.registry, version)
                .map_err(|e| ResolveError::UrlError(registry_url.to_string(), e.into()))?
        )
    }

    async fn resolve_version<T: AsRef<str>>(&mut self, package_name: T, version: T) -> Result<ResolvedVersion, ResolveError> {
        for registry in &self.registries {
            match Self::resolve_version_in_registry(registry, package_name.as_ref(), version.as_ref()).await {
                Ok(version) => return Ok(version),
                Err(ResolveError::PackageNotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(ResolveError::PackageNotFound(package_name.as_ref().to_string()))
    }

    async fn resolve_package<T: AsRef<str>>(&mut self, package_name: T) -> Result<ResolvedPackage, ResolveError> {
        for registry in &self.registries {
            match Self::resolve_package_in_registry(registry, package_name.as_ref()).await {
                Ok(package) => return Ok(package),
                Err(ResolveError::PackageNotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(ResolveError::PackageNotFound(package_name.as_ref().to_string()))
    }

    pub fn resolve_package_blocking<T: AsRef<str>>(&mut self, package_name: T) -> Result<ResolvedPackage, ResolveError> {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(self.resolve_package(package_name))
    }

    pub async fn resolve_all(&mut self, myco_toml: &MycoToml) -> Result<Vec<ResolvedVersion>, ResolveError> {
        let visited = &mut HashSet::new();
        let mut versions = Vec::new();
        let mut to_visit: Vec<ResolvedVersion> = vec![];
        let deps = myco_toml.clone_deps();
        for dep in deps {
            let version = self.resolve_version(dep.0, dep.1).await?;
            to_visit.push(version);
        }
        while let Some(version) = to_visit.pop() {
            if visited.contains(&version) {
                continue;
            }
            let myco_toml = get_myco_toml(&version).await?;
            visited.insert(version.clone());
            versions.push(version);
            let deps = myco_toml.into_deps();
            for (name, version) in deps {
                let version = self.resolve_version(name, version).await?;
                to_visit.push(version);
            }
        };
        // Filter versions to only include the highest version of each dependency name
        let mut versions_map = HashMap::new();
        for version in versions {
            if !versions_map.contains_key(&version.name) {
                versions_map.insert(version.name.clone(), version);
            } else {
                let existing_version = versions_map.get(&version.name).unwrap();
                if existing_version.version > version.version {
                    versions_map.insert(version.name.clone(), version);
                }
            }
        }
        Ok(versions_map.into_values().collect())
    }

    pub fn resolve_all_blocking(&mut self, myco_toml: &MycoToml) -> Result<Vec<ResolvedVersion>, ResolveError> {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(self.resolve_all(myco_toml))
    }
}

async fn get_myco_toml(version: &ResolvedVersion) -> Result<MycoToml, ResolveError> {
    let myco_toml: MycoToml = fetch_url_contents(&version.toml_url).await?;
    Ok(myco_toml)
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
