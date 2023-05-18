use std::fmt::Display;
use std::str::FromStr;
use anyhow::{anyhow, Error as AnyError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct PackageName {
    pub namespaces: Vec<String>,
    pub name: String,
}

impl PackageName {
    pub fn namespaces_to_string(&self) -> String {
        let mut namespaces = String::new();
        for namespace in &self.namespaces {
            namespaces.push_str(namespace);
            namespaces.push('.');
        }
        namespaces
    }

    pub fn from_str<T: AsRef<str>>(package_name: T) -> Result<Self, AnyError> {
        let package_name = package_name.as_ref();
        if !package_name.is_ascii() {
            return Err(anyhow!("Package name must be ASCII"));
        }
        if !package_name.starts_with('@') {
            return Err(anyhow!("Package name must start with @"));
        }
        let mut parts = package_name.splitn(2, '/');
        let namespaces = parts.next().unwrap().split('.').map(|s| s.to_owned()).collect();
        let name = parts.next().unwrap().to_owned();
        Ok(Self {
            namespaces,
            name,
        })
    }

    pub fn to_string(&self) -> String {
        let mut package_name = String::new();
        package_name.push_str(&self.namespaces_to_string());
        package_name.push_str(&self.name);
        package_name
    }
}

impl FromStr for PackageName {
    type Err = AnyError;

    fn from_str(package_name: &str) -> Result<Self, Self::Err> {
        Self::from_str(package_name)
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@{}", self.to_string())
    }
}

impl Serialize for PackageName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl <'de> Deserialize<'de> for PackageName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let package_name = String::deserialize(deserializer)?;
        Self::from_str(package_name).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_name_from_str() {
        let package_name = PackageName::from_str("@myco/core").unwrap();
        assert_eq!(package_name.namespaces, vec!["myco".to_owned()]);
        assert_eq!(package_name.name, "core".to_owned());
    }

    #[test]
    fn test_package_name_to_string() {
        let package_name = PackageName {
            namespaces: vec!["myco".to_owned()],
            name: "core".to_owned(),
        };
        assert_eq!(package_name.to_string(), "@myco/core".to_owned());
    }

    #[test]
    fn test_package_name_multiple_namespaces_from_str() {
        let package_name = PackageName::from_str("@myco.core/ops").unwrap();
        assert_eq!(package_name.namespaces, vec!["myco".to_owned(), "core".to_owned()]);
        assert_eq!(package_name.name, "ops".to_owned());
    }

    #[test]
    fn test_package_name_multiple_namespaces_to_string() {
        let package_name = PackageName {
            namespaces: vec!["myco".to_owned(), "core".to_owned()],
            name: "ops".to_owned(),
        };
        assert_eq!(package_name.to_string(), "@myco.core/ops".to_owned());
    }
}
