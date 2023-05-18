use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::AnyError;
use crate::deps::registry;
use crate::deps::registry::{Registry, RegistryPackageVersion};
use crate::manifest::{MycoToml, PackageName, PackageVersion};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryPackage {
    pub name: PackageName,
    pub version: Vec<RegistryPackageVersion>,
}

#[derive(Debug)]
pub enum ResolveError {
    UrlError(String, AnyError),
    ParseError(String, AnyError),
}

pub struct Resolver {
    registries: Vec<Url>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Hash)]
pub struct ResolvedVersion {
    pub name: PackageName,
    pub version: PackageVersion,
    pub pack_url: Url,
    pub toml_url: Url,
}

impl ResolvedVersion {
    fn new(name: PackageName, relative_to: &Url, version: RegistryPackageVersion) -> Result<Self, AnyError> {
        Ok(Self {
            name,
            version: version.version,
            pack_url: relative_to.join(&version.pack_url)?,
            toml_url: relative_to.join(&version.toml_url)?,
        })
    }
}

impl Resolver {
    pub fn new(
        registries: Vec<Url>,
    ) -> Self {
        Self {
            registries,
        }
    }

    async fn resolve_version(&mut self, package_name: &PackageName, version: &PackageVersion) -> Result<Option<ResolvedVersion>, ResolveError> {
        for registry_url in &self.registries {
            let registry: Registry = registry::fetch_url_contents(&registry_url).await?;
            let resolved = registry.resolve_package(&registry_url, &package_name).await?;
            if let Some(package) = resolved {
                let version = package.version.into_iter().find(|v| &v.version == version);
                if let Some(version) = version {
                    let version =
                        ResolvedVersion::new(package_name.clone(), registry_url, version)
                            .map_err(|e| ResolveError::UrlError(registry_url.to_string(), e))?;
                    return Ok(Some(version));
                }
            }
        }
        Ok(None)
    }

    async fn resolve_package(&mut self, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        for registry_url in &self.registries {
            let registry: Registry = registry::fetch_url_contents(&registry_url).await?;
            let resolved = registry.resolve_package(&registry_url, &package_name).await?;
            if let Some(package) = resolved {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }

    pub fn resolve_package_blocking(&mut self, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(self.resolve_package(package_name))
    }

    pub async fn resolve_all(&mut self, myco_toml: &MycoToml) -> Result<Vec<ResolvedVersion>, ResolveError> {
        let visited = &mut HashSet::new();
        let mut versions = Vec::new();
        let mut to_visit: Vec<ResolvedVersion> = vec![];
        let deps = myco_toml.clone_deps();
        for (name, version) in deps {
            let resolved_version = self.resolve_version(&name, &version).await?;
            if let Some(resolved_version) = resolved_version {
                to_visit.push(resolved_version);
            } else {
                eprintln!("Could not resolve dependency {} {}", name, version);
            }
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
                let resolved_version = self.resolve_version(&name, &version).await?;
                if let Some(resolved_version) = resolved_version {
                    to_visit.push(resolved_version);
                } else {
                    eprintln!("Could not resolve dependency {} {}", name, version);
                }
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
    let myco_toml: MycoToml = registry::fetch_url_contents(&version.toml_url).await?;
    Ok(myco_toml)
}
