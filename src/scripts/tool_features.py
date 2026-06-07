#!/usr/bin/env python3
"""Resolve build_toggles.toml into a Cargo `--features` list.

The toggle file groups keys into packs ([file], [search], [rsi], …)
so each toggle is coarse enough that enabling it makes sense on its
own — there's no `spawn_agent` without `wait_agent` here. Each pack
key maps to one or more Cargo features. Some packs imply others
(e.g. `file-write` implies `file-read`). Legacy `#[cfg(feature =
"tools-rsi")]`-style gates in the source are kept happy by
auto-including the matching alias features.

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


# Each pack toggle maps to the cargo features it enables. Keep in
# sync with `TOGGLE_TO_FEATURES` in build.rs.
TOGGLE_TO_FEATURES: dict[str, tuple[str, ...]] = {
    # capabilities
    "local-stt": ("local-stt",),
    "local-tts": ("local-tts",),
    "browser": (
        "browser",
        "tool-browser-navigate",
        "tool-browser-screenshot",
        "tool-browser-click",
        "tool-browser-type",
        "tool-browser-eval",
        "tool-browser-content",
        "tool-browser-wait",
        "tool-browser-find",
        "tool-browser-close",
    ),
    "pdfium": ("pdfium",),
    "rtk": ("rtk",),
    "profiling": ("profiling",),
    # channels — each maps to just its own client feature now that channels
    # are remote surfaces on the gateway, not agent tools.
    "telegram": ("telegram",),
    "whatsapp": ("whatsapp",),
    "discord": ("discord",),
    "slack": ("slack",),
    "trello": ("trello",),
    # file tier
    "file-read": ("tool-read", "tool-ls", "tool-glob", "tool-grep"),
    "file-write": ("tool-write", "tool-edit", "tool-hashline-edit"),
    # bash tier
    "bash": ("tool-bash",),
    # search
    "web-search": (
        "tool-web-search",
        "tool-exa-search",
        "tool-brave-search",
    ),
    "memory-search": (
        "tool-memory-search",
        "tool-session-search",
        "tool-channel-search",
    ),
    # workflow
    "workflow": (
        "tool-task-manager",
        "tool-session-context",
        "tool-plan",
        "tool-http-request",
        "tool-execute-code",
        "tool-notebook-edit",
        "tool-parse-document",
        "tool-config-manager",
        "tool-follow-up-question",
        "tool-cron-manage",
    ),
    # multi-agent
    "multi-agent": (
        "tool-spawn-agent",
        "tool-wait-agent",
        "tool-send-input",
        "tool-close-agent",
        "tool-resume-agent",
        "tool-team-create",
        "tool-team-delete",
        "tool-team-broadcast",
    ),
    # rsi
    "rsi": (
        "tool-feedback-record",
        "tool-feedback-analyze",
        "tool-self-improve",
        "tool-rsi-propose",
        "tool-rsi-proposals",
        "tool-tool-manage",
        "tool-rebuild",
        "tool-evolve",
        "tool-dynamic-runtime",
    ),
    # image
    "image": (
        "tool-generate-image",
        "tool-analyze-image",
        "tool-analyze-video",
    ),
    # brain
    "brain": (
        "tool-slash-command",
        "tool-rename-session",
        "tool-load-brain-file",
        "tool-write-opencrabs-file",
        "tool-a2a-send",
    ),
    # providers
    "claude-cli": ("provider-claude-cli",),
    "codex-cli": ("provider-codex-cli",),
    "opencode-cli": ("provider-opencode-cli",),
}

# Implication rules: enabling `key` also enables `other_key` first
# (recursively). Use for packs that don't make sense without a
# prerequisite.
IMPLIES: dict[str, tuple[str, ...]] = {
    "file-write": ("file-read",),
}

# Legacy `tools-*` alias features that source code's
# `#[cfg(feature = "...")]` gates depend on. Each alias is
# auto-enabled when ANY of the listed pack keys is on, so existing
# gates keep working even though the user-facing toggle is the
# coarser pack. Keep in sync with `ALIAS_FROM_PACKS` in build.rs.
ALIAS_FROM_PACKS: dict[str, tuple[str, ...]] = {
    "tools-rsi": ("rsi",),
    "tools-dynamic": ("rsi",),
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

    expected = set(TOGGLE_TO_FEATURES)
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
            f"{path}: unknown toggle keys (remove or register in TOGGLE_TO_FEATURES): "
            + ", ".join(extra)
        )
    if errors:
        fail(" | ".join(errors))

    return {key: flat[key] for key in TOGGLE_TO_FEATURES}


def expand_implies(enabled: set[str]) -> set[str]:
    """Apply IMPLIES transitively: if X implies Y, enabling X
    enables Y too. Loop until no new keys are added (handles
    chains like A → B → C)."""
    changed = True
    while changed:
        changed = False
        for key, deps in IMPLIES.items():
            if key in enabled:
                for dep in deps:
                    if dep not in enabled:
                        enabled.add(dep)
                        changed = True
    return enabled


def resolve_features(toggles: dict[str, bool]) -> list[str]:
    enabled = {key for key, on in toggles.items() if on}
    enabled = expand_implies(enabled)

    seen: set[str] = set()
    out: list[str] = []
    for key in enabled:
        for feature in TOGGLE_TO_FEATURES[key]:
            if feature in seen:
                continue
            seen.add(feature)
            out.append(feature)

    # Auto-enable coarse `tools-*` alias features when any of the
    # packs that contribute to them is on, so legacy
    # `#[cfg(feature = "tools-rsi")]`-style gates in the source
    # still resolve.
    for alias, required_packs in ALIAS_FROM_PACKS.items():
        if alias in seen:
            continue
        if any(p in enabled for p in required_packs):
            seen.add(alias)
            out.append(alias)

    return out


def main() -> None:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("build_toggles.toml")
    toggles = load_toggles(path)
    print(",".join(resolve_features(toggles)))


if __name__ == "__main__":
    main()
