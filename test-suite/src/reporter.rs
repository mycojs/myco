use std::time::Duration;
use colored::*;

use crate::TestResult;

pub struct TestReporter {
    verbose: bool,
}

impl TestReporter {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    pub fn report_test_result(&self, test_name: &str, result: &TestResult) {
        match result {
            TestResult::Passed { duration } => {
                println!(
                    "  {} {} ({}ms)",
                    "✓".green().bold(),
                    test_name,
                    duration.as_millis()
                );
            }
            TestResult::Failed { reason, output } => {
                println!("  {} {}", "✗".red().bold(), test_name);
                if self.verbose {
                    println!("    Reason: {}", reason);
                    println!("    Stdout: {:?}", output.stdout);
                    println!("    Stderr: {:?}", output.stderr);
                    println!("    Exit code: {}", output.exit_code);
                    println!("    Duration: {}ms", output.duration.as_millis());
                } else {
                    println!("    {}", reason);
                }
            }
            TestResult::Timeout { duration } => {
                println!(
                    "  {} {} (timeout after {}ms)",
                    "⏱".yellow().bold(),
                    test_name,
                    duration.as_millis()
                );
            }
            TestResult::Error { error } => {
                println!("  {} {} (error: {})", "!".red().bold(), test_name, error);
            }
        }
    }

    pub fn report_suite_summary(&self, results: &[(String, TestResult)]) {
        let total = results.len();
        let passed = results.iter().filter(|(_, r)| matches!(r, TestResult::Passed { .. })).count();
        let failed = results.iter().filter(|(_, r)| matches!(r, TestResult::Failed { .. })).count();
        let timeout = results.iter().filter(|(_, r)| matches!(r, TestResult::Timeout { .. })).count();
        let error = results.iter().filter(|(_, r)| matches!(r, TestResult::Error { .. })).count();

        println!();
        println!("Test Summary:");
        println!("  Total: {}", total);
        println!("  {} Passed: {}", "✓".green(), passed);
        
        if failed > 0 {
            println!("  {} Failed: {}", "✗".red(), failed);
        }
        if timeout > 0 {
            println!("  {} Timeout: {}", "⏱".yellow(), timeout);
        }
        if error > 0 {
            println!("  {} Error: {}", "!".red(), error);
        }

        let total_duration: Duration = results
            .iter()
            .map(|(_, result)| match result {
                TestResult::Passed { duration } => duration.clone(),
                TestResult::Failed { output, .. } => output.duration,
                TestResult::Timeout { duration } => duration.clone(),
                TestResult::Error { .. } => Duration::from_millis(0),
            })
            .sum();

        println!("  Total duration: {}ms", total_duration.as_millis());

        if passed == total {
            println!("\n{}", "All tests passed!".green().bold());
        } else {
            println!("\n{}", format!("{} tests failed.", total - passed).red().bold());
        }
    }
} 