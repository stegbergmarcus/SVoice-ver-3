mod agentic;
mod migrate;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Förhindrar att båda PTT-flöden (diktering RCtrl och action Insert) kör
/// samtidigt — de delar AudioRing och skulle tömma varandras data.
/// Första Pressed vinner, andra ignoreras tills första releases.
const OWNER_NONE: u8 = 0;
const OWNER_DICTATION: u8 = 1;
const OWNER_ACTION: u8 = 2;
static PTT_OWNER: AtomicU8 = AtomicU8::new(OWNER_NONE);

/// Senaste selection fångad när palette-hotkey trycktes. Läses av
/// `run_smart_function`-IPC när user väljer en function.
static PALETTE_SELECTION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

use futures_util::StreamExt;
use svoice_audio::vad::trim_silence;
use svoice_audio::{AudioCapture, AudioRing, VolumeMeter};
use svoice_hotkey::{register_with_role, HotKey, LlCallback, LlKeyEvent, PttMachine, PttState};
use svoice_inject::{capture_selection, inject, remember_foreground_target, InjectMethod};
use svoice_llm::{
    AnthropicClient, GeminiClient, GroqClient, LlmProvider, LlmRequest, OllamaClient, Role,
    TurnContent,
};
use svoice_settings::{ComputeMode, LlmProvider as ProviderChoice, Settings, SttProvider};
use svoice_stt::{GroqStt, PythonStt, SttConfig};
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

/// Initierar logging till stderr + en roterande fil i `%APPDATA%/svoice-v3/logs/`.
/// Returnerar WorkerGuard som måste hållas vid liv under hela process-livstiden;
/// annars droppar non-blocking appender skrivningar innan de når disken.
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "info,svoice_v3_lib=debug,svoice_audio=debug,svoice_hotkey=debug,\
             svoice_inject=debug,svoice_ipc=debug,svoice_stt=debug,svoice_llm=debug",
        )
    });

    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    let (file_layer, guard) = match log_file_writer() {
        Some((writer, guard)) => {
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer);
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    guard
}

