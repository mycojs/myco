use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use std::cmp::{Ord, Ordering};

use crate::AnyError;
use crate::deps::registry;
use crate::deps::registry::{join, Registry};
use crate::manifest::{MycoToml, PackageName, PackageVersion, Location};

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

#[derive(Debug)]
pub enum ResolveError {
    UrlError(String, AnyError),
    ParseError(String, AnyError),
}

impl ResolveError {
    pub fn into_cause(self) -> AnyError {
        match self {
            ResolveError::UrlError(_, e) => e,
            ResolveError::ParseError(_, e) => e,
        }
    }
}

pub struct Resolver {
    registries: Vec<Location>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ResolvedVersion {
    pub version: PackageVersion,
    pub pack_url: Location,
    pub toml_url: Location,
    pub integrity: String,
}

impl ResolvedVersion {
    fn new(location: &Location, version_entry: &VersionEntry) -> Result<Self, AnyError> {
        let pack_url = join(location, &format!("{}.zip", &version_entry.version)).map_err(|e| e.into_cause())?;
        let toml_url = join(location, &format!("{}.toml", &version_entry.version)).map_err(|e| e.into_cause())?;
        Ok(Self {
            version: version_entry.version.clone(),
            pack_url,
            toml_url,
            integrity: version_entry.integrity.clone(),
        })
    }
}

impl Resolver {
    pub fn new(
        registries: Vec<Location>,
    ) -> Self {
        Self {
            registries,
        }
    }

    async fn resolve_version(&mut self, package_name: &PackageName, version: &PackageVersion) -> Result<Option<ResolvedVersion>, ResolveError> {
        for location in &self.registries {
            let registry: Registry = registry::fetch_contents(&location).await?;
            let resolved = registry.resolve_package(&location, &package_name).await?;
            if let Some(package) = resolved {
                let version = package.versions.into_iter().find(|v| v.version == *version);
                if let Some(version) = version {
                    let package_location = join(&location, &package.base_path)?;
                    let version =
                        ResolvedVersion::new(&package_location, &version)
                            .map_err(|e| ResolveError::UrlError(location.to_string(), e))?;
                    return Ok(Some(version));
                }
            }
        }
        Ok(None)
    }

    async fn resolve_package(&mut self, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        for location in &self.registries {
            let registry: Registry = registry::fetch_contents(&location).await?;
            let resolved = registry.resolve_package(&location, &package_name).await?;
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

    pub async fn resolve_all(&mut self, myco_toml: &MycoToml) -> Result<BTreeMap<PackageName, ResolvedVersion>, ResolveError> {
        let visited = &mut HashSet::new();
        let mut dependencies = Vec::new();
        let mut to_visit: Vec<(PackageName, ResolvedVersion)> = vec![];
        let deps = myco_toml.clone_deps();
        for (name, version) in deps {
            let resolved_version = self.resolve_version(&name, &version).await?;
            if let Some(resolved_version) = resolved_version {
                to_visit.push((name, resolved_version));
            } else {
                eprintln!("Could not resolve dependency {} {}", name, version);
            }
        }
        while let Some((name, version)) = to_visit.pop() {
            if visited.contains(&version) {
                continue;
            }
            let myco_toml = get_myco_toml(&version).await?;
            visited.insert(version.clone());
            dependencies.push((name, version));
            let deps = myco_toml.into_deps();
            for (name, version) in deps {
                let resolved_version = self.resolve_version(&name, &version).await?;
                if let Some(resolved_version) = resolved_version {
                    to_visit.push((name, resolved_version));
                } else {
                    eprintln!("Could not resolve dependency {} {}", name, version);
                }
            }
        };
        // Filter versions to only include the highest version of each dependency name
        let mut versions_map = BTreeMap::new();
        for (name, version) in dependencies {
            if !versions_map.contains_key(&name) {
                versions_map.insert(name.clone(), version);
            } else {
                let existing_version = versions_map.get(&name).unwrap();
                if version.version > existing_version.version {
                    versions_map.insert(name, version);
                }
            }
        }
        Ok(versions_map)
    }

    pub fn resolve_all_blocking(&mut self, myco_toml: &MycoToml) -> Result<BTreeMap<PackageName, ResolvedVersion>, ResolveError> {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(self.resolve_all(myco_toml))
    }
}

async fn get_myco_toml(version: &ResolvedVersion) -> Result<MycoToml, ResolveError> {
    let myco_toml: MycoToml = registry::fetch_contents(&version.toml_url).await?;
    Ok(myco_toml)
}
