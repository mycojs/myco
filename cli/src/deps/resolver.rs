use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::AnyError;
use crate::deps::registry;
use crate::deps::registry::{join, Registry, RegistryPackageVersion};
use crate::manifest::{MycoToml, PackageName, PackageVersion, PackageVersionEntry, Location};

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
}

impl ResolvedVersion {
    fn new(location: &Location, version: RegistryPackageVersion) -> Result<Self, AnyError> {
        Ok(Self {
            version: version.version,
            pack_url: join(location, &version.pack_url).map_err(|e| e.into_cause())?,
            toml_url: join(location, &version.toml_url).map_err(|e| e.into_cause())?,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ResolvedDependency {
    Version(ResolvedVersion),
    Url(Url),
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
                let version = package.version.into_iter().find(|v| &v.version == version);
                if let Some(version) = version {
                    let version =
                        ResolvedVersion::new(location, version)
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

    pub async fn resolve_all(&mut self, myco_toml: &MycoToml) -> Result<BTreeMap<PackageName, ResolvedDependency>, ResolveError> {
        let visited = &mut HashSet::new();
        let mut dependencies = Vec::new();
        let mut to_visit: Vec<(PackageName, ResolvedVersion)> = vec![];
        let deps = myco_toml.clone_deps();
        for (name, dependency) in deps {
            match dependency {
                PackageVersionEntry::Version(version) => {
                    let resolved_version = self.resolve_version(&name, &version).await?;
                    if let Some(resolved_version) = resolved_version {
                        to_visit.push((name, resolved_version));
                    } else {
                        eprintln!("Could not resolve dependency {} {}", name, version);
                    }
                }
                PackageVersionEntry::Url { url } => {
                    dependencies.push((name.clone(), ResolvedDependency::Url(url)));
                }
            }
        }
        while let Some((name, version)) = to_visit.pop() {
            if visited.contains(&version) {
                continue;
            }
            let myco_toml = get_myco_toml(&version).await?;
            visited.insert(version.clone());
            dependencies.push((name, ResolvedDependency::Version(version)));
            let deps = myco_toml.into_deps();
            for (name, dependency) in deps {
                match dependency {
                    PackageVersionEntry::Version(version) => {
                        let resolved_version = self.resolve_version(&name, &version).await?;
                        if let Some(resolved_version) = resolved_version {
                            to_visit.push((name, resolved_version));
                        } else {
                            eprintln!("Could not resolve dependency {} {}", name, version);
                        }
                    }
                    PackageVersionEntry::Url { url } => {
                        dependencies.push((name.clone(), ResolvedDependency::Url(url)));
                    }
                }
            }
        };
        // Filter versions to only include the highest version of each dependency name
        let mut versions_map = BTreeMap::new();
        for (name, dependency) in dependencies {
            if !versions_map.contains_key(&name) {
                versions_map.insert(name.clone(), dependency);
            } else {
                let existing_dependency = versions_map.get(&name).unwrap();
                match (existing_dependency, dependency.clone()) {
                    (ResolvedDependency::Version(existing_version), ResolvedDependency::Version(version)) => {
                        if version.version > existing_version.version {
                            versions_map.insert(name, dependency);
                        }
                    }
                    (ResolvedDependency::Url(_), ResolvedDependency::Version(_)) => {
                        // Keep the path
                    }
                    (ResolvedDependency::Version(_), ResolvedDependency::Url(_)) => {
                        versions_map.insert(name, dependency);
                    }
                    (ResolvedDependency::Url(existing_url), ResolvedDependency::Url(url)) => {
                        if &url != existing_url {
                            eprintln!("Conflicting paths for dependency {}", name);
                        }
                    }
                }
            }
        }
        Ok(versions_map)
    }

    pub fn resolve_all_blocking(&mut self, myco_toml: &MycoToml) -> Result<BTreeMap<PackageName, ResolvedDependency>, ResolveError> {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(self.resolve_all(myco_toml))
    }
}

async fn get_myco_toml(version: &ResolvedVersion) -> Result<MycoToml, ResolveError> {
    let myco_toml: MycoToml = registry::fetch_contents(&version.toml_url).await?;
    Ok(myco_toml)
}
