use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttRequest {
    /// Be sidecar att ladda modell. Skickas en gång vid första STT-användning.
    Load { model: String, device: String, compute_type: String, language: String },
    /// Skicka audio för transkription. Audio följer som raw f32 little-endian på stdin
    /// direkt efter JSON-raden, `audio_samples` många floats.
    Transcribe { audio_samples: u32, sample_rate: u32, beam_size: u32 },
    /// Stäng ner sidecar gracefully.
    Shutdown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttResponse {
    Ready,
    Loaded { load_ms: u64, vram_used_mb: Option<u64> },
    Transcript { text: String, inference_ms: u64, language: String, confidence: f32 },
    Error { message: String, recoverable: bool },
}
