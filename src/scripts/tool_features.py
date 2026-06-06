#!/usr/bin/env python3
"""Resolve per-tool build toggles from Cargo.toml into Cargo features."""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover
    print("Python 3.11+ with tomllib is required", file=sys.stderr)
    sys.exit(1)


TOOL_FEATURE_MAP: dict[str, list[str]] = {
    "read_file": ["tool-read"],
    "write_file": ["tool-write"],
    "edit_file": ["tool-edit"],
    "hashline_edit": ["tool-hashline-edit"],
    "bash": ["tool-bash"],
    "ls": ["tool-ls"],
    "glob": ["tool-glob"],
    "grep": ["tool-grep"],
    "web_search": ["tool-web-search"],
    "memory_search": ["tool-memory-search"],
    "session_search": ["tool-session-search"],
    "channel_search": ["tool-channel-search"],
    "exa_search": ["tool-exa-search"],
    "brave_search": ["tool-brave-search"],
    "task_manager": ["tool-task-manager"],
    "session_context": ["tool-session-context"],
    "http_request": ["tool-http-request"],
    "plan": ["tool-plan"],
    "execute_code": ["tool-execute-code"],
    "notebook_edit": ["tool-notebook-edit"],
    "parse_document": ["tool-parse-document"],
    "config_manager": ["tool-config-manager"],
    "follow_up_question": ["tool-follow-up-question"],
    "cron_manage": ["tool-cron-manage"],
    "spawn_agent": ["tool-spawn-agent"],
    "wait_agent": ["tool-wait-agent"],
    "send_input": ["tool-send-input"],
    "close_agent": ["tool-close-agent"],
    "resume_agent": ["tool-resume-agent"],
    "team_create": ["tool-team-create"],
    "team_delete": ["tool-team-delete"],
    "team_broadcast": ["tool-team-broadcast"],
    "feedback_record": ["tool-feedback-record"],
    "feedback_analyze": ["tool-feedback-analyze"],
    "self_improve": ["tool-self-improve"],
    "rsi_propose": ["tool-rsi-propose"],
    "generate_image": ["tool-generate-image"],
    "analyze_image": ["tool-analyze-image"],
    "analyze_video": ["tool-analyze-video"],
    "slash_command": ["tool-slash-command"],
    "rename_session": ["tool-rename-session"],
    "load_brain_file": ["tool-load-brain-file"],
    "write_opencrabs_file": ["tool-write-opencrabs-file"],
    "a2a_send": ["tool-a2a-send"],
    "telegram_connect": ["tool-telegram-connect"],
    "telegram_send": ["tool-telegram-send"],
    "whatsapp_connect": ["tool-whatsapp-connect"],
    "whatsapp_send": ["tool-whatsapp-send"],
    "discord_connect": ["tool-discord-connect"],
    "discord_send": ["tool-discord-send"],
    "slack_connect": ["tool-slack-connect"],
    "slack_send": ["tool-slack-send"],
    "trello_connect": ["tool-trello-connect"],
    "trello_send": ["tool-trello-send"],
    "browser_navigate": ["tool-browser-navigate"],
    "browser_screenshot": ["tool-browser-screenshot"],
    "browser_click": ["tool-browser-click"],
    "browser_type": ["tool-browser-type"],
    "browser_eval": ["tool-browser-eval"],
    "browser_content": ["tool-browser-content"],
    "browser_wait": ["tool-browser-wait"],
    "browser_find": ["tool-browser-find"],
    "browser_close": ["tool-browser-close"],
    "rebuild": ["tool-rebuild"],
    "evolve": ["tool-evolve"],
    "tool_manage": ["tool-tool-manage"],
    "rsi_proposals": ["tool-rsi-proposals"],
    "dynamic_runtime": ["tool-dynamic-runtime"],
}


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    sys.exit(1)


def load_toggles(manifest_path: Path) -> dict[str, bool]:
    if not manifest_path.exists():
        fail(f"Manifest not found: {manifest_path}")

    with manifest_path.open("rb") as fh:
        cargo = tomllib.load(fh)

    toggles = (
        cargo.get("package", {})
        .get("metadata", {})
        .get("tool_toggles")
    )
    if not isinstance(toggles, dict):
        fail("Cargo.toml missing [package.metadata.tool_toggles]")

    expected = set(TOOL_FEATURE_MAP)
    actual = set(toggles)

    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    invalid = sorted(k for k, v in toggles.items() if not isinstance(v, bool))

    errors = []
    if missing:
        errors.append("missing keys: " + ", ".join(missing))
    if extra:
        errors.append("unknown keys: " + ", ".join(extra))
    if invalid:
        errors.append("non-boolean keys: " + ", ".join(invalid))
    if errors:
        fail("Invalid [package.metadata.tool_toggles]: " + " | ".join(errors))

    return {key: toggles[key] for key in TOOL_FEATURE_MAP}


def resolve_features(toggles: dict[str, bool]) -> list[str]:
    features: list[str] = ["rtk"]
    for tool_name, enabled in toggles.items():
        if not enabled:
            continue
        for feature in TOOL_FEATURE_MAP[tool_name]:
            if feature not in features:
                features.append(feature)
    return features


def main() -> None:
    manifest_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("Cargo.toml")
    toggles = load_toggles(manifest_path)
    print(",".join(resolve_features(toggles)))


if __name__ == "__main__":
    main()
