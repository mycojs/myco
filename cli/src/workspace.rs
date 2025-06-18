use log::{debug, error, info, warn};
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
        debug!("Discovering workspace from: {}", start_dir.display());
        let (root, manifest) = WorkspaceManifest::load_nearest(start_dir)?;
        info!("Found workspace root at: {}", root.display());

        debug!(
            "Workspace has {} members configured",
            manifest.workspace.members.len()
        );
        let mut members = Vec::new();

        for member_path in &manifest.workspace.members {
            let member_dir = root.join(member_path);
            debug!("Processing workspace member: {}", member_dir.display());

            // Try to load the member's myco.toml
            let member_manifest_path = member_dir.join("myco.toml");
            if !member_manifest_path.exists() {
                return Err(MycoError::ManifestNotFound {
                    start_dir: member_dir.display().to_string(),
                });
            }

            debug!(
                "Loading member manifest: {}",
                member_manifest_path.display()
            );
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

            debug!(
                "Found workspace member '{}' at: {}",
                name,
                member_dir.display()
            );
            members.push(WorkspaceMember {
                name,
                path: member_dir,
                manifest: member_manifest,
            });
        }

        info!(
            "Successfully discovered workspace with {} members",
            members.len()
        );
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
    info!(
        "Installing workspace dependencies for {} members",
        workspace.members.len()
    );
    debug!("Save lockfile: {}", save);

    // First, install workspace dependencies and generate myco-local.toml files
    info!("Installing workspace-specific dependencies");
    for member in &workspace.members {
        debug!("Processing workspace dependencies for: {}", member.name);
        install_member_workspace_deps(workspace, member)?;
    }

    // Collect all external dependencies from all workspace members
    info!("Collecting external dependencies from all workspace members");
    let mut aggregated_deps = BTreeMap::new();
    for member in &workspace.members {
        if let Some(deps) = &member.manifest.deps {
            debug!(
                "Processing {} dependencies from member: {}",
                deps.len(),
                member.name
            );
            for (dep_name, dep_version) in deps {
                // Only include external dependencies (not workspace dependencies)
                if !matches!(dep_version, DependencyVersion::Workspace { .. }) {
                    debug!(
                        "Adding external dependency: {} = {:?}",
                        dep_name, dep_version
                    );
                    aggregated_deps.insert(dep_name.clone(), dep_version.clone());
                }
            }
        }
    }

    info!(
        "Found {} unique external dependencies",
        aggregated_deps.len()
    );

    // If we have external dependencies, resolve them centrally
    if !aggregated_deps.is_empty() && save {
        info!("Resolving external dependencies centrally at workspace root");
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
            std::env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
        debug!("Changing to workspace root: {}", workspace.root.display());
        std::env::set_current_dir(&workspace.root).map_err(|_e| {
            MycoError::SetCurrentDirectory {
                dir: workspace.root.display().to_string(),
            }
        })?;

        let result = deps::install(aggregated_manifest, save);

        debug!("Restoring original directory: {}", original_dir.display());
        std::env::set_current_dir(&original_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: original_dir.display().to_string(),
        })?;

        result?;
        info!("Successfully resolved external dependencies");
    } else if aggregated_deps.is_empty() {
        debug!("No external dependencies to resolve");
    } else {
        debug!("Skipping central dependency resolution (save=false)");
    }

    // Then run individual dependency installation for each member (for local files and tsconfig generation)
    info!("Installing local files for each workspace member");
    for member in &workspace.members {
        debug!("Processing member: {}", member.name);
        let original_dir =
            std::env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
        debug!("Changing to member directory: {}", member.path.display());
        std::env::set_current_dir(&member.path)
            .map_err(|e| MycoError::GetCurrentDirectory { source: e })?;

        // Create a modified manifest without workspace dependencies for local install
        let mut filtered_manifest = member.manifest.clone();
        let has_external_deps = if let Some(deps) = &filtered_manifest.deps {
            let non_workspace_deps: BTreeMap<_, _> = deps
                .iter()
                .filter(|(_, version)| !matches!(version, DependencyVersion::Workspace { .. }))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let has_deps = !non_workspace_deps.is_empty();
            debug!(
                "Member {} has {} external dependencies",
                member.name,
                if has_deps {
                    non_workspace_deps.len()
                } else {
                    0
                }
            );
            filtered_manifest.deps = if has_deps {
                Some(non_workspace_deps)
            } else {
                None
            };
            has_deps
        } else {
            debug!("Member {} has no dependencies", member.name);
            false
        };

        // Only run deps::install if there are external dependencies to install
        // Otherwise, just generate the local files (tsconfig.json, .myco directory)
        let result = if has_external_deps {
            debug!(
                "Installing external dependencies for member: {}",
                member.name
            );
            // Only install local files, not the lockfile (that's centralized)
            deps::install(filtered_manifest, false)
        } else {
            debug!("Generating local files only for member: {}", member.name);
            // Generate just the local files without dependencies
            install_local_files_only(&filtered_manifest)
        };

        debug!("Restoring original directory: {}", original_dir.display());
        std::env::set_current_dir(&original_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: original_dir.display().to_string(),
        })?;

        result?;
        debug!("Successfully processed member: {}", member.name);
    }

    info!("Workspace installation completed successfully");
    Ok(())
}

