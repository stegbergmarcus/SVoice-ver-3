//! Google OAuth 2.1 PKCE-flow.
//!
//! Desktop-appar kan inte skydda client_secret, så vi använder PKCE (RFC 7636)
//! istället. client_id är publikt — user registrerar sin egen OAuth-client i
//! Google Cloud Console och matar in ID:t via Settings. Ingen client_secret
//! behövs för native desktop-clients när PKCE används.
//!
//! Flödet:
//! 1. Bind lokal HTTP-server på ephemeral port (callback_server.rs).
//! 2. Generera PKCE code-verifier + challenge, random state.
//! 3. Öppna Google-auth-URL i användarens browser.
//! 4. User godkänner → Google redirectar till `http://127.0.0.1:<port>/callback`.
//! 5. Callback-servern returnerar code + state.
//! 6. Verifiera state, byt code + verifier mot (access_token, refresh_token).
//! 7. Spara refresh_token i keyring. access_token hålls i RAM.

use std::time::Duration;

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};

use super::callback_server::{self, CallbackError};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Scopes som SVoice kan begära. User-synlig consent styrs av scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoogleScope {
    CalendarReadonly,
    CalendarEvents,
    GmailReadonly,
    GmailModify,
}

impl GoogleScope {
    pub fn url(self) -> &'static str {
        match self {
            GoogleScope::CalendarReadonly => "https://www.googleapis.com/auth/calendar.readonly",
            GoogleScope::CalendarEvents => "https://www.googleapis.com/auth/calendar.events",
            GoogleScope::GmailReadonly => "https://www.googleapis.com/auth/gmail.readonly",
            GoogleScope::GmailModify => "https://www.googleapis.com/auth/gmail.modify",
        }
    }
}

