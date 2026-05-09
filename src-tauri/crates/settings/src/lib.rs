use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use svoice_hotkey::HotKey;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub mic_device: Option<String>,

    /// Om false: höger Ctrl-PTT triggar inte STT. Sparar VRAM — sidecar
    /// spawnar aldrig om user bara vill använda action-LLM.
    pub stt_enabled: bool,
    pub stt_model: String,
    pub stt_compute_mode: ComputeMode,
    pub vad_threshold: f32,

    /// Padding (ms) som läggs till på båda sidor av RMS-baserad trim_silence
    /// innan ljudet skickas till STT. Utan padding klipps tonlösa konsonanter
    /// (s, f, t, k) eftersom deras RMS ligger under tröskeln. Default 250 ms.
    #[serde(default = "default_vad_trim_padding_ms")]
    pub vad_trim_padding_ms: u32,

    /// Om > 0: när en ny diktering injiceras inom detta antal sekunder efter
    /// föregående, prepend:as ett mellanslag så att meningarna inte klistras
    /// ihop utan mellanrum. 0 = av. Default 30 s.
    #[serde(default = "default_dictation_auto_space_seconds")]
    pub dictation_auto_space_seconds: u32,

    /// Beam search-storlek för faster-whisper (1 = greedy, snabbt; 5 = bra
    /// balans; 10 = diminishing returns). Större värde → längre inferens.
    #[serde(default = "default_beam_size")]
    pub stt_beam_size: u32,

    /// Aktivera faster-whispers inbyggda Silero-VAD. Filtrerar tystnader och
    /// icke-tal inuti audio-chunken innan transkribering → mindre
    /// hallucinering i pauser, robustare mot bakgrundsljud.
    #[serde(default = "default_true")]
    pub stt_vad_filter: bool,

    /// "Priming"-text som matas in som historisk kontext till Whisper.
    /// Stabiliserar interpunktion/stil och kan förbättra fackord-igenkänning
    /// (t.ex. medicinska termer). Tom sträng = inget prompt.
    #[serde(default = "default_initial_prompt")]
    pub stt_initial_prompt: String,

    /// Tröskel för no_speech-probability (0.0-1.0). Segment med no_speech >=
    /// threshold filtreras bort. Standard i whisper är 0.6; lägre = mer
    /// tolerant (fångar tyst tal) men risk för hallucinering från brus.
    #[serde(default = "default_no_speech_threshold")]
    pub stt_no_speech_threshold: f32,

    /// Om true: feedar tidigare transkriberad text tillbaka till modellen
    /// som kontext. Förbättrar koherens i lång flytande diktering men kan
    /// orsaka trunkering vid naturliga pauser. Default av.
    #[serde(default)]
    pub stt_condition_on_previous_text: bool,

    /// Om false: Insert-PTT triggar inte action-popup. Sparar resurser om
    /// user bara vill ha ren diktering utan LLM alls.
    pub action_llm_enabled: bool,

    /// Om true: efter STT-transkribering skickas texten till LLM för
    /// grammatik/stavning-polering INNAN inject. Långsammare men vassare.
    pub llm_polish_dictation: bool,

    /// LLM-provider för **action-popup** (Insert-PTT → svar i popup).
    /// `#[serde(alias = "llm_provider")]` migrerar tysta gamla settings.json där
    /// bara ett fält `llm_provider` fanns — det antas ha varit user:s
    /// action-provider (eftersom den primära use-casen var action-popup).
    #[serde(default, alias = "llm_provider")]
    pub action_llm_provider: LlmProvider,

    /// LLM-provider för **dikterings-polering** (RightCtrl-PTT → LLM-fixar
    /// grammatik/interpunktion innan inject). Separerad från action-LLM så
    /// user kan t.ex. köra snabb+billig Groq för diktering och Claude för
    /// kraftfull action. Auto = lokal Ollama först, Anthropic fallback.
    #[serde(default)]
    pub dictation_llm_provider: LlmProvider,

    /// Anthropic-modell. Default claude-sonnet-4-5.
    pub anthropic_model: String,

    /// Ollama-modell. Default qwen2.5:14b (stark svensk-förmåga, passar RTX 5080).
    /// Kräver `ollama pull <modell>` innan första användning.
    pub ollama_model: String,
    /// Ollama-server URL. Default http://127.0.0.1:11434.
    pub ollama_url: String,

    /// Groq LLM-modell. Default llama-3.3-70b-versatile (gratis-tier, snabb).
    pub groq_llm_model: String,
    /// Groq STT-modell. Default whisper-large-v3-turbo.
    pub groq_stt_model: String,

    /// Gemini-modell. Default `gemini-2.5-flash` (billig, snabb, stödjer
    /// Google Search-grounding). Alternativ: `gemini-2.5-pro` (dyrare,
    /// smartare). Används när action_llm_provider eller dictation_llm_provider
    /// är `Gemini`.
    pub gemini_model: String,

    /// STT-provider: "local" = KB-Whisper via Python-sidecar, "groq" = Groq API.
    pub stt_provider: SttProvider,
    /// ISO-språkkod för STT, t.ex. "sv", "en", "auto".
    pub stt_language: String,

    /// Hotkey för diktering (hold-to-talk). Standard: höger Ctrl.
    pub dictation_hotkey: HotKey,
    /// Hotkey för action-popup. Standard: Insert.
    pub action_hotkey: HotKey,
    /// Hotkey för skärmklipp till AI. Standard: Scroll Lock.
    #[serde(default = "default_screen_hotkey")]
    pub screen_hotkey: HotKey,

    /// Om true: kommandon som "läs registreringsnumret" triggar ett strikt
    /// textläge för skärmklipp. Om false behandlas alla klipp som vanliga
    /// bildfrågor.
    #[serde(default = "default_true")]
    pub screen_clip_auto_text_mode: bool,

    /// Om true: textläget skickar en gråskale/kontrastad kopia av klippet till
    /// vision-modellen. Originalbilden sparas fortfarande bara i RAM.
    #[serde(default = "default_true")]
    pub screen_clip_ocr_enhancement: bool,

    /// Google OAuth client-ID (från Google Cloud Console → "Desktop app").
    /// Om None: Google-integration disabled.
    pub google_oauth_client_id: Option<String>,

    /// Google OAuth client-secret. Google kräver att desktop-apps skickar
    /// secret i token-exchange trots PKCE. Det är INTE hemligt i native
    /// apps — kan extraheras från binären. Kopieras från samma
    /// OAuth-client i Google Cloud som client_id.
    pub google_oauth_client_secret: Option<String>,

    /// Om true: appen läggs i Windows startup-registret så den startar
    /// automatiskt vid inloggning, tyst i tray (main-fönstret är dolt
    /// by default så ingen UI flashar upp). Appliceras via
    /// `tauri-plugin-autostart` → HKCU\...\Run. Idempotent: vid app-start
    /// synkas registret mot detta värde.
    pub autostart: bool,
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
    /// Använd alltid Groq (gratis-tier, snabb).
    Groq,
    /// Använd alltid Google Gemini (billig + inbyggd Google Search-grounding).
    Gemini,
}

