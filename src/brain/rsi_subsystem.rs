//! Bash command subsystem classifier for RSI pattern detection.
//!
//! The bash tool records each invocation's command text in the
//! feedback ledger (commit 2b4d7c86 added `cmd=<text>` to the meta
//! column). To turn that raw text into actionable signal, RSI groups
//! commands by SUBSYSTEM — gh, git, docker, python, cargo, etc. —
//! so it can see "50 successful `gh issue comment` calls in a week"
//! instead of "50 successful bash calls" (which says nothing about
//! WHAT was repeated).
//!
//! The classifier is intentionally lossy: it returns a stable
//! lowercase prefix for well-known CLI tools, and `None` for
//! everything else. The goal is enabling SQL-style aggregation
//! (`GROUP BY subsystem`), not exact reconstruction of the original
//! command. Unknown commands stay un-classified so RSI doesn't
//! aggregate disparate one-offs into a misleading "other" bucket.
//!
//! Used by:
//!   - `src/brain/rsi.rs` analysis cycle, when scanning bash success
//!     events to find tool-extraction candidates.
//!   - `src/brain/tools/feedback_analyze.rs`, for surfacing
//!     subsystem-level success/failure rates in agent-facing queries.

/// Pull the bash command text out of a feedback-ledger meta string.
///
/// `enrich_metadata` in `brain/agent/service/feedback.rs` appends
/// `| cmd=<text>` to bash event metadata (for both success and
/// failure). This helper reverses that encoding so RSI can fetch
/// the command back for classification. Returns `None` when the
/// meta doesn't carry a `cmd=` marker — e.g. a non-bash event, or
/// a bash event recorded before commit 2b4d7c86 enriched the path.
pub fn extract_cmd_from_meta(meta: &str) -> Option<&str> {
    // The marker is `cmd=` either at the start (no preceding
    // snippet) or after ` | ` (joined with an error snippet).
    // Take everything after `cmd=` to the end of the meta string,
    // since the command is always the last field appended.
    let after = meta.split_once("cmd=").map(|(_, rest)| rest)?;
    if after.is_empty() { None } else { Some(after) }
}