fn log_file_writer() -> Option<(
    tracing_appender::non_blocking::NonBlocking,
    tracing_appender::non_blocking::WorkerGuard,
)> {
    let appdata = std::env::var("APPDATA").ok()?;
    let log_dir = std::path::PathBuf::from(appdata)
        .join("svoice-v3")
        .join("logs");
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("svoice: kunde inte skapa loggkatalog {log_dir:?}: {e}");
        return None;
    }
    let appender = tracing_appender::rolling::daily(log_dir, "svoice.log");
    Some(tracing_appender::non_blocking(appender))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Håll guarden vid liv hela run()-livstiden så non-blocking appender
    // hinner flusha innan processen avslutas.
    let _log_guard = init_tracing();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "svoice-v3 startar");

    let ptt = Arc::new(Mutex::new(PttMachine::new()));

    // Ctrl+Shift+Space → öppna command palette (smart-functions).
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
    let palette_shortcut = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::SHIFT),
        Code::Space,
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // Autostart: låter user lägga till/ta bort app från Windows startup.
        // Args tomma — vid start är main-fönstret dolt (tauri.conf.json) så
        // appen hamnar tyst i tray. LaunchAgent är bara macOS-relevant, vi
        // kör Windows men trait kräver att en variant anges.
        .plugin(tauri_plugin_autostart::Builder::new()
            .app_name("SVoice 3")
            .build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut(palette_shortcut)
                .expect("valid palette shortcut")
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &palette_shortcut
                        && event.state() == ShortcutState::Pressed
                    {
                        // 1. Spara target-HWND (active app just nu, innan palette
                        //    stjäl focus). Använd samma mekanism som action-popup.
                        if !remember_foreground_target() {
                            tracing::debug!("palette: target är vår egen app, skippa");
                            return;
                        }
                        // 2. Fånga markering (Ctrl+C → clipboard). OK om tom.
                        let selection = capture_selection().ok().flatten();
                        if let Ok(mut guard) = PALETTE_SELECTION.lock() {
                            *guard = selection;
                        }
                        // 3. Öppna palette-window + emit open-event.
                        if let Some(win) = app.get_webview_window("palette") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                        let _ = app.emit("palette_open", ());
                    }
                })
                .build(),
        )
        .setup(move |app| {
            tracing::info!("svoice-v3 tauri setup klar");

            // Tvinga transparent background på popup/overlay/palette-webviews.
            // Tauri's `"transparent": true` räcker inte alltid på Windows 11 DWM —
            // WebView2 ritar en default (grå/svart) background innan användaren
            // ser vår CSS. Kombinera tre ansatser för att tvinga genuin transparens:
            //   1. set_background_color(Color(0,0,0,0)) berättar för webview:n att
            //      INGEN backdrop ska ritas.
            //   2. DWMWA_SYSTEMBACKDROP_TYPE=DWMSBT_NONE stänger Windows 11's
            //      automatiska Mica/Acrylic/Tabbed backdrop-effekt.
            //   3. DWMWA_USE_HOSTBACKDROPBRUSH=0 hindrar DWM från att rita en
            //      host-backdrop-brush för parent-fönstret.
            // Utan dessa visas en grå/svart fyrkant under popup-cardet på Win11.
            for label in &["action-popup", "palette", "overlay"] {
                if let Some(win) = app.get_webview_window(label) {
                    if let Err(e) = win.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)))
                    {
                        tracing::debug!(
                            "set_background_color misslyckades för {label}: {e} (ignoreras)"
                        );
                    }
                    apply_dwm_transparency(&win, label);
                }
            }

            // Tray — main-fönstret är dolt by default, öppnas via meny eller
            // vänsterklick på tray-ikonen.
            let open_item =
                MenuItem::with_id(app, "open", "Visa inställningar", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Avsluta", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;
            let idle_img = Image::from_bytes(TRAY_IDLE_BYTES)?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(idle_img)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("SVoice 3 — vänsterklicka för inställningar")
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "open" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Kör best-effort migration av klartext anthropic_api_key → keyring.
            // Idempotent; no-op efter första migration.
            if let Err(e) = migrate::migrate_anthropic_key(&Settings::path()) {
                tracing::error!("settings-migration fel: {e}");
            }

            // Seeda smart-functions default-prompts. Idempotent — skriver bara
            // filer som saknas.
            if let Err(e) =
                svoice_smart_functions::seed_defaults(&svoice_smart_functions::default_dir())
            {
                tracing::error!("smart-functions seed fel: {e}");
            }

            // Läs användar-settings från disk (eller default).
            let user_settings = Settings::load();

            // Synka Windows startup-registret mot user-settings.autostart.
            // Körs idempotent varje app-start så registry inte spretar från
            // settings.json (t.ex. om user avinstallerat + ominstallerat till
            // annan path — gamla entry pekar fel och måste skrivas om).
            if let Err(e) = svoice_ipc::commands::sync_autostart(app.handle(), user_settings.autostart) {
                tracing::warn!("autostart-sync vid start misslyckades: {e}");
            }

            tracing::info!(
                "settings: model={}, compute={:?}, vad={:.3}, anthropic_key={}",
                user_settings.stt_model,
                user_settings.stt_compute_mode,
                user_settings.vad_threshold,
                if svoice_secrets::has_anthropic_key() {
                    "****"
                } else {
                    "none"
                },
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
                    tracing::info!(
                        "använder bundlat sidecar-script: {}",
                        bundled_script.display()
                    );
                    stt_config.script_path = bundled_script;
                }
            }

            let stt = Arc::new(PythonStt::new(stt_config));
            // Registrera PythonStt som Tauri managed state så set_settings-IPC
            // kan kalla stt.reload_config() vid modell/compute-byte.
            app.manage(stt.clone());
            let rt = Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime"));

            // Anthropic-klient och VAD-threshold hot-reloadas från disk
            // vid varje PTT-trigger (se worker-looparna nedan) så användaren
            // slipper restart för att byta modell, nyckel eller känslighet.
            if !svoice_secrets::has_anthropic_key() {
                tracing::info!(
                    "action-LLM ej konfigurerad — lägg till Anthropic-nyckel i Settings"
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

            // Auto-check för ny version 10 sek efter setup. Använder cached
            // resultat om senaste check är <24 h gammal så vi inte hamrar
            // GitHub API vid varje app-start. Bara tray-notification + logg —
            // aldrig blockande UI.
            let update_app = app_handle.clone();
            rt.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                match svoice_updates::check_latest_cached_fallback().await {
                    Ok(status) if status.available => {
                        if let Some(latest) = &status.latest_version {
                            tracing::info!("ny version {latest} tillgänglig");
                            use tauri_plugin_notification::NotificationExt;
                            if let Err(e) = update_app
                                .notification()
                                .builder()
                                .title("SVoice 3 — uppdatering tillgänglig")
                                .body(format!(
                                    "Version {latest} är nu släppt. Öppna Settings för nedladdning."
                                ))
                                .show()
                            {
                                tracing::debug!("update-notis failade: {e}");
                            }
                        }
                    }
                    Ok(_) => tracing::debug!("update-check: du kör senaste versionen"),
                    Err(e) => tracing::debug!("update-check misslyckades (no-op): {e}"),
                }
            });

            // Auto-download av default-STT-modellen vid första app-start om
            // den inte redan är cachad. Görs bara om STT är aktiverat (annars
            // behövs modellen inte ändå) och gater på STT_DOWNLOAD_IN_PROGRESS
            // så manuell Settings-download inte krockar.
            if user_settings.stt_enabled {
                let default_model = Settings::default().stt_model;
                if !svoice_ipc::is_hf_model_cached(&default_model) {
                    if svoice_ipc::STT_DOWNLOAD_IN_PROGRESS
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        let stt_clone = stt.clone();
                        let app_clone = app_handle.clone();
                        let model_clone = default_model.clone();
                        rt.spawn(async move {
                            use tauri_plugin_notification::NotificationExt;
                            let _ = app_clone
                                .notification()
                                .builder()
                                .title("SVoice")
                                .body(format!(
                                    "Laddar ner STT-modell i bakgrunden: {model_clone}"
                                ))
                                .show();
                            let app_for_cb = app_clone.clone();
                            let model_for_cb = model_clone.clone();
                            let result = stt_clone
                                .download_model(&model_clone, move |status| {
                                    let _ = app_for_cb.emit(
                                        "stt_model_download_progress",
                                        serde_json::json!({
                                            "model": &model_for_cb,
                                            "status": status,
                                        }),
                                    );
                                })
                                .await;
                            // Rensa flaggan oavsett utfall — annars kan user
                            // inte köra manuell download efter auto-fail.
                            svoice_ipc::STT_DOWNLOAD_IN_PROGRESS
                                .store(false, Ordering::SeqCst);
                            match result {
                                Ok(()) => {
                                    let _ = app_clone.emit(
                                        "stt_model_download_done",
                                        serde_json::json!({ "model": &model_clone }),
                                    );
                                    let _ = app_clone
                                        .notification()
                                        .builder()
                                        .title("SVoice")
                                        .body(format!(
                                            "STT-modell redo: {model_clone}. Håll höger Ctrl för att diktera."
                                        ))
                                        .show();
                                    tracing::info!("auto-download klar: {model_clone}");
                                }
                                Err(e) => {
                                    tracing::error!("auto-download fel: {e}");
                                    let _ = app_clone
                                        .notification()
                                        .builder()
                                        .title("SVoice")
                                        .body(
                                            "STT-modell kunde inte laddas ner. Öppna Settings och klicka Ladda ner manuellt.",
                                        )
                                        .show();
                                }
                            }
                        });
                    }
                } else {
                    tracing::info!(
                        "auto-download: default-STT-modellen {} är redan cachad, hoppar över",
                        default_model
                    );
                }
            }

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
                    ptt_worker_loop(ptt_rx, ptt_app, ptt_worker, ptt_ring, ptt_stt, ptt_rt);
                })
                .expect("kunde inte starta PTT worker-thread");

            // Läs hotkey-val. Validera: om båda är samma, använd default.
            let (dict_key, action_key) = {
                let d = user_settings.dictation_hotkey;
                let a = user_settings.action_hotkey;
                if d == a {
                    tracing::warn!(
                        "dictation_hotkey == action_hotkey ({:?}) — faller tillbaka till default",
                        d
                    );
                    (HotKey::RightCtrl, HotKey::Insert)
                } else {
                    (d, a)
                }
            };

            let ptt_cb: LlCallback = Arc::new(move |ev: LlKeyEvent| {
                if ptt_tx.send(ev).is_err() {
                    tracing::warn!("PTT worker-channel stängd; tappar event {:?}", ev);
                }
            });
            match register_with_role("dictation", dict_key, ptt_cb) {
                Ok(()) => tracing::info!("PTT aktiv: håll {:?} för att diktera", dict_key),
                Err(e) => tracing::error!("kunde inte registrera dikterings-hotkey: {e}"),
            }

            // Action-PTT (höger Alt) — iter 3 action-LLM popup.
            let (action_tx, action_rx) = mpsc::channel::<LlKeyEvent>();
            let action_app = app_handle.clone();
            let action_ring = ring.clone();
            let action_stt = stt.clone();
            let action_rt = rt.clone();
            std::thread::Builder::new()
                .name("svoice-action-worker".into())
                .spawn(move || {
                    action_worker_loop(action_rx, action_app, action_ring, action_stt, action_rt);
                })
                .expect("kunde inte starta action worker-thread");

            // Action-PTT: spara target-HWND vid keydown INNAN popupen öppnas
            // så paste_and_restore kan SetForegroundWindow tillbaka efter hide.
            // remember_foreground_target() returnerar false om target är vår
            // egen app — då skippar vi hela action-flödet (paste tillbaka till
            // vår webview triggar Ctrl-state-hang i Windows).
            let action_tx_for_cb = action_tx.clone();
            let action_cb: LlCallback = Arc::new(move |ev: LlKeyEvent| {
                if ev == LlKeyEvent::Pressed {
                    if !remember_foreground_target() {
                        // Target är vår egen app eller saknas — ignorera.
                        return;
                    }
                }
                if action_tx_for_cb.send(ev).is_err() {
                    tracing::warn!("action worker-channel stängd; tappar event {:?}", ev);
                }
            });
            match register_with_role("action", action_key, action_cb) {
                Ok(()) => tracing::info!("Action-PTT aktiv: håll {:?} för LLM-popup", action_key),
                Err(e) => tracing::error!("kunde inte registrera action-hotkey: {e}"),
            }

            // Follow-up-poll-thread: frontend IPC action_followup_start/stop sätter
            // atomiska flaggor (popup har ingen key-access via LL-hook när den är
            // fokuserad eftersom WebView2/system-hookar filter:ar keydowns bort från
            // systemhook-kedjan). Vi pollar flaggorna var 20 ms och skickar samma
            // LlKeyEvent som LL-hook hade gjort, så action_worker_loop ser en identisk
            // flow och follow-up-path triggas utan LL-hook-beroende.
            let followup_tx = action_tx.clone();
            std::thread::Builder::new()
                .name("svoice-followup-poll".into())
                .spawn(move || loop {
                    if svoice_ipc::FOLLOWUP_START_REQUESTED
                        .swap(false, Ordering::SeqCst)
                    {
                        if followup_tx.send(LlKeyEvent::Pressed).is_err() {
                            tracing::warn!(
                                "follow-up Pressed skickades men action-channel stängd"
                            );
                            break;
                        }
                    }
                    if svoice_ipc::FOLLOWUP_STOP_REQUESTED
                        .swap(false, Ordering::SeqCst)
                    {
                        if followup_tx.send(LlKeyEvent::Released).is_err() {
                            tracing::warn!(
                                "follow-up Released skickades men action-channel stängd"
                            );
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                })
                .expect("kunde inte starta followup-poll-thread");

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

            // Intercept close-event på main-fönstret. Default i Tauri 2 är
            // att X destroyer webview:en — men vi är tray-resident, så vi
            // vill bara hide:a så user kan öppna igen via tray-click.
            if let Some(main) = app.get_webview_window("main") {
                let main_clone = main.clone();
                main.on_window_event(move |ev| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                        let _ = main_clone.hide();
                        api.prevent_close();
                    }
                });
            }

            // Action-popup: stäng när user klickar utanför (focus lost).
            // Förbättrad UX — inte bara Esc utan "klicka bort" som vanliga modaler.
            // Konversations-state rensas också så nästa Insert-PTT börjar ny session.
            // Grace-period: om streaming pågår (eller just avslutats, ≤500 ms)
            // ignoreras focus-lost så user inte av misstag stänger ett pågående svar.
            if let Some(popup) = app.get_webview_window("action-popup") {
                let popup_clone = popup.clone();
                popup.on_window_event(move |ev| {
                    if let tauri::WindowEvent::Focused(false) = ev {
                        if svoice_ipc::ACTION_POPUP_STREAMING
                            .load(Ordering::SeqCst)
                        {
                            tracing::debug!(
                                "action-popup: focus-lost ignorerad — streaming pågår"
                            );
                            return;
                        }
                        if popup_clone
                            .is_visible()
                            .ok()
                            .unwrap_or(false)
                        {
                            let _ = popup_clone.hide();
                            svoice_ipc::clear_active_conversation();
                            tracing::debug!(
                                "action-popup: stängd via click-outside (focus lost)"
                            );
                        }
                    }
                });
            }

            // Palette: samma click-outside-beteende.
            if let Some(palette) = app.get_webview_window("palette") {
                let palette_clone = palette.clone();
                palette.on_window_event(move |ev| {
                    if let tauri::WindowEvent::Focused(false) = ev {
                        if palette_clone.is_visible().ok().unwrap_or(false) {
                            let _ = palette_clone.hide();
                            tracing::debug!(
                                "palette: stängd via click-outside (focus lost)"
                            );
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            svoice_ipc::action_apply,
            svoice_ipc::action_cancel,
            svoice_ipc::action_followup_start,
            svoice_ipc::action_followup_stop,
            svoice_ipc::check_for_updates,
            svoice_ipc::check_for_updates_cached,
            svoice_ipc::check_hf_cached,
            svoice_ipc::clear_anthropic_key,
            svoice_ipc::clear_gemini_key,
            svoice_ipc::clear_groq_key,
            svoice_ipc::download_stt_model,
            svoice_ipc::get_settings,
            svoice_ipc::google_connect,
            svoice_ipc::google_connection_status,
            svoice_ipc::google_disconnect,
            svoice_ipc::has_anthropic_key,
            svoice_ipc::has_gemini_key,
            svoice_ipc::has_groq_key,
            svoice_ipc::list_mic_devices,
            svoice_ipc::list_ollama_models,
            svoice_ipc::list_smart_functions,
            svoice_ipc::open_smart_functions_dir,
            svoice_ipc::pull_ollama_model,
            palette_close,
            run_smart_function,
            svoice_ipc::set_anthropic_key,
            svoice_ipc::set_gemini_key,
            svoice_ipc::set_groq_key,
            svoice_ipc::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Skicka OS-toast-notifikation vid fel. Används parallellt med event-emit
/// så user ser felet även om popup/main-window är dolda.
fn emit_error_toast(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        tracing::debug!("kunde inte visa error-toast: {e}");
    }
}

/// Windows 11-specifik: stäng Mica/Acrylic system-backdrop på ett transparent-
/// deklarerat fönster via `DwmSetWindowAttribute`. Utan detta ritar DWM en
/// systembackdrop (grå/ljusgrå "glass") under webview:n trots
/// `"transparent": true` i tauri.conf.json. Call:et är safe no-op på Win10
/// (attributet ignoreras) så vi behöver inte gate:a på version.
fn apply_dwm_transparency(win: &tauri::WebviewWindow, label: &str) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMSBT_NONE, DWMWA_SYSTEMBACKDROP_TYPE,
            DWMWA_USE_HOSTBACKDROPBRUSH,
        };

        let hwnd = match win.hwnd() {
            Ok(h) => HWND(h.0 as *mut _),
            Err(e) => {
                tracing::debug!("apply_dwm_transparency({label}): ingen HWND ({e})");
                return;
            }
        };
        // DWMSBT_NONE = 1 (Windows 11 build 22621+), betyder "rita ingen
        // system-backdrop".
        let backdrop: i32 = DWMSBT_NONE.0;
        let r = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            )
        };
        if r.is_err() {
            tracing::debug!(
                "DwmSetWindowAttribute(SYSTEMBACKDROP_TYPE) för {label}: {:?} (ignoreras på äldre Win)",
                r
            );
        }
        // DWMWA_USE_HOSTBACKDROPBRUSH = 17, värde 0 = inte använd host-backdrop.
        let off: i32 = 0;
        let r2 = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_HOSTBACKDROPBRUSH,
                &off as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            )
        };
        if r2.is_err() {
            tracing::debug!(
                "DwmSetWindowAttribute(USE_HOSTBACKDROPBRUSH) för {label}: {:?} (ignoreras)",
                r2
            );
        }
    }
    #[cfg(not(windows))]
    let _ = (win, label);
}

