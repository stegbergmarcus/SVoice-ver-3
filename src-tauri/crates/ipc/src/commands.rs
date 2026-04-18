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
/// modell-byte träder i kraft utan app-restart. Hotkey-ändringar re-bindas
/// live via svoice_hotkey::rebind_role — ingen restart behövs.
#[tauri::command]
pub async fn set_settings(
    settings: Settings,
    stt: State<'_, Arc<PythonStt>>,
) -> Result<(), String> {
    // Läs gammal version för att detektera hotkey-ändringar.
    let old = Settings::load();

    // Spara först så hot-reload-loop:en i workers ser ny disk-version.
    settings
        .save()
        .map_err(|e| format!("kunde inte spara settings: {e}"))?;

    // Hot-reload hotkeys om de ändrats. Fallback till default om user råkat
    // sätta båda samma (validering sker också i setup, men vi skyddar här).
    let (d_new, a_new) = if settings.dictation_hotkey == settings.action_hotkey {
        tracing::warn!("dictation_hotkey == action_hotkey — faller tillbaka till default");
        (
            svoice_hotkey::HotKey::RightCtrl,
            svoice_hotkey::HotKey::Insert,
        )
    } else {
        (settings.dictation_hotkey, settings.action_hotkey)
    };
    if let Err(e) = svoice_hotkey::rebind_role("dictation", old.dictation_hotkey, d_new) {
        tracing::error!("kunde inte rebinda dictation-hotkey: {e}");
    }
    if let Err(e) = svoice_hotkey::rebind_role("action", old.action_hotkey, a_new) {
        tracing::error!("kunde inte rebinda action-hotkey: {e}");
    }

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
    let _ = app.emit("ollama_pull_done", serde_json::json!({ "model": model }));

    // OS-notifikation — user kan ha stängt Settings-fönstret.
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app
        .notification()
        .builder()
        .title("SVoice")
        .body(format!("Modell nedladdad: {model}"))
        .show()
    {
        tracing::warn!("kunde inte visa notifikation: {e}");
    }

    // TODO(iter 5): STT download notification — HF-Whisper laddar ned modeller
    // transparently i Python-sidecaret vid första transcribe mot ocachad modell.
    // Kräver IPC-signal från sidecar när caching är klar.

    Ok(())
}

/// Indikera till frontend om en Anthropic-nyckel ligger i Windows Credential
/// Manager. Används för att visa `••••••••` vs tom input i Settings-UI.
#[tauri::command]
pub fn has_anthropic_key() -> bool {
    svoice_secrets::has_anthropic_key()
}

/// Spara Anthropic API-nyckel i Windows Credential Manager. Ersätter ev.
/// befintligt värde. Tom sträng betraktas som fel (anropa `clear` istället).
#[tauri::command]
pub fn set_anthropic_key(key: String) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("nyckel får inte vara tom — använd clear-kommandot istället".into());
    }
    svoice_secrets::set_anthropic_key(trimmed).map_err(|e| format!("kunde inte spara nyckel: {e}"))
}

/// Radera Anthropic API-nyckeln ur Credential Manager. No-op om den saknas.
#[tauri::command]
pub fn clear_anthropic_key() -> Result<(), String> {
    svoice_secrets::delete_anthropic_key().map_err(|e| format!("kunde inte radera nyckel: {e}"))
}

// ───────── Google OAuth ─────────

#[derive(Debug, Serialize)]
pub struct GoogleStatus {
    pub connected: bool,
    pub client_id_configured: bool,
}

/// Returnera status för Google-integration. Frontend använder detta för att
/// rendera "Anslut"- vs "Frånkoppla"-knappar + hint om client-id-konfig.
#[tauri::command]
pub fn google_connection_status() -> GoogleStatus {
    let settings = Settings::load();
    GoogleStatus {
        connected: svoice_integrations::google::oauth::is_connected(),
        client_id_configured: settings
            .google_oauth_client_id
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
    }
}

/// Starta OAuth-flowet. Öppnar browsern och väntar på callback.
/// Returnerar när user har godkänt OCH refresh-token är sparad i keyring.
/// Timeout 5 min (om user inte klickar igenom returneras fel).
#[tauri::command]
pub async fn google_connect(app: AppHandle) -> Result<(), String> {
    use svoice_integrations::google::oauth::{GoogleOAuthFlow, GoogleScope};
    use tauri_plugin_opener::OpenerExt;

    let settings = Settings::load();
    let client_id = settings
        .google_oauth_client_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "Google OAuth client-ID saknas — konfigurera i Settings först".to_string()
        })?;

    // Scopes: börja med Calendar + Gmail read-only. Full CRUD kommer senare.
    let scopes = &[
        GoogleScope::CalendarReadonly,
        GoogleScope::CalendarEvents,
        GoogleScope::GmailReadonly,
    ];

    let flow = GoogleOAuthFlow::start(client_id, scopes)
        .await
        .map_err(|e| format!("kunde inte starta OAuth: {e}"))?;

    // Öppna browsern. tauri-plugin-opener är cross-platform wrapper.
    app.opener()
        .open_url(&flow.auth_url, None::<&str>)
        .map_err(|e| format!("kunde inte öppna browser: {e}"))?;

    tracing::info!(
        "OAuth-flow startad; väntar på callback på port {}",
        flow.port
    );

    let tokens = flow
        .finalize()
        .await
        .map_err(|e| format!("OAuth misslyckades: {e}"))?;

    let refresh = tokens
        .refresh_token
        .ok_or_else(|| "Google returnerade ingen refresh-token".to_string())?;
    svoice_secrets::set_google_refresh_token(&refresh)
        .map_err(|e| format!("kunde inte spara refresh-token: {e}"))?;

    tracing::info!("Google-integration ansluten");
    Ok(())
}

/// Koppla från Google genom att radera refresh-token ur keyring.
#[tauri::command]
pub fn google_disconnect() -> Result<(), String> {
    svoice_integrations::google::oauth::disconnect()
        .map_err(|e| format!("kunde inte koppla från: {e}"))
}

// ───────── Smart functions ─────────

/// Returnera alla användar-definierade smart-functions från
/// `%APPDATA%/svoice-v3/smart_functions/`. Ogiltiga JSON-filer skippas.
#[tauri::command]
pub fn list_smart_functions() -> Vec<svoice_smart_functions::SmartFunction> {
    svoice_smart_functions::list().unwrap_or_default()
}

/// Öppna smart-functions-mappen i Explorer så user kan redigera JSON-filer.
#[tauri::command]
pub fn open_smart_functions_dir(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = svoice_smart_functions::default_dir();
    // Säkerställ att mappen finns innan vi försöker öppna.
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("kunde inte skapa mapp: {e}"))?;
    }
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("kunde inte öppna mappen: {e}"))
}
