//! Voice Processing Module
//!
//! Speech-to-text and text-to-speech services.
//! Supports:
//! - API-based STT (Groq Whisper, OpenAI-compatible, Voicebox)
//! - Local STT (whisper.cpp / rwhisper)
//! - API-based TTS (OpenAI, OpenAI-compatible, Voicebox)
//! - Local TTS (Piper)

pub mod openai_stt;
pub mod openai_tts;
pub mod voicebox_stt;
pub mod voicebox_tts;

#[cfg(feature = "local-stt")]
pub mod local_whisper;

#[cfg(feature = "local-tts")]
pub mod local_tts;

pub(crate) mod service;

pub use service::{synthesize, synthesize_speech, transcribe, transcribe_audio};

#[cfg(feature = "local-stt")]
pub use service::{preload_local_whisper, transcribe_audio_local};

/// Runtime disable flag for local STT. Set to `false` at startup via
/// `disable_local_stt()` when `config.features.local_stt = false`.
static LOCAL_STT_RUNTIME_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Runtime disable flag for local TTS. Set to `false` at startup via
/// `disable_local_tts()` when `config.features.local_tts = false`.
static LOCAL_TTS_RUNTIME_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Disable local STT at runtime (called at startup when `features.local_stt = false`).
pub fn disable_local_stt() {
    LOCAL_STT_RUNTIME_ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
    tracing::info!("Local STT disabled via features.local_stt = false");
}

/// Disable local TTS at runtime (called at startup when `features.local_tts = false`).
pub fn disable_local_tts() {
    LOCAL_TTS_RUNTIME_ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
    tracing::info!("Local TTS disabled via features.local_tts = false");
}

/// Returns true if local STT is compiled in, enabled at runtime, and can run on this machine.
///
/// On x86_64, candle (the inference backend) requires AVX2. We check for it
/// at runtime so that machines without AVX2 (e.g. Sandy Bridge) never attempt
/// local STT and get a SIGILL crash.
pub fn local_stt_available() -> bool {
    if !LOCAL_STT_RUNTIME_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    if !cfg!(feature = "local-stt") {
        return false;
    }
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        true // ARM/Apple Silicon — no AVX2 constraint
    }
}

/// Returns true if local TTS (Piper) can run on this machine and is not disabled at runtime.
/// Result is cached so the probe runs at most once per process.
pub fn local_tts_available() -> bool {
    if !LOCAL_TTS_RUNTIME_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    #[cfg(feature = "local-tts")]
    {
        static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            // Check python3 exists
            let python_ok = std::process::Command::new("python3")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !python_ok {
                return false;
            }
            // Check venv module is available (missing on some Debian/Ubuntu installs)
            std::process::Command::new("python3")
                .args(["-c", "import venv"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
    }
    #[cfg(not(feature = "local-tts"))]
    {
        false
    }
}
