//! Tests for the RTK sysadmin-command support expansion.
//!
//! Audit context (2026-06-01): our RTK savings were 35.8% on a
//! ~800-command session vs fast-rlm's 69.9% on similar volume. The
//! gap was partly workload mix (theirs was sysadmin-heavy, ours was
//! build-heavy) and partly that the agent's `ps` / `lsof` /
//! `journalctl` / DNS-tool invocations were bypassing RTK entirely
//! because the commands weren't in `RTK_SUPPORTED_COMMANDS`. Each
//! bypass meant the verbose output hit the model at full size while
//! RTK would have compressed it 80-98%.
//!
//! These tests pin the additions so a future refactor of the
//! supported-list doesn't silently drop them and re-open the gap.

use crate::rtk::rewrite::is_rtk_supported;

#[test]
fn process_inspection_commands_are_rtk_supported() {
    // `ps` is the single biggest miss — fast-rlm's top RTK win at
    // 97.9% compression on `ps auxww` (684K saved across 17 calls).
    // `top` / `lsof` / `netstat` / `ss` are the same shape: huge
    // verbose output where a filter rule can drop 95%+.
    for cmd in ["ps", "top", "lsof", "netstat", "ss"] {
        assert!(
            is_rtk_supported(cmd),
            "{cmd} must be in RTK_SUPPORTED_COMMANDS — verbose system inspection \
             output should route through RTK so the model doesn't drown in noise"
        );
    }
}

#[test]
fn log_inspection_commands_are_rtk_supported() {
    // `journalctl` without `--lines=N` can produce 10,000+ lines on
    // a busy host. `dmesg` similar. Both should route through RTK
    // so the bundled `journalctl-recent` / `dmesg-recent` filters in
    // `rtk_filters.toml.example` can cap them.
    for cmd in ["journalctl", "dmesg"] {
        assert!(
            is_rtk_supported(cmd),
            "{cmd} must be in RTK_SUPPORTED_COMMANDS — log streams need capping"
        );
    }
}

#[test]
fn dns_tools_are_rtk_supported() {
    // `dig` / `nslookup` / `host` / `traceroute` produce multi-section
    // outputs with verbose headers + answer + authority + additional.
    // The bundled `dig-answer-only` filter drops the non-answer
    // sections; without RTK routing, the agent sees all of it.
    for cmd in ["dig", "nslookup", "host", "traceroute"] {
        assert!(
            is_rtk_supported(cmd),
            "{cmd} must be in RTK_SUPPORTED_COMMANDS — DNS / network tooling \
             produces verbose multi-section output that benefits from filtering"
        );
    }
}

#[test]
fn the_pre_existing_supported_commands_are_still_there() {
    // Guard against an accidental "rewrite the whole list" refactor
    // that drops the build-workflow commands the agent already relies
    // on. These are the heaviest RTK wins in our usage data:
    // - cargo test --all-features → 99.8% saved
    // - cargo test → 100%
    // - find → 71.6%
    // - grep → 11.2% (low % but high volume)
    for cmd in [
        "git", "gh", "cargo", "grep", "find", "ls", "tree", "curl", "docker", "kubectl",
    ] {
        assert!(
            is_rtk_supported(cmd),
            "{cmd} must STAY in RTK_SUPPORTED_COMMANDS — removing it would \
             regress the heaviest existing RTK wins"
        );
    }
}

#[test]
fn rtk_meta_commands_remain_unsupported() {
    // `rtk` itself must NOT be supported (otherwise we'd recursively
    // prepend `rtk rtk ...`). Same for sudo / ssh / editors / pagers
    // / REPLs which are on the blocklist for their own reasons.
    for cmd in ["rtk", "sudo", "ssh", "vim", "less", "python"] {
        assert!(
            !is_rtk_supported(cmd),
            "{cmd} must NOT be in RTK_SUPPORTED_COMMANDS — it's either RTK \
             itself, interactive, or a REPL where filtering breaks the session"
        );
    }
}

#[test]
fn rtk_filters_toml_example_exists_at_repo_root() {
    // Soft check that the bundled filter template file is present.
    // The actual contents are read via include_str! by future
    // seed-on-missing wiring (similar to usage_pricing.toml.example).
    // For now, just confirming the file is committed at the repo root.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rtk_filters.toml.example");
    assert!(
        path.exists(),
        "rtk_filters.toml.example must exist at the repo root so users can \
         `cp` it to ~/.rtk/filters.toml and `rtk trust` to activate"
    );
}
