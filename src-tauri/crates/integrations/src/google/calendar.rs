//! Google Calendar v3 — minimala wrappers för SVoice 3 action-LLM tool-use.

use serde::{Deserialize, Serialize};

use super::client::{ClientError, GoogleClient};

const API_BASE: &str = "https://www.googleapis.com/calendar/v3";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    #[serde(default)]
    pub id: Option<String>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start: EventDateTime,
    pub end: EventDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, rename = "htmlLink", skip_serializing_if = "Option::is_none")]
    pub html_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDateTime {
    /// RFC 3339 timestamp med tidszon, t.ex. "2026-04-18T14:00:00+02:00".
    /// Om bara `date` används: all-day event, ISO 8601-datum "2026-04-18".
    #[serde(rename = "dateTime", skip_serializing_if = "Option::is_none")]
    pub date_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "timeZone", skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}

/// Lista events i primary calendar mellan två tidpunkter.
/// `time_min` och `time_max` ska vara RFC 3339 timestamps.
pub async fn list_events(
    client: &GoogleClient,
    time_min: &str,
    time_max: &str,
    max_results: u32,
) -> Result<Vec<CalendarEvent>, ClientError> {
    let url = format!(
        "{API_BASE}/calendars/primary/events?timeMin={}&timeMax={}&maxResults={}&singleEvents=true&orderBy=startTime",
        urlencoding_simple(time_min),
        urlencoding_simple(time_max),
        max_results
    );
    let json = client.get(&url).await?;
    let items = json
        .get("items")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let events: Vec<CalendarEvent> =
        serde_json::from_value(items).map_err(|e| ClientError::ApiError {
            status: 0,
            body: format!("parse-fel: {e}"),
        })?;
    Ok(events)
}

/// Skapa ett event i primary calendar. Returnerar skapad event med id + html_link.
pub async fn create_event(
    client: &GoogleClient,
    event: &CalendarEvent,
) -> Result<CalendarEvent, ClientError> {
    let url = format!("{API_BASE}/calendars/primary/events");
    let json = client.post_json(&url, event).await?;
    serde_json::from_value(json).map_err(|e| ClientError::ApiError {
        status: 0,
        body: format!("parse-fel: {e}"),
    })
}

/// Minimal URL-encode för query-params. Inte så robust som `urlencoding`-crate
/// men tillräcklig för timestamps.
fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_timestamp() {
        let t = urlencoding_simple("2026-04-18T14:00:00+02:00");
        assert_eq!(t, "2026-04-18T14%3A00%3A00%2B02%3A00");
    }

    #[test]
    fn event_serializes_minimal() {
        let ev = CalendarEvent {
            id: None,
            summary: "Möte".into(),
            description: None,
            start: EventDateTime {
                date_time: Some("2026-04-18T14:00:00+02:00".into()),
                date: None,
                time_zone: Some("Europe/Stockholm".into()),
            },
            end: EventDateTime {
                date_time: Some("2026-04-18T15:00:00+02:00".into()),
                date: None,
                time_zone: Some("Europe/Stockholm".into()),
            },
            location: None,
            html_link: None,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"summary\":\"Möte\""));
        assert!(json.contains("\"dateTime\":\"2026-04-18T14:00:00+02:00\""));
        assert!(!json.contains("\"location\"")); // None skippas
    }

    #[test]
    fn event_deserializes_with_html_link() {
        let json = r#"{
            "id": "abc",
            "summary": "Test",
            "start": {"dateTime": "2026-04-18T14:00:00+02:00"},
            "end": {"dateTime": "2026-04-18T15:00:00+02:00"},
            "htmlLink": "https://calendar.google.com/event?eid=xxx"
        }"#;
        let ev: CalendarEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev.id.as_deref(), Some("abc"));
        assert_eq!(
            ev.html_link.as_deref(),
            Some("https://calendar.google.com/event?eid=xxx")
        );
    }
}
