use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use anyhow::anyhow;

use crate::AnyError;

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct PackageVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub prerelease: Option<String>,
}

impl FromStr for PackageVersion {
    type Err = AnyError;

    fn from_str(string: &str) -> Result<Self, AnyError> {
        let string = if string.starts_with("v") {
            &string[1..]
        } else {
            string
        };
        let mut parts = string.split('.');
        let major = next_u16(&mut parts)?;
        let minor = next_u16(&mut parts)?;
        let patch_string = next_str(&mut parts)?;
        let (patch, prerelease) = if let Some(index) = patch_string.find('-') {
            let (patch, prerelease) = patch_string.split_at(index);
            let prerelease = &prerelease[1..];
            (parse_u16(patch)?, Some(prerelease.to_string()))
        } else {
            (parse_u16(patch_string)?, None)
        };
        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

impl Display for PackageVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(prerelease) = &self.prerelease {
            write!(f, "-{}", prerelease)?;
        }
        Ok(())
    }
}

impl Ord for PackageVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let major = self.major.cmp(&other.major);
        let minor = self.minor.cmp(&other.minor);
        let patch = self.patch.cmp(&other.patch);
        let prerelease = match (&self.prerelease, &other.prerelease) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        major
            .then(minor)
            .then(patch)
            .then(prerelease)
    }
}

impl PartialOrd for PackageVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl serde::Serialize for PackageVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'a> serde::Deserialize<'a> for PackageVersion {
    fn deserialize<D: serde::Deserializer<'a>>(deserializer: D) -> Result<Self, D::Error> {
        let string = String::deserialize(deserializer)?;
        PackageVersion::from_str(&string).map_err(serde::de::Error::custom)
    }
}

fn next_str<'a>(parts: &'a mut std::str::Split<'_, char>) -> Result<&'a str, AnyError> {
    Ok(parts.next()
        .ok_or(anyhow!("Invalid version string"))?)
}

fn parse_u16<T: AsRef<str>>(string: T) -> Result<u16, AnyError> {
    Ok(string.as_ref()
        .parse()
        .map_err(|e| anyhow!("Invalid version string: {}", e))?)
}

fn next_u16(parts: &mut std::str::Split<'_, char>) -> Result<u16, AnyError> {
    Ok(parse_u16(next_str(parts)?)?)
}

impl PackageVersion {
    pub fn next_major(&self) -> Self {
        Self {
            major: self.major + 1,
            minor: 0,
            patch: 0,
            prerelease: None,
        }
    }
    
    pub fn next_minor(&self) -> Self {
        Self {
            major: self.major,
            minor: self.minor + 1,
            patch: 0,
            prerelease: None,
        }
    }
    
    pub fn next_patch(&self) -> Self {
        Self {
            major: self.major,
            minor: self.minor,
            patch: self.patch + 1,
            prerelease: None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_version_from_str() {
        assert_eq!(PackageVersion::from_str("1.2.3").unwrap(), PackageVersion {
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: None,
        });
        assert_eq!(PackageVersion::from_str("v1.2.3").unwrap(), PackageVersion {
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: None,
        });
        assert_eq!(PackageVersion::from_str("1.2.3-alpha").unwrap(), PackageVersion {
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: Some("alpha".to_string()),
        });
        assert_eq!(PackageVersion::from_str("v1.2.3-alpha").unwrap(), PackageVersion {
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: Some("alpha".to_string()),
        });
    }

    #[test]
    fn test_display() {
        let versions_to_test = vec![
            "1.2.3",
            "1.2.3-alpha",
        ];
        for version in versions_to_test {
            assert_eq!(version, format!("{}", PackageVersion::from_str(version).unwrap()));
        }
    }

    #[test]
    fn test_version_from_str_invalid() {
        assert!(PackageVersion::from_str("1.2").is_err());
        assert!(PackageVersion::from_str("1.2.a").is_err());
    }

    #[test]
    fn test_ordering_versions() {
        assert!(PackageVersion::from_str("1.2.3").unwrap() < PackageVersion::from_str("1.2.4").unwrap());
        assert!(PackageVersion::from_str("1.2.3").unwrap() < PackageVersion::from_str("1.3.0").unwrap());
        assert!(PackageVersion::from_str("1.2.3").unwrap() < PackageVersion::from_str("2.0.0").unwrap());
        assert!(PackageVersion::from_str("1.2.3").unwrap() < PackageVersion::from_str("1.2.4-alpha").unwrap());
        assert_eq!(PackageVersion::from_str("1.2.3").unwrap(), PackageVersion::from_str("1.2.3").unwrap());
        assert!(PackageVersion::from_str("1.2.3-alpha").unwrap() > PackageVersion::from_str("1.2.2").unwrap());
        assert!(PackageVersion::from_str("1.2.3-alpha").unwrap() < PackageVersion::from_str("1.2.3").unwrap());
        assert!(PackageVersion::from_str("1.2.3-alpha").unwrap() < PackageVersion::from_str("1.2.3-beta").unwrap());
        assert!(PackageVersion::from_str("1.2.3-alpha").unwrap() < PackageVersion::from_str("1.2.3-beta").unwrap());
        assert_eq!(PackageVersion::from_str("1.2.3-alpha").unwrap(), PackageVersion::from_str("1.2.3-alpha").unwrap());
    }
}