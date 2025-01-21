use std::collections::{BTreeMap, HashSet};

use crate::AnyError;
use crate::deps::registry;
use crate::deps::registry::{Registry, ResolvedVersion};
use crate::manifest::{MycoToml, PackageName, PackageVersion, Location};

use super::registry::RegistryPackage;

pub struct Resolver {
    registries: Vec<Location>,
}

impl Resolver {
    pub fn new(
        registries: Vec<Location>,
    ) -> Self {
        Self {
            registries,
        }
    }

    async fn resolve_version(&mut self, package_name: &PackageName, version: &PackageVersion) -> Result<Option<ResolvedVersion>, AnyError> {
        for location in &self.registries {
            let registry: Registry = registry::fetch_contents(&location).await?;
            let resolved = registry.resolve_version(&location, &package_name, &version)?;
            if let Some(version) = resolved {
                return Ok(Some(version));
            }
        }
        Ok(None)
    }

    pub async fn resolve_package(&mut self, package_name: &PackageName) -> Result<Option<RegistryPackage>, AnyError> {
        for location in &self.registries {
            let registry: Registry = registry::fetch_contents(&location).await?;
            let resolved = registry.resolve_package(&location, &package_name)?;
            if let Some(package) = resolved {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }

    pub async fn resolve_all(&mut self, myco_toml: &MycoToml) -> Result<BTreeMap<PackageName, ResolvedVersion>, AnyError> {
        let visited = &mut HashSet::new();
        let mut versions_map: BTreeMap<PackageName, ResolvedVersion> = BTreeMap::new();
        let mut to_visit: Vec<ResolvedVersion> = vec![];
        self.resolve_package_deps(myco_toml, &mut to_visit).await?;
        
        while let Some(version) = to_visit.pop() {
            if visited.contains(&version) {
                continue;
            }
            
            // Update versions_map with highest version
            if let Some(existing_version) = versions_map.get(&version.name) {
                if version.version > existing_version.version {
                    versions_map.insert(version.name.clone(), version.clone());
                }
            } else {
                versions_map.insert(version.name.clone(), version.clone());
            }
            
            let myco_toml = get_myco_toml(&version).await?;
            visited.insert(version);
            self.resolve_package_deps(&myco_toml, &mut to_visit).await?;
        }
        
        Ok(versions_map)
    }

    pub async fn resolve_package_deps(&mut self, myco_toml: &MycoToml, to_visit: &mut Vec<ResolvedVersion>) -> Result<(), AnyError> {
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

async fn get_myco_toml(version: &ResolvedVersion) -> Result<MycoToml, AnyError> {
    let myco_toml: MycoToml = registry::fetch_contents(&version.toml_url).await?;
    Ok(myco_toml)
}
