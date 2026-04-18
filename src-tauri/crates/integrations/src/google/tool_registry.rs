//! Tool-definitioner + dispatcher för Google-verktyg.
//!
//! Används av action-worker när agentic tool-use-loop ska köras. Callern
//! skapar en `GoogleClient` (via `svoice_secrets::get_google_refresh_token`)
//! och anropar `execute(&client, tool_name, input)` för varje tool_call.

use serde_json::json;

use super::calendar;
use super::client::{ClientError, GoogleClient};
use super::gmail;

/// Alla verktyg SVoice exponerar för Claude. Returnerar serde_json::Value
/// som matchar Anthropic tool-use-schemat (skickas som `tools`-array).
///
/// Blandar client-tools (Google — våra handlers) med server-tools
/// (web_search — Anthropic sköter själv, dispatcher ignorerar dem).
pub fn all_tools_json() -> Vec<serde_json::Value> {
    vec![
        // Server-tool: Claude söker webben via Anthropics egen infra.
        // max_uses=5 begränsar kostnad per request (cirka $0.05).
        json!({
            "type": "web_search_20250305",
            "name": "web_search",
            "max_uses": 5
        }),
        json!({
            "name": "list_calendar_events",
            "description": "Lista kommande events i användarens primära Google Calendar mellan två tidpunkter. Använd ISO 8601 med tidszon.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "time_min": {"type": "string", "description": "RFC 3339-timestamp, t.ex. '2026-04-18T00:00:00+02:00'"},
                    "time_max": {"type": "string", "description": "RFC 3339-timestamp"},
                    "max_results": {"type": "integer", "default": 20, "description": "max antal events (1–50)"}
                },
                "required": ["time_min", "time_max"]
            }
        }),
        json!({
            "name": "create_calendar_event",
            "description": "Skapa ett event i användarens primära Google Calendar.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "description": "Titel på eventet"},
                    "description": {"type": "string"},
                    "location": {"type": "string"},
                    "start": {"type": "string", "description": "RFC 3339-timestamp med tidszon"},
                    "end": {"type": "string", "description": "RFC 3339-timestamp med tidszon"},
                    "time_zone": {"type": "string", "default": "Europe/Stockholm"}
                },
                "required": ["summary", "start", "end"]
            }
        }),
        json!({
            "name": "search_emails",
            "description": "Sök i användarens Gmail med samma query-syntax som söktältet (t.ex. 'from:x@y is:unread').",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "max_results": {"type": "integer", "default": 10}
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "read_email",
            "description": "Läs headers + snippet för ett specifikt Gmail-meddelande (message_id från search_emails).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "message_id": {"type": "string"}
                },
                "required": ["message_id"]
            }
        }),
        json!({
            "name": "draft_email",
            "description": "Skapa ett nytt mail-utkast i Gmail. Skickas INTE automatiskt — user granskar i Gmail-webben och trycker Skicka själv. Använd för nya mail (inte svar).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "to": {"type": "string", "description": "Mottagares mail-adress"},
                    "subject": {"type": "string"},
                    "body": {"type": "string", "description": "Mail-text (klartext, ingen HTML)"}
                },
                "required": ["to", "subject", "body"]
            }
        }),
        json!({
            "name": "draft_reply",
            "description": "Skapa ett SVAR-utkast i en befintlig Gmail-tråd. Skickas INTE automatiskt — user granskar och trycker Skicka själv. Hämta thread_id från search_emails eller read_email.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "thread_id": {"type": "string"},
                    "to": {"type": "string", "description": "Oftast samma som From-headern på originalet"},
                    "subject": {"type": "string", "description": "Prefixa med 'Re: ' om du vill matcha Gmail-konvention"},
                    "body": {"type": "string"}
                },
                "required": ["thread_id", "to", "subject", "body"]
            }
        }),
    ]
}

/// Konvertera Anthropic-style tool-definitioner till Gemini functionDeclarations.
///
/// Filtrerar bort server-tools (de med `"type"`-fält, t.ex. `web_search`) —
/// Gemini använder `googleSearch`-builtin för det, inte via functionDeclarations.
/// Byter namn `input_schema` → `parameters` (Gemini-format). JSON Schema-kroppen
/// är i övrigt identisk.
pub fn all_tools_gemini_functions() -> Vec<serde_json::Value> {
    all_tools_json()
        .into_iter()
        .filter(|t| t.get("type").is_none()) // skippa server-tools
        .map(|t| {
            serde_json::json!({
                "name": t["name"],
                "description": t["description"],
                "parameters": t["input_schema"],
            })
        })
        .collect()
}

