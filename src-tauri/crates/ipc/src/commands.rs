use serde::Serialize;
use std::sync::Arc;
use svoice_audio::list_input_devices;
use svoice_hotkey::PttState;
use svoice_inject::paste_and_restore;
use svoice_llm::{OllamaClient, OllamaModelInfo};
use svoice_settings::{ComputeMode, Settings};
use svoice_stt::{PythonStt, SttConfig};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InjectResult {
    pub method: String,
    pub chars: usize,
}

#[derive(Debug, Serialize)]
pub struct PttStateReport {
    pub state: PttState,
}

/// Hämta nuvarande settings från disk. Returnerar default om filen saknas
/// eller är korrupt (log-warning i så fall).
#[tauri::command]
pub fn get_settings() -> Settings {
    Settings::load()
}

/// Skriv settings till disk. Om STT-fält ändrats triggas hot-reload så
/// modell-byte träder i kraft utan app-restart.
#[tauri::command]
pub async fn set_settings(
    settings: Settings,
    stt: State<'_, Arc<PythonStt>>,
) -> Result<(), String> {
    // Spara först så hot-reload-loop:en i workers ser ny disk-version.
    settings
        .save()
        .map_err(|e| format!("kunde inte spara settings: {e}"))?;

    // Bygg SttConfig från ny settings och be PythonStt reload om det ändrats.
    // reload_config returnerar false om config är identisk med befintlig =>
    // inget onödigt shutdown/respawn.
    let mut stt_config = SttConfig::default();
    stt_config.model = settings.stt_model.clone();
    match settings.stt_compute_mode {
        ComputeMode::Cpu => {
            stt_config.device = "cpu".into();
            stt_config.compute_type = "int8".into();
        }
        ComputeMode::Gpu => {
            stt_config.device = "cuda".into();
            stt_config.compute_type = "float16".into();
        }
        ComputeMode::Auto => {}
    }
    // Path-resolution: preservera ev. bundlad runtime-path om den redan är satt.
    // set_settings kan inte komma åt AppHandle::resource_dir lätt här, så vi
    // lämnar default paths — main lib.rs setup sätter paths vid app-start
    // och reload_config jämför bara relevanta fields (model, device, compute).
    // Om user byter modell är det vad som matters.
    if let Err(e) = stt.reload_config(stt_config).await {
        tracing::warn!("stt reload misslyckades: {e}");
        // Rapportera inte som fel — settings är sparade, reload är best-effort.
    }
    Ok(())
}

/// Applicera action-popup LLM-resultatet.
///
/// Ordning är kritisk:
/// 1. Dölj popup-fönstret (via backend, inte frontend — mer pålitligt).
/// 2. Vänta på focus-retur + SetForegroundWindow(target_hwnd).
/// 3. Kör paste_and_restore på blocking-thread.
///
/// Utan backend-kontrollerad sekvensering kan popup-webviewen hänga kvar
/// efter frontend hide() och "äta" Ctrl+V-eventet från paste.
#[tauri::command]
pub async fn action_apply(app: tauri::AppHandle, result: String) -> Result<(), String> {
    use tauri::Manager;

    // 1. Göm popup-fönstret omedelbart från backend.
    if let Some(win) = app.get_webview_window("action-popup") {
        let _ = win.hide();
        tracing::debug!("action_apply: popup hide:ad");
    }

    // 2. Låt Windows processa focus-bytet.
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    // 3. Paste på blocking-thread (Win32-calls är synchrona).
    tokio::task::spawn_blocking(move || paste_and_restore(&result))
        .await
        .map_err(|e| format!("join error: {e}"))?
        .map_err(|e| format!("paste failed: {e}"))?;
    tracing::info!("action-popup: result applied via clipboard");
    Ok(())
}

/// Användaren avbröt action-popupen utan att applicera resultatet.
/// Göm popup-fönstret oavsett frontend-state (säkerhetsnät).
#[tauri::command]
pub fn action_cancel(app: tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("action-popup") {
        let _ = win.hide();
    }
    tracing::debug!("action-popup: cancelled by user");
}

/// Lista alla tillgängliga mic-enheter (default-enheten listas först).
/// Används av Settings-UI:ets mikrofon-dropdown.
#[tauri::command]
pub fn list_mic_devices() -> Vec<String> {
    list_input_devices()
}

/// Kolla om en HuggingFace-modell finns i lokal cache. Cache-path är
/// ~/.cache/huggingface/hub/models--<org>--<name>/snapshots/. Om den
/// finns + har innehåll: modellen är nedladdad och första-transcribe
/// blir snabbt (ingen 1-3 min väntan).
#[tauri::command]
pub fn check_hf_cached(model: String) -> bool {
    // Tauri skickar "KBLab/kb-whisper-large" -> "models--KBLab--kb-whisper-large"
    let slug = format!("models--{}", model.replace('/', "--"));
    let home = match std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let cache_path = std::path::PathBuf::from(home)
        .join(".cache")
        .join("huggingface")
        .join("hub")
        .join(&slug)
        .join("snapshots");
    if !cache_path.exists() {
        return false;
    }
    // Kolla att någon snapshot finns och att den inte är tom.
    std::fs::read_dir(&cache_path)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && std::fs::read_dir(e.path())
                        .map(|mut d| d.next().is_some())
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Lista modeller installerade i lokalt Ollama-service. Används för att
/// visa ✓/↓-status i Settings-UI och avgöra om download behövs.
#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<OllamaModelInfo>, String> {
    let settings = Settings::load();
    let client = OllamaClient::new(String::new()).with_base_url(settings.ollama_url);
    client
        .list_models()
        .await
        .map_err(|e| format!("kunde inte lista Ollama-modeller: {e}"))
}

/// Starta pull av Ollama-modell. Emittar `ollama_pull_progress`-events
/// för varje NDJSON-rad från /api/pull. Returnerar när pull är klar (eller fel).
#[tauri::command]
pub async fn pull_ollama_model(app: AppHandle, model: String) -> Result<(), String> {
    let settings = Settings::load();
    let client = OllamaClient::new(String::new()).with_base_url(settings.ollama_url);
    let app_for_cb = app.clone();
    client
        .pull_model(&model, move |progress| {
            let _ = app_for_cb.emit("ollama_pull_progress", progress);
        })
        .await
        .map_err(|e| format!("ollama pull failed: {e}"))?;
    let _ = app.emit(
        "ollama_pull_done",
        serde_json::json!({ "model": model }),
    );
    Ok(())
}