/// Subsystem this command targets, or `None` if it doesn't match a
/// known CLI tool. The returned strings are stable lowercase slugs
/// safe to use as group keys.
pub fn classify_bash_command(cmd: &str) -> Option<&'static str> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip a leading `sudo `, `time `, env-var assignments, etc. so
    // the classifier sees the actual command. We only peel one layer
    // — chained `sudo time docker ps` is rare enough that the first
    // token (sudo) is fine to classify as "shell".
    let first_raw = first_meaningful_token(trimmed)?;
    // Lowercase for case-insensitive match. `GIT push` should
    // classify the same as `git push`. Allocates one short string
    // per call — cheap relative to the work the caller is doing
    // (database aggregations).
    let first = first_raw.to_ascii_lowercase();
    Some(match first.as_str() {
        // ── Version control ──────────────────────────────────────
        "git" => "git",
        "gh" => "gh",
        "hub" => "gh", // legacy `hub` CLI predates `gh`; same domain
        "svn" => "svn",
        "hg" => "hg",
        "jj" => "jj",

        // ── Containers / orchestration ───────────────────────────
        "docker" | "docker-compose" => "docker",
        "podman" => "podman",
        "kubectl" | "kubernetes" | "k9s" => "kubectl",
        "helm" => "helm",
        "terraform" | "tofu" => "terraform",
        "ansible" | "ansible-playbook" => "ansible",

        // ── Language / build toolchains ──────────────────────────
        "cargo" | "rustc" | "rustup" => "cargo",
        "python" | "python3" | "py" => "python",
        "pip" | "pip3" | "pipx" | "uv" | "poetry" => "pip",
        "node" | "npx" => "node",
        "npm" | "yarn" | "pnpm" | "bun" => "npm",
        "go" => "go",
        "ruby" | "irb" => "ruby",
        "gem" | "bundle" => "ruby",
        "java" => "java",
        "mvn" | "gradle" | "./gradlew" => "java",
        "swift" => "swift",
        "dotnet" => "dotnet",
        "make" | "cmake" | "ninja" | "meson" => "make",

        // ── Networking / data fetch ──────────────────────────────
        "curl" | "wget" | "http" | "httpie" => "curl",
        "ssh" | "scp" | "rsync" | "sftp" => "ssh",
        "ping" | "traceroute" | "dig" | "nslookup" | "mtr" => "net",

        // ── Cloud CLIs ───────────────────────────────────────────
        "aws" => "aws",
        "gcloud" | "gsutil" => "gcloud",
        "az" => "az",
        "doctl" => "doctl",
        "flyctl" | "fly" => "fly",
        "vercel" => "vercel",
        "wrangler" => "wrangler",
        "heroku" => "heroku",

        // ── Files / search ───────────────────────────────────────
        "ls" | "find" | "fd" | "tree" => "fs",
        "grep" | "rg" | "ag" | "ack" => "grep",
        "cat" | "head" | "tail" | "less" | "more" | "bat" => "fs",
        "cp" | "mv" | "rm" | "mkdir" | "rmdir" | "touch" | "ln" | "chmod" | "chown" => "fs",
        "tar" | "zip" | "unzip" | "gzip" | "gunzip" | "7z" => "archive",

        // ── Data ─────────────────────────────────────────────────
        "psql" | "pg_dump" | "pg_restore" => "postgres",
        "mysql" | "mariadb" => "mysql",
        "sqlite3" | "sqlite" => "sqlite",
        "redis-cli" => "redis",
        "jq" | "yq" | "fx" => "jq",

        // ── Editors / view-only ──────────────────────────────────
        "vim" | "nvim" | "emacs" | "nano" | "code" => "editor",

        // ── Process / system ─────────────────────────────────────
        "ps" | "top" | "htop" | "pgrep" | "pkill" | "kill" | "killall" => "proc",
        "systemctl" | "service" | "launchctl" | "brew" | "apt" | "apt-get" | "yum" | "dnf"
        | "pacman" => "pkg",

        // ── Shell metalevel ──────────────────────────────────────
        "sudo" | "time" | "env" | "watch" | "xargs" | "tee" | "echo" | "printf" | "sleep"
        | "true" | "false" | "test" | "[[" | "[" => "shell",
        "bash" | "sh" | "zsh" | "fish" | "exec" => "shell",
        "source" | "." => "shell",

        // ── Open-source LLM / AI tooling ─────────────────────────
        "ollama" | "llama" | "llama.cpp" => "ollama",
        "huggingface-cli" | "hf" => "huggingface",

        _ => return None,
    })
}

/// Pull out the first meaningful token from a command line, skipping
/// `VAR=value` assignments at the start including quoted values like
/// `RUSTFLAGS="-C opt-level=0" cargo build` → `cargo`.
///
/// Does NOT strip `sudo` / `time` / etc. — those are themselves
/// meaningful subsystems (the user is asking for elevated privileges
/// or wall-clock timing) and the classifier returns "shell" for them.
///
/// We can't use `str::split_whitespace` because env-var values often
/// contain spaces inside quotes (`KEY="multi word value"`) and
/// split_whitespace doesn't honour quotes. A tiny hand-rolled
/// scanner handles the env-prefix case without pulling in
/// `shell_words` (which would be ~400 KB of dep for one parse).
fn first_meaningful_token(cmd: &str) -> Option<&str> {
    let bytes = cmd.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip leading whitespace.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let token_start = i;
        // Read an identifier (potential env-var name).
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        // If followed immediately by `=`, this is an env-var
        // assignment — skip its value (which may be quoted and
        // contain spaces) and look for the next token.
        if i > token_start && i < bytes.len() && bytes[i] == b'=' {
            i += 1; // past the '='
            // Honour double-quoted, single-quoted, or bare-word
            // values. Backslash-escapes inside quoted values are
            // ignored — good enough for the env-prefix case we
            // care about; full shell parsing isn't the goal.
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let q = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // past closing quote
                }
            } else {
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            }
            continue;
        }
        // Not an env assignment: find the end of this token (next
        // whitespace) and return it. We rescan from token_start in
        // case the token is `--features=foo` (dash means it's an
        // option, not an env var — we already declined to peel it).
        let mut j = token_start;
        while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        return Some(&cmd[token_start..j]);
    }
    None
}