/// Tokens vi har efter lyckad OAuth. `refresh_token` returneras bara första
/// gången user godkänner (eller om vi ber om `access_type=offline&prompt=consent`).
#[derive(Debug, Clone)]
pub struct GoogleTokens {
    pub access_token: String,
    /// Sekunder från nu tills access-token inte längre är giltig.
    pub expires_in: u64,
    /// Bara Some första gången. Spara direkt i keyring.
    pub refresh_token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GoogleAuthError {
    #[error("callback-server: {0}")]
    Callback(#[from] CallbackError),
    #[error("felaktig client_id (ej en URL): {0}")]
    InvalidClientId(String),
    #[error("URL-parse-fel: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("token-utbyte misslyckades: {0}")]
    TokenExchange(String),
    #[error("CSRF-state matchar inte — avbryter")]
    StateMismatch,
    #[error("browser kunde inte öppnas: {0}")]
    BrowserOpen(String),
    #[error("keyring: {0}")]
    Keyring(#[from] svoice_secrets::SecretsError),
    #[error("saknar refresh-token — user har inte godkänt tidigare")]
    NoRefreshToken,
}

/// Håller flow-state mellan `start()` och `finalize()`.
pub struct GoogleOAuthFlow {
    pub port: u16,
    pub auth_url: String,
    /// Behåll råparams för rebuild i finalize, så vi slipper type-alias för
    /// oauth2 v5:s komplexa state-machine-typer.
    client_id: String,
    client_secret: Option<String>,
    redirect_url: String,
    pkce_verifier: PkceCodeVerifier,
    csrf_token: CsrfToken,
    callback: callback_server::CallbackServer,
}

impl GoogleOAuthFlow {
    /// Initiera flow. Binder lokal port + genererar auth-URL. Efter detta ska
    /// caller öppna `auth_url` i browsern och sedan anropa `finalize().await`.
    ///
    /// `client_secret` krävs för Google Desktop-app-type (skickas i token-
    /// exchange trots PKCE). Passa `None` om du använder en ren public
    /// client-typ (t.ex. Installed app på andra plattformar).
    pub async fn start(
        client_id: &str,
        client_secret: Option<&str>,
        scopes: &[GoogleScope],
    ) -> Result<Self, GoogleAuthError> {
        let server = callback_server::start().await?;
        let port = server.port;
        let redirect_url = format!("http://127.0.0.1:{port}/callback");

        let mut client = BasicClient::new(ClientId::new(client_id.to_string()));
        if let Some(secret) = client_secret {
            client = client.set_client_secret(ClientSecret::new(secret.to_string()));
        }
        let client = client
            .set_auth_uri(
                AuthUrl::new(AUTH_URL.to_string())
                    .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
            )
            .set_token_uri(
                TokenUrl::new(TOKEN_URL.to_string())
                    .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
            )
            .set_redirect_uri(
                RedirectUrl::new(redirect_url.clone())
                    .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
            );

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let mut auth_req = client.authorize_url(CsrfToken::new_random);
        for scope in scopes {
            auth_req = auth_req.add_scope(Scope::new(scope.url().to_string()));
        }
        // access_type=offline så vi får refresh_token. prompt=consent tvingar
        // Google att skicka refresh-token även om user redan godkänt tidigare.
        let (auth_url, csrf_token) = auth_req
            .add_extra_param("access_type", "offline")
            .add_extra_param("prompt", "consent")
            .set_pkce_challenge(pkce_challenge)
            .url();

        Ok(GoogleOAuthFlow {
            port,
            auth_url: auth_url.to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.map(String::from),
            redirect_url,
            pkce_verifier,
            csrf_token,
            callback: server,
        })
    }

    /// Vänta på callback från browsern, verifiera state, byt code mot tokens.
    /// Timeout 5 min.
    pub async fn finalize(self) -> Result<GoogleTokens, GoogleAuthError> {
        tracing::info!("finalize: väntar på callback (max 5 min)");
        let cb =
            callback_server::wait_for_callback(self.callback, Duration::from_secs(300)).await?;
        tracing::info!(
            "finalize: callback mottagen, code.len={}, state.len={}",
            cb.code.len(),
            cb.state.len()
        );

        if cb.state != *self.csrf_token.secret() {
            tracing::error!("finalize: CSRF-state matchar inte");
            return Err(GoogleAuthError::StateMismatch);
        }
        tracing::debug!("finalize: state OK, bygger HTTP-klient för token-exchange");

        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| GoogleAuthError::TokenExchange(e.to_string()))?;

        let mut client = BasicClient::new(ClientId::new(self.client_id.clone()));
        if let Some(secret) = self.client_secret.as_deref() {
            client = client.set_client_secret(ClientSecret::new(secret.to_string()));
        }
        let client = client
            .set_auth_uri(
                AuthUrl::new(AUTH_URL.to_string())
                    .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
            )
            .set_token_uri(
                TokenUrl::new(TOKEN_URL.to_string())
                    .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
            )
            .set_redirect_uri(
                RedirectUrl::new(self.redirect_url.clone())
                    .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
            );

        tracing::info!(
            "finalize: anropar token-exchange endpoint (client_secret: {})",
            if self.client_secret.is_some() {
                "satt"
            } else {
                "saknas"
            }
        );
        let token = client
            .exchange_code(AuthorizationCode::new(cb.code))
            .set_pkce_verifier(self.pkce_verifier)
            .request_async(&http)
            .await
            .map_err(|e| {
                tracing::error!("finalize: token-exchange misslyckades: {e}");
                GoogleAuthError::TokenExchange(e.to_string())
            })?;

        tracing::info!(
            "finalize: tokens mottagna (har refresh-token: {})",
            token.refresh_token().is_some()
        );

        Ok(GoogleTokens {
            access_token: token.access_token().secret().clone(),
            expires_in: token.expires_in().map(|d| d.as_secs()).unwrap_or(3600),
            refresh_token: token.refresh_token().map(|rt| rt.secret().clone()),
        })
    }
}

/// Använd refresh-token för att få en ny access-token. Anropas transparent av
/// REST-wrappers vid 401 Unauthorized. `client_secret` krävs för Google
/// Desktop-app-typ.
pub async fn refresh_access_token(
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<GoogleTokens, GoogleAuthError> {
    let mut client = BasicClient::new(ClientId::new(client_id.to_string()));
    if let Some(secret) = client_secret {
        client = client.set_client_secret(ClientSecret::new(secret.to_string()));
    }
    let client = client
        .set_auth_uri(
            AuthUrl::new(AUTH_URL.to_string())
                .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
        )
        .set_token_uri(
            TokenUrl::new(TOKEN_URL.to_string())
                .map_err(|e| GoogleAuthError::InvalidClientId(e.to_string()))?,
        );

    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| GoogleAuthError::TokenExchange(e.to_string()))?;

    let token = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
        .request_async(&http)
        .await
        .map_err(|e| GoogleAuthError::TokenExchange(e.to_string()))?;

    Ok(GoogleTokens {
        access_token: token.access_token().secret().clone(),
        expires_in: token.expires_in().map(|d| d.as_secs()).unwrap_or(3600),
        refresh_token: token.refresh_token().map(|rt| rt.secret().clone()),
    })
}

/// Kopplar från Google genom att radera refresh-token ur keyring.
/// Google-sidan revokerar inte — user måste själv göra det via
/// https://myaccount.google.com/permissions om de vill.
pub fn disconnect() -> Result<(), GoogleAuthError> {
    svoice_secrets::delete_google_refresh_token()?;
    Ok(())
}

/// Kontrollera om vi har en sparad refresh-token (= user är ansluten).
pub fn is_connected() -> bool {
    svoice_secrets::has_google_refresh_token()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_urls_are_well_formed() {
        for scope in [
            GoogleScope::CalendarReadonly,
            GoogleScope::CalendarEvents,
            GoogleScope::GmailReadonly,
            GoogleScope::GmailModify,
        ] {
            let url = scope.url();
            assert!(url.starts_with("https://www.googleapis.com/auth/"));
        }
    }
}
