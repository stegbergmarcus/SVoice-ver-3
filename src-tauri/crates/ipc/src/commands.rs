use serde::Serialize;
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

/// Applicera action-popup LLM-resultatet: klistra in i tidigare fokuserat fönster
/// via clipboard-paste, och återställ ursprungligt clipboard-innehåll efteråt.
///
/// Kör på blocking-thread eftersom clipboard + SendInput är synchrona Win32-calls.
#[tauri::command]
pub async fn action_apply(result: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || paste_and_restore(&result))
        .await
        .map_err(|e| format!("join error: {e}"))?
        .map_err(|e| format!("paste failed: {e}"))?;
    tracing::info!(
        "action-popup: result applied via clipboard"
    );
    Ok(())
}

/// Användaren avbröt action-popupen utan att applicera resultatet.
#[tauri::command]
pub fn action_cancel() {
    tracing::debug!("action-popup: cancelled by user");
}
