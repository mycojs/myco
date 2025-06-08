use std::path::PathBuf;
use std::time::Duration;
use clap::{Parser, Subcommand};
use walkdir::WalkDir;

use myco_test_suite::*;

#[derive(Parser)]
#[command(name = "myco-test-suite")]
#[command(about = "Integration test suite for Myco runtime")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Test category to run
    #[arg(short, long)]
    category: Option<String>,

    /// Specific test to run
    #[arg(short, long)]
    test: Option<String>,

    /// Path to myco binary (auto-detected if not provided)
    #[arg(long)]
    myco_binary: Option<PathBuf>,

    /// Test timeout in milliseconds
    #[arg(long, default_value = "10000")]
    timeout: u64,
}

#[derive(Subcommand, Clone)]
enum Commands {
    /// Run all tests
    Run,
    /// List available tests
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let command = cli.command.clone().unwrap_or(Commands::Run);

    match command {
        Commands::Run => run_tests(cli).await,
        Commands::List => list_tests(cli).await,
    }
}

async fn run_tests(cli: Cli) -> anyhow::Result<()> {
    // Find myco binary
    let myco_binary = if let Some(path) = cli.myco_binary.clone() {
        path
    } else {
        find_myco_binary().await?
    };

    println!("Using myco binary: {}", myco_binary.display());
    
    let test_timeout = Duration::from_millis(cli.timeout);
    let runner = TestRunner::new(myco_binary, test_timeout);
    let reporter = TestReporter::new(cli.verbose);

    // Find test suites
    let test_suites = find_test_suites(&cli)?;
    
    if test_suites.is_empty() {
        println!("No test suites found.");
        return Ok(());
    }

    let mut all_results = Vec::new();

    for suite_path in test_suites {
        let separator = "=".repeat(60);
        println!("{}", separator);
        println!("Test suite: {}", suite_path.display());
        println!("{}", separator);
        
        let results = runner.run_test_suite(&suite_path).await?;
        
        for (test_name, result) in &results {
            reporter.report_test_result(test_name, result);
        }
        
        all_results.extend(results);
        println!();
    }

    reporter.report_suite_summary(&all_results);

    // Exit with non-zero code if any tests failed
    let has_failures = all_results.iter().any(|(_, result)| {
        !matches!(result, TestResult::Passed { .. })
    });

    if has_failures {
        std::process::exit(1);
    }

    Ok(())
}

async fn list_tests(cli: Cli) -> anyhow::Result<()> {
    let test_suites = find_test_suites(&cli)?;
    
    for suite_path in test_suites {
        let manifest_path = suite_path.join("test.toml");
        if !manifest_path.exists() {
            continue;
        }

        let manifest_content = tokio::fs::read_to_string(&manifest_path).await?;
        let manifest: TestManifest = toml::from_str(&manifest_content)?;

        println!("Suite: {} ({})", manifest.name, suite_path.display());
        println!("  Description: {}", manifest.description);
        for test_case in &manifest.tests {
            println!("  - {}", test_case.name);
        }
        println!();
    }

    Ok(())
}

fn find_test_suites(cli: &Cli) -> anyhow::Result<Vec<PathBuf>> {
    let test_dir = PathBuf::from("test-suite/tests");
    
    if !test_dir.exists() {
        anyhow::bail!("Test directory not found: {}", test_dir.display());
    }

    let mut suites = Vec::new();

    for entry in WalkDir::new(&test_dir) {
        let entry = entry?;
        if entry.file_name() == "test.toml" {
            let suite_path = entry.path().parent().unwrap().to_path_buf();
            
            // Apply filters
            if let Some(category) = &cli.category {
                let suite_relative = suite_path.strip_prefix(&test_dir)?;
                if !suite_relative.starts_with(category) {
                    continue;
                }
            }

            if let Some(test_name) = &cli.test {
                let suite_relative = suite_path.strip_prefix(&test_dir)?;
                if suite_relative.to_string_lossy() != *test_name {
                    continue;
                }
            }

            suites.push(suite_path);
        }
    }

    Ok(suites)
} 