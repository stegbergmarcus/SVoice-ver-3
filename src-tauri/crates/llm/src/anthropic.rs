//! Anthropic Claude-klient med Server-Sent-Events streaming.
//!
//! Använder `/v1/messages` med `"stream": true`. Anthropic API returnerar
//! SSE-events; vi plockar ut `content_block_delta`-events med text-deltas och
//! emittar dem som strängar genom `LlmStream`.

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::provider::{LlmError, LlmProvider, LlmRequest, LlmStream, Role};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-5";

pub struct AnthropicClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[derive(Debug, Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    temperature: f32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ApiMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseEvent {
    MessageStart,
    ContentBlockStart,
    ContentBlockDelta { delta: Delta },
    ContentBlockStop,
    MessageDelta,
    MessageStop,
    Ping,
    Error { error: ApiErrorBody },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Delta {
    TextDelta {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    message: String,
}

#[async_trait]
impl LlmProvider for AnthropicClient {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn complete_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError> {
        if self.api_key.is_empty() {
            return Err(LlmError::MissingApiKey);
        }

        let mut messages: Vec<ApiMessage> = Vec::new();
        for turn in &req.turns {
            let role = match turn.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => continue, // Anthropic lägger system separat, inte i messages.
            };
            messages.push(ApiMessage {
                role: role.into(),
                content: turn.text.clone(),
            });
        }

        let body = ApiRequest {
            model: self.model.clone(),
            max_tokens: req.max_tokens.max(64),
            temperature: req.temperature,
            stream: true,
            system: req.system,
            messages,
        };

        let resp = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status: status.as_u16(),
                body,
            });
        }

        let byte_stream = resp.bytes_stream();
        let text_stream = sse_text_deltas(byte_stream);
        Ok(Box::pin(text_stream))
    }
}

/// Konvertera byte-stream från reqwest till stream av Result<String, LlmError>
/// genom att plocka ut text-deltas ur SSE-events.
fn sse_text_deltas(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>> {
    use std::collections::VecDeque;

    // Manuell SSE-parsning: ackumulera tills vi ser "\n\n" (event-separator),
    // extrahera "data:"-rader, parsa JSON-payloaden och emit:a textdelta.
    let buf: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
    let out: std::cell::RefCell<VecDeque<Result<String, LlmError>>> =
        std::cell::RefCell::new(VecDeque::new());
    // RefCell + non-Send — OK då vi stream:ar den via async_stream-pattern.

    // Enklare: använd futures::stream::unfold med en mutable state.
    let stream = futures_util::stream::unfold(
        (
            byte_stream.boxed(),
            String::new(),
            VecDeque::<Result<String, LlmError>>::new(),
        ),
        |(mut bs, mut pending_buf, mut out_queue)| async move {
            loop {
                if let Some(item) = out_queue.pop_front() {
                    return Some((item, (bs, pending_buf, out_queue)));
                }
                match bs.next().await {
                    None => {
                        if pending_buf.is_empty() {
                            return None;
                        }
                        // Flush sista osparade event om det finns något.
                        process_sse_chunk(&pending_buf, &mut out_queue);
                        pending_buf.clear();
                        continue;
                    }
                    Some(Err(e)) => {
                        return Some((
                            Err(LlmError::Http(e.to_string())),
                            (bs, pending_buf, out_queue),
                        ));
                    }
                    Some(Ok(bytes)) => {
                        pending_buf.push_str(&String::from_utf8_lossy(&bytes));
                        // Dela på event-separator "\n\n" och processa varje complete event.
                        while let Some(idx) = pending_buf.find("\n\n") {
                            let event = pending_buf[..idx].to_string();
                            pending_buf.drain(..idx + 2);
                            process_sse_chunk(&event, &mut out_queue);
                        }
                    }
                }
            }
        },
    );
    // drop oanvänd state (ska ju finnas, men compiler-hjälp)
    let _ = buf;
    let _ = out;
    Box::pin(stream)
}

fn process_sse_chunk(chunk: &str, out: &mut std::collections::VecDeque<Result<String, LlmError>>) {
    // SSE-format: varje rad börjar med "field:value". Vi bryr oss bara om "data:".
    for line in chunk.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            let json = data.trim();
            if json.is_empty() || json == "[DONE]" {
                continue;
            }
            match serde_json::from_str::<SseEvent>(json) {
                Ok(SseEvent::ContentBlockDelta {
                    delta: Delta::TextDelta { text },
                }) => {
                    if !text.is_empty() {
                        out.push_back(Ok(text));
                    }
                }
                Ok(SseEvent::Error { error }) => {
                    out.push_back(Err(LlmError::Api {
                        status: 0,
                        body: error.message,
                    }));
                }
                Ok(_) => {} // Ignorera message_start, ping, etc.
                Err(e) => {
                    tracing::debug!("sse parse miss: {e} — payload: {json}");
                }
            }
        }
    }
}