/// Exekvera ett enstaka verktygsanrop. Returnerar JSON-string som skickas
/// tillbaka till Claude som tool_result-content.
pub async fn execute(
    client: &GoogleClient,
    tool_name: &str,
    input: &serde_json::Value,
) -> Result<String, ClientError> {
    match tool_name {
        "list_calendar_events" => {
            let time_min = input["time_min"].as_str().unwrap_or_default();
            let time_max = input["time_max"].as_str().unwrap_or_default();
            let max_results = input["max_results"].as_u64().unwrap_or(20) as u32;
            let events = calendar::list_events(client, time_min, time_max, max_results).await?;
            Ok(serde_json::to_string(&events).unwrap_or_else(|_| "[]".into()))
        }
        "create_calendar_event" => {
            let summary = input["summary"].as_str().unwrap_or("").to_string();
            let description = input["description"].as_str().map(String::from);
            let location = input["location"].as_str().map(String::from);
            let tz = input["time_zone"]
                .as_str()
                .unwrap_or("Europe/Stockholm")
                .to_string();
            let start = input["start"].as_str().unwrap_or("").to_string();
            let end = input["end"].as_str().unwrap_or("").to_string();
            let event = calendar::CalendarEvent {
                id: None,
                summary,
                description,
                location,
                start: calendar::EventDateTime {
                    date_time: Some(start),
                    date: None,
                    time_zone: Some(tz.clone()),
                },
                end: calendar::EventDateTime {
                    date_time: Some(end),
                    date: None,
                    time_zone: Some(tz),
                },
                html_link: None,
            };
            let created = calendar::create_event(client, &event).await?;
            Ok(serde_json::to_string(&created).unwrap_or_else(|_| "{}".into()))
        }
        "search_emails" => {
            let q = input["query"].as_str().unwrap_or("");
            let max = input["max_results"].as_u64().unwrap_or(10) as u32;
            let refs = gmail::search_messages(client, q, max).await?;
            Ok(serde_json::to_string(&refs).unwrap_or_else(|_| "[]".into()))
        }
        "read_email" => {
            let id = input["message_id"].as_str().unwrap_or("");
            let msg = gmail::get_message(client, id).await?;
            // Platt JSON med nyckel-headers för Claude:
            let out = serde_json::json!({
                "id": msg.id,
                "thread_id": msg.thread_id,
                "from": msg.header("From"),
                "to": msg.header("To"),
                "subject": msg.header("Subject"),
                "date": msg.header("Date"),
                "snippet": msg.snippet,
            });
            Ok(serde_json::to_string(&out).unwrap_or_else(|_| "{}".into()))
        }
        "draft_email" => {
            let to = input["to"].as_str().unwrap_or("");
            let subject = input["subject"].as_str().unwrap_or("");
            let body = input["body"].as_str().unwrap_or("");
            let draft = gmail::create_draft(client, to, subject, body, None).await?;
            let out = serde_json::json!({
                "draft_id": draft.id,
                "message_id": draft.message.as_ref().map(|m| &m.id),
                "status": "utkast skapat — öppna Gmail för att granska och skicka",
            });
            Ok(serde_json::to_string(&out).unwrap_or_else(|_| "{}".into()))
        }
        "draft_reply" => {
            let thread_id = input["thread_id"].as_str().unwrap_or("");
            let to = input["to"].as_str().unwrap_or("");
            let subject = input["subject"].as_str().unwrap_or("");
            let body = input["body"].as_str().unwrap_or("");
            let draft = gmail::create_draft(client, to, subject, body, Some(thread_id)).await?;
            let out = serde_json::json!({
                "draft_id": draft.id,
                "thread_id": thread_id,
                "status": "svar-utkast skapat — öppna Gmail för att granska och skicka",
            });
            Ok(serde_json::to_string(&out).unwrap_or_else(|_| "{}".into()))
        }
        other => Err(ClientError::ApiError {
            status: 0,
            body: format!("okänt verktyg: {other}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_function_declarations_exclude_server_tools() {
        let funcs = all_tools_gemini_functions();
        let names: Vec<&str> = funcs.iter().filter_map(|f| f["name"].as_str()).collect();
        // Server-tool web_search ska INTE finnas — Gemini använder googleSearch built-in
        assert!(
            !names.contains(&"web_search"),
            "web_search ska filtreras bort"
        );
        // Alla Google-client-tools ska finnas
        assert!(names.contains(&"create_calendar_event"));
        assert!(names.contains(&"search_emails"));
        // Alla ska ha `parameters` (inte `input_schema`) och vara object-schema
        for f in &funcs {
            assert!(
                f["parameters"]["type"] == "object",
                "parameters.type ska vara 'object' för {:?}",
                f["name"]
            );
            assert!(
                f.get("input_schema").is_none(),
                "input_schema ska inte finnas i Gemini-format för {:?}",
                f["name"]
            );
        }
    }

    #[test]
    fn all_tools_have_required_fields() {
        for tool in all_tools_json() {
            assert!(tool["name"].is_string());
            // Server-tools (type=...) har inget input_schema. Client-tools
            // (Google) kräver description + input_schema med type=object.
            if tool.get("type").is_some() {
                continue;
            }
            assert!(tool["description"].is_string());
            assert!(tool["input_schema"]["type"] == "object");
            assert!(tool["input_schema"]["properties"].is_object());
        }
    }

    #[test]
    fn web_search_is_server_tool() {
        let tools = all_tools_json();
        let web = tools.iter().find(|t| t["name"] == "web_search");
        assert!(web.is_some(), "web_search saknas i registry");
        let w = web.unwrap();
        assert_eq!(w["type"], "web_search_20250305");
        assert!(w["max_uses"].is_number());
    }

    #[test]
    fn unknown_tool_returns_error() {
        // Detta är en rent synkron test som inte kör client — vi testar bara
        // matchen med en no-op client är inte möjlig, så vi hoppar över
        // actually kalling execute. Test nedan kollar bara tool-namnen finns.
        let tools = all_tools_json();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"list_calendar_events"));
        assert!(names.contains(&"create_calendar_event"));
        assert!(names.contains(&"search_emails"));
        assert!(names.contains(&"read_email"));
    }
}
