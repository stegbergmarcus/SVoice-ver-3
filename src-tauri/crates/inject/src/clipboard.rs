use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_V,
};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard-åtkomst misslyckades: {0}")]
    Access(String),
    #[error("synthesiserad Ctrl+V misslyckades (sent={sent}, total={total})")]
    PasteFailed { sent: u32, total: u32 },
}

/// Lägger texten på clipboard och skickar Ctrl+V till aktivt fönster.
/// Sparar inte tidigare clipboard — en förbättring för senare iter.
pub fn paste_via_clipboard(text: &str) -> Result<(), ClipboardError> {
    let mut cb = arboard::Clipboard::new().map_err(|e| ClipboardError::Access(e.to_string()))?;
    cb.set_text(text)
        .map_err(|e| ClipboardError::Access(e.to_string()))?;

    // Låt clipboard synka ~30ms (vissa Electron-appar läser för snabbt annars).
    sleep(Duration::from_millis(30));

    send_ctrl_v()?;
    Ok(())
}

fn send_ctrl_v() -> Result<(), ClipboardError> {
    let inputs = [
        make_vk(VK_CONTROL, false),
        make_vk(VK_V, false),
        make_vk(VK_V, true),
        make_vk(VK_CONTROL, true),
    ];
    let total = inputs.len() as u32;
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != total {
        return Err(ClipboardError::PasteFailed { sent, total });
    }
    Ok(())
}

fn make_vk(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let flags: KEYBD_EVENT_FLAGS = if key_up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn clipboard_access_works() {
        let cb = arboard::Clipboard::new();
        assert!(cb.is_ok(), "clipboard open failed: {:?}", cb.err());
    }
}
