//! SVoice LLM-klient (iter 3).
//!
//! Provider-trait med Anthropic som primär implementation. Ollama-stöd
//! (lokalt) + OpenAI-compat kommer i iter 3b/4.
//!
//! **Designprincip:** backend är streaming-först. `complete` returnerar en
//! async stream av text-chunks så UI kan visa tokens live i action-popup.

pub mod anthropic;
pub mod ollama;
pub mod provider;

pub use anthropic::AnthropicClient;
pub use ollama::OllamaClient;
pub use provider::{LlmError, LlmProvider, LlmRequest, LlmStream, Role, TurnContent};
