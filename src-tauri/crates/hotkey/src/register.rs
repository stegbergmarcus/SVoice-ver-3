use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("kunde inte registrera någon hotkey (primär: {primary}, fallback: {fallback}, orsaker: primär={primary_err}, fallback={fallback_err})")]
    AllFailed {
        primary: String,
        fallback: String,
        primary_err: String,
        fallback_err: String,
    },
}

#[derive(Debug, Clone)]
pub struct RegisteredHotkey {
    pub label: String,
    pub shortcut: Shortcut,
}

pub type HotkeyCallback<R> =
    Arc<dyn Fn(&AppHandle<R>, &Shortcut, ShortcutEvent) + Send + Sync + 'static>;

pub fn register_ptt<R>(
    app: &AppHandle<R>,
    callback: HotkeyCallback<R>,
) -> Result<RegisteredHotkey, HotkeyError>
where
    R: Runtime,
{
    let primary = Shortcut::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::Space);
    let fallback = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);

    let gs = app.global_shortcut();

    let cb_clone = callback.clone();
    match gs.on_shortcut(primary, move |app, sc, ev| (cb_clone)(app, sc, ev)) {
        Ok(()) => {
            tracing::info!("hotkey registrerad: Win+Alt+Space");
            Ok(RegisteredHotkey {
                label: "Win+Alt+Space".into(),
                shortcut: primary,
            })
        }
        Err(primary_err) => {
            tracing::warn!(
                "primär hotkey Win+Alt+Space misslyckades ({primary_err}); provar Ctrl+Alt+Space"
            );
            let cb_clone2 = callback.clone();
            match gs.on_shortcut(fallback, move |app, sc, ev| (cb_clone2)(app, sc, ev)) {
                Ok(()) => {
                    tracing::info!("hotkey registrerad (fallback): Ctrl+Alt+Space");
                    Ok(RegisteredHotkey {
                        label: "Ctrl+Alt+Space".into(),
                        shortcut: fallback,
                    })
                }
                Err(fallback_err) => Err(HotkeyError::AllFailed {
                    primary: "Win+Alt+Space".into(),
                    fallback: "Ctrl+Alt+Space".into(),
                    primary_err: primary_err.to_string(),
                    fallback_err: fallback_err.to_string(),
                }),
            }
        }
    }
}

/// Hjälpfunktion för att detektera om ett ShortcutEvent är key-down eller key-up.
pub fn is_key_down(ev: &ShortcutEvent) -> bool {
    matches!(ev.state(), ShortcutState::Pressed)
}
