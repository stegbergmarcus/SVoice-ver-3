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

use futures_util::StreamExt;
use serde::Serialize;
use svoice_integrations::google::{tool_registry, GoogleClient};
use svoice_llm::{
    tool_step_with_choice, GeminiClient, GeminiEvent, GeminiGroundingChunk, LlmRequest, Role,
    StepOutcome, ToolConversation, ToolResult, TurnContent,
};
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

/// Simpel heuristik för att gissa om kommandot kräver externa verktyg.
/// **Används inte längre i produktion** — handle_action_released triggar
/// alltid agentic-flow i query-mode och låter Claude själv avgöra via
/// sitt system-prompt. Heuristiken missade svenska böjningar (t.ex.
/// "vädret" innehåller inte substrängen "väder"). Behålls för eventuell
/// framtida rule-based fallback och täcks av enhetstester nedan.
#[allow(dead_code)]
pub fn looks_agentic(command: &str, selection: Option<&str>) -> bool {
    // Om user har markerat text i ett fönster, är det nästan alltid en
    // transform-request (omformulera/korrigera) — ingen tool-use.
    if selection.map_or(false, |s| !s.trim().is_empty()) {
        return false;
    }
    let c = command.to_lowercase();
    tracing::debug!(
        "looks_agentic: lowered_bytes={:?} chars={}",
        c.as_bytes(),
        c.chars().count()
    );
    const KEYWORDS: &[&str] = &[
        // Kalender
        "kalender",
        "kalendern",
        "boka",
        "möte",
        "mötet",
        "mötes",
        "möten",
        "schemalägg",
        "schema",
        "träff",
        "inboka",
        "avboka",
        "flytta mötet",
        "nästa möte",
        "idag",
        "imorgon",
        "i övermorgon",
        "denna vecka",
        "nästa vecka",
        "vad har jag",
        "vad händer",
        "när är",
        // Gmail
        "mail",
        "mejl",
        "mejlet",
        "mailet",
        "maila",
        "mejla",
        "svara på",
        "skicka mail",
        "inkorgen",
        "inkorg",
        "läs mailet",
        "sök mail",
        "från marcus",
        // Webbsökning
        "sök",
        "slå upp",
        "kolla upp",
        "googla",
        "vad är",
        "vem är",
        "när var",
        "hur många",
        "aktuell",
        "senaste nyheter",
        "väder",
        "priset på",
        "har jag fått",
        "senaste mailet",
        "oläst",
    ];
    KEYWORDS.iter().any(|kw| c.contains(kw))
}

pub struct AgenticRequirements {
    pub api_key: String,
    pub model: String,
    /// Google-anslutning är optional — om saknas skickas bara web_search-
    /// verktyget till Claude (inga Calendar/Gmail-verktyg).
    pub google: Option<GoogleRequirements>,
}

pub struct GoogleRequirements {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub refresh_token: String,
}

