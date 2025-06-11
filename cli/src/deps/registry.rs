use std::{cmp::{Ord, Ordering}, fmt::Display};
use colored::*;

use serde::{Deserialize, Serialize};

use crate::manifest::{Location, PackageName, PackageVersion};
use crate::errors::MycoError;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegistryPackage {
    pub name: PackageName,
    pub versions: Vec<VersionEntry>,
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

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ResolvedVersion {
    pub name: PackageName,
    pub version: PackageVersion,
    pub pack_url: Location,
    pub toml_url: Location,
    pub integrity: String,
}

impl ResolvedVersion {
    pub fn new(
        name: PackageName,
        location: &Location,
        version_entry: &VersionEntry,
    ) -> Result<Self, MycoError> {
        let pack_url = location.join(&format!("{}.zip", &version_entry.version))?;
        let toml_url = location.join(&format!("{}.toml", &version_entry.version))?;
        Ok(Self {
            name,
            version: version_entry.version.clone(),
            pack_url,
            toml_url,
            integrity: version_entry.integrity.clone(),
        })
    }

    pub fn diff(&self, other: &ResolvedVersion) -> Option<ResolvedVersionDiff> {
        if self == other {
            None
        } else {
            Some(ResolvedVersionDiff {
                name: self.name.clone(), // The name should always be the same
                version: (self.version != other.version)
                    .then_some((self.version.clone(), other.version.clone())),
                pack_url: (self.pack_url != other.pack_url)
                    .then_some((self.pack_url.clone(), other.pack_url.clone())),
                toml_url: (self.toml_url != other.toml_url)
                    .then_some((self.toml_url.clone(), other.toml_url.clone())),
                integrity: (self.integrity != other.integrity)
                    .then_some((self.integrity.clone(), other.integrity.clone())),
            })
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ResolvedVersionDiff {
    pub name: PackageName,
    pub version: Option<(PackageVersion, PackageVersion)>,
    pub pack_url: Option<(Location, Location)>,
    pub toml_url: Option<(Location, Location)>,
    pub integrity: Option<(String, String)>,
}

impl Display for ResolvedVersionDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        
        writeln!(f, "  {}:", self.name.to_string())?;
            
        if let Some((old, new)) = &self.version {
            writeln!(f, "    version: {} -> {}", old.to_string().red(), new.to_string().green())?;
        }
        if let Some((old, new)) = &self.pack_url {
            writeln!(f, "    pack_url: {} -> {}", old.to_string().red(), new.to_string().green())?;
        }
        if let Some((old, new)) = &self.toml_url {
            writeln!(f, "    toml_url: {} -> {}", old.to_string().red(), new.to_string().green())?;
        }
        if let Some((old, new)) = &self.integrity {
            writeln!(f, "    integrity: {} -> {}", old.red(), new.green())?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Registry {
    pub namespace: Vec<RegistryNamespace>,
}

impl Registry {
    pub fn resolve_package(
        &self,
        location: &Location,
        package_name: &PackageName,
    ) -> Result<Option<RegistryPackage>, MycoError> {
        for namespace in &self.namespace {
            if let Some(package) = namespace.resolve_package(location, package_name)? {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }

    pub fn resolve_version(
        &self,
        location: &Location,
        package_name: &PackageName,
        version: &PackageVersion,
    ) -> Result<Option<ResolvedVersion>, MycoError> {
        let resolved = self.resolve_package(&location, &package_name)?;
        if let Some(package) = resolved {
            let version = package.versions.into_iter().find(|v| v.version == *version);
            if let Some(version) = version {
                let package_location = location.join(&format!("{}/", package.name))?;
                let version = ResolvedVersion::new(package.name.clone(), &package_location, &version)?;
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
    pub fn resolve_package(
        &self,
        location: &Location,
        package_name: &PackageName,
    ) -> Result<Option<RegistryPackage>, MycoError> {
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

async fn fetch_url_contents<T, S: AsRef<str>>(url: S) -> Result<T, MycoError>
where
    T: serde::de::DeserializeOwned,
{
    let url = url.as_ref();
    let text = if url.starts_with("http://") || url.starts_with("https://") {
        let resp = reqwest::get(url).await
            .map_err(|e| MycoError::PackageDownload { 
                url: url.to_string(), 
                source: Box::new(e) 
            })?;
        resp.text().await
            .map_err(|e| MycoError::PackageDownload { 
                url: url.to_string(), 
                source: Box::new(e) 
            })
    } else if url.starts_with("file://") {
        let url = url.trim_start_matches("file://");
        std::fs::read_to_string(&url)
            .map_err(|e| MycoError::ReadFile { 
                path: url.to_string(), 
                source: e 
            })
    } else {
        Err(MycoError::InvalidUrl { 
            url: url.to_string() 
        })
    }?;
    toml::from_str(&text)
        .map_err(|e| MycoError::ManifestParse { source: e })
}

pub async fn fetch_contents<T>(location: &Location) -> Result<T, MycoError>
where
    T: serde::de::DeserializeOwned,
{
    Ok(match location {
        Location::Url(url) => fetch_url_contents(url.as_str()).await?,
        Location::Path { path } => tokio::fs::read_to_string(path)
            .await
            .map_err(|e| MycoError::ReadFile { 
                path: path.display().to_string(), 
                source: e 
            })
            .and_then(|text| toml::from_str(&text)
                .map_err(|e| MycoError::ManifestParse { source: e }))?,
    })
}
