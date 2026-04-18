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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SttConfig {
    pub model: String,
    pub device: String,
    pub compute_type: String,
    pub language: String,
    pub beam_size: u32,
    pub python_path: PathBuf,
    pub python_args: Vec<String>,
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
            python_args: vec!["-3.11".into()],
            script_path: PathBuf::from("resources/python/stt_sidecar.py"),
        }
    }
}

pub struct PythonStt {
    sidecar: Arc<Mutex<Option<Sidecar>>>,
    /// Config i Mutex så den kan hot-reloadas när user byter modell/compute
    /// i Settings — reload_config() drop:ar sidecar och sätter ny config;
    /// nästa transcribe triggar ensure_loaded som spawnar sidecar igen.
    config: Arc<Mutex<SttConfig>>,
}

impl PythonStt {
    pub fn new(config: SttConfig) -> Self {
        Self {
            sidecar: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(config)),
        }
    }

    /// Byt SttConfig vid runtime. Om sidecar är spawnad shutdown:as den
    /// graceful; nästa transcribe spawnar ny sidecar med ny config.
    ///
    /// Om `new_config` är identisk med befintlig config blir det en no-op.
    ///
    /// **Viktigt:** `python_path`, `python_args` och `script_path` *bevaras*
    /// från befintlig config. De är deployment-detaljer som bestäms av
    /// main-setupen (bundlad runtime i installerad MSI vs `py` i dev) och
    /// ska inte skrivas över av IPC-callers som byggt en `SttConfig::default()`.
    /// Innan denna merge skrevs bundled-paths över vid varje settings-save →
    /// reload → fail vid nästa spawn ("can't open file .../resources/python/...").
    pub async fn reload_config(&self, new_config: SttConfig) -> Result<bool, SttError> {
        let mut current_cfg = self.config.lock().await;
        let merged = SttConfig {
            python_path: current_cfg.python_path.clone(),
            python_args: current_cfg.python_args.clone(),
            script_path: current_cfg.script_path.clone(),
            ..new_config
        };
        if *current_cfg == merged {
            return Ok(false); // ingen relevant ändring
        }
        tracing::info!(
            "STT reload: model {} → {}, device {} → {}",
            current_cfg.model,
            merged.model,
            current_cfg.device,
            merged.device
        );
        // Shutdown nuvarande sidecar om den finns.
        let mut sc_guard = self.sidecar.lock().await;
        if let Some(sc) = sc_guard.take() {
            // sc.shutdown konsumerar sc — best effort.
            let _ = sc.shutdown().await;
        }
        *current_cfg = merged;
        Ok(true)
    }

    async fn ensure_loaded(&self) -> Result<(), SttError> {
        let mut guard = self.sidecar.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let cfg = self.config.lock().await.clone();
        let sc = Sidecar::spawn(&cfg.python_path, &cfg.python_args, &cfg.script_path).await?;
        sc.send_request(&SttRequest::Load {
            model: cfg.model.clone(),
            device: cfg.device.clone(),
            compute_type: cfg.compute_type.clone(),
            language: cfg.language.clone(),
        })
        .await?;
        match sc.read_response().await? {
            SttResponse::Loaded {
                load_ms,
                vram_used_mb,
            } => {
                tracing::info!(
                    "STT-modell laddad på {} ms (VRAM: {:?} MB)",
                    load_ms,
                    vram_used_mb
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
        let beam_size = self.config.lock().await.beam_size;
        let guard = self.sidecar.lock().await;
        let sc = guard.as_ref().ok_or(SttError::NotLoaded)?;
        sc.send_request(&SttRequest::Transcribe {
            audio_samples: audio.len() as u32,
            sample_rate: 16000,
            beam_size,
        })
        .await?;
        sc.send_audio(audio).await?;
        match sc.read_response().await? {
            SttResponse::Transcript {
                text, inference_ms, ..
            } => {
                tracing::info!("STT: {} ms → \"{}\"", inference_ms, text);
                Ok(text)
            }
            SttResponse::Error { message, .. } => Err(SttError::Remote(message)),
            other => Err(SttError::Unexpected(format!("{other:?}"))),
        }
    }
}
