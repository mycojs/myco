use std::{collections::HashMap, fmt::Display};
use colored::*;

use anyhow::Error;
use serde::{Deserialize, Serialize};

use super::registry::{ResolvedVersion, ResolvedVersionDiff};

#[derive(Serialize, Deserialize)]
pub struct LockFile {
    pub package: Vec<ResolvedVersion>,
}

impl LockFile {
    pub fn save(&self) -> Result<(), std::io::Error> {
        std::fs::write("myco-lock.toml", toml::to_string_pretty(self).unwrap())
    }

    pub fn load() -> Result<Self, Error> {
        let contents = std::fs::read_to_string("myco-lock.toml");
        match contents {
            Ok(contents) => toml::from_str(&contents).map_err(|e| Error::new(e)),
            Err(e) => Err(e.into())
        }
    }

    pub fn new() -> Self {
        Self {
            package: Vec::new(),
        }
    }

    pub fn diff(&self, other: &LockFile) -> LockFileDiff {
        let mut diffs = Vec::new();
        let mut new = Vec::new();
        let mut removed = Vec::new();

        // Create lookup map for self packages
        let self_packages: HashMap<_, _> = self.package
            .iter()
            .map(|p| (&p.name, p))
            .collect();

        // Create lookup map for other packages
        let other_packages: HashMap<_, _> = other.package
            .iter()
            .map(|p| (&p.name, p))
            .collect();

        // Find modified and removed packages
        for (name, self_pkg) in self_packages.iter() {
            if let Some(other_pkg) = other_packages.get(name) {
                if let Some(diff) = self_pkg.diff(other_pkg) {
                    diffs.push(diff);
                }
            } else {
                removed.push((*self_pkg).clone());
            }
        }

        // Find new packages
        for (name, other_pkg) in other_packages.iter() {
            if !self_packages.contains_key(name) {
                new.push((*other_pkg).clone());
            }
        }

        LockFileDiff {
            diffs,
            new,
            removed,
        }
    }
}


#[derive(Serialize, Deserialize)]
pub struct LockFileDiff {
    pub diffs: Vec<ResolvedVersionDiff>,
    pub new: Vec<ResolvedVersion>,
    pub removed: Vec<ResolvedVersion>,
}

impl Display for LockFileDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.diffs.is_empty() && self.new.is_empty() && self.removed.is_empty() {
            return writeln!(f, "No changes detected");
        }

        // Show modified packages
        if !self.diffs.is_empty() {
            writeln!(f, "{}", "Package changes:")?;
            for diff in &self.diffs {
                diff.fmt(f)?;
            }
        }

        // Show new packages
        if !self.new.is_empty() {
            writeln!(f, "\nNew packages:")?;
            for package in &self.new {
                writeln!(f, "  {} {}", package.name.to_string().green(), package.version.to_string().green())?;
            }
        }

        // Show removed packages
        if !self.removed.is_empty() {
            writeln!(f, "\nRemoved packages:")?;
            for package in &self.removed {
                writeln!(f, "  {} {}", package.name.to_string().red(), package.version.to_string().red())?;
            }
        }

        Ok(())
    }
}
