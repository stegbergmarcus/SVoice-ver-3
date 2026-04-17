//! Windows Credential Manager-wrapper för SVoice 3 secrets.
//!
//! Alla secrets lagras under service `"svoice-v3"` med semantiska usernames
//! (`"anthropic_api_key"`, framtida `"google_refresh_token"` osv.). Credential
//! Manager kan filtreras på service-strängen för enkel översikt/rensning.

use keyring::Entry;
use thiserror::Error;

#[cfg(not(test))]
pub const SERVICE: &str = "svoice-v3";
#[cfg(test)]
pub const SERVICE: &str = "svoice-v3-test";
const USERNAME_ANTHROPIC: &str = "anthropic_api_key";

#[derive(Error, Debug)]
pub enum SecretsError {
    #[error("keyring backend unavailable: {0}")]
    Backend(#[from] keyring::Error),
}

fn entry(username: &str) -> Result<Entry, SecretsError> {
    Ok(Entry::new(SERVICE, username)?)
}

/// Hämta Anthropic API-nyckeln från keyring.
/// Returnerar `Ok(None)` när entry saknas (first-run); `Err` vid backend-fel.
pub fn get_anthropic_key() -> Result<Option<String>, SecretsError> {
    match entry(USERNAME_ANTHROPIC)?.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretsError::Backend(e)),
    }
}

/// Skriv (eller ersätt) Anthropic API-nyckeln i keyring.
pub fn set_anthropic_key(key: &str) -> Result<(), SecretsError> {
    entry(USERNAME_ANTHROPIC)?.set_password(key)?;
    Ok(())
}

/// Radera Anthropic API-nyckeln från keyring. No-op om den inte finns.
pub fn delete_anthropic_key() -> Result<(), SecretsError> {
    match entry(USERNAME_ANTHROPIC)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretsError::Backend(e)),
    }
}

/// Convenience för frontend — returnerar alltid bool, sväljer backend-fel.
pub fn has_anthropic_key() -> bool {
    matches!(get_anthropic_key(), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rensa state före varje test — testerna delar en global service-entry
    /// ("svoice-v3-test") i Windows Credential Manager och körs med
    /// --test-threads=1.
    fn cleanup() {
        let _ = delete_anthropic_key();
    }

    #[test]
    fn get_returns_none_when_empty() {
        cleanup();
        assert_eq!(get_anthropic_key().unwrap(), None);
    }

    #[test]
    fn set_then_get_roundtrips() {
        cleanup();
        set_anthropic_key("sk-ant-abc123").unwrap();
        assert_eq!(get_anthropic_key().unwrap().as_deref(), Some("sk-ant-abc123"));
        cleanup();
    }

    #[test]
    fn delete_removes_stored_value() {
        cleanup();
        set_anthropic_key("sk-ant-xyz").unwrap();
        delete_anthropic_key().unwrap();
        assert_eq!(get_anthropic_key().unwrap(), None);
    }

    #[test]
    fn delete_is_noop_when_absent() {
        cleanup();
        delete_anthropic_key().unwrap();
    }

    #[test]
    fn has_reflects_state() {
        cleanup();
        assert!(!has_anthropic_key());
        set_anthropic_key("sk-ant-xyz").unwrap();
        assert!(has_anthropic_key());
        delete_anthropic_key().unwrap();
        assert!(!has_anthropic_key());
    }
}
