//! Evolve Tool
//!
//! Updates OpenCrabs to the latest release. Detects the install method
//! (pre-built binary, cargo install, or source build) and uses the
//! appropriate upgrade strategy:
//!
//! - **Pre-built binary**: Downloads from GitHub releases, health-checks, swaps.
//! - **cargo install**: Runs `cargo install opencrabs --force`.
//! - **Source build**: Suggests using `/rebuild` instead.
//!
//! Before swapping binaries, it health-checks the new binary. If the swap
//! fails, it rolls back to the previous version automatically.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::brain::agent::{ProgressCallback, ProgressEvent};
use crate::utils::install::{InstallMethod, binary_name, platform_suffix};
use async_trait::async_trait;
use serde_json::Value;

const GITHUB_API: &str = "https://api.github.com/repos/adolfousier/opencrabs/releases/latest";

/// Build an honest, status-aware error string for a non-success
/// response from `releases/latest`. Replaces the prior hardcoded
/// "rate limited or unavailable" suffix that lied about every
/// non-2xx — a real 404 (no published release) and a 403 (rate
/// limit) looked identical to the user, sending us down wrong
/// debug paths.
///
/// `body_excerpt` should be the first ~300 chars of the response
/// body so the message can quote the API's own explanation when
/// it returns one (GitHub error envelopes carry a useful `message`
/// field, e.g. "API rate limit exceeded for ...").
pub(crate) fn diagnose_releases_latest_status(
    status: reqwest::StatusCode,
    body_excerpt: &str,
    ratelimit_remaining: Option<&str>,
    ratelimit_reset: Option<&str>,
) -> String {
    let code = status.as_u16();
    let body_tail = if body_excerpt.trim().is_empty() {
        String::new()
    } else {
        format!(" — API said: {}", body_excerpt.trim())
    };
    let ratelimit_tail = match (ratelimit_remaining, ratelimit_reset) {
        (Some(r), Some(reset)) => {
            format!(" [x-ratelimit-remaining={r}, x-ratelimit-reset={reset}]")
        }
        (Some(r), None) => format!(" [x-ratelimit-remaining={r}]"),
        _ => String::new(),
    };
    match code {
        404 => format!(
            "GitHub returned 404 for releases/latest — no published \
             (non-draft, non-prerelease) release exists for this repo \
             at this moment, or there's a brief publish-propagation lag. \
             Try again in a minute.{body_tail}{ratelimit_tail}"
        ),
        403 | 429 => format!(
            "GitHub rate limit hit ({code}) — unauthenticated requests \
             are capped at 60/hr per IP. Wait an hour, or set GITHUB_TOKEN \
             in your env to raise the cap to 5000/hr if you share this \
             IP.{body_tail}{ratelimit_tail}"
        ),
        500..=599 => format!(
            "GitHub API returned {code} — server-side issue, retry in a \
             few minutes.{body_tail}"
        ),
        _ => format!("GitHub API returned {status}.{body_tail}{ratelimit_tail}"),
    }
}

