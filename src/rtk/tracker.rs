//! RTK token savings tracking and metrics
//!
//! This module provides functionality to track and report token savings achieved
//! through RTK command filtering. It maintains metrics per command and provides
//! aggregate statistics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

/// Global RTK tracker instance
static GLOBAL_TRACKER: OnceLock<Arc<RtkTracker>> = OnceLock::new();

/// Get the global RTK tracker instance
pub fn global_tracker() -> Arc<RtkTracker> {
    GLOBAL_TRACKER
        .get_or_init(|| Arc::new(RtkTracker::new()))
        .clone()
}

/// Token savings for a single command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSavings {
    /// Original command that was executed
    pub command: String,
    /// Rewritten command (with rtk prefix)
    pub rewritten_command: String,
    /// Estimated tokens in original output
    pub original_tokens: usize,
    /// Actual tokens in filtered output
    pub filtered_tokens: usize,
    /// Tokens saved (original - filtered)
    pub tokens_saved: usize,
    /// Percentage savings (0-100)
    pub savings_percent: f64,
    /// When the command was executed
    pub timestamp: DateTime<Utc>,
}

/// Aggregate RTK metrics across all command executions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtkMetrics {
    /// Total commands executed through RTK
    pub total_commands: usize,
    /// Total tokens saved across all commands
    pub total_tokens_saved: usize,
    /// Average savings percentage
    pub average_savings_percent: f64,
    /// Savings breakdown by command type (first word)
    pub savings_by_command: HashMap<String, CommandSavings>,
    /// Recent savings history (last 100 commands)
    pub recent_savings: Vec<TokenSavings>,
    /// When metrics tracking started
    pub tracking_since: DateTime<Utc>,
}

/// Savings statistics for a specific command type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSavings {
    /// Number of times this command was executed
    pub execution_count: usize,
    /// Total tokens saved for this command type
    pub total_tokens_saved: usize,
    /// Average savings percentage for this command
    pub average_savings_percent: f64,
}

/// Thread-safe metrics tracker
#[derive(Debug, Clone)]
pub struct RtkTracker {
    metrics: Arc<Mutex<RtkMetrics>>,
}

impl RtkTracker {
    /// Create a new metrics tracker
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(RtkMetrics {
                total_commands: 0,
                total_tokens_saved: 0,
                average_savings_percent: 0.0,
                savings_by_command: HashMap::new(),
                recent_savings: Vec::new(),
                tracking_since: Utc::now(),
            })),
        }
    }

    /// Record token savings for a command execution
    ///
    /// Async because it acquires a `tokio::sync::Mutex`. The critical section
    /// is microseconds long, but using a sync `std::sync::Mutex` in an async
    /// path is the same class of bug that hung the daemon in issue #125 — the
    /// `which rtk` blocking process call. Defense in depth: never reach for a
    /// sync primitive from inside `bash.rs`'s async execute path.
    ///
    /// # Arguments
    /// * `savings` - The token savings data for this execution
    pub async fn record_savings(&self, savings: TokenSavings) {
        let mut metrics = self.metrics.lock().await;

        // Update totals
        metrics.total_commands += 1;
        metrics.total_tokens_saved += savings.tokens_saved;

        // Update average savings percentage
        let total_percent: f64 = metrics
            .recent_savings
            .iter()
            .map(|s| s.savings_percent)
            .sum::<f64>()
            + savings.savings_percent;
        let count = metrics.recent_savings.len() + 1;
        metrics.average_savings_percent = total_percent / count as f64;

        // Update per-command statistics
        let command_type = savings
            .command
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .to_string();

        let entry = metrics
            .savings_by_command
            .entry(command_type)
            .or_insert_with(|| CommandSavings {
                execution_count: 0,
                total_tokens_saved: 0,
                average_savings_percent: 0.0,
            });

        entry.execution_count += 1;
        entry.total_tokens_saved += savings.tokens_saved;

        // Update command-specific average using running total
        let cmd_total_percent: f64 = entry.average_savings_percent
            * (entry.execution_count - 1) as f64
            + savings.savings_percent;
        entry.average_savings_percent = cmd_total_percent / entry.execution_count as f64;

        // Add to recent history (keep last 100)
        metrics.recent_savings.push(savings);
        if metrics.recent_savings.len() > 100 {
            metrics.recent_savings.remove(0);
        }
    }

    /// Get current metrics snapshot
    pub async fn get_metrics(&self) -> RtkMetrics {
        self.metrics.lock().await.clone()
    }

    /// Get total tokens saved
    pub async fn total_tokens_saved(&self) -> usize {
        self.metrics.lock().await.total_tokens_saved
    }

    /// Get total commands executed
    pub async fn total_commands(&self) -> usize {
        self.metrics.lock().await.total_commands
    }

    /// Get average savings percentage
    pub async fn average_savings_percent(&self) -> f64 {
        self.metrics.lock().await.average_savings_percent
    }

    /// Format metrics as a human-readable string for display
    pub async fn format_report(&self) -> String {
        let metrics = self.get_metrics().await;

        let mut report = String::new();
        report.push_str("═══ RTK Token Savings Report ═══\n\n");

        report.push_str(&format!("Total Commands: {}\n", metrics.total_commands));
        report.push_str(&format!(
            "Total Tokens Saved: {}\n",
            metrics.total_tokens_saved
        ));
        report.push_str(&format!(
            "Average Savings: {:.1}%\n",
            metrics.average_savings_percent
        ));
        report.push_str(&format!(
            "Tracking Since: {}\n\n",
            metrics.tracking_since.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        report.push_str("Savings by Command Type:\n");
        let mut sorted_commands: Vec<_> = metrics.savings_by_command.iter().collect();
        sorted_commands.sort_by_key(|b| std::cmp::Reverse(b.1.total_tokens_saved));

        for (cmd, savings) in sorted_commands.iter().take(10) {
            report.push_str(&format!(
                "  {}: {} cmds, {} tokens saved, {:.1}% avg\n",
                cmd,
                savings.execution_count,
                savings.total_tokens_saved,
                savings.average_savings_percent
            ));
        }

        report
    }
}

impl Default for RtkTracker {
    fn default() -> Self {
        Self::new()
    }
}
