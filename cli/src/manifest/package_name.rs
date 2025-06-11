use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use crate::errors::MycoError;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct PackageName {
    pub namespaces: Vec<String>,
    pub name: String,
}

impl PackageName {
    pub fn namespaces_to_string(&self) -> String {
        let mut namespaces = String::new();
        namespaces.push('@');
        namespaces.push_str(&self.namespaces.join("."));
        namespaces
    }

    pub fn from_str<T: AsRef<str>>(package_name: T) -> Result<Self, MycoError> {
        let package_name = package_name.as_ref();
        if !package_name.is_ascii() {
            return Err(MycoError::InvalidPackageName { 
                name: package_name.to_string() 
            });
        }
        if !package_name.starts_with('@') {
            return Err(MycoError::InvalidPackageName { 
                name: package_name.to_string() 
            });
        }
        let package_name = &package_name[1..];
        let mut parts = package_name.splitn(2, '/');
        let namespaces = parts.next();
        if namespaces.is_none() {
            return Err(MycoError::InvalidPackageName { 
                name: package_name.to_string() 
            });
        }
        let namespaces = namespaces.unwrap().split('.').map(|s| s.to_owned()).collect();
        let name = parts.next();
        if name.is_none() {
            return Err(MycoError::InvalidPackageName { 
                name: package_name.to_string() 
            });
        }
        let name = name.unwrap().to_owned();
        Ok(Self {
            namespaces,
            name,
        })
    }

    pub fn to_string(&self) -> String {
        let mut package_name = String::new();
        package_name.push_str(&self.namespaces_to_string());
        package_name.push('/');
        package_name.push_str(&self.name);
        package_name
    }
}

impl FromStr for PackageName {
    type Err = MycoError;

    fn from_str(package_name: &str) -> Result<Self, Self::Err> {
        Self::from_str(package_name)
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl Serialize for PackageName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for PackageName {
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

    #[test]
    fn test_converting_back_and_forth() {
        let initial = "@myco.core/ops";
        let package_name = PackageName::from_str(initial).unwrap();
        let package_name = package_name.to_string();
        assert_eq!(package_name, initial);
    }
}
