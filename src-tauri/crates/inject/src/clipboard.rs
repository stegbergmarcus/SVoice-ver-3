use std::mem::size_of;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard-åtkomst misslyckades: {0}")]
    Access(String),
    #[error("synthesiserad Ctrl+V misslyckades (sent={sent}, total={total})")]
    PasteFailed { sent: u32, total: u32 },
}

// Global slot för senast sparade target-HWND (action-popup-flöde).
// HWND är en pointer-wrapper; lagras som isize för Send-safety.
static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);

/// Spara aktuellt foreground window som "target". Anropas vid action-PTT
/// keydown, innan popupen tar fokus. Senare restore:s via [`restore_target_focus`].
///
/// Returnerar `false` om aktuellt fönster tillhör vår egen process — i så fall
/// SKA action-PTT inte fortsätta, eftersom Ctrl+V tillbaka till vår webview
/// lämnar Windows i konstigt key-state (Ctrl-up "äts" av webview-eventloopen).
pub fn remember_foreground_target() -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return false;
    }
    // Kolla om fönstret tillhör vår process.
    let our_pid = std::process::id();
    let mut win_pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut win_pid)) };
    if win_pid == our_pid {
        tracing::debug!(
            "remember_foreground_target: fönstret är vårt eget (pid {our_pid}) — skippar"
        );
        return false;
    }
    TARGET_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    true
}

/// Återställ fokus till target-fönstret (om sparat). Returnerar true om
/// SetForegroundWindow lyckades. Används före paste_and_restore i action_apply.
pub fn restore_target_focus() -> bool {
    let raw = TARGET_HWND.load(Ordering::SeqCst);
    if raw == 0 {
        return false;
    }
    let hwnd = HWND(raw as *mut core::ffi::c_void);
    unsafe { SetForegroundWindow(hwnd).as_bool() }
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
///
/// Om ett target-HWND har sparats via [`remember_foreground_target`] så
/// restore:as fokus till det först — kritiskt för att Ctrl+V ska hamna i
/// rätt fönster när popupen har tagit fokus.
pub fn paste_and_restore(new_text: &str) -> Result<(), ClipboardError> {
    // Restaurera fokus till target INNAN vi skickar Ctrl+V. Utan detta
    // hamnar key-events i popup-webviewen och Ctrl-state kan bli "fast"
    // i Windows pga att webviewen sväljer eventen utan att propagera.
    let restored = restore_target_focus();
    tracing::debug!("paste_and_restore: focus-restore lyckades: {restored}");
    // Låt Windows processa focus-bytet innan SendInput.
    sleep(Duration::from_millis(60));

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
