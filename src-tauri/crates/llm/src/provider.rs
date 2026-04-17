use std::pin::Pin;

use futures_util::Stream;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContent {
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub system: Option<String>,
    pub turns: Vec<TurnContent>,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("ingen API-nyckel konfigurerad")]
    MissingApiKey,
    #[error("HTTP-fel: {0}")]
    Http(String),
    #[error("API-fel {status}: {body}")]
    Api { status: u16, body: String },
    #[error("protokoll-fel: {0}")]
    Protocol(String),
    #[error("oväntat svar: {0}")]
    Unexpected(String),
}

/// Stream av text-chunks från LLM. Varje Ok(String) är en token/delta;
/// Err avbryter streamen.
pub type LlmStream = Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>;

/// Gemensam LLM-provider-abstraktion. Alla provider-implementationer
/// (Anthropic, Ollama, OpenAI-compat) implementerar denna.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Kort namn för logging och UI ("anthropic", "ollama", etc).
    fn name(&self) -> &'static str;

    /// Starta en streaming-generering. Returnerar en stream av text-deltas.
    async fn complete_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError>;
}
