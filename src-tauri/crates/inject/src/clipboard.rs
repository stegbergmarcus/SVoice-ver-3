use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_V,
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
    send_ctrl_key(VK_V)
}

fn send_ctrl_c() -> Result<(), ClipboardError> {
    send_ctrl_key(VK_C)
}

fn send_ctrl_key(key: VIRTUAL_KEY) -> Result<(), ClipboardError> {
    let inputs = [
        make_vk(VK_CONTROL, false),
        make_vk(key, false),
        make_vk(key, true),
        make_vk(VK_CONTROL, true),
    ];
    let total = inputs.len() as u32;
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != total {
        return Err(ClipboardError::PasteFailed { sent, total });
    }
    Ok(())
}

/// Fångar markerad text i aktivt fönster genom Ctrl+C + clipboard-read, och
/// återställer det ursprungliga clipboard-innehållet efteråt.
///
/// Returnerar Some(selection) om text finns markerad, None om ingen markering
/// (dvs clipboard ändrades inte efter Ctrl+C).
pub fn capture_selection() -> Result<Option<String>, ClipboardError> {
    let mut cb = arboard::Clipboard::new().map_err(|e| ClipboardError::Access(e.to_string()))?;
    // Spara nuvarande clipboard-innehåll (text only — bilder bevaras inte).
    let original = cb.get_text().ok();

    // Töm clipboard så vi tydligt kan se om Ctrl+C faktiskt skriver något nytt.
    // (Vissa apps ignorerar Ctrl+C om inget är markerat, vilket gör att
    // clipboard bevaras men vi kan inte skilja "tom markering" från "samma
    // text redan i clipboard".)
    let sentinel = "\u{e000}__svoice_capture_sentinel__\u{e000}";
    let _ = cb.set_text(sentinel);
    sleep(Duration::from_millis(15));

    send_ctrl_c()?;
    // Låt Windows propagera Ctrl+C till target och att den skriver till clipboard.
    sleep(Duration::from_millis(80));

    let after = cb.get_text().ok();

    // Återställ ursprungligt clipboard (eller lämna tomt om inget fanns).
    match &original {
        Some(text) => {
            let _ = cb.set_text(text);
        }
        None => {
            // Skriv en tom sträng som kompromiss — arboard saknar "clear".
            let _ = cb.set_text("");
        }
    }

    match after {
        Some(text) if text != sentinel && !text.is_empty() => Ok(Some(text)),
        _ => Ok(None),
    }
}

/// Klistrar in `new_text` i aktivt fönster via Ctrl+V och återställer sedan
/// det ursprungliga clipboard-innehållet. Används av action-popup Enter:
/// vi vill inte permanent läcka LLM-resultat i clipboarden.
pub fn paste_and_restore(new_text: &str) -> Result<(), ClipboardError> {
    let mut cb = arboard::Clipboard::new().map_err(|e| ClipboardError::Access(e.to_string()))?;
    let original = cb.get_text().ok();

    cb.set_text(new_text)
        .map_err(|e| ClipboardError::Access(e.to_string()))?;
    sleep(Duration::from_millis(30));
    send_ctrl_v()?;

    // Vänta på att target-appen har processat paste före vi återställer,
    // annars skriver vi över clipboard medan Ctrl+V läser den.
    sleep(Duration::from_millis(120));

    if let Some(text) = original {
        let _ = cb.set_text(text);
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
