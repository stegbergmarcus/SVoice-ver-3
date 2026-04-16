use std::mem::size_of;

use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY,
};

#[derive(Debug, thiserror::Error)]
pub enum SendInputError {
    #[error("SendInput misslyckades vid index {index} ({sent}/{total} events skickade, GetLastError=0x{err:X})")]
    PartialSend {
        index: usize,
        sent: u32,
        total: u32,
        err: u32,
    },
    #[error("SendInput returnerade 0 för tom text — detta bör inte hända")]
    EmptyText,
}

/// Skriver Unicode-text via SendInput med KEYEVENTF_UNICODE.
/// Varje kodpunkt expanderas till UTF-16 code units, och varje code unit skickas
/// som ett key-down+key-up-par.
pub fn send_unicode(text: &str) -> Result<(), SendInputError> {
    if text.is_empty() {
        return Err(SendInputError::EmptyText);
    }

    let code_units: Vec<u16> = text.encode_utf16().collect();
    let mut inputs: Vec<INPUT> = Vec::with_capacity(code_units.len() * 2);

    for unit in &code_units {
        inputs.push(make_keyboard_input(*unit, KEYEVENTF_UNICODE));
        inputs.push(make_keyboard_input(
            *unit,
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        ));
    }

    let total = inputs.len() as u32;
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };

    if sent != total {
        let err = unsafe { GetLastError().0 };
        return Err(SendInputError::PartialSend {
            index: sent as usize,
            sent,
            total,
            err,
        });
    }

    Ok(())
}

fn make_keyboard_input(wscan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: wscan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_returns_error() {
        assert!(matches!(send_unicode(""), Err(SendInputError::EmptyText)));
    }
}
