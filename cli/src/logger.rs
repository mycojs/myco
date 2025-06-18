use colored::*;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::io::{self, Write};

pub struct SimpleLogger {
    level: LevelFilter,
    use_colors: bool,
}

impl SimpleLogger {
    pub fn new(level: LevelFilter, use_colors: bool) -> Self {
        Self { level, use_colors }
    }
}

impl Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level && metadata.target().starts_with("myco")
    }

    fn log(&self, record: &Record) {
        if self.enabled(&record.metadata()) {
            let level_str: ColoredString = match record.level() {
                Level::Error => {
                    if self.use_colors {
                        "ERROR".red().bold()
                    } else {
                        "ERROR".normal()
                    }
                }
                Level::Warn => {
                    if self.use_colors {
                        "WARN".yellow().bold()
                    } else {
                        "WARN".normal()
                    }
                }
                Level::Info => {
                    if self.use_colors {
                        "INFO".blue().bold()
                    } else {
                        "INFO".normal()
                    }
                }
                Level::Debug => {
                    if self.use_colors {
                        "DEBUG".cyan().bold()
                    } else {
                        "DEBUG".normal()
                    }
                }
                Level::Trace => {
                    if self.use_colors {
                        "TRACE".green().bold()
                    } else {
                        "TRACE".normal()
                    }
                }
            };

            let target = record.target();

            let output = if self.use_colors {
                format!("[{}] {}: {}", level_str, target.dimmed(), record.args())
            } else {
                format!("[{}] {}: {}", level_str, target, record.args())
            };

            // Write to stderr for error/warn, stdout for others
            match record.level() {
                Level::Error | Level::Warn => {
                    let _ = writeln!(io::stderr(), "{}", output);
                }
                _ => {
                    let _ = writeln!(io::stdout(), "{}", output);
                }
            }
        }
    }

    fn flush(&self) {
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
    }
}

static LOGGER: std::sync::OnceLock<SimpleLogger> = std::sync::OnceLock::new();

pub fn init_logger(level: LevelFilter, use_colors: bool) -> Result<(), SetLoggerError> {
    let logger = LOGGER.get_or_init(|| SimpleLogger::new(level, use_colors));
    log::set_logger(logger)?;
    log::set_max_level(level);
    Ok(())
}

pub fn level_from_str(level: &str) -> Option<LevelFilter> {
    match level.to_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}
