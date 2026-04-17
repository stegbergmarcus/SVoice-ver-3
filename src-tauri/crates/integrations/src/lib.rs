//! Externa integrationer för SVoice 3.
//!
//! För närvarande bara Google (Gmail + Calendar i iter 4). Senare: Outlook,
//! Slack, Notion (iter 5+).
//!
//! Arkitekturprincip: varje integration är en self-contained modul med
//! OAuth-flow, token-storage (via svoice-secrets) och REST-wrappers.

pub mod google;

pub use google::oauth::{GoogleAuthError, GoogleOAuthFlow, GoogleTokens};
