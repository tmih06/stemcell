# TOOLS.md - Tool Definitions

This file is for tool routing rules and pointers. Tool params, search/GitHub/browser routing,
and RSI instructions are already in the system prompt — don't duplicate them here.

## What belongs here

- Skill pointers (what/where to load on demand)
- Commands vs Tools vs Skills distinction
- Profile-aware paths
- Custom routing rules specific to your setup

## What does NOT belong here

- Failure logs or timestamps (use `feedback_record`)
- Full CLI references (put in skills, load on demand)
- Provider configuration (lives in config.toml + onboarding)
- System commands (basic OS knowledge)

## Skills (load on demand)

| Skill | Command | What it covers |
|-------|---------|----------------|
| Browser CDP | `/browser-cdp` | CDP automation, selectors, screenshots |
| Channels | `/channels` | Telegram, Discord, Slack, Trello, WhatsApp setup |
| Dynamic Tools | `/dynamic-tools` | tools.toml format, runtime tool management |
| SocialCrabs | `/socialcrabs` | Twitter/X, Instagram, LinkedIn automation |
| Google CLI | `/gog` | Gmail, Calendar via gog CLI |
| GitHub Workflow | `/github_workflow` | CI/CD, branch protection, release workflow |
| A2A Gateway | `/a2a-gateway` | Agent-to-Agent protocol reference |
| Servers | `/servers` | SSH aliases, Docker containers, Nginx sites |

## Commands vs Tools vs Skills

| Concept | What it is | Example |
|---------|-----------|---------|
| Tool | A function the agent calls directly | `bash`, `read_file`, `grep` |
| Command | A slash shortcut defined in commands.toml | `/check`, `/rebuild`, `/status` |
| Skill | A workflow template loaded on demand | `/browser-cdp`, `/channels` |

## Build Commands

- `/rebuild` — Build, test, and hot-restart from source
- `/check` — Run `cargo clippy` and `cargo test`
- `/evolve` — Download latest release binary

## Profile-Aware Paths

| What | Path |
|------|------|
| Brain files | `~/.opencrabs/{SOUL,USER,AGENTS,TOOLS,MEMORY,CODE,SECURITY}.md` |
| Config | `~/.opencrabs/config.toml` |
| Keys | `~/.opencrabs/keys.toml` |
| Commands | `~/.opencrabs/commands.toml` |
| Plans | `~/.opencrabs/agents/session/.opencrabs_plan_<id>.json` |
| Logs | `~/.opencrabs/logs/opencrabs.YYYY-MM-DD` |
