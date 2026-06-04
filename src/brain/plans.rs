//! Bundled reference plan files for the plan tool.
//!
//! Files are embedded via include_str! and written to the runtime directory
//! (`~/.opencrabs/plans/` for default profile) on first access if not present.

use std::fs;
use std::path::PathBuf;
use crate::config::opencrabs_home;

/// Returns the path to the plans directory for the current profile.
/// This is `opencrabs_home().join("plans")`.
pub fn plans_dir() -> PathBuf {
    opencrabs_home().join("plans")
}

/// Embedded reference plan files.
pub mod files {
    use super::*;

    /// Returns the path to a bundled file, writing it to disk if needed.
    fn resolve_path(rel_path: &str) -> PathBuf {
        let target = plans_dir().join(rel_path);
        if !target.exists() {
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let embedded = embedded_content(rel_path);
            let _ = fs::write(&target, embedded);
        }
        target
    }

    fn embedded_content(rel_path: &str) -> &'static str {
        match rel_path {
            "plan-json-spec.md" => include_str!("../../docs/reference/plans/plan-json-spec.md"),
            "coding-plans/python-fast.json" => include_str!("../../docs/reference/plans/coding-plans/python-fast.json"),
            "coding-plans/python-medium.json" => include_str!("../../docs/reference/plans/coding-plans/python-medium.json"),
            "coding-plans/python-full.json" => include_str!("../../docs/reference/plans/coding-plans/python-full.json"),
            "coding-plans/rust-fast.json" => include_str!("../../docs/reference/plans/coding-plans/rust-fast.json"),
            "coding-plans/rust-medium.json" => include_str!("../../docs/reference/plans/coding-plans/rust-medium.json"),
            "coding-plans/rust-full.json" => include_str!("../../docs/reference/plans/coding-plans/rust-full.json"),
            "coding-plans/sample-minimal-plan.json" => include_str!("../../docs/reference/plans/coding-plans/sample-minimal-plan.json"),
            _ => panic!("unknown bundled file: {}", rel_path),
        }
    }

    /// Path to the plan JSON schema specification.
    pub fn plan_spec_path() -> PathBuf {
        resolve_path("plan-json-spec.md")
    }

    /// Path to a coding plan by name.
    pub fn coding_plan_path(name: &str) -> PathBuf {
        resolve_path(&format!("coding-plans/{}.json", name))
    }

    /// Paths to all bundled coding plans.
    pub fn all_coding_plan_paths() -> Vec<PathBuf> {
        vec![
            coding_plan_path("python-fast"),
            coding_plan_path("python-medium"),
            coding_plan_path("python-full"),
            coding_plan_path("rust-fast"),
            coding_plan_path("rust-medium"),
            coding_plan_path("rust-full"),
            coding_plan_path("sample-minimal-plan"),
        ]
    }
}
