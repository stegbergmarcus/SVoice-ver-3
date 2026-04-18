use serde::Serialize;
use std::sync::{Arc, Mutex};
use svoice_audio::list_input_devices;
use svoice_hotkey::PttState;
use svoice_inject::paste_and_restore;
use svoice_llm::{OllamaClient, OllamaModelInfo, Role, TurnContent};
use svoice_settings::{ComputeMode, Settings};
use svoice_stt::{PythonStt, SttConfig};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

/// State för pågående action-popup-konversation. Används för att hålla ihop
/// follow-up-turns när user håller Insert igen medan popupen fortfarande är
/// synlig. Rensas när popupen stängs (action_apply / action_cancel) — så
/// varje ny popup-session börjar med tom context. `Mutex::new(None)` är
/// const sedan Rust 1.63 så vi kan använda static utan OnceCell.
#[derive(Debug, Clone)]
pub struct ActiveConversation {
    pub system: Option<String>,
    pub selection: Option<String>,
    pub turns: Vec<TurnContent>,
    pub mode: &'static str,
}

pub static ACTIVE_CONVERSATION: Mutex<Option<ActiveConversation>> = Mutex::new(None);

/// Hjälp-funktion så lib.rs och IPC-handlers kan rensa state på samma sätt.
pub fn clear_active_conversation() {
    if let Ok(mut guard) = ACTIVE_CONVERSATION.lock() {
        *guard = None;
    }
}

/// Lägger till en user-turn i den aktiva konversationen. Används av follow-up-
/// flödet. Om ingen aktiv konversation finns är detta en no-op och false
/// returneras — caller ska då bygga en ny konversation från scratch.
pub fn append_user_turn(text: String) -> bool {
    if let Ok(mut guard) = ACTIVE_CONVERSATION.lock() {
        if let Some(conv) = guard.as_mut() {
            conv.turns.push(TurnContent {
                role: Role::User,
                text,
            });
            return true;
        }
    }
    false
}

/// Lägger till en assistant-turn efter stream är klar så nästa follow-up
/// ser det fullständiga svaret i history.
pub fn append_assistant_turn(text: String) {
    if let Ok(mut guard) = ACTIVE_CONVERSATION.lock() {
        if let Some(conv) = guard.as_mut() {
            conv.turns.push(TurnContent {
                role: Role::Assistant,
                text,
            });
        }
    }
}

/// Ersätter active konversation med en ny (fresh session). Används vid första
/// turnen av en popup-session när vi just byggt system + selection + första
/// user-turn. Returnerar en clone av turns för stream-konsumenten.
pub fn set_active_conversation(conv: ActiveConversation) {
    if let Ok(mut guard) = ACTIVE_CONVERSATION.lock() {
        *guard = Some(conv);
    }
}

/// Läs turns + system för en follow-up-request utan att hålla Mutex över await.
pub fn snapshot_conversation() -> Option<(Option<String>, Vec<TurnContent>)> {
    let guard = ACTIVE_CONVERSATION.lock().ok()?;
    let conv = guard.as_ref()?;
    Some((conv.system.clone(), conv.turns.clone()))
}

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
    app: AppHandle,
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
    // Path-resolution: reload_config() bevarar python_path / python_args /
    // script_path från den befintliga configen så de defaults som sätts här
    // (dev-fallback) inte skriver över bundled-runtime-pathen som lib.rs
    // setupen har satt vid app-start. Settings-hot-reload rör bara
    // model / device / compute_type / language / beam_size.
    if let Err(e) = stt.reload_config(stt_config).await {
        tracing::warn!("stt reload misslyckades: {e}");
        // Rapportera inte som fel — settings är sparade, reload är best-effort.
    }

    // Synk autostart mot Windows startup-registret. Idempotent: bara skriv
    // om current state skiljer sig från önskad. Fel loggas men rapporteras
    // inte — settings är redan sparade på disk.
    if let Err(e) = sync_autostart(&app, settings.autostart) {
        tracing::warn!("autostart-sync misslyckades: {e}");
    }

    Ok(())
}

