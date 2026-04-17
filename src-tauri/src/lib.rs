use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use svoice_audio::vad::trim_silence;
use svoice_audio::{AudioCapture, AudioRing, VolumeMeter};
use svoice_hotkey::{
    install_rctrl_hook, register_hotkey, HotKey, LlCallback, LlKeyEvent, PttMachine, PttState,
};
use svoice_inject::{capture_selection, inject, remember_foreground_target, InjectMethod};
use svoice_llm::{AnthropicClient, LlmProvider, LlmRequest, Role, TurnContent};
use svoice_settings::{ComputeMode, Settings};
use svoice_stt::{PythonStt, SttConfig};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

const TRAY_IDLE_BYTES: &[u8] = include_bytes!("../icons/tray-idle.png");
const TRAY_REC_BYTES: &[u8] = include_bytes!("../icons/tray-recording.png");

const EV_PTT_STATE: &str = "ptt_state";
const EV_PTT_VOLUME: &str = "ptt_volume";
const EV_MIC_LEVEL: &str = "mic_level";
const EV_ACTION_POPUP_OPEN: &str = "action_popup_open";
const EV_ACTION_LLM_TOKEN: &str = "action_llm_token";
const EV_ACTION_LLM_DONE: &str = "action_llm_done";
const EV_ACTION_LLM_ERROR: &str = "action_llm_error";

#[derive(serde::Serialize, Clone, Copy)]
struct VolumeEvent {
    rms: f32,
}

#[derive(serde::Serialize, Clone)]
struct ActionPopupOpen {
    selection: Option<String>,
    command: String,
    mode: &'static str,
}

#[derive(serde::Serialize, Clone)]
struct ActionToken {
    text: String,
}

#[derive(serde::Serialize, Clone)]
struct ActionError {
    message: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,svoice_v3_lib=debug,svoice_audio=debug,svoice_hotkey=debug,\
                     svoice_inject=debug,svoice_ipc=debug,svoice_stt=debug,svoice_llm=debug",
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

            // Läs användar-settings från disk (eller default).
            let user_settings = Settings::load();
            tracing::info!(
                "settings: model={}, compute={:?}, vad={:.3}, anthropic_key={}",
                user_settings.stt_model,
                user_settings.stt_compute_mode,
                user_settings.vad_threshold,
                user_settings.anthropic_api_key.as_ref().map(|_| "****").unwrap_or("none"),
            );