/// Install workspace dependencies for a single member
fn install_member_workspace_deps(
    workspace: &Workspace,
    member: &WorkspaceMember,
) -> Result<(), MycoError> {
    debug!(
        "Installing workspace dependencies for member: {}",
        member.name
    );
    let mut local_toml = MycoLocalToml::default();
    let mut has_workspace_deps = false;

    // Process workspace dependencies
    for (dep_name, dep_version) in member.manifest.clone_deps() {
        if let DependencyVersion::Workspace { .. } = dep_version {
            has_workspace_deps = true;
            debug!("Processing workspace dependency: {}", dep_name);

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

            debug!("Resolved package name: {} -> {}", dep_name, package_name);

            // Find the corresponding workspace member
            if let Some(target_member) = workspace.find_member_by_name(&package_name) {
                // Calculate relative path from current member to target member
                let relative_path = calculate_relative_path(&member.path, &target_member.path)?;
                debug!("Mapping {} to relative path: {}", dep_name, relative_path);
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
        debug!("Saving myco-local.toml for member: {}", member.name);
        local_toml.save_blocking(member.path.clone())?;
    } else {
        debug!(
            "No workspace dependencies found for member: {}",
            member.name
        );
    }

    Ok(())
}

/// Install just the local files (.myco directory and tsconfig.json) without dependencies
fn install_local_files_only(myco_toml: &MycoToml) -> Result<(), MycoError> {
    debug!("Installing local files only (no dependencies)");
    use crate::deps::tsconfig;

    // Create .myco directory and myco.d.ts file
    debug!("Creating .myco directory");
    std::fs::create_dir_all(".myco").map_err(|e| MycoError::DirectoryCreation {
        path: ".myco".to_string(),
        source: e,
    })?;

    // Use the same constant from deps::mod
    debug!("Writing .myco/myco.d.ts");
    const MYCO_DTS: &str = include_str!("../../runtime/.myco/myco.d.ts");
    std::fs::write(".myco/myco.d.ts", MYCO_DTS).map_err(|e| MycoError::FileWrite {
        path: ".myco/myco.d.ts".to_string(),
        source: e,
    })?;

    // Create tsconfig.json dynamically based on myco.toml configuration
    debug!("Generating tsconfig.json");
    let tsconfig_content = tsconfig::generate_tsconfig_json(myco_toml)?;

    std::fs::write("tsconfig.json", tsconfig_content).map_err(|e| MycoError::FileWrite {
        path: "tsconfig.json".to_string(),
        source: e,
    })?;

    debug!("Successfully installed local files");
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
    info!("Running script '{}' across workspace", script);
    debug!("Package filters: {:?}", package_filters);
    let mut members_to_run = Vec::new();

    // Determine which members to run the script in
    if package_filters.is_empty() {
        debug!(
            "No package filters specified, checking all {} members",
            workspace.members.len()
        );
        // Run in all members that have the script
        for member in &workspace.members {
            if member_has_script(member, script) {
                debug!("Member '{}' has script '{}'", member.name, script);
                members_to_run.push(member);
            } else {
                debug!("Member '{}' does not have script '{}'", member.name, script);
            }
        }
    } else {
        debug!("Filtering to specific packages: {:?}", package_filters);
        // Run only in specified packages
        for package_name in package_filters {
            if let Some(member) = workspace.get_member(package_name) {
                if member_has_script(member, script) {
                    debug!("Package '{}' has script '{}'", package_name, script);
                    members_to_run.push(member);
                } else {
                    warn!(
                        "Package '{}' does not define script '{}'",
                        package_name, script
                    );
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
        warn!("No workspace members define script '{}'", script);
        println!("No workspace members define script '{}'", script);
        return Ok(());
    }

    info!(
        "Running script '{}' in {} members",
        script,
        members_to_run.len()
    );

    // Execute the script in each member sequentially
    for member in members_to_run {
        info!("Running '{}' in member: {}", script, member.name);
        println!("Running '{}' in {}", script, member.name);

        let original_dir =
            std::env::current_dir().map_err(|e| MycoError::GetCurrentDirectory { source: e })?;
        debug!("Changing to member directory: {}", member.path.display());
        std::env::set_current_dir(&member.path).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: member.path.display().to_string(),
        })?;

        let exit_code = crate::run::run(&member.manifest, &script.to_string(), None)?;
        debug!(
            "Script '{}' in '{}' exited with code: {}",
            script, member.name, exit_code
        );

        debug!("Restoring original directory: {}", original_dir.display());
        std::env::set_current_dir(&original_dir).map_err(|_e| MycoError::SetCurrentDirectory {
            dir: original_dir.display().to_string(),
        })?;

        if exit_code != 0 {
            return Err(MycoError::ScriptExecution {
                message: format!(
                    "Script '{}' failed in package '{}' with exit code {}",
                    script, member.name, exit_code
                ),
            });
        }

        info!("Successfully ran '{}' in member: {}", script, member.name);
    }

    info!("Workspace script execution completed successfully");
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
