//! Google REST-client med automatisk access-token-refresh.
//!
//! Konstrueras med client_id + refresh_token. Första anropet hämtar en färsk
//! access-token via `refresh_access_token`. Vid 401 från Google-API:et körs
//! refresh igen och requestet retrias en gång.

use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::Mutex;

use super::oauth::{refresh_access_token, GoogleAuthError, GoogleTokens};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("auth: {0}")]
    Auth(#[from] GoogleAuthError),
    #[error("reqwest: {0}")]
    Http(#[from] reqwest::Error),
    #[error("google-API returnerade {status}: {body}")]
    ApiError { status: u16, body: String },
}

pub struct GoogleClient {
    client_id: String,
    client_secret: Option<String>,
    refresh_token: String,
    http: reqwest::Client,
    cached: Mutex<Option<CachedAccessToken>>,
}

struct CachedAccessToken {
    value: String,
    expires_at: Instant,
}

impl GoogleClient {
    pub fn new(client_id: String, client_secret: Option<String>, refresh_token: String) -> Self {
        Self {
            client_id,
            client_secret,
            refresh_token,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            cached: Mutex::new(None),
        }
    }

    /// Returnera en giltig access-token. Refreshar transparent om tokenen är
    /// utgången eller saknas.
    async fn access_token(&self) -> Result<String, ClientError> {
        let mut guard = self.cached.lock().await;
        if let Some(cached) = guard.as_ref() {
            // 30s margin så vi inte använder en "just utgående" token.
            if cached.expires_at > Instant::now() + Duration::from_secs(30) {
                return Ok(cached.value.clone());
            }
        }
        let tokens: GoogleTokens = refresh_access_token(
            &self.client_id,
            self.client_secret.as_deref(),
            &self.refresh_token,
        )
        .await?;
        let expires_at = Instant::now() + Duration::from_secs(tokens.expires_in);
        *guard = Some(CachedAccessToken {
            value: tokens.access_token.clone(),
            expires_at,
        });
        Ok(tokens.access_token)
    }

    /// GET med auto-refresh vid 401.
    pub async fn get(&self, url: &str) -> Result<serde_json::Value, ClientError> {
        self.request_json(reqwest::Method::GET, url, None::<&()>)
            .await
    }

    /// POST JSON med auto-refresh vid 401.
    pub async fn post_json<B: Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<serde_json::Value, ClientError> {
        self.request_json(reqwest::Method::POST, url, Some(body))
            .await
    }

    async fn request_json<B: Serialize + ?Sized>(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&B>,
    ) -> Result<serde_json::Value, ClientError> {
        // Första försök
        let resp = self.do_request(&method, url, body).await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            tracing::info!("google-API 401 — refreshar token och retrier");
            // Invalidera cache så nästa access_token() tvingar refresh.
            *self.cached.lock().await = None;
            let retry = self.do_request(&method, url, body).await?;
            return handle_response(retry).await;
        }
        handle_response(resp).await
    }

    async fn do_request<B: Serialize + ?Sized>(
        &self,
        method: &reqwest::Method,
        url: &str,
        body: Option<&B>,
    ) -> Result<reqwest::Response, ClientError> {
        let token = self.access_token().await?;
        let mut req = self.http.request(method.clone(), url).bearer_auth(token);
        if let Some(b) = body {
            req = req.json(b);
        }
        Ok(req.send().await?)
    }
}

async fn handle_response(resp: reqwest::Response) -> Result<serde_json::Value, ClientError> {
    let status = resp.status();
    if status.is_success() {
        Ok(resp.json().await?)
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(ClientError::ApiError {
            status: status.as_u16(),
            body,
        })
    }
}
