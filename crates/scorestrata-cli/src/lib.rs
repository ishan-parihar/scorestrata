//! Scorestrata CLI library — shared utilities for CLI commands.

#![deny(missing_docs)]

use colored::Colorize;

/// Print a formatted header.
pub fn print_header(title: &str) {
    println!("\n{}", title.green().bold());
    println!("{}", "─".repeat(title.len()));
}