/// Check GitHub for a newer release. Returns `Some(latest_version)` if an
/// update is available **and** a binary asset exists for this platform,
/// `None` if already on latest, no asset ready, or on error.
pub async fn check_for_update() -> Option<String> {
    let current_version = crate::VERSION;
    let client = reqwest::Client::new();
    let resp = match client
        .get(GITHUB_API)
        .header("User-Agent", format!("opencrabs/{}", current_version))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                target: "evolve",
                url = GITHUB_API,
                error = %e,
                "background update check failed to reach GitHub"
            );
            return None;
        }
    };
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let body_excerpt: String = body.chars().take(300).collect();
        tracing::warn!(
            target: "evolve",
            url = GITHUB_API,
            %status,
            body_excerpt,
            "background update check: releases/latest returned non-2xx"
        );
        return None;
    }
    let release: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                target: "evolve",
                url = GITHUB_API,
                error = %e,
                "background update check: failed to parse releases/latest JSON"
            );
            return None;
        }
    };

    let latest_tag = match release["tag_name"].as_str() {
        Some(t) => t,
        None => {
            tracing::warn!(
                target: "evolve",
                "background update check: releases/latest payload missing tag_name"
            );
            return None;
        }
    };
    let latest_version = latest_tag.strip_prefix('v').unwrap_or(latest_tag);

    if !is_newer(latest_version, current_version) {
        return None;
    }

    // If running from source, check if Cargo.toml already has the latest version
    if let Some(source_version) = source_cargo_version()
        && source_version == latest_version
    {
        return None;
    }

    // For pre-built binary installs, only report "available" if the platform
    // asset actually exists in the release (release may still be building).
    if matches!(InstallMethod::detect(), InstallMethod::PrebuiltBinary)
        && !has_platform_asset(&release, latest_tag)
    {
        tracing::debug!(
            "Release {} exists but no asset for this platform yet",
            latest_tag
        );
        return None;
    }

    Some(latest_version.to_string())
}

/// Check whether the release JSON contains a downloadable asset for the
/// current platform.
pub(crate) fn has_platform_asset(release: &serde_json::Value, tag: &str) -> bool {
    let suffix = match platform_suffix() {
        Some(s) => s,
        None => return false,
    };
    let ext = if std::env::consts::OS == "windows" {
        "zip"
    } else {
        "tar.gz"
    };
    let expected = format!("opencrabs-{}-{}.{}", tag, suffix, ext);
    let legacy = format!("opencrabs-{}.{}", suffix, ext);

    release["assets"]
        .as_array()
        .map(|arr| {
            arr.iter().any(|a| {
                let name = a["name"].as_str().unwrap_or("");
                name == expected || name == legacy
            })
        })
        .unwrap_or(false)
}

/// Compare semver strings: returns true if `latest` is strictly newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').filter_map(|s| s.parse().ok()).collect() };
    let l = parse(latest);
    let c = parse(current);
    l > c
}

/// Try to read the version from the source Cargo.toml relative to the running
/// binary. Returns `None` if not running from a source build or file not found.
fn source_cargo_version() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let target_dir = exe.parent()?;
    let repo_root = target_dir.parent()?.parent()?;
    let cargo_toml = repo_root.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml).ok()?;
    let table: toml::Table = content.parse().ok()?;
    table
        .get("package")?
        .get("version")?
        .as_str()
        .map(String::from)
}

/// Run a health check on a binary: execute it with `--version`,
/// verify it exits cleanly within a timeout. Returns a detailed error
/// with stderr output on failure.
async fn health_check_binary(path: &std::path::Path) -> std::result::Result<(), String> {
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    tracing::info!(
        target: "evolve",
        path = %path.display(),
        size = file_size,
        "evolve: running `<binary> --version` health check"
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new(path)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => Ok(()),
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_snippet: String = stderr.chars().take(200).collect();
            tracing::warn!(
                target: "evolve",
                path = %path.display(),
                exit_status = %output.status,
                size = file_size,
                stderr_excerpt = %stderr_snippet,
                "evolve: health check exited non-zero"
            );
            Err(format!(
                "exited with {} (binary: {} bytes, platform: {}/{}{})",
                output.status,
                file_size,
                std::env::consts::OS,
                std::env::consts::ARCH,
                if stderr_snippet.is_empty() {
                    String::new()
                } else {
                    format!(", stderr: {}", stderr_snippet)
                }
            ))
        }
        Ok(Err(e)) => {
            tracing::warn!(
                target: "evolve",
                path = %path.display(),
                error = %e,
                size = file_size,
                "evolve: health check failed to spawn the binary"
            );
            Err(format!(
                "failed to spawn: {e} (binary: {file_size} bytes, platform: {}/{})",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))
        }
        Err(_) => {
            tracing::warn!(
                target: "evolve",
                path = %path.display(),
                size = file_size,
                "evolve: health check timed out after 10s"
            );
            Err(format!("timed out after 10s (binary: {file_size} bytes)"))
        }
    }
}

