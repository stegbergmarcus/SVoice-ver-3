use std::sync::{Arc, Mutex};

use svoice_hotkey::{install_rctrl_hook, LlCallback, LlKeyEvent, PttMachine, PttState};
use svoice_inject::{inject, InjectMethod};
use svoice_stt::dummy_transcribe;
use tauri::{Emitter, Manager};

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

            let app_handle = app.handle().clone();
            let ptt_cb = ptt.clone();

            let callback: LlCallback = Arc::new(move |ev: LlKeyEvent| {
                let state_after: PttState;
                match ev {
                    LlKeyEvent::Pressed => {
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_key_down();
                        state_after = m.state();
                    }
                    LlKeyEvent::Released => {
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_key_up();
                        state_after = m.state();
                    }
                }

                let _ = app_handle.emit("ptt://state", state_after);

                if ev == LlKeyEvent::Released && state_after == PttState::Processing {
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
                    let _ = app_handle.emit("ptt://state", final_state);
                }
            });

            match install_rctrl_hook(callback) {
                Ok(()) => tracing::info!("PTT aktiv: håll höger Ctrl (RightCtrl) för att diktera"),
                Err(e) => tracing::error!("kunde inte installera RightCtrl-hook: {e}"),
            }

            let _ = app.get_webview_window("main");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![svoice_ipc::run_dummy_inject])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
