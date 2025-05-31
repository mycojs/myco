use crate::{OutputExpectation, TestOutput};

#[derive(Debug)]
pub enum MatchResult {
    Success,
    Failed { reason: String },
}

pub trait OutputMatcher {
    fn matches(&self, expectation: &OutputExpectation) -> MatchResult;
}

impl OutputMatcher for TestOutput {
    fn matches(&self, expectation: &OutputExpectation) -> MatchResult {
        match expectation {
            OutputExpectation::Exact { stdout, stderr, exit_code } => {
                if self.exit_code != *exit_code {
                    return MatchResult::Failed {
                        reason: format!(
                            "Exit code mismatch: expected {}, got {}",
                            exit_code, self.exit_code
                        ),
                    };
                }

                if &self.stdout != stdout {
                    return MatchResult::Failed {
                        reason: format!(
                            "Stdout mismatch:\nExpected: {:?}\nActual: {:?}",
                            stdout, self.stdout
                        ),
                    };
                }

                if &self.stderr != stderr {
                    return MatchResult::Failed {
                        reason: format!(
                            "Stderr mismatch:\nExpected: {:?}\nActual: {:?}",
                            stderr, self.stderr
                        ),
                    };
                }

                MatchResult::Success
            }

            OutputExpectation::Pattern { stdout_pattern, stderr_pattern, exit_code } => {
                if self.exit_code != *exit_code {
                    return MatchResult::Failed {
                        reason: format!(
                            "Exit code mismatch: expected {}, got {}",
                            exit_code, self.exit_code
                        ),
                    };
                }

                if let Some(pattern) = stdout_pattern {
                    if !pattern.is_match(&self.stdout) {
                        return MatchResult::Failed {
                            reason: format!(
                                "Stdout pattern mismatch:\nPattern: {:?}\nActual: {:?}",
                                pattern.as_str(), self.stdout
                            ),
                        };
                    }
                }

                if let Some(pattern) = stderr_pattern {
                    if !pattern.is_match(&self.stderr) {
                        return MatchResult::Failed {
                            reason: format!(
                                "Stderr pattern mismatch:\nPattern: {:?}\nActual: {:?}",
                                pattern.as_str(), self.stderr
                            ),
                        };
                    }
                }

                MatchResult::Success
            }

            OutputExpectation::Contains { stdout_contains, stderr_contains, exit_code } => {
                if self.exit_code != *exit_code {
                    return MatchResult::Failed {
                        reason: format!(
                            "Exit code mismatch: expected {}, got {}",
                            exit_code, self.exit_code
                        ),
                    };
                }

                for expected in stdout_contains {
                    if !self.stdout.contains(expected) {
                        return MatchResult::Failed {
                            reason: format!(
                                "Stdout missing expected substring: {:?}\nActual stdout: {:?}",
                                expected, self.stdout
                            ),
                        };
                    }
                }

                for expected in stderr_contains {
                    if !self.stderr.contains(expected) {
                        return MatchResult::Failed {
                            reason: format!(
                                "Stderr missing expected substring: {:?}\nActual stderr: {:?}",
                                expected, self.stderr
                            ),
                        };
                    }
                }

                MatchResult::Success
            }
        }
    }
} 