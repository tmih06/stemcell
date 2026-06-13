# TOOLS.md - Tool Definitions

This file is for tool routing rules and pointers. Tool params, search/GitHub/browser routing,
and RSI instructions are already in the system prompt — don't duplicate them here.

## What belongs here

- Skill pointers (what/where to read on demand)
- Profile-aware paths
- Custom routing rules specific to your setup

## What does NOT belong here

- Failure logs or timestamps (use `feedback_record`)
- Full CLI references (put in skills, load on demand)
- Provider configuration (lives in config.toml + onboarding)
- System commands (basic OS knowledge)

## Skills (read on demand)

Skills are reference docs stored as `SKILL.md` files. When a task matches one,
read the file with your file-reading tool — it is loaded context, not an
executable command. Files live under `~/.stemcell/skills/<name>/SKILL.md`
(user overrides) or are bundled with the binary.

| Skill | Slug | What it covers |
|-------|------|----------------|
| Browser CDP | `browser-cdp` | CDP automation, selectors, screenshots |
| Channels | `channels` | Telegram, Discord, Slack, Trello, WhatsApp setup |
| Dynamic Tools | `dynamic-tools` | tools.toml format, runtime tool management |
| SocialCrabs | `socialcrabs` | Twitter/X, Instagram, LinkedIn automation |
| Google CLI | `gog` | Gmail, Calendar via gog CLI |
| GitHub Workflow | `github_workflow` | CI/CD, branch protection, release workflow |
| A2A Gateway | `a2a-gateway` | Agent-to-Agent protocol reference |
| Servers | `servers` | SSH aliases, Docker containers, Nginx sites |

## Profile-Aware Paths

| What | Path |
|------|------|
| Brain files | `~/.stemcell/{SOUL,USER,AGENTS,TOOLS,MEMORY,CODE,SECURITY}.md` |
| Config | `~/.stemcell/config.toml` |
| Keys | `~/.stemcell/keys.toml` |
| Plans | `~/.stemcell/agents/session/.stemcell_plan_<id>.json` |
| Logs | `~/.stemcell/logs/stemcell.YYYY-MM-DD` |
