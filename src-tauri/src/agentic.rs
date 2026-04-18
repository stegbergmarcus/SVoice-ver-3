//! Agentic action-flow: kör tool-use-loop mot Claude när user-kommandot
//! ser ut att behöva externa verktyg (Calendar/Gmail).
//!
//! Fallback till vanlig text-streaming sker om:
//! - Google inte ansluten
//! - Heuristiken inte triggar
//! - Anthropic-nyckel saknas
//!
//! Emittar samma events som streaming-flödet (`action_llm_token`,
//! `action_llm_done`, `action_llm_error`) så popup:en inte behöver veta om
//! svaret kom agentic eller ren text. Dessutom `action_tool_call` för
//! status-rad under körning.

use serde::Serialize;
use svoice_integrations::google::{tool_registry, GoogleClient};
use svoice_llm::{tool_step, StepOutcome, ToolConversation, ToolDef, ToolResult};
use svoice_settings::Settings;
use tauri::{AppHandle, Emitter};

pub const EV_ACTION_TOOL_CALL: &str = "action_tool_call";
/// Maximalt antal tool-use-rounds innan vi avbryter (skydd mot oändlig loop).
const MAX_ROUNDS: usize = 6;

#[derive(Debug, Serialize, Clone)]
pub struct ToolCallEvent {
    pub name: String,
    pub status: &'static str, // "running" | "done" | "error"
    pub summary: Option<String>,
}

/// Simpel heuristik: trigga agentic-flow om kommandot innehåller ord som
/// rimligen kräver externa verktyg. Konservativt — föredrar false-negative
/// (vanlig text-streaming fungerar alltid) över false-positive (onödig
/// tool-loop på en enkel fråga).
pub fn looks_agentic(command: &str, selection: Option<&str>) -> bool {
    // Om user har markerat text i ett fönster, är det nästan alltid en
    // transform-request (omformulera/korrigera) — ingen tool-use.
    if selection.map_or(false, |s| !s.trim().is_empty()) {
        return false;
    }
    let c = command.to_lowercase();
    const KEYWORDS: &[&str] = &[
        // Kalender
        "kalender", "kalendern", "boka", "möte", "mötet", "mötes", "möten",
        "schemalägg", "schema", "träff", "inboka", "avboka", "flytta mötet",
        "nästa möte", "idag", "imorgon", "i övermorgon", "denna vecka",
        "nästa vecka", "vad har jag", "vad händer", "när är",
        // Gmail
        "mail", "mejl", "mejlet", "mailet", "maila", "mejla", "svara på",
        "skicka mail", "inkorgen", "inkorg", "läs mailet", "sök mail",
        "från marcus", "har jag fått", "senaste mailet", "oläst",
    ];
    KEYWORDS.iter().any(|kw| c.contains(kw))
}

pub struct AgenticRequirements {
    pub api_key: String,
    pub model: String,
    pub client_id: String,
    pub refresh_token: String,
}

/// Samla allt som behövs för agentic flow. Returnerar None om något saknas
/// (då fallback:ar caller till vanlig streaming).
pub fn prepare_agentic(settings: &Settings) -> Option<AgenticRequirements> {
    let api_key = svoice_secrets::get_anthropic_key().ok().flatten()?;
    if api_key.is_empty() {
        return None;
    }
    let client_id = settings
        .google_oauth_client_id
        .as_deref()
        .filter(|s| !s.is_empty())?
        .to_string();
    let refresh_token = svoice_secrets::get_google_refresh_token().ok().flatten()?;
    Some(AgenticRequirements {
        api_key,
        model: settings.anthropic_model.clone(),
        client_id,
        refresh_token,
    })
}

/// Systempromt för agentic flow. Inkluderar nuvarande tidpunkt + tidszon så
/// Claude kan tolka "imorgon kl 14" korrekt.
fn system_prompt() -> String {
    use chrono::Local;
    let now = Local::now();
    let offset = now.offset().to_string();
    format!(
        "Du är en svensk agentic assistent som hjälper användaren hantera Google Calendar och Gmail \
via verktyg. Svara alltid på svenska.\n\
\n\
Nuvarande tid: {now_iso} (tidszon: Europe/Stockholm, offset {offset}).\n\
\n\
Riktlinjer:\n\
- Om user säger 'imorgon kl 14', tolka som {tomorrow_iso}T14:00:00{offset}.\n\
- Vid skapande av kalenderhändelser: ge default 60 minuter längd om inget annat anges.\n\
- När verktyg inte behövs, svara direkt i text. Använd verktyg endast när det faktiskt hjälper.\n\
- Efter ett verktyg kört klart, sammanfatta resultatet kort och naturligt på svenska.\n\
- Inga ursäkter, inga 'jag ska'; var direkt.",
        now_iso = now.format("%Y-%m-%dT%H:%M:%S"),
        tomorrow_iso = (now + chrono::Duration::days(1)).format("%Y-%m-%d"),
        offset = offset,
    )
}

fn tools_from_registry() -> Vec<ToolDef> {
    tool_registry::all_tools_json()
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect()
}

