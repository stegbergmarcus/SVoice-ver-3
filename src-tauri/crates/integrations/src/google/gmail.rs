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
    fn message_ref_parses() {
        let json = r#"{"id":"abc123","threadId":"thr456"}"#;
        let m: MessageRef = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "abc123");
        assert_eq!(m.thread_id, "thr456");
    }
}