            // Bygg SttConfig.
            let mut stt_config = SttConfig::default();
            stt_config.model = user_settings.stt_model.clone();
            match user_settings.stt_compute_mode {
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
            if let Ok(res_dir) = app.path().resource_dir() {
                let bundled_python = res_dir
                    .join("python-runtime")
                    .join("python")
                    .join("python.exe");
                if bundled_python.exists() {
                    tracing::info!("använder bundlad Python: {}", bundled_python.display());
                    stt_config.python_path = bundled_python;
                    stt_config.python_args = vec![];
                }
                let bundled_script = res_dir.join("python").join("stt_sidecar.py");
                if bundled_script.exists() {
                    tracing::info!("använder bundlat sidecar-script: {}", bundled_script.display());
                    stt_config.script_path = bundled_script;
                }
            }

            let stt = Arc::new(PythonStt::new(stt_config));
            let rt = Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime"));
            let vad_threshold = user_settings.vad_threshold;

            // Anthropic-klient om API-nyckel är konfigurerad.
            let anthropic: Option<Arc<dyn LlmProvider>> = user_settings
                .anthropic_api_key
                .as_ref()
                .filter(|k| !k.is_empty())
                .map(|key| {
                    let client = AnthropicClient::new(key.clone())
                        .with_model(user_settings.anthropic_model.clone());
                    Arc::new(client) as Arc<dyn LlmProvider>
                });
            if anthropic.is_none() {
                tracing::info!(
                    "action-LLM ej konfigurerad — lägg till anthropic_api_key i settings"
                );
            }

            let app_handle = app.handle().clone();

            // Audio-ownership: skapa ringen i setup-scope (Arc, Send+Sync).
            // AudioCapture (!Send pga cpal::Stream på Windows) skapas inuti
            // en egen "audio-owner"-tråd som håller streamen vid liv.
            let ring = Arc::new(AudioRing::new(16000 * 30));

            // Audio-owner thread — skapar capture, parker forever.
            let audio_ring = ring.clone();
            let mic_app = app_handle.clone();
            std::thread::Builder::new()
                .name("svoice-audio-owner".into())
                .spawn(move || {
                    let rms_cb: svoice_audio::capture::RmsCallback = Arc::new(move |rms: f32| {
                        emit_event(&mic_app, EV_MIC_LEVEL, VolumeEvent { rms });
                    });
                    let _capture = match AudioCapture::start_with_rms(audio_ring, Some(rms_cb)) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("kunde inte starta audio-capture: {e}");
                            return;
                        }
                    };
                    tracing::info!("audio-owner: capture aktiv");
                    // Blockera forever — tråden äger streamen tills app-exit.
                    loop {
                        std::thread::park();
                    }
                })
                .expect("kunde inte starta audio-owner-tråd");

            // Dikterings-PTT (höger Ctrl) — befintlig iter 2-workflow.
            let ptt_worker = ptt.clone();
            let (ptt_tx, ptt_rx) = mpsc::channel::<LlKeyEvent>();
            let ptt_app = app_handle.clone();
            let ptt_ring = ring.clone();
            let ptt_stt = stt.clone();
            let ptt_rt = rt.clone();
            std::thread::Builder::new()
                .name("svoice-ptt-worker".into())
                .spawn(move || {
                    ptt_worker_loop(
                        ptt_rx,
                        ptt_app,
                        ptt_worker,
                        ptt_ring,
                        ptt_stt,
                        ptt_rt,
                        vad_threshold,
                    );
                })
                .expect("kunde inte starta PTT worker-thread");

            let ptt_cb: LlCallback = Arc::new(move |ev: LlKeyEvent| {
                if ptt_tx.send(ev).is_err() {
                    tracing::warn!("PTT worker-channel stängd; tappar event {:?}", ev);
                }
            });
            match install_rctrl_hook(ptt_cb) {
                Ok(()) => tracing::info!("PTT aktiv: håll höger Ctrl för att diktera"),
                Err(e) => tracing::error!("kunde inte installera RightCtrl-hook: {e}"),
            }

            // Action-PTT (höger Alt) — iter 3 action-LLM popup.
            let (action_tx, action_rx) = mpsc::channel::<LlKeyEvent>();
            let action_app = app_handle.clone();
            let action_ring = ring.clone();
            let action_stt = stt.clone();
            let action_rt = rt.clone();
            let action_llm = anthropic.clone();
            std::thread::Builder::new()
                .name("svoice-action-worker".into())
                .spawn(move || {
                    action_worker_loop(
                        action_rx,
                        action_app,
                        action_ring,
                        action_stt,
                        action_rt,
                        action_llm,
                        vad_threshold,
                    );
                })
                .expect("kunde inte starta action worker-thread");

            // Action-PTT: spara target-HWND vid keydown INNAN popupen öppnas
            // så paste_and_restore kan SetForegroundWindow tillbaka efter hide.
            // Utan detta hamnade Ctrl+V i popup-webviewen och Ctrl-state blev
            // "fast" i Windows-session.
            let action_cb: LlCallback = Arc::new(move |ev: LlKeyEvent| {
                if ev == LlKeyEvent::Pressed {
                    remember_foreground_target();
                }
                if action_tx.send(ev).is_err() {
                    tracing::warn!("action worker-channel stängd; tappar event {:?}", ev);
                }
            });
            match register_hotkey(HotKey::Insert, action_cb) {
                Ok(()) => tracing::info!("Action-PTT aktiv: håll Insert för LLM-popup"),
                Err(e) => tracing::error!("kunde inte registrera Insert-hook: {e}"),
            }

            let _ = app.get_webview_window("main");

            // Positionera overlay: centrerat horisontellt, ~60 px ovan botten
            // (ger space för taskbar + lite andrum). Använder primärskärmens
            // size vid app-start — om user flyttar mellan skärmar behöver
            // overlay manuellt positioneras om (iter 4).
            if let Some(overlay) = app.get_webview_window("overlay") {
                if let Ok(Some(monitor)) = overlay.primary_monitor() {
                    let scr = monitor.size();
                    // Overlay-storlek från tauri.conf.json (200 × 56).
                    let ow: i32 = 200;
                    let oh: i32 = 56;
                    let x = (scr.width as i32 - ow) / 2;
                    // ~60 px från botten (ovanför typisk Windows-taskbar 40-48 px).
                    let y = scr.height as i32 - oh - 60;
                    let _ = overlay.set_position(tauri::PhysicalPosition::new(x, y));
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            svoice_ipc::get_settings,
            svoice_ipc::set_settings,
            svoice_ipc::action_apply,
            svoice_ipc::action_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn emit_event<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::debug!("emit '{event}' misslyckades: {e}");
    }
}

