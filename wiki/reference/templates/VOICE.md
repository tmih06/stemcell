# VOICE.md — Voice Configuration

Voice settings for TTS (text-to-speech) and STT (speech-to-text) integration.

## TTS Providers

Configure text-to-speech providers:

```toml
[voice.tts]
provider = "elevenlabs"  # or "coqui", "openai", "gtts"
voice_id = "rachel"      # provider-specific voice ID
```

### Supported Providers

| Provider | Description | Voice IDs |
|----------|-------------|-----------|
| **ElevenLabs** | High-quality neural voices | Rachel, Adam, Antoni, Bella, etc. |
| **Coqui** | Open-source TTS | Varies by model |
| **OpenAI** | TTS-1 models | alloy, echo, fable, onyx, nova, shimmer |
| **GTTS** | Google Translate TTS | N/A (single voice) |

## Voice Preferences

Per-user voice settings:

```toml
[voice.preferences."+15551234567"]
voice_id = "rachel"
speed = 1.0    # 0.5 - 2.0
pitch = 0      # -12 to +12 semitones
```

## STT — Speech-to-Text

**Supported Backends:**
- OpenAI Whisper API
- Local Whisper model (via whisper.cpp)
- FasterWhisper

## Voice Message Handling

When receiving voice messages (Telegram/WhatsApp):

1. Download audio file
2. Transcribe via STT
3. Process text normally
4. Optionally respond with TTS voice message

