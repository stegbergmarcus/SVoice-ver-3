#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod send_input;

#[cfg(windows)]
pub use clipboard::{paste_via_clipboard, ClipboardError};
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

/// Försöker SendInput först. Vid fel (PartialSend) faller tillbaka till clipboard-paste.
#[cfg(windows)]
pub fn inject(text: &str) -> Result<InjectMethod, InjectError> {
    match send_unicode(text) {
        Ok(()) => {
            tracing::debug!(
                "inject: SendInput lyckades ({} tecken)",
                text.chars().count()
            );
            Ok(InjectMethod::SendInput)
        }
        Err(send_err) => {
            tracing::warn!(
                "inject: SendInput misslyckades ({send_err}); faller tillbaka till clipboard"
            );
            match paste_via_clipboard(text) {
                Ok(()) => {
                    tracing::debug!("inject: clipboard-fallback lyckades");
                    Ok(InjectMethod::Clipboard)
                }
                Err(cb_err) => Err(InjectError::BothFailed {
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