/// Samla allt som behövs för agentic flow. Returnerar None om Anthropic-
/// nyckel saknas (utan Claude går det inte att köra agentic alls).
/// Google-anslutning är optional.
pub fn prepare_agentic(settings: &Settings) -> Option<AgenticRequirements> {
    let api_key = svoice_secrets::get_anthropic_key().ok().flatten()?;
    if api_key.is_empty() {
        return None;
    }
    let google = match (
        settings.google_oauth_client_id.as_deref().filter(|s| !s.is_empty()),
        svoice_secrets::get_google_refresh_token().ok().flatten(),
    ) {
        (Some(cid), Some(refresh)) => Some(GoogleRequirements {
            client_id: cid.to_string(),
            client_secret: settings
                .google_oauth_client_secret
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(String::from),
            refresh_token: refresh,
        }),
        _ => None,
    };
    Some(AgenticRequirements {
        api_key,
        model: settings.anthropic_model.clone(),
        google,
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
via verktyg, och söker på webben för realtidsinfo. Svara alltid på svenska.\n\
\n\
Nuvarande tid: {now_iso} (tidszon: Europe/Stockholm, offset {offset}).\n\
\n\
Riktlinjer:\n\
- Om user säger 'imorgon kl 14', tolka som {tomorrow_iso}T14:00:00{offset}.\n\
- Vid skapande av kalenderhändelser: ge default 60 minuter längd om inget annat anges.\n\
- KRITISKT: Om användaren ber om realtidsdata eller om du INTE kan vara säker på \
att svaret stämmer idag, använd web_search. Din träningsdata är månader gammal — \
aktiekurser, väder, nyheter, valutor, sport-resultat, priser, datum/händelser \
efter träning har förändrats. Att gissa från minne är ett FEL när tool finns.\n\
- Om användaren säger 'sök', 'googla', 'kolla upp', 'slå upp', 'leta', \
'hitta info om', 'vad säger webben' eller liknande — det är en direkt instruktion \
att använda web_search. Följ den.\n\
- För resonemang, språk-frågor, matematik, kodförklaringar etc: svara direkt utan \
verktyg. Onödig sökning är slöseri med tid.\n\
- När verktyg använts, sammanfatta resultatet kort på svenska och ange källa kort \
(t.ex. 'enligt smhi.se' eller 'enligt senaste från dn.se').\n\
- Inga ursäkter, inga 'jag ska'; var direkt.",
        now_iso = now.format("%Y-%m-%dT%H:%M:%S"),
        tomorrow_iso = (now + chrono::Duration::days(1)).format("%Y-%m-%d"),
        offset = offset,
    )
}

fn tools_from_registry() -> Vec<serde_json::Value> {
    tool_registry::all_tools_json()
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
    // Bygg Google-client om ansluten. Filtrera bort Google-tools ur registry
    // om inte — så Claude inte försöker anropa Calendar/Gmail utan tokens.
    let google_client = req.google.as_ref().map(|g| {
        GoogleClient::new(
            g.client_id.clone(),
            g.client_secret.clone(),
            g.refresh_token.clone(),
        )
    });
    let tools: Vec<serde_json::Value> = tools_from_registry()
        .into_iter()
        .filter(|t| {
            // Behåll server-tools + Google-tools endast om Google är anslutna.
            if t.get("type").is_some() {
                // Server-tool (web_search) — alltid tillgängligt.
                true
            } else {
                google_client.is_some()
            }
        })
        .collect();
    let sys_prompt = system_prompt();
    let mut conv = ToolConversation::new(Some(sys_prompt.clone()), command.to_string());
    // Heuristisk tool_choice: om kommandot tydligt indikerar realtidsdata
    // (väder, aktier, nyheter, "just nu" etc) — tvinga Claude att använda
    // web_search istället för att lita på auto-val. Claude 4.5 är annars
    // benägen att svara från träningsdata trots system-prompt. För generella
    // frågor (fakta, språk, resonemang) lämnar vi auto så Claude själv
    // avgör — onödig websökning är slöseri med tokens och latens.
    let forced_web_search = requires_realtime_lookup(command);
    let initial_tool_choice = if forced_web_search && !tools.is_empty() {
        tracing::info!("tool_choice: tvingar web_search för realtids-query");
        Some(serde_json::json!({
            "type": "tool",
            "name": "web_search"
        }))
    } else {
        None
    };
    // Samla all text som Claude streamade till popupen — används för att spara
    // assistant-turnen i svoice_ipc::ACTIVE_CONVERSATION så nästa follow-up
    // ser hela konversationen (annars skickas bara user-turnen och Claude
    // tappar context om vad som just sagts).
    let mut assistant_accum = String::new();

    for round in 0..MAX_ROUNDS {
        // Bara round 0 får tvingad tool_choice. Efter första tool-resultat
        // ska Claude kunna välja att svara i text (type=auto) annars fastnar
        // loopen i en oändlig kedja av tool-calls.
        let choice = if round == 0 {
            initial_tool_choice.clone()
        } else {
            None
        };
        let outcome = tool_step_with_choice(
            &req.api_key,
            &req.model,
            &mut conv,
            &tools,
            1024,
            0.3,
            choice,
        )
        .await?;
        match outcome {
            StepOutcome::Finished { text } => {
                if !text.is_empty() {
                    assistant_accum.push_str(&text);
                    svoice_ipc::mark_action_streaming();
                    let _ = app.emit(ev_token, serde_json::json!({ "text": text }));
                }
                // Spara final assistant-svar + sätt system-prompten på
                // konversationen så follow-up-request kan byggas med full
                // context + svenska/realtids-riktlinjer.
                if !assistant_accum.is_empty() {
                    svoice_ipc::append_assistant_turn(assistant_accum.clone());
                }
                ensure_conversation_system(&sys_prompt);
                let _ = app.emit(ev_done, ());
                svoice_ipc::schedule_action_streaming_clear();
                return Ok(());
            }
            StepOutcome::NeedTools {
                calls,
                partial_text,
                assistant_blocks,
            } => {
                // Om Claude sagt något innan tool_use, emittera det direkt
                // och ackumulera så final turn sparas korrekt.
                if !partial_text.is_empty() {
                    assistant_accum.push_str(&partial_text);
                    svoice_ipc::mark_action_streaming();
                    let _ = app.emit(ev_token, serde_json::json!({ "text": partial_text }));
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
                    let (content, is_error, summary) = match google_client.as_ref() {
                        Some(g) => match tool_registry::execute(g, &call.name, &call.input).await {
                            Ok(text) => {
                                let s = short_summary_of_result(&call.name, &text);
                                (text, false, s)
                            }
                            Err(e) => {
                                let err_json = format!("{{\"error\":\"{}\"}}", e);
                                (err_json, true, Some(format!("fel: {e}")))
                            }
                        },
                        None => {
                            let err = format!(
                                "{{\"error\":\"Google-anslutning saknas för {}\"}}",
                                call.name
                            );
                            (err, true, Some("Google ej ansluten".into()))
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

/// Heuristik för att *tvinga* web_search via tool_choice i round 0. Används
/// som failsafe för fall där Claude annars skulle gissa från träningsdata
/// trots system-promptens realtids-instruktion. LLM:er är generellt lata
/// med tool-use — de underskattar sin egen kunskaps-cutoff.
///
/// Listan är avsiktligt kort: bara de signaler där det är entydigt
/// olämpligt att svara från minne (väder, kurser, specifika datum).
/// För mjuka fall ("sök efter X", "googla Y") förlitar vi oss på att
/// Claude förstår naturligt språk — systempromten nämner explicit dessa
/// trigger-ord. Om Claude ändå skulle svika lägger vi enkelt till
/// keywords här senare.
fn requires_realtime_lookup(command: &str) -> bool {
    let c = command.to_lowercase();
    const REALTIME_STEMS: &[&str] = &[
        // Väder — alltid realtid
        "väder",
        "vädret",
        "temperatur",
        "prognos",
        "regnar",
        "snöar",
        // Finans — minut-för-minut-data
        "aktiekurs",
        "börsen",
        "bitcoin",
        "växelkurs",
        "valutakurs",
        // Tidsspecifika signaler — "just nu", "idag" → behövs realtid
        "just nu",
        "idag",
        "ikväll",
        "senaste nyt",
        "senaste nyheter",
    ];
    REALTIME_STEMS.iter().any(|stem| c.contains(stem))
}

/// Sätt system-prompten på den aktiva konversationen om den inte redan
/// är satt. Anropas när agentic-flow klar så follow-up-request kan skickas
/// med samma riktlinjer (svenska, realtids-direktiv) istället för ingen
/// system-prompt alls.
fn ensure_conversation_system(sys: &str) {
    if let Ok(mut guard) = svoice_ipc::commands::ACTIVE_CONVERSATION.lock() {
        if let Some(conv) = guard.as_mut() {
            if conv.system.is_none() {
                conv.system = Some(sys.to_string());
            }
        }
    }
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
        "list_calendar_events" | "search_emails" => {
            parsed.as_array().map(|a| format!("{} träffar", a.len()))
        }
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

/// Kör en fresh query mot Gemini med Google Search-grounding aktiverat.
/// Parallell path till [`run_agentic`] (Claude+Google-tools) — används när
/// user valt Gemini som action-provider. Gemini's inbyggda grounding är
/// skarpare på realtidsdata (väder, aktier, nyheter) än Claude's web_search
/// eftersom modellen gör sökningen integrerat istället för via separat tool.
///
/// Emittar samma events som [`run_agentic`]:
/// - `action_llm_token` per text-chunk
/// - `action_tool_call` (name="web_search") när grounding-metadata visar
///   att Gemini faktiskt gjort en sökning
/// - `action_llm_done` vid slut
pub async fn run_agentic_gemini(
    app: &AppHandle,
    command: &str,
    api_key: String,
    model: String,
    ev_token: &'static str,
    ev_done: &'static str,
) -> anyhow::Result<()> {
    let sys_prompt = system_prompt();
    let client = GeminiClient::new(api_key)
        .with_model(model)
        .with_grounding(true);
    let req = LlmRequest {
        system: Some(sys_prompt.clone()),
        turns: vec![TurnContent {
            role: Role::User,
            text: command.to_string(),
        }],
        temperature: 0.3,
        max_tokens: 1024,
    };

    let mut stream = client.complete_stream_events(req).await?;
    let mut assistant_accum = String::new();
    let mut grounding_seen = false;
    let mut grounding_chunks: Vec<GeminiGroundingChunk> = Vec::new();

    while let Some(ev) = stream.next().await {
        match ev? {
            GeminiEvent::Text(text) => {
                if !text.is_empty() {
                    assistant_accum.push_str(&text);
                    svoice_ipc::mark_action_streaming();
                    let _ = app.emit(ev_token, serde_json::json!({ "text": text }));
                }
            }
            GeminiEvent::Grounding { queries, chunks } => {
                if !grounding_seen {
                    // Första grounding-signalen → visa "Söker på nätet"-chip.
                    // Summary = första search-query (om flera, visa första).
                    let summary = queries
                        .first()
                        .map(|q| format!("söker: {q}"))
                        .or_else(|| Some("söker på webben".into()));
                    let _ = app.emit(
                        EV_ACTION_TOOL_CALL,
                        ToolCallEvent {
                            name: "web_search".into(),
                            status: "running",
                            summary,
                        },
                    );
                    grounding_seen = true;
                }
                for c in chunks {
                    if !grounding_chunks.iter().any(|g| g.uri == c.uri && g.title == c.title) {
                        grounding_chunks.push(c);
                    }
                }
            }
            // run_agentic_gemini skickar inte functionDeclarations — Gemini ska inte
            // returnera FunctionCall-events här. Logga och ignorera defensivt om det
            // ändå sker (t.ex. om modellen hallucerar ett tool-call).
            GeminiEvent::FunctionCall { name, .. } => {
                tracing::warn!(
                    "run_agentic_gemini: oväntat FunctionCall-event för '{name}' ignoreras"
                );
            }
        }
    }

    if grounding_seen {
        // Summera 1-3 första käll-titles för status-chippet.
        let summary = if grounding_chunks.is_empty() {
            Some("sökning klar".into())
        } else {
            let titles: Vec<String> = grounding_chunks
                .iter()
                .take(3)
                .map(|c| {
                    if c.title.is_empty() {
                        c.uri.clone()
                    } else {
                        c.title.clone()
                    }
                })
                .collect();
            Some(titles.join(", "))
        };
        let _ = app.emit(
            EV_ACTION_TOOL_CALL,
            ToolCallEvent {
                name: "web_search".into(),
                status: "done",
                summary,
            },
        );
    }

    if !assistant_accum.is_empty() {
        svoice_ipc::append_assistant_turn(assistant_accum);
    }
    ensure_conversation_system(&sys_prompt);
    let _ = app.emit(ev_done, ());
    svoice_ipc::schedule_action_streaming_clear();
    Ok(())
}

/// Kör Gemini agentic-flow med full Google-tool-access (Calendar, Gmail)
/// + Google Search-grounding. Används när user har Google ansluten och
/// valt Gemini som action-provider. Utan Google-anslutning används
/// [`run_agentic_gemini`] istället (bara grounding, inga Google-tools).
///
/// Emittar samma events som [`run_agentic`] och [`run_agentic_gemini`]:
/// - `action_llm_token` per text-chunk (live under streaming)
/// - `action_tool_call` för varje tool-call-chip (running/done/error)
/// - `action_llm_done` vid slut
pub async fn run_agentic_gemini_tools(
    app: &AppHandle,
    command: &str,
    api_key: String,
    model: String,
    google: GoogleRequirements,
    ev_token: &'static str,
    ev_done: &'static str,
) -> anyhow::Result<()> {
    use svoice_llm::GeminiClient;

    let sys_prompt = system_prompt();
    let client = GeminiClient::new(api_key).with_model(model);
    let google_client = svoice_integrations::google::GoogleClient::new(
        google.client_id,
        google.client_secret,
        google.refresh_token,
    );

    // Gemini functionDeclarations (Anthropic-format → Gemini-format, utan server-tools).
    let function_declarations = tool_registry::all_tools_gemini_functions();

    // Bygg upp contents-arrayen; startar med en user-turn.
    let mut contents: Vec<serde_json::Value> = vec![serde_json::json!({
        "role": "user",
        "parts": [{ "text": command }]
    })];

    let mut assistant_accum = String::new();
    let mut grounding_chunks_accum: Vec<GeminiGroundingChunk> = Vec::new();
    let mut grounding_seen_chip = false;

    for round in 0..MAX_ROUNDS {
        tracing::info!(
            "Gemini agentic (tools) round {round}, {} function_declarations",
            function_declarations.len()
        );

        let mut stream = client
            .stream_tools(
                Some(&sys_prompt),
                contents.clone(),
                function_declarations.clone(),
                true, // enable_grounding — Gemini kan blanda websökning + Calendar/Gmail
                0.3,
                1024,
            )
            .await?;

        // Samla model-svar: text-parts (emittade live) + functionCall-parts (körs efter strömmen).
        let mut round_text = String::new();
        // (name, args, thought_signature) — signature måste skickas tillbaka
        // i samma part som functionCall i nästa roundtrip (Gemini 3+ krav).
        let mut function_calls: Vec<(String, serde_json::Value, Option<String>)> = Vec::new();

        while let Some(ev) = stream.next().await {
            match ev? {
                GeminiEvent::Text(t) => {
                    if !t.is_empty() {
                        round_text.push_str(&t);
                        assistant_accum.push_str(&t);
                        svoice_ipc::mark_action_streaming();
                        let _ = app.emit(ev_token, serde_json::json!({ "text": t }));
                    }
                }
                GeminiEvent::FunctionCall { name, args, thought_signature } => {
                    function_calls.push((name, args, thought_signature));
                }
                GeminiEvent::Grounding { queries, chunks } => {
                    if !grounding_seen_chip {
                        let summary = queries
                            .first()
                            .map(|q| format!("söker: {q}"))
                            .or_else(|| Some("söker på webben".into()));
                        let _ = app.emit(
                            EV_ACTION_TOOL_CALL,
                            ToolCallEvent {
                                name: "web_search".into(),
                                status: "running",
                                summary,
                            },
                        );
                        grounding_seen_chip = true;
                    }
                    for c in chunks {
                        if !grounding_chunks_accum
                            .iter()
                            .any(|g| g.uri == c.uri && g.title == c.title)
                        {
                            grounding_chunks_accum.push(c);
                        }
                    }
                }
            }
        }

        // Inga function_calls → Gemini är klart. Avsluta loop.
        if function_calls.is_empty() {
            break;
        }

        // Lägg till model-turnen i contents (text + functionCalls i samma content-parts).
        let mut model_parts: Vec<serde_json::Value> = Vec::new();
        if !round_text.is_empty() {
            model_parts.push(serde_json::json!({ "text": round_text }));
        }
        for (name, args, signature) in &function_calls {
            let mut part = serde_json::json!({
                "functionCall": { "name": name, "args": args }
            });
            if let Some(sig) = signature {
                part["thoughtSignature"] = serde_json::Value::String(sig.clone());
            }
            model_parts.push(part);
        }
        contents.push(serde_json::json!({
            "role": "model",
            "parts": model_parts
        }));

        // Exekvera tool-calls, bygg functionResponse-parts.
        let mut response_parts: Vec<serde_json::Value> = Vec::new();
        for (name, args, _signature) in &function_calls {
            let _ = app.emit(
                EV_ACTION_TOOL_CALL,
                ToolCallEvent {
                    name: name.clone(),
                    status: "running",
                    summary: short_summary_of_input(name, args),
                },
            );
            let (response_json, is_error, summary) =
                match tool_registry::execute(&google_client, name, args).await {
                    Ok(text) => {
                        // Parsa JSON-strängen tillbaka till Value för Gemini-protokollet.
                        let parsed: serde_json::Value = serde_json::from_str(&text)
                            .unwrap_or_else(|_| serde_json::json!({ "raw": text }));
                        let sum = short_summary_of_result(name, &serde_json::to_string(&parsed).unwrap_or_default());
                        (parsed, false, sum)
                    }
                    Err(e) => {
                        let err_json = serde_json::json!({ "error": e.to_string() });
                        (err_json, true, Some(format!("fel: {e}")))
                    }
                };
            let _ = app.emit(
                EV_ACTION_TOOL_CALL,
                ToolCallEvent {
                    name: name.clone(),
                    status: if is_error { "error" } else { "done" },
                    summary,
                },
            );
            response_parts.push(serde_json::json!({
                "functionResponse": {
                    "name": name,
                    "response": response_json,
                }
            }));
        }

        // Tool-results som user-turn (Gemini-konvention: functionResponse i user-role).
        contents.push(serde_json::json!({
            "role": "user",
            "parts": response_parts
        }));
    }

    // Loop klar. Emittera done-chip för grounding om vi såg det.
    if grounding_seen_chip {
        let summary = if grounding_chunks_accum.is_empty() {
            Some("sökning klar".into())
        } else {
            let titles: Vec<String> = grounding_chunks_accum
                .iter()
                .take(3)
                .map(|c| {
                    if c.title.is_empty() {
                        c.uri.clone()
                    } else {
                        c.title.clone()
                    }
                })
                .collect();
            Some(titles.join(", "))
        };
        let _ = app.emit(
            EV_ACTION_TOOL_CALL,
            ToolCallEvent {
                name: "web_search".into(),
                status: "done",
                summary,
            },
        );
    }

    if !assistant_accum.is_empty() {
        svoice_ipc::append_assistant_turn(assistant_accum);
    }
    ensure_conversation_system(&sys_prompt);
    let _ = app.emit(ev_done, ());
    svoice_ipc::schedule_action_streaming_clear();
    Ok(())
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
    fn heuristic_triggers_on_web_search_words() {
        // Web search-keywords triggar agentic flöde.
        assert!(looks_agentic("vad är huvudstaden i Sverige", None));
        assert!(looks_agentic("slå upp priset på bitcoin", None));
        assert!(looks_agentic("googla senaste nyheter om AI", None));
    }

    #[test]
    fn heuristic_false_on_non_agentic_commands() {
        // Enkla kommandon utan tool-behov triggar inte agentic.
        assert!(!looks_agentic("översätt detta till engelska", None));
        assert!(!looks_agentic("skriv en dikt om hösten", None));
        assert!(!looks_agentic("förklara rekursion", None));
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
