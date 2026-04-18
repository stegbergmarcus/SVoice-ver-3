//! Engångs-migrationer som körs vid app-start. Varje funktion är idempotent
//! (säkert att köra varje start; no-op om inget att flytta).

use std::path::Path;

/// Om `settings.json` har `anthropic_api_key` som non-empty sträng, flytta
/// den till keyring och ta bort fältet ur JSON. Best-effort: loggar och
/// returnerar Ok(()) vid fel så app-start inte blockeras.
pub fn migrate_anthropic_key(settings_path: &Path) -> anyhow::Result<()> {
    let Ok(raw) = std::fs::read_to_string(settings_path) else {
        return Ok(()); // ingen fil → ingen migration
    };
    let mut value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("migrate: kan inte parsa settings.json: {e}");
            return Ok(());
        }
    };
    let Some(obj) = value.as_object_mut() else {
        return Ok(());
    };
    let Some(key_value) = obj.get("anthropic_api_key").and_then(|v| v.as_str()) else {
        return Ok(()); // fältet saknas eller är null/non-string
    };
    if key_value.is_empty() {
        // Städa bort tomt fält utan keyring-skrivning
        let _ = obj.remove("anthropic_api_key");
        write_atomic(settings_path, &value)?;
        return Ok(());
    }
    // Clone the value before releasing the borrow on obj
    let key_value = key_value.to_string();
    match svoice_secrets::set_anthropic_key(&key_value) {
        Ok(()) => {
            obj.remove("anthropic_api_key");
            write_atomic(settings_path, &value)?;
            tracing::info!("migrerat anthropic_api_key till Windows Credential Manager");
        }
        Err(e) => {
            tracing::error!(
                "keyring-migrering misslyckades: {e} — klartext kvar tills nästa försök"
            );
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid path"))?;
    let tmp = parent.join("settings.json.tmp");
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Note: tests share the real Windows Credential Manager (keyring uses
    // windows-native backend). Because svoice-secrets uses cfg(test)-switched
    // SERVICE="svoice-v3-test", only test-suite in svoice-secrets uses that.
    // Tests HERE hit the production SERVICE="svoice-v3" — ensure cleanup.

    fn cleanup_keyring() {
        let _ = svoice_secrets::delete_anthropic_key();
    }

    #[test]
    fn migrates_plaintext_key_and_strips_json() {
        cleanup_keyring();

        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"anthropic_api_key":"sk-ant-legacy","stt_model":"whisper"}"#,
        )
        .unwrap();

        migrate_anthropic_key(&path).unwrap();

        assert_eq!(
            svoice_secrets::get_anthropic_key().unwrap().as_deref(),
            Some("sk-ant-legacy")
        );
        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(after.get("anthropic_api_key").is_none());
        assert_eq!(
            after.get("stt_model").and_then(|v| v.as_str()),
            Some("whisper")
        );

        cleanup_keyring();
    }

    #[test]
    fn noop_when_no_legacy_key() {
        cleanup_keyring();

        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = r#"{"stt_model":"whisper"}"#;
        std::fs::write(&path, original).unwrap();

        migrate_anthropic_key(&path).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(!svoice_secrets::has_anthropic_key());
    }

    #[test]
    fn handles_missing_settings_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        migrate_anthropic_key(&path).unwrap();
    }

    #[test]
    fn strips_empty_string_without_writing_keyring() {
        cleanup_keyring();

        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"anthropic_api_key":"","stt_model":"w"}"#).unwrap();

        migrate_anthropic_key(&path).unwrap();

        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(after.get("anthropic_api_key").is_none());
        assert!(!svoice_secrets::has_anthropic_key());
    }
}
