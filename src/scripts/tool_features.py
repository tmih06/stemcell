#!/usr/bin/env python3
"""Resolve build_toggles.toml into a Cargo `--features` list.

The toggle file groups keys into sections (currently `[tools]`,
`[channels]`, `[capabilities]`) so adding a new feature category is
just: add a section to the file + add an entry to TOGGLE_TO_FEATURE
below.

Usage:
    python3 tool_features.py [path/to/build_toggles.toml]

Prints a comma-separated list of features to enable.
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover
    print("Python 3.11+ with tomllib is required", file=sys.stderr)
    sys.exit(1)


# Maps every recognised toggle key to the single cargo feature it
# enables. Keep in sync with `TOGGLE_TO_FEATURE` in build.rs.
TOGGLE_TO_FEATURE: dict[str, str] = {
    # tools
    "read_file": "tool-read",
    "write_file": "tool-write",
    "edit_file": "tool-edit",
    "hashline_edit": "tool-hashline-edit",
    "bash": "tool-bash",
    "ls": "tool-ls",
    "glob": "tool-glob",
    "grep": "tool-grep",
    "web_search": "tool-web-search",
    "memory_search": "tool-memory-search",
    "session_search": "tool-session-search",
    "channel_search": "tool-channel-search",
    "exa_search": "tool-exa-search",
    "brave_search": "tool-brave-search",
    "task_manager": "tool-task-manager",
    "session_context": "tool-session-context",
    "http_request": "tool-http-request",
    "plan": "tool-plan",
    "execute_code": "tool-execute-code",
    "notebook_edit": "tool-notebook-edit",
    "parse_document": "tool-parse-document",
    "config_manager": "tool-config-manager",
    "follow_up_question": "tool-follow-up-question",
    "cron_manage": "tool-cron-manage",
    "spawn_agent": "tool-spawn-agent",
    "wait_agent": "tool-wait-agent",
    "send_input": "tool-send-input",
    "close_agent": "tool-close-agent",
    "resume_agent": "tool-resume-agent",
    "team_create": "tool-team-create",
    "team_delete": "tool-team-delete",
    "team_broadcast": "tool-team-broadcast",
    "feedback_record": "tool-feedback-record",
    "feedback_analyze": "tool-feedback-analyze",
    "self_improve": "tool-self-improve",
    "rsi_propose": "tool-rsi-propose",
    "generate_image": "tool-generate-image",
    "analyze_image": "tool-analyze-image",
    "analyze_video": "tool-analyze-video",
    "slash_command": "tool-slash-command",
    "rename_session": "tool-rename-session",
    "load_brain_file": "tool-load-brain-file",
    "write_opencrabs_file": "tool-write-opencrabs-file",
    "a2a_send": "tool-a2a-send",
    "telegram_connect": "tool-telegram-connect",
    "telegram_send": "tool-telegram-send",
    "whatsapp_connect": "tool-whatsapp-connect",
    "whatsapp_send": "tool-whatsapp-send",
    "discord_connect": "tool-discord-connect",
    "discord_send": "tool-discord-send",
    "slack_connect": "tool-slack-connect",
    "slack_send": "tool-slack-send",
    "trello_connect": "tool-trello-connect",
    "trello_send": "tool-trello-send",
    "browser_navigate": "tool-browser-navigate",
    "browser_screenshot": "tool-browser-screenshot",
    "browser_click": "tool-browser-click",
    "browser_type": "tool-browser-type",
    "browser_eval": "tool-browser-eval",
    "browser_content": "tool-browser-content",
    "browser_wait": "tool-browser-wait",
    "browser_find": "tool-browser-find",
    "browser_close": "tool-browser-close",
    "rebuild": "tool-rebuild",
    "evolve": "tool-evolve",
    "tool_manage": "tool-tool-manage",
    "rsi_proposals": "tool-rsi-proposals",
    "dynamic_runtime": "tool-dynamic-runtime",
    # channels
    "telegram": "telegram",
    "whatsapp": "whatsapp",
    "discord": "discord",
    "slack": "slack",
    "trello": "trello",
    # capabilities
    "local-stt": "local-stt",
    "local-tts": "local-tts",
    "browser": "browser",
    "pdfium": "pdfium",
    "rtk": "rtk",
}


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    sys.exit(1)


def load_toggles(path: Path) -> dict[str, bool]:
    if not path.exists():
        fail(f"toggle file not found: {path}")

    with path.open("rb") as fh:
        data = tomllib.load(fh)

    if not isinstance(data, dict):
        fail(f"{path}: top level must be a TOML table")

    flat: dict[str, bool] = {}
    for section, entries in data.items():
        if not isinstance(entries, dict):
            fail(f"{path}: [{section}] must be a table of booleans")
        for key, value in entries.items():
            if not isinstance(value, bool):
                fail(f"{path}: [{section}] {key!r} must be a boolean, got {type(value).__name__}")
            flat[key] = value

    expected = set(TOGGLE_TO_FEATURE)
    actual = set(flat)

    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    errors: list[str] = []
    if missing:
        errors.append(
            f"{path}: missing toggle keys (add them to the appropriate section): "
            + ", ".join(missing)
        )
    if extra:
        errors.append(
            f"{path}: unknown toggle keys (remove or register in TOGGLE_TO_FEATURE): "
            + ", ".join(extra)
        )
    if errors:
        fail(" | ".join(errors))

    # Preserve canonical order so the output is stable for caching.
    return {key: flat[key] for key in TOGGLE_TO_FEATURE}


# Coarse `tools-*` alias features that source code's `#[cfg(feature = …)]`
# gates depend on. Each alias is auto-enabled when any of its sub-tools is
# on, so existing cfg gates (e.g. `#[cfg(feature = "tools-rsi")]`) keep
# working even though the user-facing toggle is the per-tool one.
ALIAS_SUB_TOOLS: dict[str, tuple[str, ...]] = {
    "tools-rsi": (
        "tool-feedback-record",
        "tool-feedback-analyze",
        "tool-self-improve",
        "tool-rsi-propose",
    ),
    "tools-dynamic": (
        "tool-tool-manage",
        "tool-rsi-proposals",
        "tool-dynamic-runtime",
    ),
}


def resolve_features(toggles: dict[str, bool]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for key, enabled in toggles.items():
        if not enabled:
            continue
        feature = TOGGLE_TO_FEATURE[key]
        if feature in seen:
            continue
        seen.add(feature)
        out.append(feature)
    # Auto-enable coarse `tools-*` alias features when any of their
    # sub-tools is on, so legacy `#[cfg(feature = "tools-rsi")]`-style
    # gates in the source still resolve.
    for alias, sub_tools in ALIAS_SUB_TOOLS.items():
        if alias in seen:
            continue
        if any(st in seen for st in sub_tools):
            seen.add(alias)
            out.append(alias)
    return out


def main() -> None:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("build_toggles.toml")
    toggles = load_toggles(path)
    print(",".join(resolve_features(toggles)))


if __name__ == "__main__":
    main()
