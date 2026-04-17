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
const USERNAME_GOOGLE_REFRESH: &str = "google_refresh_token";

#[derive(Error, Debug)]
pub enum SecretsError {
    #[error("keyring backend unavailable: {0}")]
    Backend(#[from] keyring::Error),
}

fn entry(username: &str) -> Result<Entry, SecretsError> {
    Ok(Entry::new(SERVICE, username)?)
}

/// Generisk secret-getter. Returnerar `Ok(None)` när entry saknas.
fn get_secret(username: &str) -> Result<Option<String>, SecretsError> {
    match entry(username)?.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretsError::Backend(e)),
    }
}

/// Generisk secret-setter. Ersätter existerande värde.
fn set_secret(username: &str, value: &str) -> Result<(), SecretsError> {
    entry(username)?.set_password(value)?;
    Ok(())
}

/// Generisk secret-radering. No-op om entry saknas.
fn delete_secret(username: &str) -> Result<(), SecretsError> {
    match entry(username)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretsError::Backend(e)),
    }
}

// ───────── Anthropic API-nyckel ─────────

pub fn get_anthropic_key() -> Result<Option<String>, SecretsError> {
    get_secret(USERNAME_ANTHROPIC)
}

pub fn set_anthropic_key(key: &str) -> Result<(), SecretsError> {
    set_secret(USERNAME_ANTHROPIC, key)
}

pub fn delete_anthropic_key() -> Result<(), SecretsError> {
    delete_secret(USERNAME_ANTHROPIC)
}

pub fn has_anthropic_key() -> bool {
    matches!(get_anthropic_key(), Ok(Some(_)))
}

// ───────── Google OAuth refresh-token ─────────

pub fn get_google_refresh_token() -> Result<Option<String>, SecretsError> {
    get_secret(USERNAME_GOOGLE_REFRESH)
}

pub fn set_google_refresh_token(token: &str) -> Result<(), SecretsError> {
    set_secret(USERNAME_GOOGLE_REFRESH, token)
}

pub fn delete_google_refresh_token() -> Result<(), SecretsError> {
    delete_secret(USERNAME_GOOGLE_REFRESH)
}

pub fn has_google_refresh_token() -> bool {
    matches!(get_google_refresh_token(), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rensa state före varje test — testerna delar en global service-entry
    /// ("svoice-v3-test") i Windows Credential Manager och körs med
    /// --test-threads=1.
    fn cleanup() {
        let _ = delete_anthropic_key();
        let _ = delete_google_refresh_token();
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

    #[test]
    fn google_refresh_token_roundtrips() {
        cleanup();
        assert!(!has_google_refresh_token());
        set_google_refresh_token("1//abc_refresh").unwrap();
        assert_eq!(
            get_google_refresh_token().unwrap().as_deref(),
            Some("1//abc_refresh")
        );
        assert!(has_google_refresh_token());
        delete_google_refresh_token().unwrap();
        assert!(!has_google_refresh_token());
    }

    #[test]
    fn secrets_are_isolated_by_username() {
        cleanup();
        set_anthropic_key("sk-ant-A").unwrap();
        set_google_refresh_token("goog-B").unwrap();
        assert_eq!(get_anthropic_key().unwrap().as_deref(), Some("sk-ant-A"));
        assert_eq!(
            get_google_refresh_token().unwrap().as_deref(),
            Some("goog-B")
        );
        cleanup();
    }
}
