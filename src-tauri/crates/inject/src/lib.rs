#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod send_input;

#[cfg(windows)]
pub use clipboard::{capture_selection, paste_and_restore, paste_via_clipboard, ClipboardError};
#[cfg(windows)]
pub use send_input::{send_unicode, SendInputError};

#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    #[cfg(windows)]
    #[error(transparent)]
    SendInput(#[from] SendInputError),
    #[cfg(windows)]
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
    #[error("båda injektionsvägarna misslyckades (send_input: {send_input}, clipboard: {clipboard})")]
    BothFailed {
        send_input: String,
        clipboard: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectMethod {
    SendInput,
    Clipboard,
}

/// Försöker clipboard-paste först (snabbast och robust mot modifier-interference
/// under långa SendInput-sekvenser). Vid fel faller tillbaka till Unicode
/// SendInput för fält där Ctrl+V inte fungerar (vissa URL-fält, lösenordsfält).
#[cfg(windows)]
pub fn inject(text: &str) -> Result<InjectMethod, InjectError> {
    match paste_via_clipboard(text) {
        Ok(()) => {
            tracing::debug!(
                "inject: clipboard-paste lyckades ({} tecken)",
                text.chars().count()
            );
            Ok(InjectMethod::Clipboard)
        }
        Err(cb_err) => {
            tracing::warn!(
                "inject: clipboard-paste misslyckades ({cb_err}); faller tillbaka till SendInput"
            );
            match send_unicode(text) {
                Ok(()) => {
                    tracing::debug!("inject: SendInput-fallback lyckades");
                    Ok(InjectMethod::SendInput)
                }
                Err(send_err) => Err(InjectError::BothFailed {
                    send_input: send_err.to_string(),
                    clipboard: cb_err.to_string(),
                }),
            }
        }
    }
}

#[cfg(not(windows))]
pub fn inject(_text: &str) -> Result<InjectMethod, InjectError> {
    unimplemented!("text-injektion stöds bara på Windows i iter 1")
}
