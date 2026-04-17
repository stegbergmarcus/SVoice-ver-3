use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::protocol::{SttRequest, SttResponse};
use crate::sidecar::{Sidecar, SidecarError};

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error("modell ej laddad")]
    NotLoaded,
    #[error("sidecar svarade med fel: {0}")]
    Remote(String),
    #[error("oväntat svar: {0}")]
    Unexpected(String),
}

#[derive(Debug, Clone)]
pub struct SttConfig {
    pub model: String,
    pub device: String,
    pub compute_type: String,
    pub language: String,
    pub beam_size: u32,
    pub python_path: PathBuf,
    pub script_path: PathBuf,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            model: "KBLab/kb-whisper-medium".into(),
            device: "cuda".into(),
            compute_type: "float16".into(),
            language: "sv".into(),
            beam_size: 3,
            python_path: PathBuf::from("py"),
            script_path: PathBuf::from("src-tauri/resources/python/stt_sidecar.py"),
        }
    }
}

pub struct PythonStt {
    sidecar: Arc<Mutex<Option<Sidecar>>>,
    config: SttConfig,
}

impl PythonStt {
    pub fn new(config: SttConfig) -> Self {
        Self { sidecar: Arc::new(Mutex::new(None)), config }
    }

    async fn ensure_loaded(&self) -> Result<(), SttError> {
        let mut guard = self.sidecar.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let sc = Sidecar::spawn(&self.config.python_path, &self.config.script_path).await?;
        sc.send_request(&SttRequest::Load {
            model: self.config.model.clone(),
            device: self.config.device.clone(),
            compute_type: self.config.compute_type.clone(),
            language: self.config.language.clone(),
        })
        .await?;
        match sc.read_response().await? {
            SttResponse::Loaded { load_ms, vram_used_mb } => {
                tracing::info!(
                    "STT-modell laddad på {} ms (VRAM: {:?} MB)",
                    load_ms, vram_used_mb
                );
            }
            SttResponse::Error { message, .. } => return Err(SttError::Remote(message)),
            other => return Err(SttError::Unexpected(format!("{other:?}"))),
        }
        *guard = Some(sc);
        Ok(())
    }

    pub async fn transcribe(&self, audio: &[f32]) -> Result<String, SttError> {
        self.ensure_loaded().await?;
        let guard = self.sidecar.lock().await;
        let sc = guard.as_ref().ok_or(SttError::NotLoaded)?;
        sc.send_request(&SttRequest::Transcribe {
            audio_samples: audio.len() as u32,
            sample_rate: 16000,
            beam_size: self.config.beam_size,
        })
        .await?;
        sc.send_audio(audio).await?;
        match sc.read_response().await? {
            SttResponse::Transcript { text, inference_ms, .. } => {
                tracing::info!("STT: {} ms → \"{}\"", inference_ms, text);
                Ok(text)
            }
            SttResponse::Error { message, .. } => Err(SttError::Remote(message)),
            other => Err(SttError::Unexpected(format!("{other:?}"))),
        }
    }
}
