use std::collections::HashSet;

use crate::errors::MycoError;

use crate::manifest::{Location, MycoToml, PackageName, PackageVersion};

use super::{
    lockfile::LockFile,
    registry::{fetch_contents, Registry, RegistryPackage, ResolvedVersion},
};

pub struct Resolver {
    pub registries: Vec<Location>,
}

impl Resolver {
    pub fn new(registries: Vec<Location>) -> Self {
        Self { registries }
    }

    async fn resolve_version(&mut self, package_name: &PackageName, version: &PackageVersion) -> Result<Option<ResolvedVersion>, MycoError> {
        for registry_location in &self.registries {
            let registry: Registry = fetch_contents(registry_location).await?;
            let version = registry.resolve_version(registry_location, package_name, version)?;
            if version.is_some() {
                return Ok(version);
            }
        }
        Ok(None)
    }

    pub async fn resolve_package(&mut self, package_name: &PackageName) -> Result<Option<RegistryPackage>, MycoError> {
        for registry_location in &self.registries {
            let registry: Registry = fetch_contents(registry_location).await?;
            let package = registry.resolve_package(registry_location, package_name)?;
            if package.is_some() {
                return Ok(package);
            }
        }
        Ok(None)
    }

    pub async fn generate_lockfile(&mut self, myco_toml: &MycoToml) -> Result<LockFile, MycoError> {
        let mut lockfile = LockFile::new();
        let mut to_visit: Vec<ResolvedVersion> = Vec::new();

        // First, resolve all top-level dependencies
        for (package_name, version) in myco_toml.clone_deps() {
            let resolved_version = self
                .resolve_version(&package_name, &version)
                .await?
                .ok_or_else(|| MycoError::PackageNotFound { 
                    package: package_name.to_string() 
                })?;
            to_visit.push(resolved_version.clone());
            lockfile.package.push(resolved_version);
        }

        // Then, resolve all dependencies of those dependencies
        self.resolve_package_deps(myco_toml, &mut to_visit).await?;

        // Deduplicate lockfile packages
        let mut seen: HashSet<PackageName> = HashSet::new();
        lockfile.package.retain(|p| seen.insert(p.name.clone()));

        Ok(lockfile)
    }

    pub async fn resolve_package_deps(&mut self, _myco_toml: &MycoToml, to_visit: &mut Vec<ResolvedVersion>) -> Result<(), MycoError> {
        let mut visited: HashSet<PackageName> = HashSet::new();
        while let Some(package) = to_visit.pop() {
            if visited.contains(&package.name) {
                continue;
            }
            visited.insert(package.name.clone());

            let package_myco_toml = get_myco_toml(&package).await?;

            for (dep_name, dep_version) in package_myco_toml.clone_deps() {
                let resolved_dep = self
                    .resolve_version(&dep_name, &dep_version)
                    .await?
                    .ok_or_else(|| MycoError::PackageNotFound { 
                        package: dep_name.to_string() 
                    })?;
                to_visit.push(resolved_dep);
            }
        }
        Ok(())
    }
}

async fn get_myco_toml(version: &ResolvedVersion) -> Result<MycoToml, MycoError> {
    fetch_contents(&version.toml_url).await
}
