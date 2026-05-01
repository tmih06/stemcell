//! RSI Proposed Tools / Commands inbox.
//!
//! The autonomous RSI loop cannot directly install new tools or slash
//! commands — that path goes through `tool_manage` / `config_manager`,
//! which require user approval and are not in RSI's restricted tool
//! whitelist by design (a hallucinated `rm -rf` shell tool is a much
//! bigger blast radius than a hallucinated paragraph in `SOUL.md`).
//!
//! This module is the workaround: RSI writes *proposals* into TOML
//! inboxes under `~/.opencrabs/rsi/`. The user-facing agent reads them
//! on request ("show me proposed tools", "implement the proposed
//! command") and applies them via the same plumbing as `tool_manage`
//! `add` and `config_manager` `add_command`. Applied/rejected entries
//! are archived per-day so the trail is auditable forever.
//!
//! ## Layout
//!
//! ```text
//! ~/.opencrabs/rsi/
//! ├── proposed_tools.toml      # pending tool proposals
//! ├── proposed_commands.toml   # pending command proposals
//! ├── applied/
//! │   ├── 2026-05-01-tools.toml
//! │   └── 2026-05-01-commands.toml
//! └── rejected/
//!     ├── 2026-05-01-tools.toml
//!     └── 2026-05-01-commands.toml
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::brain::commands::UserCommand;
use crate::brain::tools::dynamic::tool::DynamicToolDef;

const PROPOSED_TOOLS_FILE: &str = "proposed_tools.toml";
const PROPOSED_COMMANDS_FILE: &str = "proposed_commands.toml";
const APPLIED_DIR: &str = "applied";
const REJECTED_DIR: &str = "rejected";

/// A pending tool proposal authored by the autonomous RSI loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProposal {
    /// Stable id (e.g. `prop_tool_2026-05-01_a1b2c3`). Used by `apply` / `reject`.
    pub id: String,
    /// UTC timestamp the proposal was filed.
    pub created_at: DateTime<Utc>,
    /// Who proposed it. `"rsi-autonomous"` for the background loop;
    /// could be `"user-suggested"` etc. in future.
    pub proposer: String,
    /// Free-text justification — what evidence in the feedback ledger
    /// drove this proposal? Shown to the user before they apply.
    pub rationale: String,
    /// The actual tool definition. Persisted verbatim to `tools.toml`
    /// on apply, with no further transformation.
    pub def: DynamicToolDef,
}

