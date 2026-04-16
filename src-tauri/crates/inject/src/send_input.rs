use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY,
};

/// Antal UTF-16 code units som skickas per SendInput-anrop. Större batchar
/// skriver ut texten snabbare, men Windows input queue rate-limitar stora
/// batchar (~40+ events → tappade tecken). 5 code units (10 events) ger
/// bra balans: ~5× snabbare än 1-i-taget, fortfarande stabilt.
const CHARS_PER_BATCH: usize = 5;

/// Paus mellan SendInput-batchar. Låter Windows input queue flusha.
const INTER_BATCH_DELAY: Duration = Duration::from_millis(5);

#[derive(Debug, thiserror::Error)]
pub enum SendInputError {
    #[error("SendInput misslyckades vid batch {batch_index} (first code unit 0x{unit:04X}, sent={sent}/{total}, GetLastError=0x{err:X})")]
    PartialSend {
        batch_index: usize,
        unit: u16,
        sent: u32,
        total: u32,
        err: u32,
    },
    #[error("SendInput returnerade 0 för tom text — detta bör inte hända")]
    EmptyText,
}

/// Skriver Unicode-text via SendInput med KEYEVENTF_UNICODE.
/// Skickar tecken i små batchar (se CHARS_PER_BATCH) för att undvika Windows
/// input-queue-rate-limiting som visar sig som tappade/repeterande tecken.
pub fn send_unicode(text: &str) -> Result<(), SendInputError> {
    if text.is_empty() {
        return Err(SendInputError::EmptyText);
    }

    let code_units: Vec<u16> = text.encode_utf16().collect();

    for (batch_index, chunk) in code_units.chunks(CHARS_PER_BATCH).enumerate() {
        let mut inputs: Vec<INPUT> = Vec::with_capacity(chunk.len() * 2);
        for unit in chunk {
            inputs.push(make_keyboard_input(*unit, KEYEVENTF_UNICODE));
            inputs.push(make_keyboard_input(
                *unit,
                KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
            ));
        }
        let total = inputs.len() as u32;
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        tracing::trace!(
            "inject batch {}: {} units, sent={}/{}",
            batch_index,
            chunk.len(),
            sent,
            total
        );
        if sent != total {
            let err = unsafe { GetLastError().0 };
            return Err(SendInputError::PartialSend {
                batch_index,
                unit: *chunk.first().unwrap_or(&0),
                sent,
                total,
                err,
            });
        }
        sleep(INTER_BATCH_DELAY);
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