/// Kör hela agentic-flödet. Emittar `action_llm_token` för slut-text så
/// popup:en kan rendera på vanligt sätt (en single chunk istället för
/// streaming).
///
/// Returnerar Err om flowet misslyckades — caller ska då emitta
/// `action_llm_error`.
pub async fn run_agentic(
    app: &AppHandle,
    command: &str,
    req: AgenticRequirements,
    ev_token: &'static str,
    ev_done: &'static str,
) -> anyhow::Result<()> {
    let google = GoogleClient::new(req.client_id, req.refresh_token);
    let tools = tools_from_registry();
    let mut conv = ToolConversation::new(Some(system_prompt()), command.to_string());

    for round in 0..MAX_ROUNDS {
        let outcome = tool_step(&req.api_key, &req.model, &mut conv, &tools, 1024, 0.3).await?;
        match outcome {
            StepOutcome::Finished { text } => {
                if !text.is_empty() {
                    let _ = app.emit(ev_token, serde_json::json!({ "text": text }));
                }
                let _ = app.emit(ev_done, ());
                return Ok(());
            }
            StepOutcome::NeedTools {
                calls,
                partial_text,
                assistant_blocks,
            } => {
                // Om Claude sagt något innan tool_use, emittera det direkt.
                if !partial_text.is_empty() {
                    let _ = app.emit(
                        ev_token,
                        serde_json::json!({ "text": partial_text }),
                    );
                }

                let mut results: Vec<ToolResult> = Vec::with_capacity(calls.len());
                for call in &calls {
                    let _ = app.emit(
                        EV_ACTION_TOOL_CALL,
                        ToolCallEvent {
                            name: call.name.clone(),
                            status: "running",
                            summary: short_summary_of_input(&call.name, &call.input),
                        },
                    );
                    let (content, is_error, summary) =
                        match tool_registry::execute(&google, &call.name, &call.input).await {
                            Ok(text) => {
                                let s = short_summary_of_result(&call.name, &text);
                                (text, false, s)
                            }
                            Err(e) => {
                                let err_json = format!("{{\"error\":\"{}\"}}", e);
                                (err_json, true, Some(format!("fel: {e}")))
                            }
                        };
                    let _ = app.emit(
                        EV_ACTION_TOOL_CALL,
                        ToolCallEvent {
                            name: call.name.clone(),
                            status: if is_error { "error" } else { "done" },
                            summary,
                        },
                    );
                    results.push(ToolResult {
                        tool_use_id: call.id.clone(),
                        content,
                        is_error,
                    });
                }

                conv.add_tool_roundtrip(assistant_blocks, &results);
                tracing::debug!("agentic round {} klar, {} tool-calls", round, calls.len());
            }
        }
    }

    anyhow::bail!("agentic-loop nådde max {MAX_ROUNDS} rounds utan att nå end_turn")
}

/// Kort läsbar beskrivning för UI-status vid tool-start.
fn short_summary_of_input(tool: &str, input: &serde_json::Value) -> Option<String> {
    match tool {
        "list_calendar_events" => {
            let min = input.get("time_min").and_then(|v| v.as_str()).unwrap_or("");
            let max = input.get("time_max").and_then(|v| v.as_str()).unwrap_or("");
            Some(format!("listar events {min} → {max}"))
        }
        "create_calendar_event" => input
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|s| format!("skapar \"{s}\"")),
        "search_emails" => input
            .get("query")
            .and_then(|v| v.as_str())
            .map(|q| format!("söker: {q}")),
        "read_email" => Some("läser mail".into()),
        _ => None,
    }
}

/// Kort beskrivning av resultat — "3 events hittades" etc.
fn short_summary_of_result(tool: &str, json_text: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(json_text).ok()?;
    match tool {
        "list_calendar_events" | "search_emails" => parsed
            .as_array()
            .map(|a| format!("{} träffar", a.len())),
        "create_calendar_event" => parsed
            .get("htmlLink")
            .and_then(|v| v.as_str())
            .map(|_| "möte skapat".to_string()),
        "read_email" => parsed
            .get("subject")
            .and_then(|v| v.as_str())
            .map(|s| format!("ämne: {s}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_triggers_on_calendar_words() {
        assert!(looks_agentic("boka ett möte imorgon kl 14", None));
        assert!(looks_agentic("vad har jag i kalendern idag", None));
        assert!(looks_agentic("nästa möte", None));
    }

    #[test]
    fn heuristic_triggers_on_mail_words() {
        assert!(looks_agentic("har jag fått mail från Marcus", None));
        assert!(looks_agentic("sök mail om kontraktet", None));
    }

    #[test]
    fn heuristic_false_on_transform() {
        assert!(!looks_agentic(
            "gör detta mer formellt",
            Some("hej på dig Marcus")
        ));
    }

    #[test]
    fn heuristic_false_on_plain_question() {
        assert!(!looks_agentic("vad är huvudstaden i Sverige", None));
        assert!(!looks_agentic("översätt detta till engelska", None));
    }

    #[test]
    fn summary_of_create_event_uses_summary_field() {
        let input = serde_json::json!({"summary": "Lunchmöte", "start": "...", "end": "..."});
        let s = short_summary_of_input("create_calendar_event", &input);
        assert_eq!(s.as_deref(), Some("skapar \"Lunchmöte\""));
    }

    #[test]
    fn result_summary_for_list_counts_items() {
        let json = r#"[{"id":"1"},{"id":"2"},{"id":"3"}]"#;
        let s = short_summary_of_result("list_calendar_events", json);
        assert_eq!(s.as_deref(), Some("3 träffar"));
    }
}
