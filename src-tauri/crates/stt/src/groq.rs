//! Groq Whisper-API som STT-provider.
//!
//! Endpoint: POST https://api.groq.com/openai/v1/audio/transcriptions
//! Auth:     Bearer <api_key>
//! Format:   multipart/form-data med WAV-fil
//!
//! Groq Whisper Large v3 Turbo: ~100x snabbare än lokal på medel-CPU,
//! gratis-tier 25 req/minut. Kräver internet + API-nyckel.

use serde::Deserialize;

const API_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
pub const DEFAULT_MODEL: &str = "whisper-large-v3-turbo";

#[derive(Debug, thiserror::Error)]
pub enum GroqSttError {
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Groq API {status}: {body}")]
    Api { status: u16, body: String },
    #[error("saknar API-nyckel")]
    MissingKey,
}

pub struct GroqStt {
    api_key: String,
    model: String,
    /// ISO-språkkod eller "auto". Default "sv".
    language: String,
    http: reqwest::Client,
}

impl GroqStt {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.into(),
            language: "sv".into(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest"),
        }
    }

    pub fn with_model(mut self, m: impl Into<String>) -> Self {
        self.model = m.into();
        self
    }

    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = lang.into();
        self
    }

    /// Transkribera 16kHz mono f32-samples till text.
    pub async fn transcribe(&self, audio: &[f32]) -> Result<String, GroqSttError> {
        if self.api_key.is_empty() {
            return Err(GroqSttError::MissingKey);
        }
        let wav = encode_wav_16khz_mono(audio);

        let form = reqwest::multipart::Form::new()
            .text("model", self.model.clone())
            .text("response_format", "json")
            .part(
                "file",
                reqwest::multipart::Part::bytes(wav)
                    .file_name("audio.wav")
                    .mime_str("audio/wav")
                    .expect("valid mime"),
            );
        let form = if self.language != "auto" && !self.language.is_empty() {
            form.text("language", self.language.clone())
        } else {
            form
        };

        let resp = self
            .http
            .post(API_URL)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GroqSttError::Api {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: TranscriptionResponse = resp.json().await?;
        Ok(parsed.text)
    }
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// WAV-encoding för 16kHz mono 16-bit PCM — 44-byte header + samples.
/// f32-samples clampas till [-1.0, 1.0] och skalas till i16.
fn encode_wav_16khz_mono(samples: &[f32]) -> Vec<u8> {
    let sample_rate: u32 = 16000;
    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * 2; // 16-bit mono = 2 bytes/sample
    let data_size = num_samples * 2;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + data_size as usize);
    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        buf.extend_from_slice(&pcm.to_le_bytes());
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_has_correct_magic_and_sizes() {
        let samples = vec![0.5_f32; 16000]; // 1 sekund
        let wav = encode_wav_16khz_mono(&samples);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        // 44 + 16000*2 = 32044 total
        assert_eq!(wav.len(), 44 + 16000 * 2);
    }

    #[test]
    fn f32_clamps_to_i16_range() {
        let samples = vec![2.0_f32, -2.0, 0.0]; // utanför range
        let wav = encode_wav_16khz_mono(&samples);
        // Sista 6 bytes = 3 * i16 samples
        let s0 = i16::from_le_bytes([wav[44], wav[45]]);
        let s1 = i16::from_le_bytes([wav[46], wav[47]]);
        let s2 = i16::from_le_bytes([wav[48], wav[49]]);
        assert_eq!(s0, i16::MAX); // 2.0 → clamped till 1.0 → i16::MAX
        assert_eq!(s1, -i16::MAX); // -2.0 → -1.0 → -i16::MAX
        assert_eq!(s2, 0);
    }

    #[test]
    fn empty_audio_produces_only_header() {
        let wav = encode_wav_16khz_mono(&[]);
        assert_eq!(wav.len(), 44);
    }
}
