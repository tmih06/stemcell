---
name: dynamic-tools
description: "Runtime tool management with tool_manage and tools.toml format. Create, enable, disable, reload tools without restart. (/dynamic-tools, tool_manage, runtime tools)"
---

# Dynamic Tools Reference

Runtime tool creation — define tools in `~/.opencrabs/tools.toml` and they become callable immediately without restart or rebuild.

## tool_manage Meta-Tool

| Action | Required | What |
|--------|----------|------|
| `list` | — | Show all dynamic tools with enabled/disabled status |
| `add` | `name`, `description`, `executor`, `method`/`command` | Create new tool (persists to tools.toml) |
| `remove` | `name` | Delete a tool |
| `enable` / `disable` | `name` | Toggle without removing |
| `reload` | — | Hot-reload from tools.toml |

## Executor Types

- **`http`** — Make HTTP requests. Config: `method`, `url`, `headers` (optional), `params`.
- **`shell`** — Run shell commands. Config: `command`, `params`.

## tools.toml Format

```toml
[[tools]]
name = "check_api_health"
description = "Check if the production API is responding"
executor_type = "http"
enabled = true

[tools.executor_config]
method = "GET"
url = "https://api.example.com/health"

[[tools]]
name = "disk_usage"
description = "Check disk usage on the system"
executor_type = "shell"
enabled = true

[tools.executor_config]
command = "df"
args = ["-h"]
```

## How It Works

1. On startup, tools from `tools.toml` load into the `ToolRegistry` alongside compiled tools.
2. Dynamic tools appear in the LLM's tool list — the agent calls them autonomously.
3. Use `tool_manage add` to create new tools at runtime.
4. Use `tool_manage reload` to pick up manual edits to `tools.toml`.
5. Unlike `commands.toml` (user-triggered slash commands), these are **agent-callable tools**.
