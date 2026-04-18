//! Lokal HTTP-server som tar emot Googles OAuth-callback.
//!
//! Flödet:
//! 1. `start()` binder på `127.0.0.1:<ephemeral>` och returnerar port + en
//!    `oneshot::Receiver<CallbackResult>` som completar när callback:en tas emot.
//! 2. Browsern skickar `GET /callback?code=...&state=...` (eller `?error=...`).
//! 3. Server svarar med en HTML-sida ("du kan stänga fliken") och stänger.
//!
//! Servern körs tills den får en callback ELLER en timeout/cancellation utifrån.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[derive(Debug, thiserror::Error)]
pub enum CallbackError {
    #[error("kunde inte binda lokal port: {0}")]
    Bind(std::io::Error),
    #[error("OAuth-callback timade ut")]
    Timeout,
    #[error("I/O-fel: {0}")]
    Io(#[from] std::io::Error),
    #[error("OAuth returnerade fel: {0}")]
    OAuthError(String),
    #[error("ogiltig callback — saknar code/state")]
    InvalidCallback,
}

#[derive(Debug)]
pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

/// Resultat från `start()`.
pub struct CallbackServer {
    pub port: u16,
    pub receiver: oneshot::Receiver<Result<CallbackResult, CallbackError>>,
}

/// Starta HTTP-servern på ephemeral localhost-port. Servern stängs så fort
/// callback tagits emot och svar skickats.
///
/// Anropa innan du öppnar browsern. Skicka `port` i redirect-URI:n.
pub async fn start() -> Result<CallbackServer, CallbackError> {
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("valid addr");
    let listener = TcpListener::bind(addr).await.map_err(CallbackError::Bind)?;
    let port = listener.local_addr()?.port();
    tracing::info!("OAuth callback-server lyssnar på 127.0.0.1:{port}");

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        // Accept exactly one connection; after sending response, drop listener.
        let result = accept_once(listener).await;
        let _ = tx.send(result);
    });

    Ok(CallbackServer { port, receiver: rx })
}

/// Vänta på callback med timeout (default 5 min = generös för user att klicka igenom).
pub async fn wait_for_callback(
    server: CallbackServer,
    timeout: Duration,
) -> Result<CallbackResult, CallbackError> {
    tokio::time::timeout(timeout, server.receiver)
        .await
        .map_err(|_| CallbackError::Timeout)?
        .map_err(|_| CallbackError::InvalidCallback)?
}

async fn accept_once(listener: TcpListener) -> Result<CallbackResult, CallbackError> {
    let (mut stream, peer) = listener.accept().await?;
    tracing::debug!("OAuth callback från {peer}");

    // Läs HTTP-request header. Vi behöver bara första raden.
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let req = String::from_utf8_lossy(&buf[..n]);

    // Exempel: "GET /callback?code=4/0A...&state=xyz HTTP/1.1"
    let first_line = req.lines().next().unwrap_or_default();
    let mut parts = first_line.split_whitespace();
    let _method = parts.next();
    let path_and_query = parts.next().unwrap_or("/");

    let response_html = include_str!("callback_response.html");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_html.len(),
        response_html
    );

    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;

    let parsed = parse_query(path_and_query);
    match &parsed {
        Ok(r) => tracing::info!(
            "callback-server: parse OK, code.len={}, state.len={}",
            r.code.len(),
            r.state.len()
        ),
        Err(e) => tracing::error!("callback-server: parse misslyckades: {e}"),
    }
    parsed
}

fn parse_query(path_and_query: &str) -> Result<CallbackResult, CallbackError> {
    let query = path_and_query.split_once('?').map(|(_, q)| q).unwrap_or("");

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;

    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded = urlencoding_decode(v);
        match k {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            "error" => error = Some(decoded),
            _ => {}
        }
    }

    if let Some(err) = error {
        return Err(CallbackError::OAuthError(err));
    }

    match (code, state) {
        (Some(code), Some(state)) => Ok(CallbackResult { code, state }),
        _ => Err(CallbackError::InvalidCallback),
    }
}

/// Minimal URL-decode för callback-params. Stödjer `%XX` + `+` → mellanslag.
fn urlencoding_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                } else {
                    out.push('%');
                }
                i += 3;
            }
            b => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_code_and_state() {
        let r = parse_query("/callback?code=abc123&state=xyz").unwrap();
        assert_eq!(r.code, "abc123");
        assert_eq!(r.state, "xyz");
    }

    #[test]
    fn parses_with_url_encoding() {
        let r = parse_query("/callback?code=4%2F0ABC_DEF&state=st%2Bate").unwrap();
        assert_eq!(r.code, "4/0ABC_DEF");
        assert_eq!(r.state, "st+ate");
    }

    #[test]
    fn returns_oauth_error() {
        let r = parse_query("/callback?error=access_denied&state=x");
        match r {
            Err(CallbackError::OAuthError(e)) => assert_eq!(e, "access_denied"),
            other => panic!("expected OAuthError, got {other:?}"),
        }
    }

    #[test]
    fn missing_code_or_state_is_invalid() {
        assert!(matches!(
            parse_query("/callback?code=abc"),
            Err(CallbackError::InvalidCallback)
        ));
        assert!(matches!(
            parse_query("/callback?state=xyz"),
            Err(CallbackError::InvalidCallback)
        ));
    }

    #[test]
    fn url_decode_handles_edge_cases() {
        assert_eq!(urlencoding_decode("a+b"), "a b");
        assert_eq!(urlencoding_decode("a%20b"), "a b");
        assert_eq!(urlencoding_decode("%2F"), "/");
        assert_eq!(urlencoding_decode(""), "");
        assert_eq!(urlencoding_decode("plain"), "plain");
    }
}
