//! Google-integrationer för SVoice 3.
//!
//! `oauth` hanterar OAuth 2.1 PKCE-flowet (anslut/koppla-från, token-refresh).
//! Refresh-token lagras i Windows Credential Manager via svoice-secrets;
//! access-token hålls bara i RAM (kort livslängd, ~1h).
//!
//! I iter 4 fas 2 läggs `calendar` och `gmail` till med REST-wrappers.

pub mod calendar;
pub mod callback_server;
pub mod client;
pub mod gmail;
pub mod oauth;
pub mod tool_registry;

pub use client::{ClientError, GoogleClient};
