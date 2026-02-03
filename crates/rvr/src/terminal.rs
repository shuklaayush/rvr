//! Terminal UI utilities for progress indication and styled output.
//!
//! Provides spinners, progress bars, and styled output helpers for CLI commands.

use std::borrow::Cow;
use std::io::{self, Write};
use std::time::Duration;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

/// Spinner for indeterminate progress.
pub struct Spinner {
    bar: ProgressBar,
}

impl Spinner {
    /// Create a new spinner with a message.
    pub fn new(message: impl Into<Cow<'static, str>>) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        bar.set_message(message);
        bar.enable_steady_tick(Duration::from_millis(80));
        Self { bar }
    }


    /// Finish the spinner with a success message.
    pub fn finish_with_success(&self, message: &str) {
        self.bar.finish_and_clear();
        eprintln!("{} {}", style("✓").green().bold(), message);
    }

    /// Finish the spinner with a failure message.
    pub fn finish_with_failure(&self, message: &str) {
        self.bar.finish_and_clear();
        eprintln!("{} {}", style("✗").red().bold(), message);
    }

}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.bar.finish_and_clear();
    }
}

// ============================================================================
// Styled output helpers
// ============================================================================

/// Print an info message to stderr.
pub fn info(message: &str) {
    eprintln!("{} {}", style("→").cyan(), message);
}

/// Print a success message to stderr.
pub fn success(message: &str) {
    eprintln!("{} {}", style("✓").green().bold(), message);
}

/// Print an error message to stderr.
pub fn error(message: &str) {
    eprintln!("{} {}", style("✗").red().bold(), message);
}

/// Print a path output (like "-> /path/to/file").
pub fn path_output(path: &std::path::Path) {
    eprintln!("  {} {}", style("→").dim(), style(path.display()).dim());
}

// Table output (for benchmark results)
// ============================================================================

/// A builder for styled tables.
#[allow(dead_code)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    alignments: Vec<Alignment>,
}

/// Column alignment.
#[derive(Clone, Copy, Default)]
#[allow(dead_code)]
pub enum Alignment {
    #[default]
    Left,
    Right,
    Center,
}

#[allow(dead_code)]
impl Table {
    /// Create a new table with headers.
    pub fn new(headers: Vec<&str>) -> Self {
        let count = headers.len();
        Self {
            headers: headers.into_iter().map(String::from).collect(),
            rows: Vec::new(),
            alignments: vec![Alignment::Left; count],
        }
    }

    /// Set column alignments.
    pub fn with_alignments(mut self, alignments: Vec<Alignment>) -> Self {
        self.alignments = alignments;
        self
    }

    /// Add a row to the table.
    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    /// Render the table as a markdown table.
    pub fn render(&self) -> String {
        if self.headers.is_empty() {
            return String::new();
        }

        // Calculate column widths
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        let mut output = String::new();

        // Header row
        output.push('|');
        for (i, header) in self.headers.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(0);
            output.push_str(&format!(" {:^w$} |", header, w = w));
        }
        output.push('\n');

        // Separator row
        output.push('|');
        for (i, &width) in widths.iter().enumerate() {
            let align = self.alignments.get(i).copied().unwrap_or_default();
            let sep = match align {
                Alignment::Left => format!(":{:-<w$}|", "", w = width + 1),
                Alignment::Right => format!("{:-<w$}:|", "", w = width + 1),
                Alignment::Center => format!(":{:-<w$}:|", "", w = width),
            };
            output.push_str(&sep);
        }
        output.push('\n');

        // Data rows
        for row in &self.rows {
            output.push('|');
            for (i, cell) in row.iter().enumerate() {
                let w = widths.get(i).copied().unwrap_or(0);
                let align = self.alignments.get(i).copied().unwrap_or_default();
                let formatted = match align {
                    Alignment::Left => format!(" {:<w$} |", cell, w = w),
                    Alignment::Right => format!(" {:>w$} |", cell, w = w),
                    Alignment::Center => format!(" {:^w$} |", cell, w = w),
                };
                output.push_str(&formatted);
            }
            output.push('\n');
        }

        output
    }

    /// Print the table to stdout.
    pub fn print(&self) {
        print!("{}", self.render());
        let _ = io::stdout().flush();
    }
}
