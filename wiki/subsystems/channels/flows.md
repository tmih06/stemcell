# Channels — Flows

## Channel Connection Flow

```
Start
  │
  ▼
Feature check ──No──▶ Skip (channel disabled)
  │
 Yes
  ▼
ChannelFactory::create(config)
  │
  ▼
ChannelManager::connect(channel)
  │
  ├──▶ Telegram:  teloxide::Bot::new → start_polling → update listener
  ├──▶ Discord:  serenity::Client::new → start → event handler
  ├──▶ Slack:    slack-morphism socket mode → connect → event stream
  ├──▶ WhatsApp: whatsapp-rust QR pairing → connect → message stream
  ├──▶ Trello:   HTTP client → optional polling interval
  └──▶ Voice:    (called by Telegram/other channels, not standalone)
  │
  ▼
generate_connection_greeting()
  │
  ▼
Handler loop (message receive)
```

## Telegram Update Flow

```
Telegram Polling
  │
  ▼
Update received (teloxide)
  │
  ▼
handler.rs::handle_update
  │
  ├──▶ /command → CommandHandler
  │
  └──▶ Text/Photo/Voice
         │
         ▼
       session_resolve.rs
         ├── Owner DM → shared TUI session
         └── Group    → per-group session
         │
         ▼
       AgentService::process_message
         │
         ├──▶ STT (voice notes) → transcribe
         │
         └──▶ LLM call
              │
              ▼
            Response
              │
              ▼
            send.rs::reply_text / reply_photo / reply_voice
```

## Channel Message Flow (General)

```
Channel receive
  │
  ▼
Handler::handle_message
  │
  ▼
session_resolve::resolve(channel, user, chat)
  │
  ├── Existing session? → resume
  └── New session?      → session_init → create
  │
  ▼
AgentService::process_message(text, context)
  │
  ├── Tool calls → execute → continue
  │
  └── Response
       │
       ▼
     Channel send reply
```

## Voice STT/TTS Flow

```
Audio input (voice note, microphone)
  │
  ▼
transcribe_audio(bytes, voice_config)
  │
  ├── Primary STT provider
  │   ├── voicebox_stt
  │   ├── openai_stt (Groq Whisper / OpenAI-compatible)
  │   └── local_whisper (rwhisper, candle)
  │
  ├── On failure: walk stt_fallback_chain
  │   (user-configured order, e.g. ["groq", "openai_compatible", "local"])
  │
  ▼
Text
  │
  ▼
AgentService → LLM → Response text
  │
  ▼
synthesize_speech(text, voice_config)
  │
  ├── voicebox_tts
  ├── openai_tts
  └── local_tts (Piper via rodio)
  │
  ▼
Audio output (voice reply)
```

## Trello Flow

```
Optional polling (or command-triggered)
  │
  ▼
client.rs::fetch_board / fetch_list / fetch_card
  │
  ▼
handler.rs → agent.rs → LLM interprets action
  │
  ▼
client.rs::perform_action (create/update/move card, etc.)
  │
  ▼
Response formatted as message
```
