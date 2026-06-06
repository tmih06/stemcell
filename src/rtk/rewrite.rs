//! RTK command rewriting functionality
//!
//! RTK works as a direct proxy: you run `rtk git status` instead of `git status`.
//! RTK intercepts the command, runs it internally, filters/compresses the output,
//! and returns the token-optimized version.
//!
//! This module maintains the list of supported commands and handles the rewriting.

use tokio::sync::OnceCell;

/// Commands that RTK supports as a proxy (from `rtk --help`).
/// When the first token of a bash command matches one of these, we prepend `rtk`.
///
/// Meta-commands (gain, config, init, session, telemetry, learn, discover) are
/// excluded because they're RTK's own management commands, not proxies.
const RTK_SUPPORTED_COMMANDS: &[&str] = &[
    "git",
    "gh",
    "glab",
    "aws",
    "psql",
    "pnpm",
    "npm",
    "npx",
    "cargo",
    "docker",
    "kubectl",
    "grep",
    "find",
    "ls",
    "tree",
    "diff",
    "curl",
    "wget",
    "jest",
    "vitest",
    "prisma",
    "tsc",
    "next",
    "dotnet",
    "playwright",
    "prettier",
    "eslint",
    // Sysadmin / system-inspection family — added 2026-06-01 after
    // an RTK-savings audit showed 0% compression on these because
    // they were bypassing RTK entirely. fast-rlm's stats show
    // `ps auxww` compressing to 97.9% via RTK's TOML filter, so the
    // agent's process / network / log inspections were leaking
    // verbose output to the model at full size. Each entry below is
    // a command whose default output is verbose (full process table,
    // socket list, log lines, DNS resolution chain) and that RTK
    // can either filter natively or via a TOML rule. RTK's
    // `is_rtk_supported` upstream gate ensures we only forward if
    // RTK actually knows how to handle the subcommand.
    "ps",
    "top",
    "lsof",
    "netstat",
    "ss",
    "journalctl",
    "dmesg",
    "dig",
    "nslookup",
    "host",
    "traceroute",
];

/// Commands that should NEVER be rewritten even if they match the list above.
/// These are either too interactive, have side effects we don't want RTK to
/// intercept, or are already RTK meta-commands.
const RTK_BLOCKLIST: &[&str] = &[
    "rtk",       // Don't double-prepend
    "sudo",      // Don't rewrite elevated commands
    "ssh",       // Interactive, needs TTY
    "scp",       // Binary transfer
    "sftp",      // Interactive
    "rsync",     // Binary transfer
    "vim",       // Editor
    "vi",        // Editor
    "nvim",      // Editor
    "nano",      // Editor
    "emacs",     // Editor
    "less",      // Pager
    "more",      // Pager
    "man",       // Pager
    "python",    // REPL
    "python3",   // REPL
    "node",      // REPL
    "mysql",     // REPL
    "redis-cli", // REPL
    "psql",      // REPL (even though rtk supports it, we handle it via tool)
];

/// Result of RTK command rewriting
#[derive(Debug, Clone)]
pub struct RtkResult {
    /// The rewritten command (with rtk prefix)
    pub rewritten_command: String,
    /// Whether the command was actually rewritten
    pub was_rewritten: bool,
    /// Original command for reference
    pub original_command: String,
}

/// Cached RTK binary path lookup.
///
/// Uses `tokio::sync::OnceCell` so the async initialization never blocks the
/// tokio runtime. The previous `std::sync::OnceLock` + `std::process::Command`
/// combination blocked the worker thread on the first call (issue #125).
static RTK_BINARY: OnceCell<Option<String>> = OnceCell::const_new();

/// Find the RTK binary path (async, cached).
///
/// Checks in order:
/// 1. Bundled binary in the same directory as the OpenCrabs executable
/// 2. Bundled binary in `bin/` subdirectory relative to the executable
/// 3. System PATH via `which rtk` (spawned as a non-blocking tokio child)
///
/// Result is cached after the first call. Concurrent callers await the same
/// initialization — no thundering herd, no blocking.
async fn find_rtk_binary() -> Option<String> {
    RTK_BINARY
        .get_or_init(|| async {
            // Check for bundled binary in the same directory as the executable
            if let Ok(exe_path) = std::env::current_exe()
                && let Some(exe_dir) = exe_path.parent()
            {
                // Check ./rtk (same directory)
                let bundled_path = exe_dir.join("rtk");
                if bundled_path.exists() && bundled_path.is_file() {
                    tracing::info!("RTK binary found bundled at: {:?}", bundled_path);
                    return Some(bundled_path.to_string_lossy().to_string());
                }

                // Check ./bin/rtk (bin subdirectory)
                let bin_path = exe_dir.join("bin").join("rtk");
                if bin_path.exists() && bin_path.is_file() {
                    tracing::info!("RTK binary found bundled at: {:?}", bin_path);
                    return Some(bin_path.to_string_lossy().to_string());
                }
            }

            // Fall back to PATH lookup — async so we never block the runtime.
            match tokio::process::Command::new("which")
                .arg("rtk")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    tracing::info!("RTK binary found in PATH: {}", path);
                    // Store the bare name; tokio::process::Command resolves via PATH at exec time.
                    Some("rtk".to_string())
                }
                Ok(_) => {
                    tracing::info!("RTK binary not found in PATH or bundled");
                    None
                }
                Err(_) => {
                    tracing::warn!("Failed to check for rtk binary availability");
                    None
                }
            }
        })
        .await
        .clone()
}

