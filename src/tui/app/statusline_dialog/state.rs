//! `/statusline` dialog app-side state.

use crate::config::StatusLineConfig;

/// One toggleable status-bar field: a display label, the `config.toml`
/// key under `[statusline]`, and accessors into `StatusLineConfig`.
///
/// Keeping the mapping in one ordered table means the dialog list, the
/// config struct, and the persistence key all stay in sync — adding a
/// field is a single entry here plus the `StatusLineConfig` bool.
pub struct FieldSpec {
    /// Human-readable label shown in the checklist.
    pub label: &'static str,
    /// Key under `[statusline]` in config.toml.
    pub key: &'static str,
    /// Read the flag from a config snapshot.
    pub get: fn(&StatusLineConfig) -> bool,
    /// Flip the flag on a config snapshot.
    pub set: fn(&mut StatusLineConfig, bool),
}

/// Ordered list of toggleable fields. Order = display order in the dialog.
pub const FIELDS: &[FieldSpec] = &[
    FieldSpec {
        label: "Session name",
        key: "session_name",
        get: |c| c.session_name,
        set: |c, v| c.session_name = v,
    },
    FieldSpec {
        label: "Provider / model",
        key: "provider_model",
        get: |c| c.provider_model,
        set: |c, v| c.provider_model = v,
    },
    FieldSpec {
        label: "Profile",
        key: "profile",
        get: |c| c.profile,
        set: |c, v| c.profile = v,
    },
    FieldSpec {
        label: "Working directory",
        key: "working_dir",
        get: |c| c.working_dir,
        set: |c, v| c.working_dir = v,
    },
    FieldSpec {
        label: "Git branch",
        key: "git_branch",
        get: |c| c.git_branch,
        set: |c, v| c.git_branch = v,
    },
    FieldSpec {
        label: "Tokens / sec",
        key: "tokens_per_sec",
        get: |c| c.tokens_per_sec,
        set: |c, v| c.tokens_per_sec = v,
    },
    FieldSpec {
        label: "Approval policy",
        key: "approval_policy",
        get: |c| c.approval_policy,
        set: |c, v| c.approval_policy = v,
    },
    FieldSpec {
        label: "Split-pane indicator",
        key: "split_pane",
        get: |c| c.split_pane,
        set: |c, v| c.split_pane = v,
    },
];

/// Runtime state for the `/statusline` dialog. Single struct so `App`
/// carries only one field (`pub statusline_dialog: StatusLineDialogState`).
#[derive(Debug, Clone, Default)]
pub struct StatusLineDialogState {
    /// Index into `FIELDS` of the highlighted row.
    pub selected_index: usize,
}

impl StatusLineDialogState {
    /// Reset selection to the top (called by `actions::open`).
    pub fn reset(&mut self) {
        self.selected_index = 0;
    }
}