pub struct EvolveTool {
    progress: Option<ProgressCallback>,
}

impl EvolveTool {
    pub fn new(progress: Option<ProgressCallback>) -> Self {
        Self { progress }
    }
}

#[async_trait]
impl Tool for EvolveTool {
    fn name(&self) -> &str {
        "evolve"
    }

    fn description(&self) -> &str {
        "Check for and install the latest OpenCrabs release. \
         Automatically detects the install method (pre-built binary, \
         cargo install, or source) and uses the right update strategy. \
         Hot-restarts into the new version after installation."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "check_only": {
                    "type": "boolean",
                    "description": "If true, only check for updates without installing. Default: false."
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::SystemModification]
    }

    fn requires_approval(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolExecutionContext) -> Result<ToolResult> {
        let check_only = input
            .get("check_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let current_version = crate::VERSION;
        let sid = context.session_id;
        let install_method = InstallMethod::detect();

        // Emit progress
        if let Some(ref cb) = self.progress {
            cb(
                sid,
                ProgressEvent::IntermediateText {
                    text: format!(
                        "Checking for updates (install: {})...",
                        install_method.description()
                    ),
                    reasoning: None,
                },
            );
        }

        // Fetch latest release info from GitHub
        let client = reqwest::Client::new();
        tracing::info!(
            target: "evolve",
            url = GITHUB_API,
            current_version,
            install_method = install_method.description(),
            os = std::env::consts::OS,
            arch = std::env::consts::ARCH,
            session_id = %sid,
            check_only,
            "evolve: fetching releases/latest"
        );
        let resp = match client
            .get(GITHUB_API)
            .header("User-Agent", format!("opencrabs/{}", current_version))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    target: "evolve",
                    url = GITHUB_API,
                    error = %e,
                    session_id = %sid,
                    "evolve: network error reaching GitHub"
                );
                return Ok(ToolResult::error(format!(
                    "Failed to reach GitHub ({GITHUB_API}): {e}"
                )));
            }
        };
        let status = resp.status();
        let ratelimit_remaining = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let ratelimit_reset = resp
            .headers()
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let release: Value = if status.is_success() {
            match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        target: "evolve",
                        url = GITHUB_API,
                        error = %e,
                        session_id = %sid,
                        "evolve: 200 response but JSON parse failed"
                    );
                    return Ok(ToolResult::error(format!(
                        "Failed to parse release info from {GITHUB_API}: {e}"
                    )));
                }
            }
        } else {
            let body = resp.text().await.unwrap_or_default();
            let body_excerpt: String = body.chars().take(300).collect();
            tracing::warn!(
                target: "evolve",
                url = GITHUB_API,
                %status,
                ratelimit_remaining = ratelimit_remaining.as_deref().unwrap_or("-"),
                ratelimit_reset = ratelimit_reset.as_deref().unwrap_or("-"),
                body_excerpt = %body_excerpt,
                session_id = %sid,
                "evolve: releases/latest returned non-2xx"
            );
            return Ok(ToolResult::error(diagnose_releases_latest_status(
                status,
                &body_excerpt,
                ratelimit_remaining.as_deref(),
                ratelimit_reset.as_deref(),
            )));
        };

        let latest_tag = release["tag_name"].as_str().unwrap_or("unknown");
        let latest_version = latest_tag.strip_prefix('v').unwrap_or(latest_tag);

        // Compare versions
        if latest_version == current_version {
            return Ok(ToolResult::success(format!(
                "Already on the latest version (v{}).",
                current_version
            )));
        }

        // For pre-built binary installs, verify the platform asset exists
        // before reporting the update as available (release may still be building).
        if matches!(install_method, InstallMethod::PrebuiltBinary)
            && !has_platform_asset(&release, latest_tag)
        {
            let asset_count = release["assets"].as_array().map(|a| a.len()).unwrap_or(0);
            return Ok(ToolResult::error(format!(
                "v{} release exists but the binary for {}/{} is not available yet \
                 ({} assets uploaded so far). The release may still be building — \
                 try again in a few minutes.",
                latest_version,
                std::env::consts::OS,
                std::env::consts::ARCH,
                asset_count
            )));
        }

        if check_only {
            return Ok(ToolResult::success(format!(
                "Update available: v{} -> v{} (install method: {}). Run /evolve to install.",
                current_version,
                latest_version,
                install_method.description()
            )));
        }

        // Dispatch based on install method
        match install_method {
            InstallMethod::Source(_) => {
                return Ok(ToolResult::success(format!(
                    "Update available: v{} -> v{}. You're running from source — use /rebuild \
                     to pull and build the latest version, or `git checkout v{}` to switch.",
                    current_version, latest_version, latest_version
                )));
            }
            InstallMethod::CargoInstall => {
                return self
                    .evolve_via_cargo_install(sid, current_version, latest_version)
                    .await;
            }
            InstallMethod::PrebuiltBinary => {
                return self
                    .evolve_via_binary_download(
                        sid,
                        &client,
                        &release,
                        current_version,
                        latest_tag,
                        latest_version,
                    )
                    .await;
            }
        }
    }
}

