use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub mic_device: Option<String>,
    pub stt_model: String,
    pub stt_compute_mode: ComputeMode,
    pub vad_threshold: f32,

    /// Vilken LLM-provider som action-popup ska använda.
    /// Auto = försök lokal först, fallback till API om inte tillgänglig.
    pub llm_provider: LlmProvider,

    /// Anthropic Claude API-nyckel för action-popup (klartext i JSON — keyring iter 4+).
    pub anthropic_api_key: Option<String>,
    /// Anthropic-modell. Default claude-sonnet-4-5.
    pub anthropic_model: String,

    /// Ollama-modell. Default qwen2.5:14b (stark svensk-förmåga, passar RTX 5080).
    /// Kräver `ollama pull <modell>` innan första användning.
    pub ollama_model: String,
    /// Ollama-server URL. Default http://127.0.0.1:11434.
    pub ollama_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    /// Försök Ollama först (lokalt), fallback till Anthropic om otillgänglig.
    Auto,
    /// Använd alltid Anthropic Claude (cloud API).
    Claude,
    /// Använd alltid lokal Ollama.
    Ollama,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mic_device: None,
            stt_model: "KBLab/kb-whisper-large".into(),
            stt_compute_mode: ComputeMode::Auto,
            vad_threshold: 0.005,
            llm_provider: LlmProvider::Auto,
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-5".into(),
            ollama_model: "qwen2.5:14b".into(),
            ollama_url: "http://127.0.0.1:11434".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComputeMode {
    Auto,
    Cpu,
    Gpu,
}

impl Settings {
    pub fn path() -> PathBuf {
        let appdata = std::env::var("APPDATA").expect("APPDATA");
        PathBuf::from(appdata).join("svoice-v3").join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values_match_spec() {
        let s = Settings::default();
        assert!(s.mic_device.is_none());
        assert_eq!(s.stt_model, "KBLab/kb-whisper-large");
        assert_eq!(s.stt_compute_mode, ComputeMode::Auto);
        assert!((s.vad_threshold - 0.005).abs() < 1e-6);
        assert_eq!(s.llm_provider, LlmProvider::Auto);
        assert_eq!(s.ollama_model, "qwen2.5:14b");
    }

    #[test]
    fn roundtrip_via_json() {
        let original = Settings {
            mic_device: Some("Yeti Classic".into()),
            stt_model: "kb-whisper-large".into(),
            stt_compute_mode: ComputeMode::Gpu,
            vad_threshold: 0.01,
            llm_provider: LlmProvider::Ollama,
            anthropic_api_key: Some("sk-ant-test".into()),
            anthropic_model: "claude-opus-4-7".into(),
            ollama_model: "qwen2.5:32b".into(),
            ollama_url: "http://127.0.0.1:11434".into(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(original.mic_device, restored.mic_device);
        assert_eq!(original.stt_model, restored.stt_model);
        assert_eq!(original.stt_compute_mode, restored.stt_compute_mode);
        assert_eq!(original.anthropic_api_key, restored.anthropic_api_key);
        assert_eq!(original.anthropic_model, restored.anthropic_model);
        assert_eq!(original.llm_provider, restored.llm_provider);
        assert_eq!(original.ollama_model, restored.ollama_model);
    }

    #[test]
    fn compute_mode_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&ComputeMode::Auto).unwrap(), "\"auto\"");
        assert_eq!(serde_json::to_string(&ComputeMode::Cpu).unwrap(), "\"cpu\"");
        assert_eq!(serde_json::to_string(&ComputeMode::Gpu).unwrap(), "\"gpu\"");
    }
}