// === Dikterings-PTT (RCtrl) ===

#[allow(unused_assignments, unused_variables)]
fn ptt_worker_loop(
    rx: mpsc::Receiver<LlKeyEvent>,
    app_handle: AppHandle,
    ptt: Arc<Mutex<PttMachine>>,
    ring: Arc<AudioRing>,
    stt: Arc<PythonStt>,
    rt: Arc<tokio::runtime::Runtime>,
    vad_threshold: f32,
) {
    let mut meter: Option<VolumeMeter> = None;

    for ev in rx {
        let state_after = apply_event(&ptt, ev);
        emit_event(&app_handle, EV_PTT_STATE, state_after);
        update_tray_for_state(&app_handle, state_after);

        if ev == LlKeyEvent::Pressed && state_after == PttState::Recording {
            ring.clear();
            meter = start_volume_meter(&app_handle);
        }

        if ev == LlKeyEvent::Released && state_after == PttState::Processing {
            meter = None;
            emit_event(&app_handle, EV_PTT_VOLUME, VolumeEvent { rms: 0.0 });
            std::thread::sleep(std::time::Duration::from_millis(50));
            perform_transcribe_and_inject(&ring, &stt, &rt, vad_threshold);

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
        emit_event(&app_h, EV_PTT_VOLUME, VolumeEvent { rms });
    }) {
        Ok(m) => Some(m),
        Err(e) => {
            tracing::error!("kunde inte starta volym-mätare: {e}");
            None
        }
    }
}

