use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY,
};

/// Windows input queue begränsar hur många keyboard-events den levererar per
/// batch. Stora `SendInput`-batchar (typ 80 events för 40 tecken) returnerar OK
/// men target-fönstret tappar bort events mitt i, vilket visar sig som
/// repeterande / hängande tecken. Genom att skicka en code unit åt gången med
/// en kort paus får Windows tid att flusha input queue per tecken.
const INTER_CHAR_DELAY: Duration = Duration::from_millis(5);

#[derive(Debug, thiserror::Error)]
pub enum SendInputError {
    #[error("SendInput misslyckades vid tecken {index} (code unit 0x{unit:04X}, sent={sent}/{total}, GetLastError=0x{err:X})")]
    PartialSend {
        index: usize,
        unit: u16,
        sent: u32,
        total: u32,
        err: u32,
    },
    #[error("SendInput returnerade 0 för tom text — detta bör inte hända")]
    EmptyText,
}

/// Skriver Unicode-text via SendInput med KEYEVENTF_UNICODE.
/// En UTF-16 code unit åt gången för att undvika Windows input-queue-rate-limiting.
pub fn send_unicode(text: &str) -> Result<(), SendInputError> {
    if text.is_empty() {
        return Err(SendInputError::EmptyText);
    }

    let code_units: Vec<u16> = text.encode_utf16().collect();

    for (index, unit) in code_units.iter().enumerate() {
        let inputs = [
            make_keyboard_input(*unit, KEYEVENTF_UNICODE),
            make_keyboard_input(*unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
        ];
        let total = inputs.len() as u32;
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        tracing::trace!("inject char {}: unit=0x{:04X} sent={}/{}", index, unit, sent, total);
        if sent != total {
            let err = unsafe { GetLastError().0 };
            return Err(SendInputError::PartialSend {
                index,
                unit: *unit,
                sent,
                total,
                err,
            });
        }
        sleep(INTER_CHAR_DELAY);
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
