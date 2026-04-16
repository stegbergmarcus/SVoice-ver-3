use std::sync::{Arc, Mutex};

use svoice_hotkey::{is_key_down, register_ptt, HotkeyCallback, PttMachine, PttState};
use svoice_inject::{inject, InjectMethod};
use svoice_stt::dummy_transcribe;
use tauri::{AppHandle, Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,svoice=debug")),
        )
        .init();

    let ptt = Arc::new(Mutex::new(PttMachine::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            tracing::info!("svoice-v3 startar");

            let ptt_cb = ptt.clone();
            let callback: HotkeyCallback<tauri::Wry> = Arc::new(
                move |app: &AppHandle<tauri::Wry>, _sc, ev| {
                    let state_after: PttState;
                    if is_key_down(&ev) {
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_key_down();
                        state_after = m.state();
                    } else {
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_key_up();
                        state_after = m.state();
                    }

                    let _ = app.emit("ptt://state", state_after);

                    if !is_key_down(&ev) && state_after == PttState::Processing {
                        let text = dummy_transcribe();
                        match inject(&text) {
                            Ok(method) => {
                                let method_str = match method {
                                    InjectMethod::SendInput => "send_input",
                                    InjectMethod::Clipboard => "clipboard",
                                };
                                tracing::info!("inject OK via {method_str}");
                            }
                            Err(e) => tracing::error!("inject FAIL: {e}"),
                        }
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_finish_processing();
                        let final_state = m.state();
                        let _ = app.emit("ptt://state", final_state);
                    }
                },
            );

            match register_ptt(&app.handle(), callback) {
                Ok(reg) => tracing::info!("hotkey aktiv: {}", reg.label),
                Err(e) => tracing::error!("hotkey-registrering misslyckades: {e}"),
            }

            let _ = app.get_webview_window("main");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![svoice_ipc::run_dummy_inject])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