fn perform_transcribe_and_inject(
    ring: &AudioRing,
    stt: &PythonStt,
    rt: &tokio::runtime::Runtime,
    vad_threshold: f32,
) {
    let audio = ring.drain();
    let (start, end) = trim_silence(&audio, 16000, vad_threshold);
    let segment = &audio[start..end];
    if segment.is_empty() {
        tracing::warn!("inget tal detekterat (VAD trimmade allt)");
        return;
    }
    match rt.block_on(stt.transcribe(segment)) {
        Ok(text) => {
            if text.is_empty() {
                tracing::warn!("STT returnerade tom text");
                return;
            }
            match inject(&text) {
                Ok(method) => {
                    let method_str = match method {
                        InjectMethod::SendInput => "send_input",
                        InjectMethod::Clipboard => "clipboard",
                    };
                    tracing::info!("inject OK via {method_str}: \"{}\"", text);
                }
                Err(e) => tracing::error!("inject FAIL: {e}"),
            }
        }
        Err(e) => tracing::error!("STT-fel: {e}"),
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

// === Action-LLM-PTT (RAlt) ===

fn action_worker_loop(
    rx: mpsc::Receiver<LlKeyEvent>,
    app_handle: AppHandle,
    ring: Arc<AudioRing>,
    stt: Arc<PythonStt>,
    rt: Arc<tokio::runtime::Runtime>,
    llm: Option<Arc<dyn LlmProvider>>,
    vad_threshold: f32,
) {
    for ev in rx {
        match ev {
            LlKeyEvent::Pressed => {
                // Rensa ring så vi bara fångar audio som kommer NU.
                ring.clear();
                tracing::debug!("action-PTT: recording");
                emit_event(&app_handle, EV_PTT_STATE, PttState::Recording);
                update_tray_for_state(&app_handle, PttState::Recording);
            }
            LlKeyEvent::Released => {
                tracing::debug!("action-PTT: released, processing...");
                emit_event(&app_handle, EV_PTT_STATE, PttState::Processing);
                update_tray_for_state(&app_handle, PttState::Processing);
                if let Err(e) =
                    handle_action_released(&app_handle, &ring, &stt, &rt, &llm, vad_threshold)
                {
                    tracing::error!("action-PTT fel: {e}");
                    emit_event(
                        &app_handle,
                        EV_ACTION_LLM_ERROR,
                        ActionError {
                            message: e.to_string(),
                        },
                    );
                }
                // Overlay tillbaka till idle efter transkription + popup-open.
                // (LLM-streaming fortsätter async men overlay-statet är
                // frikopplat — processing-state visas bara under STT-delen.)
                emit_event(&app_handle, EV_PTT_STATE, PttState::Idle);
                update_tray_for_state(&app_handle, PttState::Idle);
            }
        }
    }
}

fn handle_action_released(
    app_handle: &AppHandle,
    ring: &Arc<AudioRing>,
    stt: &Arc<PythonStt>,
    rt: &Arc<tokio::runtime::Runtime>,
    llm: &Option<Arc<dyn LlmProvider>>,
    vad_threshold: f32,
) -> anyhow::Result<()> {
    // 1. Fånga markering i aktivt fönster via clipboard-snapshot.
    //    Görs FÖRST innan vi transkriberar — target-fönstret har fortfarande fokus.
    std::thread::sleep(std::time::Duration::from_millis(40));
    let selection = match capture_selection() {
        Ok(sel) => sel,
        Err(e) => {
            tracing::warn!("capture_selection misslyckades: {e}");
            None
        }
    };
    if let Some(s) = &selection {
        tracing::info!("action: fångade selection ({} tecken)", s.chars().count());
    } else {
        tracing::info!("action: ingen markering");
    }

    // 2. Transkribera user's röstkommando.
    let audio = ring.drain();
    let (start, end) = trim_silence(&audio, 16000, vad_threshold);
    let segment = &audio[start..end];
    if segment.is_empty() {
        anyhow::bail!("inget röstkommando detekterat");
    }
    let command = rt.block_on(stt.transcribe(segment))?;
    let command = command.trim().to_string();
    if command.is_empty() {
        anyhow::bail!("STT returnerade tom text");
    }
    tracing::info!("action: command = \"{}\"", command);

    // 3. Bestäm mode baserat på om selection finns.
    let mode: &'static str = if selection.as_ref().map_or(false, |s| !s.trim().is_empty()) {
        "transform"
    } else {
        "query"
    };

    // 4. Öppna popup-fönstret. Om det är dolt, visa det.
    if let Some(win) = app_handle.get_webview_window("action-popup") {
        let _ = win.show();
        let _ = win.set_focus();
    }

    // 5. Emit open-event till popup-en.
    emit_event(
        app_handle,
        EV_ACTION_POPUP_OPEN,
        ActionPopupOpen {
            selection: selection.clone(),
            command: command.clone(),
            mode,
        },
    );

    // 6. Kör LLM i bakgrunden; streama tokens.
    let Some(llm) = llm.clone() else {
        emit_event(
            app_handle,
            EV_ACTION_LLM_ERROR,
            ActionError {
                message: "Ingen LLM-nyckel konfigurerad. Lägg till anthropic_api_key i inställningarna.".into(),
            },
        );
        return Ok(());
    };

    let llm_req = build_llm_request(mode, selection.as_deref(), &command);
    let app_clone = app_handle.clone();
    let rt_clone = rt.clone();
    rt.spawn(async move {
        match llm.complete_stream(llm_req).await {
            Ok(mut stream) => {
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => {
                            emit_event(&app_clone, EV_ACTION_LLM_TOKEN, ActionToken { text });
                        }
                        Err(e) => {
                            emit_event(
                                &app_clone,
                                EV_ACTION_LLM_ERROR,
                                ActionError {
                                    message: e.to_string(),
                                },
                            );
                            return;
                        }
                    }
                }
                emit_event(&app_clone, EV_ACTION_LLM_DONE, ());
            }
            Err(e) => {
                emit_event(
                    &app_clone,
                    EV_ACTION_LLM_ERROR,
                    ActionError {
                        message: e.to_string(),
                    },
                );
            }
        }
        let _ = rt_clone;
    });

    Ok(())
}

fn build_llm_request(mode: &str, selection: Option<&str>, command: &str) -> LlmRequest {
    let (system, user) = match mode {
        "transform" => {
            let sys = "Du är en svensk text-redigeringsassistent. Användaren ger dig \
en markerad textsträng och ett redigeringskommando. Returnera endast den \
modifierade texten — ingen förklaring, ingen markdown, inget 'Här är:'. \
Bara den råa texten som ska ersätta originalet.";
            let user_msg = format!(
                "Kommando: {}\n\n---\nTEXT ATT REDIGERA:\n{}",
                command,
                selection.unwrap_or("")
            );
            (sys.to_string(), user_msg)
        }
        _ => {
            let sys = "Du är en kort, direkt svensk assistent. Svara koncist på svenska \
utan extra prata. Om frågan kräver en lista, ge en kort numrerad lista.";
            (sys.to_string(), command.to_string())
        }
    };

    LlmRequest {
        system: Some(system),
        turns: vec![TurnContent {
            role: Role::User,
            text: user,
        }],
        temperature: 0.3,
        max_tokens: 1024,
    }
}
