//! Google Gemini-klient via `generativelanguage.googleapis.com`.
//!
//! Endpoint:
//!   POST /v1beta/models/{MODEL}:streamGenerateContent?alt=sse
//!
//! Skillnader mot OpenAI-kompatibla format:
//! - `contents[].parts[].text` (inte `messages[].content`)
//! - System-prompt i separat `systemInstruction.parts[].text`-fält
//! - Roller: `"user"` / `"model"` (inte `"assistant"`)
//! - Grounding: `"tools": [{"googleSearch": {}}]` aktiverar Google Search
//!   och lägger `groundingMetadata` på sista chunken
//!
//! `complete_stream` implementerar `LlmProvider`-traiten och ger bara text.
//! Agentic-flow behöver grounding-metadata och använder istället
//! [`GeminiClient::complete_stream_events`] som yieldar
//! [`GeminiEvent::Text`]-chunks plus eventuella [`GeminiEvent::Grounding`]-
//! events när API:et rapporterar käll-URL:er för web-sökningar.

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::provider::{LlmError, LlmProvider, LlmRequest, LlmStream, Role, TurnContent};

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";
pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

pub struct GeminiClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
    enable_grounding: bool,
}

impl GeminiClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.into(),
            client: reqwest::Client::new(),
            enable_grounding: false,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_grounding(mut self, enabled: bool) -> Self {
        self.enable_grounding = enabled;
        self
    }
}

/// Grounding-käll-information som Gemini returnerar när Google Search-
/// tool:et används. Vi lagrar både `title` (human-readable, används i UI)
/// och `uri` (Gemini-redirect-URL:n — användbar om user klickar för att
/// följa till källan).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiGroundingChunk {
    pub title: String,
    pub uri: String,
}

/// Event från Gemini-streamen. `Text` yieldas per content-delta, `Grounding`
/// när grounding-metadata hittas (typiskt bara i sista chunken, men vi
/// emittar för varje chunk där metadata förekommer).
#[derive(Debug, Clone)]
pub enum GeminiEvent {
    Text(String),
    Grounding {
        queries: Vec<String>,
        chunks: Vec<GeminiGroundingChunk>,
    },
}

fn role_to_gemini(r: &Role) -> &'static str {
    match r {
        Role::User => "user",
        Role::Assistant => "model",
        // System turns är egentligen inte tillåtna i Gemini-contents (system-
        // prompt ligger i `systemInstruction`), men om caller råkar skicka
        // en System-turn faller vi tillbaka till "user" för att inte krascha.
        Role::System => "user",
    }
}

/// Slå ihop consecutive turns med samma roll. Gemini's `contents[]` måste
/// alternera mellan `user` och `model` — om stream:en failade mitt i en
/// agentic-round kan ACTIVE_CONVERSATION sluta med två user-turns i följd
/// när follow-up appendar. API:et svarar då med `400: Please ensure that
/// roles alternate between user and model`. Merge:a till en single turn
/// med två radbrytningar emellan så meningen bevaras.
fn canonicalize_turns(turns: &[TurnContent]) -> Vec<TurnContent> {
    let mut out: Vec<TurnContent> = Vec::with_capacity(turns.len());
    for t in turns {
        match out.last_mut() {
            Some(prev)
                if std::mem::discriminant(&prev.role)
                    == std::mem::discriminant(&t.role) =>
            {
                prev.text.push_str("\n\n");
                prev.text.push_str(&t.text);
            }
            _ => out.push(t.clone()),
        }
    }
    out
}