impl EvolveTool {
    /// Update via `cargo install opencrabs --force`.
    async fn evolve_via_cargo_install(
        &self,
        sid: uuid::Uuid,
        current_version: &str,
        latest_version: &str,
    ) -> Result<ToolResult> {
        if let Some(ref cb) = self.progress {
            cb(
                sid,
                ProgressEvent::IntermediateText {
                    text: format!(
                        "Updating via cargo install (v{} -> v{})...",
                        current_version, latest_version
                    ),
                    reasoning: None,
                },
            );
        }

        tracing::info!(
            target: "evolve",
            current_version,
            latest_version,
            session_id = %sid,
            "evolve: running `cargo install opencrabs --force`"
        );
        let output = tokio::process::Command::new("cargo")
            .args(["install", "opencrabs", "--force"])
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| {
                tracing::warn!(
                    target: "evolve",
                    error = %e,
                    session_id = %sid,
                    "evolve: failed to spawn `cargo` — is the Rust toolchain installed?"
                );
                super::error::ToolError::Execution(format!("Failed to spawn cargo: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_excerpt: String = stderr.chars().take(500).collect();
            tracing::warn!(
                target: "evolve",
                exit_status = %output.status,
                stderr_excerpt = %stderr_excerpt,
                session_id = %sid,
                "evolve: cargo install failed"
            );
            return Ok(ToolResult::error(format!(
                "cargo install failed: {stderr_excerpt}"
            )));
        }

        // Signal restart
        if let Some(ref cb) = self.progress {
            cb(
                sid,
                ProgressEvent::RestartReady {
                    status: format!(
                        "Evolved via cargo install: v{} -> v{}. Restarting now.",
                        current_version, latest_version
                    ),
                },
            );
        }

        Ok(ToolResult::success(format!(
            "Evolved from v{} to v{} via cargo install. Restarting into the new version.",
            current_version, latest_version
        )))
    }

    /// Update by downloading a pre-built binary from GitHub releases.
    async fn evolve_via_binary_download(
        &self,
        sid: uuid::Uuid,
        client: &reqwest::Client,
        release: &Value,
        current_version: &str,
        latest_tag: &str,
        latest_version: &str,
    ) -> Result<ToolResult> {
        let suffix = match platform_suffix() {
            Some(s) => s,
            None => {
                return Ok(ToolResult::error(format!(
                    "Unsupported platform: {}/{}. Use /rebuild to build from source.",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                )));
            }
        };

        let is_windows = std::env::consts::OS == "windows";
        let ext = if is_windows { "zip" } else { "tar.gz" };
        let expected_asset = format!("opencrabs-{}-{}.{}", latest_tag, suffix, ext);

        let assets = release["assets"].as_array();
        let download_url = assets
            .and_then(|arr| {
                arr.iter().find_map(|a| {
                    let name = a["name"].as_str()?;
                    if name == expected_asset {
                        a["browser_download_url"].as_str().map(String::from)
                    } else {
                        None
                    }
                })
            })
            .or_else(|| {
                // Fallback: try legacy naming without version tag
                let legacy_asset = format!("opencrabs-{}.{}", suffix, ext);
                assets.and_then(|arr| {
                    arr.iter().find_map(|a| {
                        let name = a["name"].as_str()?;
                        if name == legacy_asset {
                            a["browser_download_url"].as_str().map(String::from)
                        } else {
                            None
                        }
                    })
                })
            });

        let download_url = match download_url {
            Some(url) => url,
            None => {
                return Ok(ToolResult::error(format!(
                    "No binary found for {} in v{}. Expected: {}. \
                     Available assets: {}. Use /rebuild to build from source.",
                    suffix,
                    latest_version,
                    expected_asset,
                    assets
                        .map(|arr| arr
                            .iter()
                            .filter_map(|a| a["name"].as_str())
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()
                )));
            }
        };

        // Download
        if let Some(ref cb) = self.progress {
            cb(
                sid,
                ProgressEvent::IntermediateText {
                    text: format!("Downloading opencrabs v{}...", latest_version),
                    reasoning: None,
                },
            );
        }

        tracing::info!(
            target: "evolve",
            url = %download_url,
            expected_asset = %expected_asset,
            session_id = %sid,
            "evolve: downloading release asset"
        );
        let archive_bytes = match client.get(&download_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let content_length = resp.content_length();
                match resp.bytes().await {
                    Ok(b) if b.is_empty() => {
                        tracing::warn!(
                            target: "evolve",
                            url = %download_url,
                            content_length = ?content_length,
                            session_id = %sid,
                            "evolve: download returned empty body"
                        );
                        return Ok(ToolResult::error(format!(
                            "Download from {download_url} returned an empty file \
                             (content-length={content_length:?}). The release asset \
                             may still be uploading — try again in a few minutes."
                        )));
                    }
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!(
                            target: "evolve",
                            url = %download_url,
                            error = %e,
                            session_id = %sid,
                            "evolve: download body read failed"
                        );
                        return Ok(ToolResult::error(format!(
                            "Download from {download_url} failed mid-stream: {e}"
                        )));
                    }
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let body_excerpt: String = body.chars().take(200).collect();
                tracing::warn!(
                    target: "evolve",
                    url = %download_url,
                    %status,
                    body_excerpt = %body_excerpt,
                    session_id = %sid,
                    "evolve: download returned non-2xx"
                );
                return Ok(ToolResult::error(format!(
                    "Download from {download_url} failed with status {status}{}",
                    if body_excerpt.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" — body: {body_excerpt}")
                    }
                )));
            }
            Err(e) => {
                tracing::warn!(
                    target: "evolve",
                    url = %download_url,
                    error = %e,
                    session_id = %sid,
                    "evolve: download request failed to send"
                );
                return Ok(ToolResult::error(format!(
                    "Download from {download_url} failed: {e}"
                )));
            }
        };

        tracing::info!(
            target: "evolve",
            asset = %expected_asset,
            bytes = archive_bytes.len(),
            session_id = %sid,
            "evolve: download complete"
        );

        // Extract
        let bin_name = binary_name();
        let binary_data = if is_windows {
            extract_from_zip(&archive_bytes, bin_name)?
        } else {
            extract_from_tar_gz(&archive_bytes, bin_name)?
        };

        // Locate current executable
        let exe_path = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    target: "evolve",
                    error = %e,
                    session_id = %sid,
                    "evolve: current_exe() failed — cannot locate running binary"
                );
                return Ok(ToolResult::error(format!(
                    "Cannot locate current binary: {e}"
                )));
            }
        };

        // Write temp file
        let tmp_path = exe_path.with_extension("evolve_tmp");
        if let Err(e) = tokio::fs::write(&tmp_path, &binary_data).await {
            tracing::warn!(
                target: "evolve",
                tmp_path = %tmp_path.display(),
                error = %e,
                session_id = %sid,
                "evolve: failed to write temp binary"
            );
            return Ok(ToolResult::error(format!(
                "Failed to write new binary to {}: {e}",
                tmp_path.display()
            )));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            if let Err(e) = std::fs::set_permissions(&tmp_path, perms) {
                tracing::warn!(
                    target: "evolve",
                    tmp_path = %tmp_path.display(),
                    error = %e,
                    session_id = %sid,
                    "evolve: failed to set 0o755 on temp binary"
                );
                let _ = std::fs::remove_file(&tmp_path);
                return Ok(ToolResult::error(format!(
                    "Failed to set permissions on {}: {e}",
                    tmp_path.display()
                )));
            }
        }

        // Health-check before swap
        if let Some(ref cb) = self.progress {
            cb(
                sid,
                ProgressEvent::IntermediateText {
                    text: "Verifying new binary...".into(),
                    reasoning: None,
                },
            );
        }

        if let Err(reason) = health_check_binary(&tmp_path).await {
            tracing::warn!(
                target: "evolve",
                tmp_path = %tmp_path.display(),
                %reason,
                session_id = %sid,
                "evolve: pre-swap health check failed, discarding new binary"
            );
            let _ = std::fs::remove_file(&tmp_path);
            return Ok(ToolResult::error(format!(
                "Health check failed ({reason}). Keeping current v{current_version}."
            )));
        }

        // Backup
        let backup_path = exe_path.with_extension("evolve_backup");
        if let Err(e) = std::fs::copy(&exe_path, &backup_path) {
            tracing::warn!(
                target: "evolve",
                exe_path = %exe_path.display(),
                backup_path = %backup_path.display(),
                error = %e,
                session_id = %sid,
                "evolve: backup copy failed — rollback will not be possible if swap goes bad"
            );
        }

        // Unlink old binary first so the directory entry is freed. On Linux,
        // rename(2) by itself already replaces the directory entry atomically
        // without touching the old inode (the running process keeps its mapped
        // memory).  We still do remove_file first as a belt-and-suspenders
        // guard against NFS / FUSE mounts where rename(2) may behave
        // differently when the target is a running executable.
        let _ = std::fs::remove_file(&exe_path);
        if let Err(e) = std::fs::rename(&tmp_path, &exe_path) {
            tracing::warn!(
                target: "evolve",
                tmp_path = %tmp_path.display(),
                exe_path = %exe_path.display(),
                error = %e,
                session_id = %sid,
                "evolve: atomic rename of tmp -> exe failed"
            );
            let _ = std::fs::remove_file(&tmp_path);
            return Ok(ToolResult::error(format!(
                "Failed to replace binary at {}: {e}",
                exe_path.display()
            )));
        }

        // Post-swap verification
        if let Err(reason) = health_check_binary(&exe_path).await {
            if backup_path.exists() {
                if let Err(e) = std::fs::rename(&backup_path, &exe_path) {
                    tracing::error!(
                        target: "evolve",
                        exe_path = %exe_path.display(),
                        backup_path = %backup_path.display(),
                        post_swap_reason = %reason,
                        rollback_error = %e,
                        session_id = %sid,
                        "evolve: CRITICAL — post-swap health check failed AND rollback failed; \
                         binary at exe_path is broken and backup could not be restored. \
                         Manual recovery needed."
                    );
                    return Ok(ToolResult::error(format!(
                        "CRITICAL: New binary failed ({reason}) AND rollback failed: {e}. \
                         Manual recovery needed (backup is at {}).",
                        backup_path.display()
                    )));
                }
                tracing::error!(
                    target: "evolve",
                    exe_path = %exe_path.display(),
                    post_swap_reason = %reason,
                    session_id = %sid,
                    "evolve: post-swap health check failed, rolled back to previous version"
                );
                return Ok(ToolResult::error(format!(
                    "New binary failed post-swap ({reason}). Rolled back to v{current_version}."
                )));
            }
            tracing::error!(
                target: "evolve",
                exe_path = %exe_path.display(),
                post_swap_reason = %reason,
                session_id = %sid,
                "evolve: post-swap health check failed and no backup exists for rollback"
            );
            return Ok(ToolResult::error(format!(
                "New binary failed post-swap ({reason}). No backup for rollback."
            )));
        }

        let _ = std::fs::remove_file(&backup_path);

        // Schedule a delayed daemon restart for systemd-managed services.
        // This runs 3 seconds after the tool returns, giving the current
        // response enough time to be delivered before the daemon exits.
        //
        // We use systemd-run --on-active=N, which creates a transient timer
        // unit tracked by PID 1, outside our service cgroup.  This means the
        // timer survives even after `systemctl restart opencrabs*.service`
        // kills the current process.
        //
        // Only units matching the glob pattern are restarted, so adding a
        // new profile (e.g. opencrabs-staging.service) picks it up
        // automatically with no code change.
        if std::path::Path::new("/run/systemd/system").exists() {
            let unit_name = format!("opencrabs-evolve-{}", std::process::id());
            let _ = std::process::Command::new("systemd-run")
                .args([
                    "--on-active=3",
                    &format!("--unit={}", unit_name),
                    "systemctl",
                    "restart",
                    "opencrabs*.service",
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }

        // Signal restart
        if let Some(ref cb) = self.progress {
            cb(
                sid,
                ProgressEvent::RestartReady {
                    status: format!(
                        "Evolved: v{} -> v{}. Restarting now.",
                        current_version, latest_version
                    ),
                },
            );
        }

        Ok(ToolResult::success(format!(
            "Evolved from v{} to v{}. Restarting into the new version.",
            current_version, latest_version
        )))
    }
}

