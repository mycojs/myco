use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::deps;
use crate::errors::MycoError;
use crate::manifest::{myco_local::MycoLocalToml, DependencyVersion, MycoToml, WorkspaceManifest};

#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    pub name: String,
    pub path: PathBuf,
    pub manifest: MycoToml,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub manifest: WorkspaceManifest,
    pub members: Vec<WorkspaceMember>,
}

impl Workspace {
    /// Discover a workspace starting from the given directory
    pub fn discover(start_dir: PathBuf) -> Result<Self, MycoError> {
        let (root, manifest) = WorkspaceManifest::load_nearest(start_dir)?;

        let mut members = Vec::new();
        for member_path in &manifest.workspace.members {
            let member_dir = root.join(member_path);

            // Try to load the member's myco.toml
            let member_manifest_path = member_dir.join("myco.toml");
            if !member_manifest_path.exists() {
                return Err(MycoError::ManifestNotFound {
                    start_dir: member_dir.display().to_string(),
                });
            }

            let contents = std::fs::read_to_string(&member_manifest_path).map_err(|e| {
                MycoError::ReadFile {
                    path: member_manifest_path.display().to_string(),
                    source: e,
                }
            })?;

            let member_manifest: MycoToml =
                toml::from_str(&contents).map_err(|e| MycoError::ManifestParse { source: e })?;

            // Get the package name from the manifest
            let name = member_manifest
                .package
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| member_path.clone());

            members.push(WorkspaceMember {
                name,
                path: member_dir,
                manifest: member_manifest,
            });
        }

        Ok(Workspace {
            root,
            manifest,
            members,
        })
    }

    /// Get a member by name
    pub fn get_member(&self, name: &str) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| m.name == name)
    }

    /// Get all member names
    pub fn member_names(&self) -> Vec<String> {
        self.members.iter().map(|m| m.name.clone()).collect()
    }

    /// Find a workspace member by package name
    pub fn find_member_by_name(&self, name: &str) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| {
            if let Some(package) = &m.manifest.package {
                package.name == name
            } else {
                false
            }
        })
    }
}

/// Install dependencies for all workspace members
pub fn install_workspace(workspace: &Workspace, save: bool) -> Result<(), MycoError> {
    // First, install workspace dependencies and generate myco-local.toml files
    for member in &workspace.members {
        install_member_workspace_deps(workspace, member)?;
    }

    // Collect all external dependencies from all workspace members
    let mut aggregated_deps = BTreeMap::new();
    for member in &workspace.members {
        if let Some(deps) = &member.manifest.deps {
            for (dep_name, dep_version) in deps {
                // Only include external dependencies (not workspace dependencies)
                if !matches!(dep_version, DependencyVersion::Workspace { .. }) {
                    aggregated_deps.insert(dep_name.clone(), dep_version.clone());
                }
            }
        }
    }

    // If we have external dependencies, resolve them centrally
    if !aggregated_deps.is_empty() && save {
        // Create an aggregated manifest for dependency resolution
        let aggregated_manifest = MycoToml {
            package: None, // Workspace root doesn't have package info
            run: workspace.manifest.run.clone(),
            registries: workspace.manifest.registries.clone(),
            deps: Some(aggregated_deps),
            tsconfig: workspace.manifest.tsconfig.clone(),
        };

        // Change to workspace root to generate lockfile there
        let original_dir =
            std::env::current_dir().map_err(|e| MycoError::CurrentDirectory { source: e })?;
        std::env::set_current_dir(&workspace.root)
            .map_err(|e| MycoError::CurrentDirectory { source: e })?;

        let result = deps::install(aggregated_manifest, save);

        std::env::set_current_dir(&original_dir)
            .map_err(|e| MycoError::CurrentDirectory { source: e })?;

        result?;
    }

    // Then run individual dependency installation for each member (for local files and tsconfig generation)
    for member in &workspace.members {
        let original_dir =
            std::env::current_dir().map_err(|e| MycoError::CurrentDirectory { source: e })?;
        std::env::set_current_dir(&member.path)
            .map_err(|e| MycoError::CurrentDirectory { source: e })?;

        // Create a modified manifest without workspace dependencies for local install
        let mut filtered_manifest = member.manifest.clone();
        let has_external_deps = if let Some(deps) = &filtered_manifest.deps {
            let non_workspace_deps: BTreeMap<_, _> = deps
                .iter()
                .filter(|(_, version)| !matches!(version, DependencyVersion::Workspace { .. }))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let has_deps = !non_workspace_deps.is_empty();
            filtered_manifest.deps = if has_deps {
                Some(non_workspace_deps)
            } else {
                None
            };
            has_deps
        } else {
            false
        };

        // Only run deps::install if there are external dependencies to install
        // Otherwise, just generate the local files (tsconfig.json, .myco directory)
        let result = if has_external_deps {
            // Only install local files, not the lockfile (that's centralized)
            deps::install(filtered_manifest, false)
        } else {
            // Generate just the local files without dependencies
            install_local_files_only(&filtered_manifest)
        };

        std::env::set_current_dir(&original_dir)
            .map_err(|e| MycoError::CurrentDirectory { source: e })?;

        result?;
    }

    Ok(())
}

