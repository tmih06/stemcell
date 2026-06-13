//! `/export` dialog app-side state.

/// What the export action should do with the rendered transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    /// Copy the transcript to the system clipboard.
    Clipboard,
    /// Write the transcript to a timestamped file under `~/.stemcell/exports/`.
    File,
    /// Both copy to clipboard and write a file.
    Both,
}

/// One selectable export option: a display label, a one-line description, and
/// the action it performs. Order = display order in the dialog.
pub struct ExportOption {
    pub label: &'static str,
    pub description: &'static str,
    pub target: ExportTarget,
}

/// Ordered list of export options shown in the dialog.
pub const EXPORT_OPTIONS: &[ExportOption] = &[
    ExportOption {
        label: "Copy to clipboard",
        description: "Copy the full transcript to your clipboard",
        target: ExportTarget::Clipboard,
    },
    ExportOption {
        label: "Export to file",
        description: "Write the transcript to ~/.stemcell/exports/",
        target: ExportTarget::File,
    },
    ExportOption {
        label: "Both",
        description: "Copy to clipboard and write a file",
        target: ExportTarget::Both,
    },
];

/// Runtime state for the `/export` dialog. Single struct so `App` carries
/// only one field (`pub export_dialog: ExportDialogState`).
#[derive(Debug, Clone, Default)]
pub struct ExportDialogState {
    /// Index into `EXPORT_OPTIONS` of the highlighted row.
    pub selected_index: usize,
}

impl ExportDialogState {
    /// Reset selection to the top (called by `actions::open`).
    pub fn reset(&mut self) {
        self.selected_index = 0;
    }
}
