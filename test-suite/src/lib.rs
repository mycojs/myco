use std::collections::HashMap;
use std::time::Duration;
use regex::Regex;
use serde::{Deserialize, Serialize};

pub mod matcher;
pub mod runner;
pub mod reporter;

pub use matcher::*;
pub use runner::*;
pub use reporter::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestManifest {
    pub name: String,
    pub description: String,
    pub tests: Vec<TestCase>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestCase {
    pub name: String,
    pub script: String,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub environment_variables: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub expected_stdout: Option<String>,
    #[serde(default)]
    pub expected_stderr: Option<String>,
    #[serde(default)]
    pub expected_exit_code: i32,
    #[serde(default)]
    pub expected_stdout_pattern: Option<String>,
    #[serde(default)]
    pub expected_stderr_pattern: Option<String>,
    #[serde(default)]
    pub expected_stdout_contains: Vec<String>,
    #[serde(default)]
    pub expected_stderr_contains: Vec<String>,
}

fn default_timeout() -> u64 {
    5000
}

#[derive(Debug)]
pub struct TestOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
}

#[derive(Debug)]
pub enum TestResult {
    Passed { duration: Duration },
    Failed { reason: String, output: TestOutput },
    Timeout { duration: Duration },
    Error { error: String },
}

#[derive(Debug)]
pub enum OutputExpectation {
    Exact { stdout: String, stderr: String, exit_code: i32 },
    Pattern { stdout_pattern: Option<Regex>, stderr_pattern: Option<Regex>, exit_code: i32 },
    Contains { stdout_contains: Vec<String>, stderr_contains: Vec<String>, exit_code: i32 },
}

impl TestCase {
    pub fn to_output_expectation(&self) -> anyhow::Result<OutputExpectation> {
        // Handle exact string matching
        if let Some(stdout) = &self.expected_stdout {
            let stderr = self.expected_stderr.clone().unwrap_or_default();
            return Ok(OutputExpectation::Exact {
                stdout: stdout.clone(),
                stderr,
                exit_code: self.expected_exit_code,
            });
        }

        // Handle pattern matching
        if self.expected_stdout_pattern.is_some() || self.expected_stderr_pattern.is_some() {
            let stdout_pattern = if let Some(pattern) = &self.expected_stdout_pattern {
                Some(Regex::new(pattern)?)
            } else {
                None
            };
            let stderr_pattern = if let Some(pattern) = &self.expected_stderr_pattern {
                Some(Regex::new(pattern)?)
            } else {
                None
            };
            return Ok(OutputExpectation::Pattern {
                stdout_pattern,
                stderr_pattern,
                exit_code: self.expected_exit_code,
            });
        }

        // Handle contains matching
        if !self.expected_stdout_contains.is_empty() || !self.expected_stderr_contains.is_empty() {
            return Ok(OutputExpectation::Contains {
                stdout_contains: self.expected_stdout_contains.clone(),
                stderr_contains: self.expected_stderr_contains.clone(),
                exit_code: self.expected_exit_code,
            });
        }

        // Default: just check exit code
        Ok(OutputExpectation::Exact {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: self.expected_exit_code,
        })
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
} 