fn emit_event<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::debug!("emit '{event}' misslyckades: {e}");
    }
}

/// Emittar ett `action_llm_token`-event och sätter streaming-flaggan true.
/// Flaggan hindrar click-outside-hide under streaming.
fn emit_action_token(app: &AppHandle, text: String) {
    svoice_ipc::mark_action_streaming();
    emit_event(app, EV_ACTION_LLM_TOKEN, ActionToken { text });
}

/// Emittar `action_llm_done` och schemalägger clear av streaming-flaggan
/// 500 ms senare (grace-period så user inte råkar stänga popupen direkt).
fn emit_action_done(app: &AppHandle) {
    emit_event(app, EV_ACTION_LLM_DONE, ());
    svoice_ipc::schedule_action_streaming_clear();
}

/// Visa main-fönstret (Settings) och ge det fokus. Anropas från tray-menyn
/// eller vid vänsterklick på tray-ikonen.
fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
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
) {
    let mut meter: Option<VolumeMeter> = None;

    for ev in rx {
        // Simultan-PTT-lockout: ignorera dictation Pressed om action redan äger.
        if ev == LlKeyEvent::Pressed
            && PTT_OWNER
                .compare_exchange(
                    OWNER_NONE,
                    OWNER_DICTATION,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_err()
        {
            tracing::debug!("dictation-PTT: skippas — action-PTT äger just nu");
            continue;
        }
        // Bara hantera Released om vi faktiskt äger låset.
        if ev == LlKeyEvent::Released && PTT_OWNER.load(Ordering::SeqCst) != OWNER_DICTATION {
            continue;
        }

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
            // Hot-reload settings så alla ändringar träder i kraft utan restart.
            let current = Settings::load();
            if !current.stt_enabled {
                tracing::info!("STT avstängd — hoppar över transkribering");
            } else {
                perform_transcribe_and_inject(&ring, &stt, &rt, &current);
            }

            let final_state = {
                let mut m = ptt_lock(&ptt);
                m.on_finish_processing();
                m.state()
            };
            emit_event(&app_handle, EV_PTT_STATE, final_state);
            update_tray_for_state(&app_handle, final_state);
            // Släpp låset.
            PTT_OWNER.store(OWNER_NONE, Ordering::SeqCst);
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
    settings: &Settings,
) {
    let audio = ring.drain();
    let (start, end) = trim_silence(&audio, 16000, settings.vad_threshold);
    let segment = &audio[start..end];
    if segment.is_empty() {
        tracing::warn!("inget tal detekterat (VAD trimmade allt)");
        return;
    }
    let raw_text = match rt.block_on(transcribe_dispatch(settings, stt, segment)) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("STT-fel: {e}");
            // (STT-fel syns inte som toast — för ofta trivialt som "för kort tal".
            //  Action-popup-fel toast:as i action-worker-loopen.)
            return;
        }
    };
    if raw_text.is_empty() {
        tracing::warn!("STT returnerade tom text");
        return;
    }

    // LLM-polering om aktiverad i settings. Använder samma provider-
    // selection som action-popup.
    let final_text = if settings.llm_polish_dictation {
        match rt.block_on(polish_transcript(&raw_text, settings)) {
            Ok(polished) => {
                tracing::info!("LLM-polering: \"{}\" → \"{}\"", raw_text, polished);
                polished
            }
            Err(e) => {
                tracing::warn!("LLM-polering misslyckades ({e}), injectar råtext");
                raw_text
            }
        }
    } else {
        raw_text
    };

    match inject(&final_text) {
        Ok(method) => {
            let method_str = match method {
                InjectMethod::SendInput => "send_input",
                InjectMethod::Clipboard => "clipboard",
            };
            tracing::info!("inject OK via {method_str}: \"{}\"", final_text);
        }
        Err(e) => tracing::error!("inject FAIL: {e}"),
    }
}

