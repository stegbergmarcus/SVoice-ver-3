//! Groq LLM-klient — OpenAI-compatible chat/completions API.
//!
//! Endpoint: POST https://api.groq.com/openai/v1/chat/completions
//! Streaming via SSE (`stream: true`), chunks i OpenAI-format med
//! `choices[0].delta.content` för tokens.
//!
//! Default-modell: llama-3.3-70b-versatile (gratis, snabb).
//! Andra bra modeller: openai/gpt-oss-120b, moonshotai/kimi-k2-instruct.

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::provider::{LlmError, LlmProvider, LlmRequest, LlmStream, Role};

const API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "llama-3.3-70b-versatile";

pub struct GroqClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl GroqClient {
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
    messages: Vec<ApiMessage>,
}

#[derive(Debug, Deserialize)]
struct SseChunk {
    choices: Vec<SseChoice>,
}

#[derive(Debug, Deserialize)]
struct SseChoice {
    #[serde(default)]
    delta: SseDelta,
}

#[derive(Debug, Deserialize, Default)]
struct SseDelta {
    #[serde(default)]
    content: Option<String>,
}

fn role_to_string(r: &Role) -> String {
    match r {
        Role::User => "user".into(),
        Role::Assistant => "assistant".into(),
        Role::System => "system".into(),
    }
}

#[async_trait]
impl LlmProvider for GroqClient {
    fn name(&self) -> &'static str {
        "groq"
    }

    async fn complete_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError> {
        if self.api_key.is_empty() {
            return Err(LlmError::MissingApiKey);
        }

        let mut messages = Vec::with_capacity(req.turns.len() + 1);
        if let Some(sys) = &req.system {
            messages.push(ApiMessage {
                role: "system".into(),
                content: sys.clone(),
            });
        }
        for t in &req.turns {
            messages.push(ApiMessage {
                role: role_to_string(&t.role),
                content: t.text.clone(),
            });
        }

        let body = ApiRequest {
            model: self.model.clone(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: true,
            messages,
        };

        let resp = self
            .client
            .post(API_URL)
            .bearer_auth(&self.api_key)
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

        // SSE-parsing: splitta på rader, plocka "data: {json}"-rader, parsa
        // chunks och emittera text-deltas. "data: [DONE]" avslutar.
        let byte_stream = resp.bytes_stream();
        let stream = sse_to_text_stream(byte_stream);
        Ok(Box::pin(stream) as LlmStream)
    }
}

fn sse_to_text_stream<S>(byte_stream: S) -> impl Stream<Item = Result<String, LlmError>>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    async_stream::try_stream! {
        let mut buf = String::new();
        let mut bs = Box::pin(byte_stream);
        while let Some(chunk) = bs.next().await {
            let bytes = chunk.map_err(|e| LlmError::Http(e.to_string()))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));
            // Processa kompletta rader.
            loop {
                let Some(nl) = buf.find('\n') else { break };
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);
                if line.is_empty() || !line.starts_with("data:") {
                    continue;
                }
                let data = line.trim_start_matches("data:").trim();
                if data == "[DONE]" {
                    return;
                }
                let parsed: Result<SseChunk, _> = serde_json::from_str(data);
                match parsed {
                    Ok(c) => {
                        for ch in c.choices {
                            if let Some(text) = ch.delta.content {
                                if !text.is_empty() {
                                    yield text;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("groq SSE parse skippad: {e} (line: {})", data);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_api_key_returns_error() {
        let client = GroqClient::new("");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(client.complete_stream(LlmRequest {
            system: None,
            turns: vec![],
            temperature: 0.3,
            max_tokens: 100,
        }));
        assert!(matches!(result, Err(LlmError::MissingApiKey)));
    }

    #[test]
    fn default_model_is_latest_llama() {
        let client = GroqClient::new("k");
        assert_eq!(client.model, "llama-3.3-70b-versatile");
    }
}