/// A pending command proposal authored by the autonomous RSI loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandProposal {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub proposer: String,
    pub rationale: String,
    /// Persisted verbatim to `commands.toml` on apply.
    pub command: UserCommand,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ToolProposalsFile {
    #[serde(default)]
    proposals: Vec<ToolProposal>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CommandProposalsFile {
    #[serde(default)]
    proposals: Vec<CommandProposal>,
}

/// On-disk store for proposals. Stateless — every method re-reads the
/// inbox file before mutating, so it's safe to call from concurrent
/// task contexts (the RSI background loop and the user-facing agent
/// can both touch the inbox without coordination).
#[derive(Debug, Clone)]
pub struct ProposalsStore {
    rsi_dir: PathBuf,
}

impl ProposalsStore {
    /// Create a store rooted at `<opencrabs_home>/rsi/`.
    pub fn new() -> Self {
        Self {
            rsi_dir: crate::config::opencrabs_home().join("rsi"),
        }
    }

    /// Override the rsi dir — only used by tests so they can write to
    /// a tmpdir without touching the user's actual inbox.
    pub fn with_dir(rsi_dir: PathBuf) -> Self {
        Self { rsi_dir }
    }

    fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.rsi_dir)
            .with_context(|| format!("creating {}", self.rsi_dir.display()))?;
        fs::create_dir_all(self.rsi_dir.join(APPLIED_DIR))?;
        fs::create_dir_all(self.rsi_dir.join(REJECTED_DIR))?;
        Ok(())
    }

    fn tools_path(&self) -> PathBuf {
        self.rsi_dir.join(PROPOSED_TOOLS_FILE)
    }

    fn commands_path(&self) -> PathBuf {
        self.rsi_dir.join(PROPOSED_COMMANDS_FILE)
    }

    fn read_tools(&self) -> ToolProposalsFile {
        Self::read_file(&self.tools_path()).unwrap_or_default()
    }

    fn read_commands(&self) -> CommandProposalsFile {
        Self::read_file(&self.commands_path()).unwrap_or_default()
    }

    fn read_file<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
        let contents = fs::read_to_string(path).ok()?;
        match toml::from_str(&contents) {
            Ok(parsed) => Some(parsed),
            Err(e) => {
                tracing::warn!("ProposalsStore: failed to parse {}: {}", path.display(), e);
                None
            }
        }
    }

    fn write_tools(&self, file: &ToolProposalsFile) -> Result<()> {
        self.ensure_dirs()?;
        let toml_str = toml::to_string_pretty(file)?;
        fs::write(self.tools_path(), toml_str)?;
        Ok(())
    }

    fn write_commands(&self, file: &CommandProposalsFile) -> Result<()> {
        self.ensure_dirs()?;
        let toml_str = toml::to_string_pretty(file)?;
        fs::write(self.commands_path(), toml_str)?;
        Ok(())
    }

    /// Append a tool proposal. Generates the id, dedups against existing
    /// entries with the same `def.name` (latest wins), and persists.
    pub fn add_tool_proposal(
        &self,
        proposer: impl Into<String>,
        rationale: impl Into<String>,
        def: DynamicToolDef,
    ) -> Result<String> {
        let mut file = self.read_tools();
        let id = generate_id("tool", &def.name);

        // Dedup: a fresh proposal for the same tool name supersedes the
        // older one — keeps the inbox from filling with retries when RSI
        // observes the same gap on multiple cycles.
        file.proposals.retain(|p| p.def.name != def.name);

        file.proposals.push(ToolProposal {
            id: id.clone(),
            created_at: Utc::now(),
            proposer: proposer.into(),
            rationale: rationale.into(),
            def,
        });
        self.write_tools(&file)?;
        Ok(id)
    }

    pub fn add_command_proposal(
        &self,
        proposer: impl Into<String>,
        rationale: impl Into<String>,
        command: UserCommand,
    ) -> Result<String> {
        let mut file = self.read_commands();
        let id = generate_id("cmd", &command.name);
        file.proposals.retain(|p| p.command.name != command.name);
        file.proposals.push(CommandProposal {
            id: id.clone(),
            created_at: Utc::now(),
            proposer: proposer.into(),
            rationale: rationale.into(),
            command,
        });
        self.write_commands(&file)?;
        Ok(id)
    }

    pub fn list_tool_proposals(&self) -> Vec<ToolProposal> {
        self.read_tools().proposals
    }

    pub fn list_command_proposals(&self) -> Vec<CommandProposal> {
        self.read_commands().proposals
    }

    /// Total number of pending entries across both inboxes — used by
    /// the TUI session-start banner.
    pub fn pending_count(&self) -> usize {
        self.read_tools().proposals.len() + self.read_commands().proposals.len()
    }

    /// Remove a tool proposal by id and return it (so the caller can
    /// install it). Returns `None` if no proposal with that id exists.
    pub fn take_tool_proposal(&self, id: &str) -> Result<Option<ToolProposal>> {
        let mut file = self.read_tools();
        let pos = file.proposals.iter().position(|p| p.id == id);
        let Some(idx) = pos else {
            return Ok(None);
        };
        let taken = file.proposals.remove(idx);
        self.write_tools(&file)?;
        Ok(Some(taken))
    }

    pub fn take_command_proposal(&self, id: &str) -> Result<Option<CommandProposal>> {
        let mut file = self.read_commands();
        let pos = file.proposals.iter().position(|p| p.id == id);
        let Some(idx) = pos else {
            return Ok(None);
        };
        let taken = file.proposals.remove(idx);
        self.write_commands(&file)?;
        Ok(Some(taken))
    }

    /// Append an applied tool proposal to the daily archive
    /// (`applied/YYYY-MM-DD-tools.toml`).
    pub fn archive_applied_tool(&self, proposal: &ToolProposal) -> Result<()> {
        self.archive(APPLIED_DIR, "tools", proposal)
    }

    pub fn archive_applied_command(&self, proposal: &CommandProposal) -> Result<()> {
        self.archive(APPLIED_DIR, "commands", proposal)
    }

    pub fn archive_rejected_tool(
        &self,
        proposal: &ToolProposal,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut wrapped = ArchivedProposal {
            inner: proposal.clone(),
            reason: reason.map(str::to_string),
        };
        // ArchivedProposal serializes inner via flatten so this is the
        // same shape as the input plus an optional `reason` field —
        // makes the archive readable by the same tooling that reads
        // the inbox.
        wrapped.reason = reason.map(str::to_string);
        self.archive(REJECTED_DIR, "tools", &wrapped)
    }

    pub fn archive_rejected_command(
        &self,
        proposal: &CommandProposal,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut wrapped = ArchivedProposal {
            inner: proposal.clone(),
            reason: reason.map(str::to_string),
        };
        wrapped.reason = reason.map(str::to_string);
        self.archive(REJECTED_DIR, "commands", &wrapped)
    }

    fn archive<T: Serialize>(&self, dir_name: &str, kind: &str, proposal: &T) -> Result<()> {
        self.ensure_dirs()?;
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let path = self
            .rsi_dir
            .join(dir_name)
            .join(format!("{}-{}.toml", date, kind));

        let mut existing = String::new();
        if path.exists() {
            existing = fs::read_to_string(&path).unwrap_or_default();
            if !existing.is_empty() && !existing.ends_with('\n') {
                existing.push('\n');
            }
        }

        // Each archive entry is its own `[[proposals]]` table; we just
        // append. Re-parsing on read is fine — these are append-only.
        let mut wrapper = toml::map::Map::new();
        wrapper.insert(
            "proposals".to_string(),
            toml::Value::Array(vec![toml::Value::try_from(proposal)?]),
        );
        let chunk = toml::to_string_pretty(&toml::Value::Table(wrapper))?;
        existing.push_str(&chunk);
        existing.push('\n');
        fs::write(&path, existing)?;
        Ok(())
    }
}

impl Default for ProposalsStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper used when archiving a rejected proposal — adds an optional
/// `reason` field next to the original proposal payload.
#[derive(Debug, Serialize)]
struct ArchivedProposal<T: Serialize> {
    #[serde(flatten)]
    inner: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn generate_id(kind: &str, name: &str) -> String {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let suffix_short: String = suffix.chars().take(6).collect();
    let safe_name: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("prop_{}_{}_{}_{}", kind, date, safe_name, suffix_short)
}
