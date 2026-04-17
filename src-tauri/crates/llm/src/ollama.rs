//! Ollama-klient med NDJSON-streaming.
//!
//! Ollama körs lokalt på http://localhost:11434 (default). Vi använder
//! `/api/chat` med `"stream": true` som returnerar en stream av NDJSON-
//! rader, där varje rad har `{"message": {"content": "..."}, "done": false}`.
//!
//! Användaren måste själv:
//!   - Installera Ollama (https://ollama.com)
//!   - Köra `ollama pull <model>` för att ladda ner modeller
//!   - Ha Ollama-servicen igång (autostartar på Windows efter install)
//!
//! [`OllamaClient::is_healthy`] kan användas för att välja mellan Ollama
//! och Anthropic vid runtime.

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::Duration;

use crate::provider::{LlmError, LlmProvider, LlmRequest, LlmStream, Role};

const DEFAULT_URL: &str = "http://127.0.0.1:11434";
const HEALTH_TIMEOUT: Duration = Duration::from_millis(800);

pub struct OllamaClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaClient {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            base_url: DEFAULT_URL.into(),
            model: model.into(),
            client: reqwest::Client::builder()
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Pinga /api/tags med kort timeout — returnerar true om Ollama-servicen
    /// svarar, false annars. Används för att välja mellan lokal och cloud LLM.
    pub async fn is_healthy(&self) -> bool {
        match tokio::time::timeout(
            HEALTH_TIMEOUT,
            self.client.get(format!("{}/api/tags", self.base_url)).send(),
        )
        .await
        {
            Ok(Ok(resp)) => resp.status().is_success(),
            _ => false,
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: ChatOptions,
}

#[derive(Debug, Serialize)]
struct ChatOptions {
    temperature: f32,
    num_predict: i32,
}

#[derive(Debug, Deserialize)]
struct ChatStreamEvent {
    #[serde(default)]
    message: Option<ChatStreamDelta>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamDelta {
    #[serde(default)]
    content: String,
}

#[async_trait]
impl LlmProvider for OllamaClient {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn complete_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError> {
        let mut messages: Vec<ChatMessage> = Vec::new();
        if let Some(sys) = &req.system {
            messages.push(ChatMessage {
                role: "system".into(),
                content: sys.clone(),
            });
        }
        for turn in &req.turns {
            let role = match turn.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };
            messages.push(ChatMessage {
                role: role.into(),
                content: turn.text.clone(),
            });
        }

        let body = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            options: ChatOptions {
                temperature: req.temperature,
                num_predict: req.max_tokens as i32,
            },
        };

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
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
        let text_stream = ndjson_content_deltas(byte_stream);
        Ok(Box::pin(text_stream))
    }
}

/// Konvertera Ollamas NDJSON-stream till text-deltas.
fn ndjson_content_deltas(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>> {
    use std::collections::VecDeque;
    let stream = futures_util::stream::unfold(
        (byte_stream.boxed(), String::new(), VecDeque::<Result<String, LlmError>>::new()),
        |(mut bs, mut buf, mut out)| async move {
            loop {
                if let Some(item) = out.pop_front() {
                    return Some((item, (bs, buf, out)));
                }
                match bs.next().await {
                    None => {
                        // Flush sista raden om det finns något kvar.
                        if !buf.is_empty() {
                            process_ndjson_line(&buf, &mut out);
                            buf.clear();
                            continue;
                        }
                        return None;
                    }
                    Some(Err(e)) => {
                        return Some((Err(LlmError::Http(e.to_string())), (bs, buf, out)));
                    }
                    Some(Ok(bytes)) => {
                        buf.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(idx) = buf.find('\n') {
                            let line = buf[..idx].to_string();
                            buf.drain(..idx + 1);
                            if !line.trim().is_empty() {
                                process_ndjson_line(&line, &mut out);
                            }
                        }
                    }
                }
            }
        },
    );
    Box::pin(stream)
}

fn process_ndjson_line(line: &str, out: &mut std::collections::VecDeque<Result<String, LlmError>>) {
    match serde_json::from_str::<ChatStreamEvent>(line.trim()) {
        Ok(ev) => {
            if let Some(err) = ev.error {
                out.push_back(Err(LlmError::Api {
                    status: 0,
                    body: err,
                }));
                return;
            }
            if let Some(msg) = ev.message {
                if !msg.content.is_empty() {
                    out.push_back(Ok(msg.content));
                }
            }
            if ev.done {
                // Naturlig stream-slut; inga fler events förväntas.
            }
        }
        Err(e) => {
            tracing::debug!("ollama ndjson parse miss: {e} — payload: {line}");
        }
    }
}