/// Använd vald LLM-provider för att snabbpolera en STT-transkription
/// (grammatik, stavning, interpunktion). Returnerar den polerade texten
/// eller error om ingen provider är konfigurerad/når fram.
async fn polish_transcript(raw: &str, settings: &Settings) -> anyhow::Result<String> {
    use futures_util::StreamExt;
    let anthropic_key = svoice_secrets::get_anthropic_key().ok().flatten();
    // Dikterings-polering använder explicit `dictation_llm_provider` så user
    // kan köra t.ex. snabb+billig Groq här och Claude för action-popup.
    let llm = select_llm_provider(
        settings.dictation_llm_provider,
        settings,
        anthropic_key.as_deref(),
    )
    .await
    .ok_or_else(|| anyhow::anyhow!("ingen LLM-provider konfigurerad för polering"))?;
    let req = LlmRequest {
        system: Some(
            "Du är en svensk grammatik- och interpunktions-korrigerare. \
Du får en rå transcription från tal-till-text. Returnera den korrigerade versionen — \
fixa grammatik, kommatering, saknade punkter, ord som låter lika men stavas olika. \
Ändra INTE innebörden. Lägg INTE till eller ta bort info. Bara rätta. \
Returnera BARA den korrigerade texten, inga förklaringar."
                .into(),
        ),
        turns: vec![TurnContent {
            role: Role::User,
            text: raw.to_string(),
        }],
        temperature: 0.1,
        max_tokens: 512,
    };
    let mut stream = llm.complete_stream(req).await?;
    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        out.push_str(&chunk?);
    }
    Ok(out.trim().to_string())
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
) {
    for ev in rx {
        match ev {
            LlKeyEvent::Pressed => {
                // Simultan-PTT-lockout: skippa om dictation äger just nu.
                if PTT_OWNER
                    .compare_exchange(OWNER_NONE, OWNER_ACTION, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    tracing::debug!("action-PTT: skippas — dictation-PTT äger just nu");
                    continue;
                }

                // Om popup redan är synlig: detta är en follow-up. Popupen själv
                // är vår egen app, så remember_foreground_target returnerar false.
                // Vi ska då INTE skippa — target-HWND från föregående turn
                // ligger kvar i TARGET_HWND och återanvänds vid paste.
                let popup_already_visible = app_handle
                    .get_webview_window("action-popup")
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);

                if !popup_already_visible {
                    // Fresh session: spara target-HWND medan target-appen har
                    // fokus. Skippas om target är vår egen app (analogt med
                    // palette-hotkey).
                    if !remember_foreground_target() {
                        tracing::debug!("action-PTT: target är vår egen app, skippar");
                        PTT_OWNER.store(OWNER_NONE, Ordering::SeqCst);
                        continue;
                    }
                }
                ring.clear();
                tracing::debug!(
                    "action-PTT: recording (follow_up={})",
                    popup_already_visible
                );
                emit_event(&app_handle, EV_PTT_STATE, PttState::Recording);
                update_tray_for_state(&app_handle, PttState::Recording);
            }
            LlKeyEvent::Released => {
                // Bara hantera Released om vi är ägare.
                if PTT_OWNER.load(Ordering::SeqCst) != OWNER_ACTION {
                    continue;
                }
                // Hot-reload settings — kolla action-LLM-toggle först.
                let current = Settings::load();
                if !current.action_llm_enabled {
                    tracing::info!("Action-LLM avstängd — ignorerar Insert-release");
                    emit_event(&app_handle, EV_PTT_STATE, PttState::Idle);
                    update_tray_for_state(&app_handle, PttState::Idle);
                    PTT_OWNER.store(OWNER_NONE, Ordering::SeqCst);
                    continue;
                }
                tracing::debug!("action-PTT: released, processing...");
                emit_event(&app_handle, EV_PTT_STATE, PttState::Processing);
                update_tray_for_state(&app_handle, PttState::Processing);

                // Follow-up-detection: om popupen fortfarande är synlig OCH
                // vi har en aktiv konversation i state, tolka nya Insert-PTT
                // som en uppföljningsfråga. Då skippas capture_selection (det
                // ger ändå fel — popupen äger fokus) och nytt user-turn läggs
                // till i existerande konversation.
                let popup_visible = app_handle
                    .get_webview_window("action-popup")
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                let has_active_conv = svoice_ipc::snapshot_conversation().is_some();
                let is_follow_up = popup_visible && has_active_conv;

                let captured_selection = if is_follow_up {
                    tracing::info!("action: follow-up turn i pågående konversation");
                    None
                } else {
                    // Fresh session — rensa ev. stale state och fånga ny selection.
                    svoice_ipc::clear_active_conversation();
                    // KRITISK ORDNING: fånga markering INNAN popup öppnas. Popup.show()
                    // stjäl fokus från target-appen, så om capture_selection körs
                    // efter stjäls Ctrl+C av popup-webviewen och inget läses.
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    let sel = match capture_selection() {
                        Ok(sel) => sel,
                        Err(e) => {
                            tracing::warn!("capture_selection misslyckades: {e}");
                            None
                        }
                    };
                    if let Some(s) = &sel {
                        tracing::info!(
                            "action: fångade selection ({} tecken)",
                            s.chars().count()
                        );
                    } else {
                        tracing::info!("action: ingen markering");
                    }
                    sel
                };

                // Öppna popup-fönstret. Redan synlig vid follow-up = no-op.
                if let Some(win) = app_handle.get_webview_window("action-popup") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
                emit_event(
                    &app_handle,
                    EV_ACTION_POPUP_OPEN,
                    ActionPopupOpen {
                        selection: captured_selection.clone(),
                        command: if is_follow_up {
                            "uppföljning…".into()
                        } else {
                            "lyssnar…".into()
                        },
                        mode: if is_follow_up {
                            "follow_up"
                        } else if captured_selection
                            .as_ref()
                            .map_or(false, |s| !s.trim().is_empty())
                        {
                            "transform"
                        } else {
                            "query"
                        },
                    },
                );

                // Bygg LLM-provider från den settings vi redan laddade ovan.
                let anthropic_key = svoice_secrets::get_anthropic_key().ok().flatten();
                let llm = rt.block_on(select_llm_provider(
                    current.action_llm_provider,
                    &current,
                    anthropic_key.as_deref(),
                ));

                if let Err(e) = handle_action_released(
                    &app_handle,
                    &ring,
                    &stt,
                    &rt,
                    &llm,
                    &current,
                    captured_selection,
                    is_follow_up,
                ) {
                    tracing::error!("action-PTT fel: {e}");
                    emit_event(
                        &app_handle,
                        EV_ACTION_LLM_ERROR,
                        ActionError {
                            message: e.to_string(),
                        },
                    );
                    emit_error_toast(&app_handle, "Action-PTT misslyckades", &e.to_string());
                }
                emit_event(&app_handle, EV_PTT_STATE, PttState::Idle);
                update_tray_for_state(&app_handle, PttState::Idle);
                // Släpp låset.
                PTT_OWNER.store(OWNER_NONE, Ordering::SeqCst);
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
    settings: &Settings,
    selection: Option<String>,
    is_follow_up: bool,
) -> anyhow::Result<()> {
    let vad_threshold = settings.vad_threshold;

    // Transkribera user's röstkommando.
    let audio = ring.drain();
    let (start, end) = trim_silence(&audio, 16000, vad_threshold);
    let segment = &audio[start..end];
    if segment.is_empty() {
        anyhow::bail!("inget röstkommando detekterat");
    }
    let command = rt.block_on(transcribe_dispatch(settings, stt, segment))?;
    let command = command.trim().to_string();
    if command.is_empty() {
        anyhow::bail!("STT returnerade tom text");
    }
    tracing::info!("action: command = \"{}\"", command);

    // Bestäm mode. Vid follow-up använder vi samma mode som original-
    // konversationen (vilket redan är inbakat i dess turns); vi räknar
    // det som "query" för agentic-triggers-beslut men bygger LLM-req
    // från stored conversation istället för scratch.
    let mode: &'static str = if is_follow_up {
        "query"
    } else if selection.as_ref().map_or(false, |s| !s.trim().is_empty()) {
        "transform"
    } else {
        "query"
    };

    // Emit popup-open-event med riktig command-text. Popup redan synlig
    // (action_worker_loop öppnade den), bara uppdatera innehåll.
    emit_event(
        app_handle,
        EV_ACTION_POPUP_OPEN,
        ActionPopupOpen {
            selection: selection.clone(),
            command: command.clone(),
            mode: if is_follow_up { "follow_up" } else { mode },
        },
    );

    // Gemini-agentic-path: om user valt Gemini som action-provider, kör med
    // Google Search-grounding istället för Claude's agentic flow. Skarpare
    // på realtidsdata eftersom Gemini gör sökningen inbyggt och lägger
    // käll-URL:er på svaret via `groundingMetadata`.
    let use_gemini_agentic = !is_follow_up
        && mode == "query"
        && settings.action_llm_provider == ProviderChoice::Gemini;
    if use_gemini_agentic {
        if let Some(key) = svoice_secrets::get_gemini_key().ok().flatten() {
            tracing::info!("Gemini agentic flow triggas för command: \"{}\"", command);
            // Spara en "tom" konversation så follow-up efter Gemini-agentic
            // kan bygga vidare som fri text (Gemini-svar blir assistant-turn).
            svoice_ipc::set_active_conversation(svoice_ipc::ActiveConversation {
                system: None,
                selection: selection.clone(),
                turns: vec![TurnContent {
                    role: Role::User,
                    text: command.clone(),
                }],
                mode,
            });
            let app_clone = app_handle.clone();
            let command_clone = command.clone();
            let model_clone = settings.gemini_model.clone();
            rt.spawn(async move {
                if let Err(e) = agentic::run_agentic_gemini(
                    &app_clone,
                    &command_clone,
                    key,
                    model_clone,
                    EV_ACTION_LLM_TOKEN,
                    EV_ACTION_LLM_DONE,
                )
                .await
                {
                    tracing::error!("Gemini agentic flow fel: {e}");
                    emit_event(
                        &app_clone,
                        EV_ACTION_LLM_ERROR,
                        ActionError {
                            message: format!("Gemini: {e}"),
                        },
                    );
                    svoice_ipc::clear_action_streaming();
                }
            });
            return Ok(());
        } else {
            tracing::warn!(
                "Gemini vald som action-provider men nyckel saknas — faller tillbaka till Claude-agentic/standard"
            );
        }
    }

    // Agentic path — alltid för NY session i query-mode om Anthropic-nyckel
    // finns. Claude avgör själv via sin system-prompt om verktyg (web_search,
    // calendar, gmail) behövs. Tidigare rule-based `looks_agentic`-heuristik
    // var för spröd: svenska böjningar kastar om bokstavsordning (t.ex.
    // "vädret" innehåller inte substrängen "väder" eftersom E och R är
    // transponerade) så keyword-matching missade uppenbart-agentiska frågor.
    // Extra tool-definitions kostar ~200 tokens per request — försumbart.
    let prep = agentic::prepare_agentic(settings);
    tracing::info!(
        "agentic-gate: follow_up={} mode={} api_key_ok={}",
        is_follow_up,
        mode,
        prep.is_some()
    );
    if !is_follow_up && mode == "query" && prep.is_some() {
        let req = prep.expect("prep is_some just checked");
        {
            tracing::info!("agentic flow triggas för command: \"{}\"", command);
            // Spara en "tom" konversation så follow-up efter agentic kan
            // bygga vidare som fri text (agentic-svar blir assistant-turn).
            svoice_ipc::set_active_conversation(svoice_ipc::ActiveConversation {
                system: None,
                selection: selection.clone(),
                turns: vec![TurnContent {
                    role: Role::User,
                    text: command.clone(),
                }],
                mode,
            });
            let app_clone = app_handle.clone();
            let command_clone = command.clone();
            rt.spawn(async move {
                if let Err(e) = agentic::run_agentic(
                    &app_clone,
                    &command_clone,
                    req,
                    EV_ACTION_LLM_TOKEN,
                    EV_ACTION_LLM_DONE,
                )
                .await
                {
                    tracing::error!("agentic flow fel: {e}");
                    emit_event(
                        &app_clone,
                        EV_ACTION_LLM_ERROR,
                        ActionError {
                            message: format!("agentic: {e}"),
                        },
                    );
                    svoice_ipc::clear_action_streaming();
                }
            });
            return Ok(());
        }
    }

    // Vanlig streaming-path.
    let Some(llm) = llm.clone() else {
        emit_event(
            app_handle,
            EV_ACTION_LLM_ERROR,
            ActionError {
                message:
                    "Ingen LLM-nyckel konfigurerad. Lägg till anthropic_api_key i inställningarna."
                        .into(),
            },
        );
        return Ok(());
    };

    // Bygg LLM-request. Follow-up återanvänder hela lagrade konversationen +
    // appenderar nytt user-turn; ny session bygger turns via build_llm_request.
    let llm_req = if is_follow_up {
        // Append user-turn till stored conversation och hämta snapshot för LLM.
        svoice_ipc::append_user_turn(command.clone());
        let (system, turns) = svoice_ipc::snapshot_conversation().ok_or_else(|| {
            anyhow::anyhow!("follow-up utan aktiv konversation (race?)")
        })?;
        LlmRequest {
            system,
            turns,
            temperature: 0.3,
            max_tokens: 1024,
        }
    } else {
        let fresh = build_llm_request(mode, selection.as_deref(), &command);
        // Spara i state så nästa Insert-PTT (follow-up) kan bygga vidare.
        svoice_ipc::set_active_conversation(svoice_ipc::ActiveConversation {
            system: fresh.system.clone(),
            selection: selection.clone(),
            turns: fresh.turns.clone(),
            mode,
        });
        fresh
    };

    let app_clone = app_handle.clone();
    let rt_clone = rt.clone();
    rt.spawn(async move {
        let mut assistant_accum = String::new();
        match llm.complete_stream(llm_req).await {
            Ok(mut stream) => {
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => {
                            assistant_accum.push_str(&text);
                            emit_action_token(&app_clone, text);
                        }
                        Err(e) => {
                            emit_event(
                                &app_clone,
                                EV_ACTION_LLM_ERROR,
                                ActionError {
                                    message: e.to_string(),
                                },
                            );
                            svoice_ipc::clear_action_streaming();
                            return;
                        }
                    }
                }
                // Spara assistant-turn så nästa follow-up ser hela
                // konversationen inkl. detta svar.
                if !assistant_accum.is_empty() {
                    svoice_ipc::append_assistant_turn(assistant_accum);
                }
                emit_action_done(&app_clone);
            }
            Err(e) => {
                emit_event(
                    &app_clone,
                    EV_ACTION_LLM_ERROR,
                    ActionError {
                        message: e.to_string(),
                    },
                );
                svoice_ipc::clear_action_streaming();
            }
        }
        let _ = rt_clone;
    });

    Ok(())
}

