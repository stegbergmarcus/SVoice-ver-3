//! Hämtar kb-whisper-medium från Hugging Face i CT2-format.
//!
//! Vi försöker kandidater i ordning:
//! 1. En färdigkonverterad kb-whisper (om communityn publicerat en)
//! 2. Systran/faster-whisper-medium (flerspråkig baseline — engelsk-tonad men
//!    fungerar som "proof of pipeline")
//!
//! Modellfiler cachas i `%APPDATA%/svoice-v3/models/`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use hf_hub::api::tokio::{ApiBuilder, ApiRepo};

const CANDIDATES: &[&str] = &[
    "KBLab/kb-whisper-medium-ctranslate2",
    "Systran/faster-whisper-medium",
];

/// Nedladdningsfiler som CTranslate2 behöver. tokenizer.json finns vanligtvis
/// för OpenAI Whisper-kompatibla modeller; om vocabulary.json saknas får det gå.
const REQUIRED_FILES: &[&str] = &["config.json", "model.bin", "tokenizer.json"];
const OPTIONAL_FILES: &[&str] = &["vocabulary.json", "preprocessor_config.json"];

pub async fn download_kb_whisper_medium() -> Result<PathBuf> {
    let api = ApiBuilder::new()
        .with_cache_dir(cache_dir())
        .build()
        .context("kunde inte bygga hf-hub API")?;

    for repo_id in CANDIDATES {
        tracing::info!("försöker ladda ner {}", repo_id);
        let repo = api.model((*repo_id).to_string());
        match fetch_snapshot(&repo).await {
            Ok(path) => {
                tracing::info!("modell nedladdad: {}", path.display());
                return Ok(path);
            }
            Err(e) => tracing::warn!("kunde inte ladda {}: {}", repo_id, e),
        }
    }

    anyhow::bail!("ingen kandidat-modell kunde laddas från Hugging Face")
}

async fn fetch_snapshot(repo: &ApiRepo) -> Result<PathBuf> {
    let mut dir: Option<PathBuf> = None;
    for f in REQUIRED_FILES {
        let path = repo
            .get(f)
            .await
            .with_context(|| format!("misslyckades hämta {f}"))?;
        if dir.is_none() {
            dir = path.parent().map(|p| p.to_path_buf());
        }
    }
    for f in OPTIONAL_FILES {
        let _ = repo.get(f).await; // tyst ignorering
    }
    dir.context("kunde inte härleda modellens katalog")
}

fn cache_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").expect("APPDATA inte satt");
    PathBuf::from(appdata).join("svoice-v3").join("models")
}
