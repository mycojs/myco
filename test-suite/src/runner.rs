use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Command;
use tokio::time::timeout;

use crate::{TestCase, TestOutput, TestResult, TestManifest};
use crate::matcher::{MatchResult, OutputMatcher};

pub struct TestRunner {
    myco_binary_path: PathBuf,
}

impl TestRunner {
    pub fn new(myco_binary_path: PathBuf, _test_timeout: Duration) -> Self {
        Self {
            myco_binary_path,
        }
    }

    pub async fn run_test_case(
        &self,
        test_case: &TestCase,
        test_dir: &Path,
        temp_dir: &TempDir,
    ) -> TestResult {
        let start_time = Instant::now();
        
        // Construct script path
        let script_path = test_dir.join(&test_case.script);
        if !script_path.exists() {
            return TestResult::Error {
                error: format!("Test script not found: {}", script_path.display()),
            };
        }

        // Copy the script to temp directory
        let temp_script_path = temp_dir.path().join(&test_case.script);
        if let Err(e) = tokio::fs::copy(&script_path, &temp_script_path).await {
            return TestResult::Error {
                error: format!("Failed to copy script to temp directory: {}", e),
            };
        }

        // Copy myco.toml if it exists in the test directory
        let myco_toml_path = test_dir.join("myco.toml");
        if myco_toml_path.exists() {
            let temp_toml_path = temp_dir.path().join("myco.toml");
            if let Err(e) = tokio::fs::copy(&myco_toml_path, &temp_toml_path).await {
                return TestResult::Error {
                    error: format!("Failed to copy myco.toml to temp directory: {}", e),
                };
            }
        } else {
            // Create a minimal myco.toml
            let temp_toml_path = temp_dir.path().join("myco.toml");
            let minimal_toml = r#"[project]
name = "test"
version = "0.1.0"
"#;
            if let Err(e) = tokio::fs::write(&temp_toml_path, minimal_toml).await {
                return TestResult::Error {
                    error: format!("Failed to create myco.toml in temp directory: {}", e),
                };
            }
        }

        // Set up working directory (always use temp directory)
        let working_dir = temp_dir.path().to_path_buf();

        // Build command
        let mut cmd = Command::new(&self.myco_binary_path);
        cmd.arg("run")
            .arg(&test_case.script)  // Use just the script name since we're in the right directory
            .current_dir(&working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // Set environment variables
        for (key, value) in &test_case.environment_variables {
            cmd.env(key, value);
        }

        // Execute with timeout
        let test_timeout = test_case.timeout();
        let result = timeout(test_timeout, cmd.output()).await;

        let duration = start_time.elapsed();

        match result {
            Ok(Ok(output)) => {
                let test_output = TestOutput {
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                    duration,
                };

                // Validate output
                match test_case.to_output_expectation() {
                    Ok(expectation) => {
                        match test_output.matches(&expectation) {
                            MatchResult::Success => TestResult::Passed { duration },
                            MatchResult::Failed { reason } => TestResult::Failed {
                                reason,
                                output: test_output,
                            },
                        }
                    }
                    Err(err) => TestResult::Error {
                        error: format!("Failed to create output expectation: {}", err),
                    },
                }
            }
            Ok(Err(err)) => TestResult::Error {
                error: format!("Failed to execute command: {}", err),
            },
            Err(_) => TestResult::Timeout { duration },
        }
    }

    pub async fn run_test_suite(&self, suite_path: &Path) -> anyhow::Result<Vec<(String, TestResult)>> {
        let manifest_path = suite_path.join("test.yaml");
        if !manifest_path.exists() {
            anyhow::bail!("Test manifest not found: {}", manifest_path.display());
        }

        // Load test manifest
        let manifest_content = tokio::fs::read_to_string(&manifest_path).await?;
        let manifest: TestManifest = serde_yaml::from_str(&manifest_content)?;

        println!("Running test suite: {}", manifest.name);
        println!("Description: {}", manifest.description);
        println!("Tests: {}", manifest.tests.len());
        println!();

        let mut results = Vec::new();

        for test_case in &manifest.tests {
            println!("Running test: {}", test_case.name);
            
            // Create temporary directory for this test
            let temp_dir = tempfile::tempdir()?;
            
            let result = self.run_test_case(test_case, suite_path, &temp_dir).await;
            
            results.push((test_case.name.clone(), result));
        }

        Ok(results)
    }
}

pub async fn find_myco_binary() -> anyhow::Result<PathBuf> {
    // First try to find in target/debug
    let debug_path = PathBuf::from("target/debug/myco");
    if debug_path.exists() {
        return Ok(std::fs::canonicalize(debug_path)?);
    }

    // Try target/release
    let release_path = PathBuf::from("target/release/myco");
    if release_path.exists() {
        return Ok(std::fs::canonicalize(release_path)?);
    }

    // Try to find using which
    let output = Command::new("which")
        .arg("myco")
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            let path_str = stdout_str.trim();
            Ok(PathBuf::from(path_str))
        }
        _ => anyhow::bail!("Could not find myco binary. Please build with 'cargo build' first."),
    }
} 