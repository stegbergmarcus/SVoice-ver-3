//! Windows Credential Manager-wrapper för SVoice 3 secrets.
//!
//! Alla secrets lagras under service `"svoice-v3"` med semantiska usernames
//! (`"anthropic_api_key"`, framtida `"google_refresh_token"` osv.). Credential
//! Manager kan filtreras på service-strängen för enkel översikt/rensning.

use keyring::Entry;
use thiserror::Error;

pub const SERVICE: &str = "svoice-v3";
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
