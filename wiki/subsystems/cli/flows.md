# CLI & Config — Flows

## CLI Dispatch Flow

```
main()
  │
  ▼
Cli::parse()                              (args.rs:384)
  │
  ├── profile::set_active_profile()        (args.rs:387)
  │
  ├── commands::load_config()              (args.rs:404)
  │
  └── match command:
       │
       ├── None / Chat ──▶ ui::cmd_chat(config, session, force_onboard)
       │
       ├── Onboard      ──▶ ui::cmd_chat(config, None, true)
       │
       ├── Run          ──▶ commands::cmd_run(config, prompt, auto_approve, format)
       │
       ├── Agent        ──▶ cmd_run (single msg) or cmd_agent_interactive (multi-turn)
       │
       ├── Status       ──▶ commands::cmd_status(config)
       │
       ├── Doctor       ──▶ commands::cmd_doctor(config)
       │
       ├── Init         ──▶ commands::cmd_init(config, force)
       │
       ├── Config       ──▶ commands::cmd_config(config, show_secrets)
       │
       ├── Db           ──▶ commands::cmd_db(config, operation)
       │
       ├── Logs         ──▶ commands::cmd_logs(operation)
       │
       ├── Channel      ──▶ commands::cmd_channel(config, operation)
       │
       ├── Memory       ──▶ commands::cmd_memory(operation)
       │
       ├── Session      ──▶ commands::cmd_session(config, operation)
       │
       ├── Service      ──▶ commands::cmd_service(operation)
       │
       ├── Daemon       ──▶ ui::cmd_daemon(config)
       │
       ├── Profile      ──▶ commands::cmd_profile(operation)
       │
       ├── Cron         ──▶ cron::cmd_cron(config, operation)
       │
       ├── Completions  ──▶ clap_complete::generate(shell)
       │
       ├── Version      ──▶ println!("stemcell {}", env!("CARGO_PKG_VERSION"))
       │
       └── Evolve       ──▶ commands::cmd_evolve(check_only)
```

## Chat Flow

```
args → Chat
  │
  ▼
ui::cmd_chat(config, session_id, force_onboard)
  │
  ├── force_onboard? → launch onboarding wizard
  └── normal          → init services → start TUI loop
       │
       ├── Provider + AgentService
       ├── Database connection
       └── Brain loader
  │
  ▼
TUI event loop (ratatui)
  │
  ├── User input → LLM call → render response
  └── Ctrl+C/Ctrl+D → graceful shutdown
```

## Run Flow

```
args → Run <prompt>
  │
  ▼
commands::cmd_run(config, prompt, auto_approve, format)
  │
  ├── Init provider
  ├── Single LLM call
  │
  └── Print response → exit
```

## Agent Flow

```
args → Agent
  │
  ▼
commands::cmd_agent_interactive(config, auto_approve)
  │
  ├── Init provider + services
  ├── Agent loop:
  │   ├── Read stdin
  │   ├── LLM call (with possible tool calls)
  │   └── Write stdout
  └── Ctrl+C/Ctrl+D → exit
```

## Daemon Flow

```
args → Daemon
  │
  ▼
ui::cmd_daemon(config)
  │
  ├── Init channel manager
  ├── Start all enabled channel bots (Telegram, Discord, Slack, WhatsApp)
  │
  └── Block forever (systemd/launchd manages lifecycle)
```

## Config Loading Flow

```
load_config(path_override?)
  │
  ▼
Resolve config directory (~/.config/stemcell/ or STEMCELL_HOME)
  │
  ▼
Read config.toml
  │
  ▼
Read keys.toml
  │
  ▼
merge_provider_keys(config, keys)          (types.rs)
  │
  ▼
Deserialize → Config struct               (types.rs)
  │
  ▼
Validate                                   (health.rs)
  │
  ▼
Return Config
```

## Config Hot-Reload Flow

```
notify::Watcher watches config.toml
  │
  ▼
File change event
  │
  ▼
Re-read config.toml + keys.toml
  │
  ▼
Deserialize → validate
  │
  ▼
Apply: update ChannelManager, provider, etc.
  │
  ▼
Log: "Config hot-reloaded"
```
