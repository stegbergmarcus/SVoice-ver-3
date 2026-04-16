//! LowLevelKeyboardHook för att fånga enskilda tangenter (t.ex. RightCtrl) som
//! push-to-talk. `tauri-plugin-global-shortcut` stödjer bara modifier+key, så för
//! hold-to-talk på en enskild tangent behöver vi en Windows-specifik keyboard hook.
//!
//! Begränsningar:
//! - Endast en hook åt gången (global statiskt state).
//! - Måste installeras från en tråd med aktiv message loop (Tauri main thread OK).
//! - Key-repeat-events ignoreras via atomic-flagga så vi bara triggar *transitioner*.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_RCONTROL;
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

#[derive(Debug, thiserror::Error)]
pub enum LlHookError {
    #[error("SetWindowsHookExW misslyckades: {0}")]
    InstallFailed(String),
    #[error("en hook är redan installerad")]
    AlreadyInstalled,
}

// Global callback-slot. OnceLock+Mutex låter oss initiera vid install och
// hook_proc kan nå den utan att vara closure.
static CALLBACK: OnceLock<Mutex<Option<LlCallback>>> = OnceLock::new();
static HOOK_HANDLE: OnceLock<Mutex<Option<HookHandle>>> = OnceLock::new();
static RCTRL_IS_DOWN: AtomicBool = AtomicBool::new(false);

fn callback_slot() -> &'static Mutex<Option<LlCallback>> {
    CALLBACK.get_or_init(|| Mutex::new(None))
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
        if kb.vkCode == VK_RCONTROL.0 as u32 {
            let msg = w_param.0 as u32;
            if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                if !RCTRL_IS_DOWN.swap(true, Ordering::SeqCst) {
                    if let Some(cb) = callback_slot().lock().unwrap().as_ref() {
                        cb(LlKeyEvent::Pressed);
                    }
                }
            } else if msg == WM_KEYUP || msg == WM_SYSKEYUP {
                if RCTRL_IS_DOWN.swap(false, Ordering::SeqCst) {
                    if let Some(cb) = callback_slot().lock().unwrap().as_ref() {
                        cb(LlKeyEvent::Released);
                    }
                }
            }
            // Konsumera höger Ctrl så fokuserat fönster aldrig ser "Ctrl är
            // nedtryckt". Annars blir vår SendInject-text tolkad som Ctrl+<char>
            // medan användaren håller tangenten, och inject avbryts mitt i.
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

/// Installerar en LowLevelKeyboardHook som triggar `callback` vid RightCtrl-
/// transitioner (press/release). Auto-repeats filtreras bort.
pub fn install_rctrl_hook(callback: LlCallback) -> Result<(), LlHookError> {
    {
        let mut slot = hook_handle_slot().lock().unwrap();
        if slot.is_some() {
            return Err(LlHookError::AlreadyInstalled);
        }
        *callback_slot().lock().unwrap() = Some(callback);
        let hhook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
            .map_err(|e| LlHookError::InstallFailed(e.to_string()))?;
        *slot = Some(HookHandle(hhook.0 as isize));
    }
    tracing::info!("LowLevelKeyboardHook installerad för RightCtrl (PTT)");
    Ok(())
}

/// Avinstallerar hooken. Anropa från samma tråd som installerade den.
pub fn uninstall_rctrl_hook() {
    if let Some(h) = hook_handle_slot().lock().unwrap().take() {
        unsafe {
            let hh = HHOOK(h.0 as *mut core::ffi::c_void);
            let _ = UnhookWindowsHookEx(hh);
        }
    }
    *callback_slot().lock().unwrap() = None;
    RCTRL_IS_DOWN.store(false, Ordering::SeqCst);
    tracing::info!("LowLevelKeyboardHook avinstallerad");
}
