pub mod ptt_state;
pub mod register;

#[cfg(windows)]
pub mod ll_hook;

pub use ptt_state::{PttEvent, PttMachine, PttState};
pub use register::{is_key_down, register_ptt, HotkeyCallback, HotkeyError, RegisteredHotkey};

#[cfg(windows)]
pub use ll_hook::{install_rctrl_hook, uninstall_rctrl_hook, LlCallback, LlHookError, LlKeyEvent};