impl Default for LlmProvider {
    fn default() -> Self {
        // Auto = lokalt först, fallback cloud. Matchar privacy-first-default.
        LlmProvider::Auto
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SttProvider {
    /// Lokal KB-Whisper via Python-sidecar. Svenska-optimerad.
    Local,
    /// Groq Whisper-API (kräver internet + API-nyckel, ~100x snabbare).
    Groq,
}

fn default_beam_size() -> u32 {
    5
}

fn default_vad_trim_padding_ms() -> u32 {
    250
}

fn default_dictation_auto_space_seconds() -> u32 {
    30
}

fn default_true() -> bool {
    true
}

fn default_initial_prompt() -> String {
    "Svensk diktering. Korrekt interpunktion och stor bokstav.".into()
}

fn default_no_speech_threshold() -> f32 {
    0.5
}

fn default_screen_hotkey() -> HotKey {
    HotKey::ScrollLock
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mic_device: None,
            stt_enabled: true,
            stt_model: "KBLab/kb-whisper-base".into(),
            stt_compute_mode: ComputeMode::Auto,
            vad_threshold: 0.005,
            vad_trim_padding_ms: default_vad_trim_padding_ms(),
            dictation_auto_space_seconds: default_dictation_auto_space_seconds(),
            stt_beam_size: default_beam_size(),
            stt_vad_filter: default_true(),
            stt_initial_prompt: default_initial_prompt(),
            stt_no_speech_threshold: default_no_speech_threshold(),
            stt_condition_on_previous_text: false,
            action_llm_enabled: true,
            llm_polish_dictation: false,
            action_llm_provider: LlmProvider::Auto,
            dictation_llm_provider: LlmProvider::Auto,
            anthropic_model: "claude-sonnet-4-5".into(),
            ollama_model: "qwen2.5:14b".into(),
            ollama_url: "http://127.0.0.1:11434".into(),
            groq_llm_model: "llama-3.3-70b-versatile".into(),
            groq_stt_model: "whisper-large-v3-turbo".into(),
            gemini_model: "gemini-2.5-flash".into(),
            stt_provider: SttProvider::Local,
            stt_language: "sv".into(),
            dictation_hotkey: HotKey::RightCtrl,
            action_hotkey: HotKey::Insert,
            screen_hotkey: default_screen_hotkey(),
            screen_clip_auto_text_mode: true,
            screen_clip_ocr_enhancement: true,
            google_oauth_client_id: None,
            google_oauth_client_secret: None,
            autostart: false,
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
        PathBuf::from(appdata)
            .join("svoice-v3")
            .join("settings.json")
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
        assert_eq!(s.stt_model, "KBLab/kb-whisper-base");
        assert_eq!(s.stt_compute_mode, ComputeMode::Auto);
        assert!((s.vad_threshold - 0.005).abs() < 1e-6);
        assert_eq!(s.vad_trim_padding_ms, 250);
        assert_eq!(s.dictation_auto_space_seconds, 30);
        assert_eq!(s.action_llm_provider, LlmProvider::Auto);
        assert_eq!(s.dictation_llm_provider, LlmProvider::Auto);
        assert_eq!(s.ollama_model, "qwen2.5:14b");
        assert_eq!(s.dictation_hotkey, HotKey::RightCtrl);
        assert_eq!(s.action_hotkey, HotKey::Insert);
        assert_eq!(s.screen_hotkey, HotKey::ScrollLock);
        assert!(s.screen_clip_auto_text_mode);
        assert!(s.screen_clip_ocr_enhancement);
        assert_eq!(s.stt_provider, SttProvider::Local);
        assert_eq!(s.stt_language, "sv");
        assert_eq!(s.groq_llm_model, "llama-3.3-70b-versatile");
        assert_eq!(s.groq_stt_model, "whisper-large-v3-turbo");
        assert_eq!(s.gemini_model, "gemini-2.5-flash");
    }

    #[test]
    fn roundtrip_via_json() {
        let original = Settings {
            mic_device: Some("Yeti Classic".into()),
            stt_enabled: true,
            stt_model: "kb-whisper-large".into(),
            stt_compute_mode: ComputeMode::Gpu,
            vad_threshold: 0.01,
            vad_trim_padding_ms: 400,
            dictation_auto_space_seconds: 60,
            stt_beam_size: 7,
            stt_vad_filter: false,
            stt_initial_prompt: "Medicinsk journalanteckning.".into(),
            stt_no_speech_threshold: 0.7,
            stt_condition_on_previous_text: true,
            action_llm_enabled: true,
            llm_polish_dictation: true,
            action_llm_provider: LlmProvider::Gemini,
            dictation_llm_provider: LlmProvider::Groq,
            anthropic_model: "claude-opus-4-7".into(),
            ollama_model: "qwen2.5:32b".into(),
            ollama_url: "http://127.0.0.1:11434".into(),
            groq_llm_model: "llama-3.3-70b-versatile".into(),
            groq_stt_model: "whisper-large-v3-turbo".into(),
            gemini_model: "gemini-2.5-pro".into(),
            stt_provider: SttProvider::Groq,
            stt_language: "en".into(),
            dictation_hotkey: HotKey::F12,
            action_hotkey: HotKey::Pause,
            screen_hotkey: HotKey::ScrollLock,
            screen_clip_auto_text_mode: false,
            screen_clip_ocr_enhancement: false,
            google_oauth_client_id: Some("1234.apps.googleusercontent.com".into()),
            google_oauth_client_secret: Some("GOCSPX-abc123".into()),
            autostart: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(original.mic_device, restored.mic_device);
        assert_eq!(original.stt_model, restored.stt_model);
        assert_eq!(original.stt_compute_mode, restored.stt_compute_mode);
        assert_eq!(original.anthropic_model, restored.anthropic_model);
        assert_eq!(original.action_llm_provider, restored.action_llm_provider);
        assert_eq!(
            original.dictation_llm_provider,
            restored.dictation_llm_provider
        );
        assert_eq!(original.ollama_model, restored.ollama_model);
        assert_eq!(original.gemini_model, restored.gemini_model);
        assert_eq!(original.dictation_hotkey, restored.dictation_hotkey);
        assert_eq!(original.action_hotkey, restored.action_hotkey);
        assert_eq!(original.screen_hotkey, restored.screen_hotkey);
        assert_eq!(
            original.screen_clip_auto_text_mode,
            restored.screen_clip_auto_text_mode
        );
        assert_eq!(
            original.screen_clip_ocr_enhancement,
            restored.screen_clip_ocr_enhancement
        );
        assert_eq!(
            original.google_oauth_client_id,
            restored.google_oauth_client_id
        );
    }

    #[test]
    fn compute_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ComputeMode::Auto).unwrap(),
            "\"auto\""
        );
        assert_eq!(serde_json::to_string(&ComputeMode::Cpu).unwrap(), "\"cpu\"");
        assert_eq!(serde_json::to_string(&ComputeMode::Gpu).unwrap(), "\"gpu\"");
    }
}