/// Install workspace dependencies for a single member
fn install_member_workspace_deps(
    workspace: &Workspace,
    member: &WorkspaceMember,
) -> Result<(), MycoError> {
    let mut local_toml = MycoLocalToml::default();
    let mut has_workspace_deps = false;

    // Process workspace dependencies
    for (dep_name, dep_version) in member.manifest.clone_deps() {
        if let DependencyVersion::Workspace { .. } = dep_version {
            has_workspace_deps = true;

            // Extract the actual package name from dependency name
            // Handle @local/ prefix (e.g., "@local/lib-std" -> "lib-std")
            let package_name = if dep_name.to_string().starts_with("@local/") {
                dep_name
                    .to_string()
                    .strip_prefix("@local/")
                    .unwrap_or(&dep_name.to_string())
                    .to_string()
            } else {
                dep_name.to_string()
            };

            // Find the corresponding workspace member
            if let Some(target_member) = workspace.find_member_by_name(&package_name) {
                // Calculate relative path from current member to target member
                let relative_path = calculate_relative_path(&member.path, &target_member.path)?;
                local_toml.add_resolve(dep_name.to_string(), relative_path);
            } else {
                return Err(MycoError::DependencyResolution {
                    message: format!("Workspace dependency '{}' (resolved to '{}') not found in workspace members", dep_name, package_name),
                });
            }
        }
    }

    // Only create myco-local.toml if there are workspace dependencies
    if has_workspace_deps {
        local_toml.save_blocking(member.path.clone())?;
    }

    Ok(())
}

/// Install just the local files (.myco directory and tsconfig.json) without dependencies
fn install_local_files_only(myco_toml: &MycoToml) -> Result<(), MycoError> {
    use crate::deps::tsconfig;

    // Create .myco directory and myco.d.ts file
    std::fs::create_dir_all(".myco").map_err(|e| MycoError::DirectoryCreation {
        path: ".myco".to_string(),
        source: e,
    })?;

    // Use the same constant from deps::mod
    const MYCO_DTS: &str = include_str!("../../runtime/.myco/myco.d.ts");
    std::fs::write(".myco/myco.d.ts", MYCO_DTS).map_err(|e| MycoError::FileWrite {
        path: ".myco/myco.d.ts".to_string(),
        source: e,
    })?;

    // Create tsconfig.json dynamically based on myco.toml configuration
    let tsconfig_content = tsconfig::generate_tsconfig_json(myco_toml)?;

    std::fs::write("tsconfig.json", tsconfig_content).map_err(|e| MycoError::FileWrite {
        path: "tsconfig.json".to_string(),
        source: e,
    })?;

    Ok(())
}

/// Calculate relative path from source to target
fn calculate_relative_path(from: &PathBuf, to: &PathBuf) -> Result<String, MycoError> {
    let relative = pathdiff::diff_paths(to, from).ok_or_else(|| MycoError::Internal {
        message: format!(
            "Could not calculate relative path from {} to {}",
            from.display(),
            to.display()
        ),
    })?;

    Ok(relative.to_string_lossy().to_string())
}

/// Run a script in all workspace members that define it
pub fn run_workspace_script(
    workspace: &Workspace,
    script: &str,
    package_filters: &[String],
) -> Result<(), MycoError> {
    let mut members_to_run = Vec::new();

    // Determine which members to run the script in
    if package_filters.is_empty() {
        // Run in all members that have the script
        for member in &workspace.members {
            if member_has_script(member, script) {
                members_to_run.push(member);
            }
        }
    } else {
        // Run only in specified packages
        for package_name in package_filters {
            if let Some(member) = workspace.get_member(package_name) {
                if member_has_script(member, script) {
                    members_to_run.push(member);
                } else {
                    eprintln!(
                        "Warning: Package '{}' does not define script '{}'",
                        package_name, script
                    );
                }
            } else {
                return Err(MycoError::Internal {
                    message: format!("Package '{}' not found in workspace", package_name),
                });
            }
        }
    }

    if members_to_run.is_empty() {
        println!("No workspace members define script '{}'", script);
        return Ok(());
    }

    // Execute the script in each member sequentially
    for member in members_to_run {
        println!("Running '{}' in {}", script, member.name);

        let original_dir =
            std::env::current_dir().map_err(|e| MycoError::CurrentDirectory { source: e })?;
        std::env::set_current_dir(&member.path)
            .map_err(|e| MycoError::CurrentDirectory { source: e })?;

        let exit_code = crate::run::run(&member.manifest, &script.to_string(), None)?;

        std::env::set_current_dir(&original_dir)
            .map_err(|e| MycoError::CurrentDirectory { source: e })?;

        if exit_code != 0 {
            return Err(MycoError::ScriptExecution {
                message: format!(
                    "Script '{}' failed in package '{}' with exit code {}",
                    script, member.name, exit_code
                ),
            });
        }
    }

    Ok(())
}

/// Check if a workspace member has a script defined
fn member_has_script(member: &WorkspaceMember, script: &str) -> bool {
    if let Some(run_scripts) = &member.manifest.run {
        run_scripts.contains_key(script)
    } else {
        false
    }
}
