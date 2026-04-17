use serde::Serialize;
use svoice_audio::list_input_devices;
use svoice_hotkey::PttState;
use svoice_inject::paste_and_restore;
use svoice_settings::Settings;

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

/// Skriv settings till disk. Returnerar fel-sträng till frontend på failure
/// så att UI kan visa toast.
#[tauri::command]
pub fn set_settings(settings: Settings) -> Result<(), String> {
    settings.save().map_err(|e| format!("kunde inte spara settings: {e}"))
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
