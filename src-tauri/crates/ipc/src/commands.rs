use serde::Serialize;
use svoice_hotkey::PttState;

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
