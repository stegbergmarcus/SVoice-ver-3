pub mod ptt_state;
pub mod register;

pub use ptt_state::{PttEvent, PttMachine, PttState};
pub use register::{is_key_down, register_ptt, HotkeyCallback, HotkeyError, RegisteredHotkey};
