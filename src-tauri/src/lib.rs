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

const EV_PTT_STATE: &str = "ptt_state";
const EV_PTT_VOLUME: &str = "ptt_volume";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Explicit per-crate filter — "svoice=debug" matchar inte targets
                // som heter svoice_audio / svoice_hotkey osv (ingen prefix-semantik
                // i tracing-subscribers EnvFilter).
                tracing_subscriber::EnvFilter::new(
                    "info,svoice_v3_lib=debug,svoice_audio=debug,svoice_hotkey=debug,\
                     svoice_inject=debug,svoice_ipc=debug,svoice_stt=debug",
                )
            }),
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

/// Emit Tauri-event och logga fel (debug-nivå — normalt betyder ett emit-fel
/// att appen stänger eller att webview ännu inte är redo).
fn emit_event<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::debug!("emit '{event}' misslyckades: {e}");
    }
}

// VolumeMeter-slot:en hålls bara för sin Drop-semantik (när Some(m) drop:s
// stoppas streamen). Compiler kan inte se Drop-side-effekten.
#[allow(unused_assignments, unused_variables)]
fn ptt_worker_loop(
    rx: mpsc::Receiver<LlKeyEvent>,
    app_handle: AppHandle,
    ptt: Arc<Mutex<PttMachine>>,
) {
    let mut meter: Option<VolumeMeter> = None;

    for ev in rx {
        let state_after = apply_event(&ptt, ev);

        emit_event(&app_handle, EV_PTT_STATE, state_after);
        update_tray_for_state(&app_handle, state_after);

        if ev == LlKeyEvent::Pressed && state_after == PttState::Recording {
            meter = start_volume_meter(&app_handle);
        }

        if ev == LlKeyEvent::Released && state_after == PttState::Processing {
            // Stäng volym-streamen före inject (inject tar ~40ms).
            meter = None;
            emit_event(&app_handle, EV_PTT_VOLUME, 0.0f32);

            // Låt Windows helt registrera RightCtrl-release innan vi inject:ar.
            // Utan denna paus hinner vissa target-fönster fortfarande se Ctrl
            // nedtryckt när de tar emot Unicode-tecknen.
            std::thread::sleep(std::time::Duration::from_millis(50));

            perform_inject();

            let final_state = {
                let mut m = ptt_lock(&ptt);
                m.on_finish_processing();
                m.state()
            };
            emit_event(&app_handle, EV_PTT_STATE, final_state);
            update_tray_for_state(&app_handle, final_state);
        }
    }
}

/// Applicera PTT-state-transition. Panic:ar om PttMachine's mutex är poisoned —
/// det kan bara hända om en tidigare lock-holder paniserat, och vi gör aldrig
/// något fallibelt inne i critical section, så detta är praktiskt omöjligt.
fn apply_event(ptt: &Mutex<PttMachine>, ev: LlKeyEvent) -> PttState {
    let mut m = ptt_lock(ptt);
    match ev {
        LlKeyEvent::Pressed => {
            m.on_key_down();
        }
        LlKeyEvent::Released => {
            m.on_key_up();
        }
    }
    m.state()
}

fn ptt_lock(ptt: &Mutex<PttMachine>) -> std::sync::MutexGuard<'_, PttMachine> {
    ptt.lock()
        .expect("PttMachine-mutex poisoned — inget critical section panic:ar")
}

fn start_volume_meter(app_handle: &AppHandle) -> Option<VolumeMeter> {
    let app_h = app_handle.clone();
    match VolumeMeter::start(move |rms| {
        emit_event(&app_h, EV_PTT_VOLUME, rms);
    }) {
        Ok(m) => Some(m),
        Err(e) => {
            tracing::error!("kunde inte starta volym-mätare: {e}");
            None
        }
    }
}

fn perform_inject() {
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
}

fn update_tray_for_state(app: &AppHandle, state: PttState) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    let bytes = match state {
        PttState::Recording => TRAY_REC_BYTES,
        _ => TRAY_IDLE_BYTES,
    };
    if let Ok(img) = Image::from_bytes(bytes) {
        if let Err(e) = tray.set_icon(Some(img)) {
            tracing::debug!("tray.set_icon misslyckades: {e}");
        }
    }
    let tip = match state {
        PttState::Idle => "SVoice 3 — idle",
        PttState::Recording => "SVoice 3 — spelar in",
        PttState::Processing => "SVoice 3 — transkriberar",
    };
    if let Err(e) = tray.set_tooltip(Some(tip)) {
        tracing::debug!("tray.set_tooltip misslyckades: {e}");
    }
}