/// Extract a named file from a .tar.gz archive in memory.
fn extract_from_tar_gz(data: &[u8], file_name: &str) -> Result<Vec<u8>> {
    use std::io::Read;

    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .map_err(|e| super::error::ToolError::Execution(format!("Failed to read archive: {}", e)))?
    {
        let mut entry = entry.map_err(|e| {
            super::error::ToolError::Execution(format!("Failed to read entry: {}", e))
        })?;

        let path = entry
            .path()
            .map_err(|e| {
                super::error::ToolError::Execution(format!("Invalid path in archive: {}", e))
            })?
            .to_path_buf();

        if path.file_name().and_then(|n| n.to_str()) == Some(file_name) {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| {
                super::error::ToolError::Execution(format!("Failed to extract: {}", e))
            })?;
            return Ok(buf);
        }
    }

    Err(super::error::ToolError::Execution(format!(
        "'{}' not found in archive",
        file_name
    )))
}

/// Extract a named file from a .zip archive in memory.
fn extract_from_zip(data: &[u8], file_name: &str) -> Result<Vec<u8>> {
    use std::io::Read;

    let reader = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| super::error::ToolError::Execution(format!("Failed to read zip: {}", e)))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            super::error::ToolError::Execution(format!("Failed to read zip entry: {}", e))
        })?;

        if file.name().ends_with(file_name) {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(|e| {
                super::error::ToolError::Execution(format!("Failed to extract: {}", e))
            })?;
            return Ok(buf);
        }
    }

    Err(super::error::ToolError::Execution(format!(
        "'{}' not found in zip",
        file_name
    )))
}
