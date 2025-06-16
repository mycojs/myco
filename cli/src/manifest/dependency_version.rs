use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::manifest::PackageVersion;

/// A dependency version specification in myco.toml
/// Can be either a specific version or a workspace dependency
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyVersion {
    Version(PackageVersion),
    Workspace { workspace: bool },
}

impl Display for DependencyVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyVersion::Version(version) => write!(f, "{}", version),
            DependencyVersion::Workspace { .. } => write!(f, "workspace"),
        }
    }
}
