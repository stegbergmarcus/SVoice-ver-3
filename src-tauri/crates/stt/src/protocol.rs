use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttRequest {
    /// Be sidecar att ladda modell. Skickas en gång vid första STT-användning.
    Load {
        model: String,
        device: String,
        compute_type: String,
        language: String,
    },
    /// Skicka audio för transkription. Audio följer som raw f32 little-endian på stdin
    /// direkt efter JSON-raden, `audio_samples` många floats.
    Transcribe {
        audio_samples: u32,
        sample_rate: u32,
        beam_size: u32,
        vad_filter: bool,
        initial_prompt: String,
        no_speech_threshold: f32,
        condition_on_previous_text: bool,
    },
    /// Be sidecar att ladda ner en HF-modell till disk-cache utan att
    /// ladda den i VRAM. Idempotent: no-op om modellen redan är komplett
    /// cachad. Efter download kan user byta till modellen via Load.
    DownloadModel { model: String },
    /// Stäng ner sidecar gracefully.
    Shutdown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttResponse {
    Ready,
    Loaded {
        load_ms: u64,
        vram_used_mb: Option<u64>,
    },
    Transcript {
        text: String,
        inference_ms: u64,
        language: String,
        confidence: f32,
    },
    /// Sidecar har startat download — används för UI "startar..."-status.
    /// Skickas direkt efter DownloadModel-request mottagen.
    DownloadStarted {
        model: String,
    },
    /// Download klar. `elapsed_ms` = tid från request till klar.
    Downloaded {
        model: String,
        elapsed_ms: u64,
    },
    Error {
        message: String,
        recoverable: bool,
    },
}
