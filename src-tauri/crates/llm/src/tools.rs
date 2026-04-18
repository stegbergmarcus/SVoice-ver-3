//! Tool-use-schema för Anthropic Messages API.
//!
//! Claude kan i icke-streaming-läge svara med `stop_reason == "tool_use"` +
//! `content`-block av typen `tool_use`. Caller kör verktyg lokalt och matar
//! tillbaka `tool_result`-block i nästa request. Loopen avslutas när Claude
//! returnerar `stop_reason == "end_turn"` med text-block.
//!
//! Denna modul innehåller datamodell + `step()`-funktionen som gör ETT varv i
//! loopen. Caller (t.ex. action-worker) orkestrerar själva loopen.

use serde::{Deserialize, Serialize};

use crate::provider::LlmError;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

/// Verktygs-definition som skickas till Claude.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Ett tool-call från Claude som caller ska exekvera.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Resultat av ett exekverat verktyg. `content` kan vara JSON-text eller
/// ren text; Claude ser det som plain text i nästa request.
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "is_false")]
    pub is_error: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Utfall av ett loop-steg.
#[derive(Debug)]
pub enum StepOutcome {
    /// Claude är klar. Text är samlat svar till user.
    Finished { text: String },
    /// Claude vill exekvera verktyg. Caller ska anropa dem och skicka tillbaka
    /// via `conv.add_tool_roundtrip(assistant_blocks, &results)`.
    NeedTools {
        calls: Vec<ToolCall>,
        partial_text: String,
        /// Rå content-array från Claude — skicka oförändrad till
        /// `add_tool_roundtrip` så assistant-turn:en bevaras korrekt.
        assistant_blocks: serde_json::Value,
    },
}

/// Konversations-state. Bygg med `new(system, user)`. Efter varje step ska
/// caller lägga till assistant-messagen + ev. tool_results med `add_turn`.
#[derive(Debug)]
pub struct ToolConversation {
    system: Option<String>,
    /// Rå content-block-array per message (user och assistant växlas).
    messages: Vec<serde_json::Value>,
}

impl ToolConversation {
    /// Skapa ny konversation med ett system-prompt och ett första user-meddelande.
    pub fn new(system: Option<String>, user_prompt: String) -> Self {
        Self {
            system,
            messages: vec![serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": user_prompt}],
            })],
        }
    }

    /// Lägg till ett råa assistant-message (från Claudes respons) OCH
    /// motsvarande user-message med tool_results. Ska bara anropas när
    /// `StepOutcome::NeedTools` returnerats.
    pub fn add_tool_roundtrip(
        &mut self,
        assistant_content: serde_json::Value,
        results: &[ToolResult],
    ) {
        self.messages.push(serde_json::json!({
            "role": "assistant",
            "content": assistant_content,
        }));
        let content: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": r.tool_use_id,
                    "content": r.content,
                    "is_error": r.is_error,
                })
            })
            .collect();
        self.messages.push(serde_json::json!({
            "role": "user",
            "content": content,
        }));
    }
}

/// Kör ett varv i tool-use-loopen. Non-streaming: hela svaret parsas på en gång.
///
/// `tools` är en raw JSON-array som kan blanda client-tools (med `input_schema`)
/// och Anthropics server-side tools (t.ex. `{"type": "web_search_20250305",
/// "name": "web_search", "max_uses": 5}`). Server-tools hanteras transparent
/// av Anthropic — våra handlers triggas inte för dem.
pub async fn step(
    api_key: &str,
    model: &str,
    conv: &mut ToolConversation,
    tools: &[serde_json::Value],
    max_tokens: u32,
    temperature: f32,
) -> Result<StepOutcome, LlmError> {
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "messages": conv.messages.clone(),
        "tools": tools,
    });
    if let Some(sys) = &conv.system {
        body["system"] = serde_json::json!(sys);
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(API_URL)
        .header("x-api-key", api_key)
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

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LlmError::Protocol(e.to_string()))?;
    parse_response(json, conv)
}