/// Runtime enable/disable flag for RTK output filtering.
///
/// Set to `false` at startup when `config.features.rtk = false`.
/// Defaults to `true` (enabled) for backward compatibility.
/// Using an atomic so it can be read from async contexts without locking.
static RTK_RUNTIME_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Disable RTK output filtering at runtime.
///
/// Called at startup when `config.features.rtk = false`. Once disabled,
/// `is_rtk_available()` returns `false` regardless of whether the binary
/// is present, effectively bypassing all RTK rewriting.
pub fn disable_rtk() {
    RTK_RUNTIME_ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
    tracing::info!("RTK output filtering disabled via features.rtk = false");
}

/// Check if the rtk binary is available (bundled or in PATH). Async + cached.
/// Returns `false` if RTK has been disabled at runtime via `disable_rtk()`.
pub async fn is_rtk_available() -> bool {
    if !RTK_RUNTIME_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    find_rtk_binary().await.is_some()
}

/// Extract the first real command token from a shell command string.
///
/// Skips leading env var assignments (`FOO=bar cmd`) and returns the
/// actual command name.
pub(crate) fn first_command_token(command: &str) -> &str {
    for token in command.split_whitespace() {
        // Skip env var assignments like FOO=bar
        if token.contains('=') && !token.starts_with('-') && !token.starts_with('/') {
            continue;
        }
        return token;
    }
    ""
}

/// Check if a command token is supported by RTK for rewriting.
pub(crate) fn is_rtk_supported(token: &str) -> bool {
    // Strip leading path: /usr/bin/git → git
    let basename = token.rsplit('/').next().unwrap_or(token);

    if RTK_BLOCKLIST.contains(&basename) {
        return false;
    }

    RTK_SUPPORTED_COMMANDS.contains(&basename)
}

/// Rewrite a bash command to use RTK as a proxy.
///
/// If the command's first token is RTK-supported (git, cargo, npm, etc.),
/// prepends the RTK binary path to the command. Otherwise returns None.
///
/// The RTK binary path is determined by checking bundled locations first,
/// then falling back to PATH.
///
/// # Example
/// ```rust,ignore
/// use opencrabs::rtk::rewrite_command;
///
/// let result = rewrite_command("git status");
/// // Returns Some(RtkResult { rewritten_command: "/path/to/rtk git status", ... })
///
/// let result = rewrite_command("echo hello");
/// // Returns None (echo is not RTK-supported)
/// ```
pub async fn rewrite_command(command: &str) -> Option<RtkResult> {
    let rtk_binary = find_rtk_binary().await?;

    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Don't rewrite commands that already start with rtk
    if trimmed.starts_with("rtk ") || trimmed == "rtk" {
        return None;
    }

    // Handle chained commands: only rewrite if the FIRST command is supported.
    // For `git status && echo done`, we rewrite to `rtk git status && echo done`.
    // For `echo start && git status`, we don't rewrite (first cmd not supported).
    let first_token = first_command_token(trimmed);

    if !is_rtk_supported(first_token) {
        tracing::debug!(
            "RTK: command '{}' not supported (token: '{}')",
            command,
            first_token
        );
        return None;
    }

    let rewritten = format!("{} {}", rtk_binary, trimmed);

    tracing::debug!("RTK rewrote: '{}' -> '{}'", command, rewritten);

    Some(RtkResult {
        rewritten_command: rewritten,
        was_rewritten: true,
        original_command: command.to_string(),
    })
}

/// Convenience wrapper: returns just the rewritten string or None.
#[allow(dead_code)]
pub async fn rewrite_command_string(command: &str) -> Option<String> {
    rewrite_command(command).await.map(|r| r.rewritten_command)
}