/// Väljer LLM-provider baserat på `choice` + settings. Auto prövar Ollama
/// först (snabb health-check mot localhost:11434) och fallback till
/// Anthropic. Uttryckligt val (Claude/Ollama/Groq) gör ingen fallback.
/// `choice` passeras explicit så caller kan använda olika providers för
/// diktering vs action-popup (settings har två separata fält).
async fn select_llm_provider(
    choice: ProviderChoice,
    settings: &Settings,
    anthropic_key: Option<&str>,
) -> Option<Arc<dyn LlmProvider>> {
    let build_anthropic = || -> Option<Arc<dyn LlmProvider>> {
        anthropic_key.filter(|k| !k.is_empty()).map(|key| {
            Arc::new(
                AnthropicClient::new(key.to_string()).with_model(settings.anthropic_model.clone()),
            ) as Arc<dyn LlmProvider>
        })
    };
    let build_ollama = || -> Arc<dyn LlmProvider> {
        Arc::new(
            OllamaClient::new(settings.ollama_model.clone())
                .with_base_url(settings.ollama_url.clone()),
        )
    };
    let build_groq = || -> Option<Arc<dyn LlmProvider>> {
        svoice_secrets::get_groq_key()
            .ok()
            .flatten()
            .filter(|k| !k.is_empty())
            .map(|key| {
                Arc::new(GroqClient::new(key).with_model(settings.groq_llm_model.clone()))
                    as Arc<dyn LlmProvider>
            })
    };
    let build_gemini = || -> Option<Arc<dyn LlmProvider>> {
        svoice_secrets::get_gemini_key()
            .ok()
            .flatten()
            .filter(|k| !k.is_empty())
            .map(|key| {
                Arc::new(GeminiClient::new(key).with_model(settings.gemini_model.clone()))
                    as Arc<dyn LlmProvider>
            })
    };

    match choice {
        ProviderChoice::Claude => build_anthropic(),
        ProviderChoice::Ollama => Some(build_ollama()),
        ProviderChoice::Groq => build_groq(),
        ProviderChoice::Gemini => build_gemini(),
        ProviderChoice::Auto => {
            let ollama = OllamaClient::new(settings.ollama_model.clone())
                .with_base_url(settings.ollama_url.clone());
            if ollama.is_healthy().await {
                tracing::info!(
                    "action-LLM: använder lokal Ollama ({})",
                    settings.ollama_model
                );
                Some(Arc::new(ollama))
            } else if let Some(g) = build_groq() {
                tracing::info!("action-LLM: Ollama otillgänglig, använder Groq");
                Some(g)
            } else if let Some(g) = build_gemini() {
                tracing::info!("action-LLM: Ollama + Groq otillgängliga, använder Gemini");
                Some(g)
            } else {
                tracing::info!(
                    "action-LLM: Ollama/Groq/Gemini otillgängliga, faller tillbaka till Anthropic"
                );
                build_anthropic()
            }
        }
    }
}