fn build_request_body(req: &LlmRequest, enable_grounding: bool) -> serde_json::Value {
    let canon = canonicalize_turns(&req.turns);
    let contents: Vec<serde_json::Value> = canon
        .iter()
        .map(|t| {
            serde_json::json!({
                "role": role_to_gemini(&t.role),
                "parts": [{ "text": t.text }]
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": {
            "temperature": req.temperature,
            "maxOutputTokens": req.max_tokens,
        }
    });

    if let Some(sys) = &req.system {
        body["systemInstruction"] = serde_json::json!({
            "parts": [{ "text": sys }]
        });
    }

    if enable_grounding {
        body["tools"] = serde_json::json!([{ "googleSearch": {} }]);
    }

    body
}

#[async_trait]
impl LlmProvider for GeminiClient {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn complete_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError> {
        let events = self.complete_stream_events(req).await?;
        // Filtrera bort grounding-events så trait-callers bara ser text.
        let text_only = events.filter_map(|ev| async move {
            match ev {
                Ok(GeminiEvent::Text(t)) => Some(Ok(t)),
                Ok(GeminiEvent::Grounding { .. }) => None,
                Err(e) => Some(Err(e)),
            }
        });
        Ok(Box::pin(text_only) as LlmStream)
    }
}

impl GeminiClient {
    /// Starta en streaming-generering som yieldar [`GeminiEvent`]s. Används
    /// av agentic-pathen för att fånga grounding-metadata. För vanlig
    /// text-streaming (via `LlmProvider`-traiten) används [`complete_stream`]
    /// som internt mappar bort grounding-events.
    pub async fn complete_stream_events(
        &self,
        req: LlmRequest,
    ) -> Result<
        std::pin::Pin<Box<dyn Stream<Item = Result<GeminiEvent, LlmError>> + Send>>,
        LlmError,
    > {
        if self.api_key.is_empty() {
            return Err(LlmError::MissingApiKey);
        }

        let url = format!(
            "{API_BASE}/{model}:streamGenerateContent?alt=sse",
            model = self.model
        );
        let body = build_request_body(&req, self.enable_grounding);

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
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
        let stream = sse_to_event_stream(byte_stream);
        Ok(Box::pin(stream))
    }
}

// ---------- SSE-parsing ----------

#[derive(Debug, Deserialize)]
struct SseChunk {
    #[serde(default)]
    candidates: Vec<SseCandidate>,
}

#[derive(Debug, Deserialize)]
struct SseCandidate {
    #[serde(default)]
    content: Option<SseContent>,
    #[serde(default, rename = "finishReason")]
    finish_reason: Option<String>,
    #[serde(default, rename = "groundingMetadata")]
    grounding_metadata: Option<SseGroundingMetadata>,
}

#[derive(Debug, Deserialize)]
struct SseContent {
    #[serde(default)]
    parts: Vec<SsePart>,
}

#[derive(Debug, Deserialize)]
struct SsePart {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseGroundingMetadata {
    #[serde(default, rename = "webSearchQueries")]
    web_search_queries: Vec<String>,
    #[serde(default, rename = "groundingChunks")]
    grounding_chunks: Vec<SseGroundingChunk>,
}

#[derive(Debug, Deserialize)]
struct SseGroundingChunk {
    #[serde(default)]
    web: Option<SseGroundingWeb>,
}

#[derive(Debug, Deserialize)]
struct SseGroundingWeb {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

fn sse_to_event_stream<S>(byte_stream: S) -> impl Stream<Item = Result<GeminiEvent, LlmError>>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    async_stream::try_stream! {
        let mut buf = String::new();
        let mut bs = Box::pin(byte_stream);
        while let Some(chunk) = bs.next().await {
            let bytes = chunk.map_err(|e| LlmError::Http(e.to_string()))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));
            loop {
                let Some(nl) = buf.find('\n') else { break };
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);
                if line.is_empty() || !line.starts_with("data:") {
                    continue;
                }
                let data = line.trim_start_matches("data:").trim();
                // Gemini avslutar strömmen genom att stänga connection
                // (ingen explicit `[DONE]`-sentinel som OpenAI), men vi
                // accepterar den ändå för defensiv parsing.
                if data == "[DONE]" {
                    return;
                }
                let parsed: Result<SseChunk, _> = serde_json::from_str(data);
                match parsed {
                    Ok(c) => {
                        for cand in c.candidates {
                            if let Some(content) = cand.content {
                                for part in content.parts {
                                    if let Some(text) = part.text {
                                        if !text.is_empty() {
                                            yield GeminiEvent::Text(text);
                                        }
                                    }
                                }
                            }
                            if let Some(meta) = cand.grounding_metadata {
                                let chunks: Vec<GeminiGroundingChunk> = meta
                                    .grounding_chunks
                                    .into_iter()
                                    .filter_map(|gc| gc.web)
                                    .filter_map(|w| {
                                        Some(GeminiGroundingChunk {
                                            title: w.title.unwrap_or_default(),
                                            uri: w.uri.unwrap_or_default(),
                                        })
                                    })
                                    .filter(|c| !c.title.is_empty() || !c.uri.is_empty())
                                    .collect();
                                if !meta.web_search_queries.is_empty() || !chunks.is_empty() {
                                    yield GeminiEvent::Grounding {
                                        queries: meta.web_search_queries,
                                        chunks,
                                    };
                                }
                            }
                            // Om Gemini blockerat svar på säkerhets-grund,
                            // rapportera det som protokollfel så UI:t kan visa
                            // ett begripligt meddelande istället för att bara
                            // få tomt svar.
                            if let Some(reason) = &cand.finish_reason {
                                if reason == "SAFETY" || reason == "BLOCKLIST" || reason == "PROHIBITED_CONTENT" {
                                    Err(LlmError::Protocol(format!(
                                        "Gemini blockerade svaret (finishReason={reason})"
                                    )))?;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("gemini SSE parse skippad: {e} (line: {})", data);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::TurnContent;

    #[test]
    fn missing_api_key_returns_error() {
        let client = GeminiClient::new("");
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
    fn default_model_is_flash() {
        let client = GeminiClient::new("k");
        assert_eq!(client.model, "gemini-2.5-flash");
    }

    #[test]
    fn with_model_overrides_default() {
        let client = GeminiClient::new("k").with_model("gemini-2.5-pro");
        assert_eq!(client.model, "gemini-2.5-pro");
    }

    #[test]
    fn grounding_off_by_default() {
        let client = GeminiClient::new("k");
        assert!(!client.enable_grounding);
    }

    #[test]
    fn with_grounding_flips_flag() {
        let client = GeminiClient::new("k").with_grounding(true);
        assert!(client.enable_grounding);
    }

    #[test]
    fn request_body_maps_roles_user_and_model() {
        let req = LlmRequest {
            system: None,
            turns: vec![
                TurnContent {
                    role: Role::User,
                    text: "hej".into(),
                },
                TurnContent {
                    role: Role::Assistant,
                    text: "hej på dig".into(),
                },
            ],
            temperature: 0.3,
            max_tokens: 64,
        };
        let body = build_request_body(&req, false);
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "hej");
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["text"], "hej på dig");
    }

    #[test]
    fn request_body_puts_system_in_system_instruction() {
        let req = LlmRequest {
            system: Some("Du är svensk.".into()),
            turns: vec![TurnContent {
                role: Role::User,
                text: "x".into(),
            }],
            temperature: 0.3,
            max_tokens: 64,
        };
        let body = build_request_body(&req, false);
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "Du är svensk."
        );
        // System får inte smyga in i contents[].role
        for c in body["contents"].as_array().unwrap() {
            assert_ne!(c["role"], "system");
        }
    }

    #[test]
    fn request_body_includes_google_search_when_grounding_on() {
        let req = LlmRequest {
            system: None,
            turns: vec![TurnContent {
                role: Role::User,
                text: "väder".into(),
            }],
            temperature: 0.3,
            max_tokens: 64,
        };
        let body = build_request_body(&req, true);
        assert!(body["tools"].is_array());
        assert_eq!(body["tools"][0]["googleSearch"], serde_json::json!({}));
    }

    #[test]
    fn request_body_omits_tools_when_grounding_off() {
        let req = LlmRequest {
            system: None,
            turns: vec![TurnContent {
                role: Role::User,
                text: "hej".into(),
            }],
            temperature: 0.3,
            max_tokens: 64,
        };
        let body = build_request_body(&req, false);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn canonicalize_merges_consecutive_same_role_turns() {
        let turns = vec![
            TurnContent {
                role: Role::User,
                text: "fråga 1".into(),
            },
            TurnContent {
                role: Role::User,
                text: "fråga 2".into(),
            },
            TurnContent {
                role: Role::Assistant,
                text: "svar".into(),
            },
            TurnContent {
                role: Role::User,
                text: "fråga 3".into(),
            },
        ];
        let out = canonicalize_turns(&turns);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0].role, Role::User));
        assert_eq!(out[0].text, "fråga 1\n\nfråga 2");
        assert!(matches!(out[1].role, Role::Assistant));
        assert_eq!(out[1].text, "svar");
        assert!(matches!(out[2].role, Role::User));
        assert_eq!(out[2].text, "fråga 3");
    }

    #[test]
    fn canonicalize_leaves_alternating_turns_unchanged() {
        let turns = vec![
            TurnContent {
                role: Role::User,
                text: "a".into(),
            },
            TurnContent {
                role: Role::Assistant,
                text: "b".into(),
            },
            TurnContent {
                role: Role::User,
                text: "c".into(),
            },
        ];
        let out = canonicalize_turns(&turns);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].text, "a");
        assert_eq!(out[1].text, "b");
        assert_eq!(out[2].text, "c");
    }

    #[test]
    fn request_body_produces_alternating_roles_after_canonicalize() {
        // Simulera en trasig history där stream:en failade mitt i (Gemini
        // agentic round utan assistant-turn) och follow-up appendade ett
        // nytt user-turn. Utan canonicalize skulle Gemini svara 400.
        let req = LlmRequest {
            system: None,
            turns: vec![
                TurnContent {
                    role: Role::User,
                    text: "vädret i Stockholm".into(),
                },
                TurnContent {
                    role: Role::User,
                    text: "och i Göteborg?".into(),
                },
            ],
            temperature: 0.3,
            max_tokens: 64,
        };
        let body = build_request_body(&req, false);
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(
            contents[0]["parts"][0]["text"],
            "vädret i Stockholm\n\noch i Göteborg?"
        );
    }

    #[test]
    fn request_body_sets_generation_config() {
        let req = LlmRequest {
            system: None,
            turns: vec![TurnContent {
                role: Role::User,
                text: "hej".into(),
            }],
            temperature: 0.7,
            max_tokens: 2048,
        };
        let body = build_request_body(&req, false);
        assert!((body["generationConfig"]["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 2048);
    }
}
