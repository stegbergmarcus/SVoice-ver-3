use serde::Serialize;
use svoice_hotkey::PttState;
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