/// Transkribera via vald STT-provider. För Groq krävs API-nyckel i keyring —
/// saknas den (eller om API-call failar) faller vi tillbaka till lokal STT.
async fn transcribe_dispatch(
    settings: &Settings,
    local: &PythonStt,
    audio: &[f32],
) -> anyhow::Result<String> {
    match settings.stt_provider {
        SttProvider::Local => Ok(local.transcribe(audio).await?),
        SttProvider::Groq => {
            let Some(key) = svoice_secrets::get_groq_key()
                .ok()
                .flatten()
                .filter(|k| !k.is_empty())
            else {
                tracing::warn!("Groq STT vald men API-nyckel saknas — faller tillbaka till lokal");
                return Ok(local.transcribe(audio).await?);
            };
            let client = GroqStt::new(key)
                .with_model(settings.groq_stt_model.clone())
                .with_language(settings.stt_language.clone());
            match client.transcribe(audio).await {
                Ok(text) => Ok(text),
                Err(e) => {
                    tracing::error!("Groq STT fel: {e} — faller tillbaka till lokal");
                    Ok(local.transcribe(audio).await?)
                }
            }
        }
    }
}

/// Stäng palette-fönstret från backend. Mer pålitligt än frontend
/// `webview.hide()` som ibland lämnar kvar ett synligt svart window.
#[tauri::command]
fn palette_close(app: AppHandle) {
    if let Some(win) = app.get_webview_window("palette") {
        let _ = win.hide();
    }
}

