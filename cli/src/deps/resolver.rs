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
            let resolved = registry.resolve_version(&location, &package_name, &version)?;
            if let Some(version) = resolved {
                return Ok(Some(version));
            }
        }
        Ok(None)
    }

    pub async fn resolve_package(&mut self, package_name: &PackageName) -> Result<Option<RegistryPackage>, ResolveError> {
        for location in &self.registries {
            let registry: Registry = registry::fetch_contents(&location).await?;
            let resolved = registry.resolve_package(&location, &package_name)?;
            if let Some(package) = resolved {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }

    pub async fn resolve_all(&mut self, myco_toml: &MycoToml) -> Result<BTreeMap<PackageName, ResolvedVersion>, ResolveError> {
        let visited = &mut HashSet::new();
        let mut dependencies = Vec::new();
        let mut to_visit: Vec<ResolvedVersion> = vec![];
        self.resolve_package_deps(myco_toml, &mut to_visit).await?;
        while let Some(version) = to_visit.pop() {
            if visited.contains(&version) {
                continue;
            }
            let myco_toml = get_myco_toml(&version).await?;
            visited.insert(version.clone());
            dependencies.push(version);
            self.resolve_package_deps(&myco_toml, &mut to_visit).await?;
        };
        // Filter versions to only include the highest version of each dependency name
        let mut versions_map = BTreeMap::new();
        for resolved_version in dependencies {
            if !versions_map.contains_key(&resolved_version.name) {
                versions_map.insert(resolved_version.name.clone(), resolved_version);
            } else {
                let existing_version = versions_map.get(&resolved_version.name).unwrap();
                if resolved_version.version > existing_version.version {
                    versions_map.insert(resolved_version.name.clone(), resolved_version);
                }
            }
        }
        Ok(versions_map)
    }

    pub async fn resolve_package_deps(&mut self, myco_toml: &MycoToml, to_visit: &mut Vec<ResolvedVersion>) -> Result<(), ResolveError> {
        let deps = myco_toml.clone_deps();
        for (name, version) in deps {
            let resolved_version = self.resolve_version(&name, &version).await?;
            if let Some(resolved_version) = resolved_version {
                to_visit.push(resolved_version);
            } else {
                eprintln!("Could not resolve dependency {} {}", name, version);
            }
        }
        Ok(())
    }
}

async fn get_myco_toml(version: &ResolvedVersion) -> Result<MycoToml, ResolveError> {
    let myco_toml: MycoToml = registry::fetch_contents(&version.toml_url).await?;
    Ok(myco_toml)
}