fn parse_response(
    json: serde_json::Value,
    conv: &mut ToolConversation,
) -> Result<StepOutcome, LlmError> {
    let stop_reason = json
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let content = json
        .get("content")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));

    let blocks = content
        .as_array()
        .cloned()
        .ok_or_else(|| LlmError::Protocol("content är inte array".into()))?;

    let mut text_buf = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in &blocks {
        let ty = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match ty {
            "text" => {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    text_buf.push_str(t);
                }
            }
            "tool_use" => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                tool_calls.push(ToolCall { id, name, input });
            }
            _ => {}
        }
    }

    if stop_reason == "tool_use" && !tool_calls.is_empty() {
        // Returnera assistant-blocks rå; caller matchar tool-results och
        // anropar add_tool_roundtrip(blocks, &results).
        Ok(StepOutcome::NeedTools {
            calls: tool_calls,
            partial_text: text_buf,
            assistant_blocks: serde_json::Value::Array(blocks),
        })
    } else {
        // end_turn eller max_tokens → klar. Lägg till assistant i conv för framtida ev. use.
        conv.messages.push(serde_json::json!({
            "role": "assistant",
            "content": blocks,
        }));
        Ok(StepOutcome::Finished { text: text_buf })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_roundtrip() {
        let mut conv = ToolConversation::new(Some("You are helpful".into()), "Hej".into());
        assert_eq!(conv.messages.len(), 1);
        conv.add_tool_roundtrip(
            serde_json::json!([{"type": "tool_use", "id": "t1", "name": "ping", "input": {}}]),
            &[ToolResult {
                tool_use_id: "t1".into(),
                content: "{\"pong\": true}".into(),
                is_error: false,
            }],
        );
        assert_eq!(conv.messages.len(), 3);
        assert_eq!(conv.messages[1]["role"], "assistant");
        assert_eq!(conv.messages[2]["role"], "user");
        let tool_result = &conv.messages[2]["content"][0];
        assert_eq!(tool_result["type"], "tool_result");
        assert_eq!(tool_result["tool_use_id"], "t1");
    }

    #[test]
    fn parses_finished_text_response() {
        let mut conv = ToolConversation::new(None, "hej".into());
        let json = serde_json::json!({
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "Hej Marcus!"}]
        });
        let outcome = parse_response(json, &mut conv).unwrap();
        match outcome {
            StepOutcome::Finished { text } => assert_eq!(text, "Hej Marcus!"),
            _ => panic!("förväntade Finished"),
        }
    }

    #[test]
    fn parses_tool_use_response() {
        let mut conv = ToolConversation::new(None, "lägg till möte".into());
        let json = serde_json::json!({
            "stop_reason": "tool_use",
            "content": [
                {"type": "text", "text": "Skapar möte..."},
                {"type": "tool_use", "id": "toolu_01", "name": "create_event",
                 "input": {"summary": "Möte", "start": "2026-04-19T14:00"}}
            ]
        });
        let outcome = parse_response(json, &mut conv).unwrap();
        match outcome {
            StepOutcome::NeedTools {
                calls,
                partial_text,
                assistant_blocks,
            } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_event");
                assert_eq!(calls[0].id, "toolu_01");
                assert_eq!(partial_text, "Skapar möte...");
                let arr = assistant_blocks.as_array().unwrap();
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[1]["type"], "tool_use");
            }
            _ => panic!("förväntade NeedTools"),
        }
    }

    #[test]
    fn roundtrip_with_assistant_blocks() {
        let mut conv = ToolConversation::new(Some("sys".into()), "hej".into());
        let blocks = serde_json::json!([
            {"type": "tool_use", "id": "t1", "name": "noop", "input": {}}
        ]);
        conv.add_tool_roundtrip(
            blocks,
            &[ToolResult {
                tool_use_id: "t1".into(),
                content: "\"ok\"".into(),
                is_error: false,
            }],
        );
        assert_eq!(conv.messages.len(), 3);
        assert_eq!(conv.messages[1]["role"], "assistant");
        assert_eq!(conv.messages[1]["content"][0]["id"], "t1");
    }
}