/// Tauri-command: kör en smart-function by id. Triggas när user väljer en
/// function i command palette. Flow:
/// 1. Hämta function från disk.
/// 2. Använd selection från PALETTE_SELECTION (fångad vid hotkey-press).
/// 3. Bygg LlmRequest med function's system + interpolated user_template.
/// 4. Göm palette, öppna action-popup.
/// 5. Streama tokens. User trycker Enter → paste till target-HWND.
#[tauri::command]
async fn run_smart_function(app: AppHandle, id: String) -> Result<(), String> {
    // 1. Hitta function.
    let fns = svoice_smart_functions::list()
        .map_err(|e| format!("kunde inte läsa smart-functions: {e}"))?;
    let sf = fns
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| format!("okänd smart-function: {id}"))?;

    // 2. Läs selection + kontrollera mode-krav.
    let selection = PALETTE_SELECTION
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .filter(|s| !s.is_empty());

    // 3. Göm palette (om den fortfarande syns).
    if let Some(win) = app.get_webview_window("palette") {
        let _ = win.hide();
    }

    // 4. Öppna action-popup TIDIGT så ev. fel syns där (user ser "laddar…" →
    //    ersatt med error eller tokens).
    if let Some(win) = app.get_webview_window("action-popup") {
        let _ = win.show();
        let _ = win.set_focus();
    }
    let mode = match sf.mode {
        svoice_smart_functions::SmartMode::Transform => "transform",
        svoice_smart_functions::SmartMode::Query => "query",
    };
    let _ = app.emit(
        EV_ACTION_POPUP_OPEN,
        ActionPopupOpen {
            selection: selection.clone(),
            command: sf.name.clone(),
            mode,
        },
    );

    // 5. Verifiera mode-krav (efter popup öppnad så error syns där).
    if sf.mode == svoice_smart_functions::SmartMode::Transform && selection.is_none() {
        let msg = "Markera text innan du kör en transform-function.".to_string();
        let _ = app.emit(EV_ACTION_LLM_ERROR, ActionError { message: msg.clone() });
        return Err(msg);
    }

    // 6. Bygg LLM-provider.
    let settings = Settings::load();
    let anthropic_key = svoice_secrets::get_anthropic_key().ok().flatten();
    let llm = match select_llm_provider(
        settings.action_llm_provider,
        &settings,
        anthropic_key.as_deref(),
    )
    .await
    {
        Some(l) => l,
        None => {
            let msg = "Ingen LLM-provider konfigurerad. Lägg till API-nyckel (Claude/Groq) eller starta Ollama.".to_string();
            let _ = app.emit(EV_ACTION_LLM_ERROR, ActionError { message: msg.clone() });
            emit_error_toast(&app, "Smart-function misslyckades", &msg);
            return Err(msg);
        }
    };

    // 7. Bygg prompt.
    let user_msg =
        svoice_smart_functions::build_user_prompt(&sf.user_template, selection.as_deref(), None);
    let req = LlmRequest {
        system: Some(sf.system.clone()),
        turns: vec![TurnContent {
            role: Role::User,
            text: user_msg,
        }],
        temperature: 0.3,
        max_tokens: 1024,
    };

    // 7. Streama tokens i bakgrunden. Använd en ny tokio-runtime eftersom
    //    vi inte har tillgång till den delade rt här. Billigt för en request.
    let app_clone = app.clone();
    tokio::spawn(async move {
        match llm.complete_stream(req).await {
            Ok(mut stream) => {
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => {
                            emit_action_token(&app_clone, text);
                        }
                        Err(e) => {
                            let _ = app_clone.emit(
                                EV_ACTION_LLM_ERROR,
                                ActionError {
                                    message: e.to_string(),
                                },
                            );
                            svoice_ipc::clear_action_streaming();
                            return;
                        }
                    }
                }
                emit_action_done(&app_clone);
            }
            Err(e) => {
                let _ = app_clone.emit(
                    EV_ACTION_LLM_ERROR,
                    ActionError {
                        message: e.to_string(),
                    },
                );
                svoice_ipc::clear_action_streaming();
            }
        }
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
