//! LowLevelKeyboardHook för att fånga enskilda tangenter som push-to-talk.
//! `tauri-plugin-global-shortcut` stödjer bara modifier+key, så för hold-to-talk
//! på en enskild tangent behöver vi en Windows-specifik keyboard hook.
//!
//! Windows tillåter bara EN LowLevelKeyboardHook per process, så vi multiplexar
//! flera target-keys genom samma hook. Varje key har egen tracker-state och
//! egen callback.
//!
//! Begränsningar:
//! - Måste installeras från en tråd med aktiv message loop (Tauri main thread OK).
//! - Key-repeat-events ignoreras via atomic-flagga så vi bara triggar *transitioner*.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_CAPITAL, VK_END, VK_F12, VK_HOME, VK_INSERT, VK_PAUSE, VK_RCONTROL, VK_RMENU, VK_SCROLL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub type LlCallback = Arc<dyn Fn(LlKeyEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlKeyEvent {
    Pressed,
    Released,
}

/// Vilken target-tangent ett callback ska lyssna på.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotKey {
    /// Höger Ctrl — diktering (default).
    RightCtrl,
    /// Insert — action-LLM popup (default).
    Insert,
    /// Höger Alt.
    RightAlt,
    /// F12.
    F12,
    /// Pause/Break.
    Pause,
    /// Scroll Lock.
    ScrollLock,
    /// Caps Lock.
    CapsLock,
    /// Home.
    Home,
    /// End.
    End,
}

impl HotKey {
    pub fn vk_code(self) -> u32 {
        match self {
            HotKey::RightCtrl => VK_RCONTROL.0 as u32,
            HotKey::Insert => VK_INSERT.0 as u32,
            HotKey::RightAlt => VK_RMENU.0 as u32,
            HotKey::F12 => VK_F12.0 as u32,
            HotKey::Pause => VK_PAUSE.0 as u32,
            HotKey::ScrollLock => VK_SCROLL.0 as u32,
            HotKey::CapsLock => VK_CAPITAL.0 as u32,
            HotKey::Home => VK_HOME.0 as u32,
            HotKey::End => VK_END.0 as u32,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlHookError {
    #[error("SetWindowsHookExW misslyckades: {0}")]
    InstallFailed(String),
    #[error("en hook är redan installerad")]
    AlreadyInstalled,
}

struct HookState {
    /// key → (is_down-flagga, callback)
    registered: HashMap<u32, KeyEntry>,
}

struct KeyEntry {
    is_down: AtomicBool,
    callback: LlCallback,
}

// Global state. Delas mellan install/uninstall/hook_proc.
static STATE: OnceLock<Mutex<HookState>> = OnceLock::new();
static HOOK_HANDLE: OnceLock<Mutex<Option<HookHandle>>> = OnceLock::new();

fn state_slot() -> &'static Mutex<HookState> {
    STATE.get_or_init(|| {
        Mutex::new(HookState {
            registered: HashMap::new(),
        })
    })
}

fn hook_handle_slot() -> &'static Mutex<Option<HookHandle>> {
    HOOK_HANDLE.get_or_init(|| Mutex::new(None))
}

// HHOOK är en pointer-wrapper. Vi lagrar som isize för Send-safety.
struct HookHandle(isize);
// SAFETY: HHOOK ägs exklusivt av hook-systemet; vi tappar den bara vid unhook.
unsafe impl Send for HookHandle {}

unsafe extern "system" fn hook_proc(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode;
        let msg = w_param.0 as u32;
        let pressed = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let released = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if pressed || released {
            // Bestäm om denna key är registrerad och om vi ska fire callback.
            // VIKTIGT: vi måste ALLTID konsumera registrerade keys (LRESULT 1),
            // inklusive key-repeats, annars ser target-fönstret repeat-events
            // och tror tangenten är fast nedtryckt → Ctrl/modifier-state blir
            // korrupt efter release.
            let outcome = {
                let state = state_slot().lock().unwrap();
                state.registered.get(&vk).map(|entry| {
                    if pressed {
                        // Fire callback bara vid initial keydown, inte vid repeats.
                        let initial = !entry.is_down.swap(true, Ordering::SeqCst);
                        (initial.then(|| entry.callback.clone()), LlKeyEvent::Pressed)
                    } else {
                        let was_down = entry.is_down.swap(false, Ordering::SeqCst);
                        (was_down.then(|| entry.callback.clone()), LlKeyEvent::Released)
                    }
                })
            };
            if let Some((maybe_cb, ev)) = outcome {
                if let Some(cb) = maybe_cb {
                    cb(ev);
                }
                // Konsumera eventet OAVSETT om callback firade — så target
                // aldrig ser repeats eller osymmetriska down/up-events.
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

/// Säkerställer att global hook är installerad. Idempotent.
fn ensure_hook_installed() -> Result<(), LlHookError> {
    let mut slot = hook_handle_slot().lock().unwrap();
    if slot.is_some() {
        return Ok(());
    }
    let hhook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
        .map_err(|e| LlHookError::InstallFailed(e.to_string()))?;
    *slot = Some(HookHandle(hhook.0 as isize));
    tracing::info!("LowLevelKeyboardHook installerad (multi-key)");
    Ok(())
}

/// Registrera `callback` för `key`. Ersätter existerande registration om en finns.
pub fn register_hotkey(key: HotKey, callback: LlCallback) -> Result<(), LlHookError> {
    ensure_hook_installed()?;
    let mut state = state_slot().lock().unwrap();
    state.registered.insert(
        key.vk_code(),
        KeyEntry {
            is_down: AtomicBool::new(false),
            callback,
        },
    );
    tracing::info!("hotkey registrerad: {:?}", key);
    Ok(())
}

/// Bakåt-kompatibel wrapper — används av befintlig dikterings-kod i iter 2.
pub fn install_rctrl_hook(callback: LlCallback) -> Result<(), LlHookError> {
    register_hotkey(HotKey::RightCtrl, callback)
}

/// Avinstallerar hooken och clear:ar alla registrerade callbacks.
pub fn uninstall_hook() {
    if let Some(h) = hook_handle_slot().lock().unwrap().take() {
        unsafe {
            let hh = HHOOK(h.0 as *mut core::ffi::c_void);
            let _ = UnhookWindowsHookEx(hh);
        }
    }
    state_slot().lock().unwrap().registered.clear();
    tracing::info!("LowLevelKeyboardHook avinstallerad");
}
