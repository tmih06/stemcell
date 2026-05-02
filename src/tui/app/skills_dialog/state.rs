//! Skills dialog app-side state.

use crate::brain::skills::Skill;

/// Runtime state for the `/skills` dialog. Single struct so `AppState`
/// only carries one field (`pub skills_dialog: SkillsDialogState`); all
/// dialog-specific behaviour lives in this module.
#[derive(Debug, Clone, Default)]
pub struct SkillsDialogState {
    /// Live filter string. Type-to-narrow against name + description.
    pub filter: String,
    /// Index into the *filtered* list (not the unfiltered loaded set).
    pub selected_index: usize,
    /// Vertical scroll offset for the card list.
    pub scroll_offset: u16,
}

impl SkillsDialogState {
    /// Reset to a clean slate (called by `actions::open`).
    pub fn reset(&mut self) {
        self.filter.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }
}

/// Filter `skills` by `query` (case-insensitive substring on name or
/// description). Empty query passes everything through.
pub fn matching<'a>(skills: &'a [Skill], query: &str) -> Vec<&'a Skill> {
    if query.is_empty() {
        return skills.iter().collect();
    }
    let q = query.to_lowercase();
    skills
        .iter()
        .filter(|s| s.name.to_lowercase().contains(&q) || s.description.to_lowercase().contains(&q))
        .collect()
}
