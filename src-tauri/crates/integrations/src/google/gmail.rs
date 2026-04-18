//! Gmail v1 — minimala read-wrappers för SVoice 3 action-LLM tool-use.

use serde::{Deserialize, Serialize};

use super::client::{ClientError, GoogleClient};

const API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

/// Resultat från messages.list — bara metadata-pek (id + threadId).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRef {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
}

/// Detaljerad message — innehåller headers och snippet. Full body hoppar vi
/// över för nu (kräver payload-traversering med MIME-parsing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub payload: Option<MessagePayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    #[serde(default)]
    pub headers: Vec<Header>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl Message {
    /// Plocka ett header-värde (case-insensitive). `From`, `Subject`, `Date`.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.payload.as_ref().and_then(|p| {
            p.headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case(name))
                .map(|h| h.value.as_str())
        })
    }
}

/// Sök meddelanden med Gmail-query (samma syntax som search i UI:et,
/// t.ex. "from:marcus@stegberg.se is:unread"). Returnerar max `max_results`.
pub async fn search_messages(
    client: &GoogleClient,
    query: &str,
    max_results: u32,
) -> Result<Vec<MessageRef>, ClientError> {
    let url = format!(
        "{API_BASE}/messages?q={}&maxResults={}",
        urlencoding_simple(query),
        max_results
    );
    let json = client.get(&url).await?;
    let items = json
        .get("messages")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    serde_json::from_value(items).map_err(|e| ClientError::ApiError {
        status: 0,
        body: format!("parse-fel: {e}"),
    })
}

/// Hämta metadata-nivå för ett meddelande (headers + snippet, ingen full body).
pub async fn get_message(client: &GoogleClient, message_id: &str) -> Result<Message, ClientError> {
    let url = format!(
        "{API_BASE}/messages/{message_id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date&metadataHeaders=To"
    );
    let json = client.get(&url).await?;
    serde_json::from_value(json).map_err(|e| ClientError::ApiError {
        status: 0,
        body: format!("parse-fel: {e}"),
    })
}

/// Skapa ett utkast (DRAFT). Skickas INTE automatiskt — user granskar i
/// Gmail-webben och trycker Skicka själv. Skyddar mot felaktiga mail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: String,
    #[serde(default)]
    pub message: Option<MessageRef>,
}

pub async fn create_draft(
    client: &GoogleClient,
    to: &str,
    subject: &str,
    body: &str,
    thread_id: Option<&str>,
) -> Result<Draft, ClientError> {
    let raw = build_rfc822(to, subject, body);
    let encoded = base64url_encode(raw.as_bytes());
    let mut message = serde_json::json!({ "raw": encoded });
    if let Some(tid) = thread_id {
        message["threadId"] = serde_json::Value::String(tid.to_string());
    }
    let req = serde_json::json!({ "message": message });
    let url = format!("{API_BASE}/drafts");
    let json = client.post_json(&url, &req).await?;
    serde_json::from_value(json).map_err(|e| ClientError::ApiError {
        status: 0,
        body: format!("parse-fel: {e}"),
    })
}

fn build_rfc822(to: &str, subject: &str, body: &str) -> String {
    format!(
        "To: {to}\r\nSubject: {subject}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}"
    )
}

/// Base64url utan padding (Gmail-krav).
fn base64url_encode(bytes: &[u8]) -> String {
    const A: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4 + 2) / 3);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let (b0, b1, b2) = (bytes[i], bytes[i + 1], bytes[i + 2]);
        out.push(A[(b0 >> 2) as usize] as char);
        out.push(A[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(A[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        out.push(A[(b2 & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        out.push(A[(bytes[i] >> 2) as usize] as char);
        out.push(A[((bytes[i] & 0x03) << 4) as usize] as char);
    } else if rem == 2 {
        let (b0, b1) = (bytes[i], bytes[i + 1]);
        out.push(A[(b0 >> 2) as usize] as char);
        out.push(A[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(A[((b1 & 0x0f) << 2) as usize] as char);
    }
    out
}

fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_lookup_is_case_insensitive() {
        let msg = Message {
            id: "1".into(),
            thread_id: "t1".into(),
            snippet: "".into(),
            payload: Some(MessagePayload {
                headers: vec![
                    Header {
                        name: "Subject".into(),
                        value: "Hej".into(),
                    },
                    Header {
                        name: "From".into(),
                        value: "a@b.se".into(),
                    },
                ],
            }),
        };
        assert_eq!(msg.header("subject"), Some("Hej"));
        assert_eq!(msg.header("FROM"), Some("a@b.se"));
        assert_eq!(msg.header("X-Missing"), None);
    }

    #[test]
    fn search_encodes_query() {
        let q = urlencoding_simple("from:marcus is:unread");
        assert!(q.contains("%3A")); // :
        assert!(q.contains("%20")); // space
    }

    #[test]
    fn base64url_matches_reference() {
        // "hello world" → "aGVsbG8gd29ybGQ" (utan padding)
        assert_eq!(base64url_encode(b"hello world"), "aGVsbG8gd29ybGQ");
        assert_eq!(base64url_encode(b""), "");
        assert_eq!(base64url_encode(b"M"), "TQ");
        assert_eq!(base64url_encode(b"Ma"), "TWE");
    }

    #[test]
    fn rfc822_has_required_headers() {
        let msg = build_rfc822("a@b.se", "Hej!", "Text");
        assert!(msg.contains("To: a@b.se"));
        assert!(msg.contains("Subject: Hej!"));
        assert!(msg.contains("charset=utf-8"));
        assert!(msg.ends_with("Text"));
    }

    #[test]
    fn message_ref_parses() {
        let json = r#"{"id":"abc123","threadId":"thr456"}"#;
        let m: MessageRef = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "abc123");
        assert_eq!(m.thread_id, "thr456");
    }
}
