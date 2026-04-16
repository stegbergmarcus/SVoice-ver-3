use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use svoice_audio::VolumeMeter;
use svoice_hotkey::{install_rctrl_hook, LlCallback, LlKeyEvent, PttMachine, PttState};
use svoice_inject::{inject, InjectMethod};
use svoice_stt::dummy_transcribe;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

const TRAY_IDLE_BYTES: &[u8] = include_bytes!("../icons/tray-idle.png");
const TRAY_REC_BYTES: &[u8] = include_bytes!("../icons/tray-recording.png");

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

            // Tray
            let quit_item = MenuItem::with_id(app, "quit", "Avsluta", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;
            let idle_img = Image::from_bytes(TRAY_IDLE_BYTES)?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(idle_img)
                .menu(&menu)
                .tooltip("SVoice 3 — idle")
                .on_menu_event(|app, ev| {
                    if ev.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // PTT worker
            let app_handle = app.handle().clone();
            let ptt_worker = ptt.clone();
            let (tx, rx) = mpsc::channel::<LlKeyEvent>();

            std::thread::Builder::new()
                .name("svoice-ptt-worker".into())
                .spawn(move || {
                    ptt_worker_loop(rx, app_handle, ptt_worker);
                })
                .expect("kunde inte starta PTT worker-thread");

            let callback: LlCallback = Arc::new(move |ev: LlKeyEvent| {
                if tx.send(ev).is_err() {
                    tracing::warn!("PTT worker-channel stängd; tappar event {:?}", ev);
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

fn ptt_worker_loop(
    rx: mpsc::Receiver<LlKeyEvent>,
    app_handle: AppHandle,
    ptt: Arc<Mutex<PttMachine>>,
) {
    // VolumeMeter lever bara medan vi faktiskt spelar in. Drop av Option stänger streamen.
    let mut meter: Option<VolumeMeter> = None;

    for ev in rx {
        let state_after: PttState;
        match ev {
            LlKeyEvent::Pressed => {
                let mut m = ptt.lock().unwrap();
                m.on_key_down();
                state_after = m.state();
            }
            LlKeyEvent::Released => {
                let mut m = ptt.lock().unwrap();
                m.on_key_up();
                state_after = m.state();
            }
        }

        let _ = app_handle.emit("ptt_state", state_after);
        update_tray_for_state(&app_handle, state_after);

        if ev == LlKeyEvent::Pressed && state_after == PttState::Recording {
            // Starta volym-mätaren medan PTT hålls.
            let app_h = app_handle.clone();
            match VolumeMeter::start(move |rms| {
                let _ = app_h.emit("ptt_volume", rms);
            }) {
                Ok(m) => meter = Some(m),
                Err(e) => tracing::error!("kunde inte starta volym-mätare: {e}"),
            }
        }

        if ev == LlKeyEvent::Released && state_after == PttState::Processing {
            // Stäng volym-streamen före inject (inject tar ~40ms).
            meter = None;
            let _ = app_handle.emit("ptt_volume", 0.0f32);

            // Låt Windows helt registrera RightCtrl-release innan vi inject:ar.
            // Utan denna paus hinner vissa target-fönster fortfarande se Ctrl
            // nedtryckt när de tar emot Unicode-tecknen.
            std::thread::sleep(std::time::Duration::from_millis(50));

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
            let mut m = ptt.lock().unwrap();
            m.on_finish_processing();
            let final_state = m.state();
            let _ = app_handle.emit("ptt_state", final_state);
            update_tray_for_state(&app_handle, final_state);
        }
    }
}

fn update_tray_for_state(app: &AppHandle, state: PttState) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let bytes = match state {
            PttState::Recording => TRAY_REC_BYTES,
            _ => TRAY_IDLE_BYTES,
        };
        if let Ok(img) = Image::from_bytes(bytes) {
            let _ = tray.set_icon(Some(img));
        }
        let tip = match state {
            PttState::Idle => "SVoice 3 — idle",
            PttState::Recording => "SVoice 3 — spelar in",
            PttState::Processing => "SVoice 3 — transkriberar",
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}