/// Synka Windows startup-registret mot `desired`. Idempotent och
/// exponerad på crate-nivå så lib.rs setup kan anropa den vid app-start
/// (om user flyttade binären etc. kan registret peka på fel path och
/// behöver skrivas om).
pub fn sync_autostart(app: &AppHandle, desired: bool) -> Result<(), String> {
    let mgr = app.autolaunch();
    let currently = mgr
        .is_enabled()
        .map_err(|e| format!("autolaunch is_enabled: {e}"))?;
    if currently == desired {
        return Ok(());
    }
    if desired {
        mgr.enable()
            .map_err(|e| format!("autolaunch enable: {e}"))?;
        tracing::info!("autostart aktiverad i Windows registret");
    } else {
        mgr.disable()
            .map_err(|e| format!("autolaunch disable: {e}"))?;
        tracing::info!("autostart inaktiverad i Windows registret");
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
    // 4. Rensa follow-up-state så nästa Insert-PTT börjar helt från noll.
    clear_active_conversation();
    ACTION_POPUP_STREAMING.store(false, std::sync::atomic::Ordering::SeqCst);
    tracing::info!("action-popup: result applied via clipboard, conversation cleared");
    Ok(())
}

/// Användaren avbröt action-popupen utan att applicera resultatet.
/// Göm popup-fönstret oavsett frontend-state (säkerhetsnät).
/// Rensar också konversations-state så nästa Insert-PTT börjar ny session.
#[tauri::command]
pub fn action_cancel(app: tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("action-popup") {
        let _ = win.hide();
    }
    clear_active_conversation();
    ACTION_POPUP_STREAMING.store(false, std::sync::atomic::Ordering::SeqCst);
    tracing::debug!("action-popup: cancelled by user, conversation cleared");
}

/// Starta follow-up PTT från popup-frontend (Space-nedtryckning i popup-
/// webview). När popup har fokus blockeras Insert-keyevent av
/// WebView2/system-hookar — frontend→IPC-vägen är mer pålitlig.
/// lib.rs läser en flagga och triggar action-worker Pressed-flow.
#[tauri::command]
pub fn action_followup_start() {
    FOLLOWUP_START_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
    tracing::debug!("action_followup_start IPC mottagen");
}

/// Släpp follow-up PTT (Space-release i popup).
#[tauri::command]
pub fn action_followup_stop() {
    FOLLOWUP_STOP_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
    tracing::debug!("action_followup_stop IPC mottagen");
}

/// Flaggor som lib.rs pollar i audio-owner-loopen. Enkelt mekanism för
/// att crate-cross-trigger action-worker utan att ta IPC på audio-thread.
pub static FOLLOWUP_START_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
pub static FOLLOWUP_STOP_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Sätts `true` från att första `action_llm_token` emittas tills 500 ms efter
/// `action_llm_done`. Under denna period skippas click-outside-hide så user
/// inte tappar ett pågående (eller nyss-levererat) svar genom att oavsiktligt
/// klicka på skrivbordet.
pub static ACTION_POPUP_STREAMING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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

// ───────── Groq API-nyckel ─────────

#[tauri::command]
pub fn has_groq_key() -> bool {
    svoice_secrets::has_groq_key()
}

#[tauri::command]
pub fn set_groq_key(key: String) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("nyckel får inte vara tom — använd clear istället".into());
    }
    svoice_secrets::set_groq_key(trimmed)
        .map_err(|e| format!("kunde inte spara Groq-nyckel: {e}"))
}

#[tauri::command]
pub fn clear_groq_key() -> Result<(), String> {
    svoice_secrets::delete_groq_key()
        .map_err(|e| format!("kunde inte radera Groq-nyckel: {e}"))
}

// ───────── Gemini API-nyckel ─────────

#[tauri::command]
pub fn has_gemini_key() -> bool {
    svoice_secrets::has_gemini_key()
}

#[tauri::command]
pub fn set_gemini_key(key: String) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("nyckel får inte vara tom — använd clear istället".into());
    }
    svoice_secrets::set_gemini_key(trimmed)
        .map_err(|e| format!("kunde inte spara Gemini-nyckel: {e}"))
}

#[tauri::command]
pub fn clear_gemini_key() -> Result<(), String> {
    svoice_secrets::delete_gemini_key()
        .map_err(|e| format!("kunde inte radera Gemini-nyckel: {e}"))
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
    let client_secret = settings
        .google_oauth_client_secret
        .as_deref()
        .filter(|s| !s.is_empty());

    // Scopes: Calendar läs+skriv, Gmail modify (läs + skapa drafts).
    // GmailModify = läs + skapa drafts + arkivera/flytta; skickar INTE mail
    // — för det krävs gmail.send, som vi medvetet utelämnar (alla drafts
    // kräver manuell skicka-bekräftelse i Gmail-webben).
    let scopes = &[
        GoogleScope::CalendarReadonly,
        GoogleScope::CalendarEvents,
        GoogleScope::GmailModify,
    ];

    let flow = GoogleOAuthFlow::start(client_id, client_secret, scopes)
        .await
        .map_err(|e| {
            tracing::error!("OAuth start misslyckades: {e}");
            format!("kunde inte starta OAuth: {e}")
        })?;

    // Öppna browsern. tauri-plugin-opener är cross-platform wrapper.
    app.opener()
        .open_url(&flow.auth_url, None::<&str>)
        .map_err(|e| {
            tracing::error!("browser-öppning misslyckades: {e}");
            format!("kunde inte öppna browser: {e}")
        })?;

    tracing::info!(
        "OAuth-flow startad; väntar på callback på port {}",
        flow.port
    );

    let tokens = flow.finalize().await.map_err(|e| {
        tracing::error!("OAuth finalize misslyckades: {e}");
        format!("OAuth misslyckades: {e}")
    })?;

    let refresh = tokens.refresh_token.ok_or_else(|| {
        tracing::error!(
            "Google returnerade ingen refresh-token — user måste kanske återkalla access och godkänna igen"
        );
        "Google returnerade ingen refresh-token".to_string()
    })?;
    svoice_secrets::set_google_refresh_token(&refresh).map_err(|e| {
        tracing::error!("kunde inte spara refresh-token: {e}");
        format!("kunde inte spara refresh-token: {e}")
    })?;

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
