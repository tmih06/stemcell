//! Startup job: report which brain/system files were found on disk.
//!
//! Report-only — inspects the brain directory (`~/.stemcell/`) for the core
//! files that are always injected (SOUL.md, USER.md) and the contextual files
//! the agent can load on demand. Surfaces a one-line count so the user can see
//! at a glance which system files the agent booted with, without grepping the
//! brain directory themselves.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct BrainFilesJob;

#[async_trait]
impl StartupJob for BrainFilesJob {
    fn name(&self) -> &'static str {
        "brain-files"
    }

    async fn run(&self, _ctx: &StartupContext) -> anyhow::Result<Option<String>> {
        let dir = crate::brain::prompt_builder::BrainLoader::resolve_path();

        let core: Vec<&str> = crate::brain::prompt_builder::CORE_BRAIN_FILES
            .iter()
            .filter(|(name, _)| dir.join(name).exists())
            .map(|(name, _)| *name)
            .collect();

        let contextual: Vec<&str> = crate::brain::prompt_builder::CONTEXTUAL_BRAIN_FILES
            .iter()
            .filter(|(name, _)| dir.join(name).exists())
            .map(|(name, _)| *name)
            .collect();

        let note = if core.is_empty() && contextual.is_empty() {
            "no brain files found".to_string()
        } else {
            format!(
                "{} core ({}), {} contextual available",
                core.len(),
                if core.is_empty() {
                    "none".to_string()
                } else {
                    core.join(", ")
                },
                contextual.len()
            )
        };
        tracing::debug!("[startup] brain-files: {note}");
        Ok(Some(note))
    }
}
