use serde::Serialize;
use svoice_hotkey::PttState;
use svoice_inject::{inject, InjectError, InjectMethod};
use svoice_stt::dummy_transcribe;

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

/// Kör ett end-to-end inject av dummy-STT-texten. Används både som smoke-command
/// från UI och som resultat i PTT-loop (via hotkey callback).
#[tauri::command]
pub fn run_dummy_inject() -> Result<InjectResult, String> {
    let text = dummy_transcribe();
    match inject(&text) {
        Ok(method) => Ok(InjectResult {
            method: match method {
                InjectMethod::SendInput => "send_input".into(),
                InjectMethod::Clipboard => "clipboard".into(),
            },
            chars: text.chars().count(),
        }),
        Err(e) => Err(map_inject_error(e)),
    }
}

fn map_inject_error(e: InjectError) -> String {
    format!("inject-fel: {e}")
}
