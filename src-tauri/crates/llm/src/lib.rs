//! SVoice LLM-klient (iter 3).
//!
//! Provider-trait med Anthropic som primär implementation. Ollama-stöd
//! (lokalt) + OpenAI-compat kommer i iter 3b/4.
//!
//! **Designprincip:** backend är streaming-först. `complete` returnerar en
//! async stream av text-chunks så UI kan visa tokens live i action-popup.

pub mod anthropic;
pub mod groq;
pub mod ollama;
pub mod provider;
pub mod tools;

pub use anthropic::AnthropicClient;
pub use groq::GroqClient;
pub use ollama::{OllamaClient, OllamaModelInfo, PullProgress};
pub use provider::{LlmError, LlmProvider, LlmRequest, LlmStream, Role, TurnContent};
pub use tools::{
    step as tool_step, step_with_choice as tool_step_with_choice, StepOutcome, ToolCall,
    ToolConversation, ToolDef, ToolResult,
};
