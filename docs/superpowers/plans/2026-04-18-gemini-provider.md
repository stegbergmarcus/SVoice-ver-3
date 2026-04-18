# Gemini som femte LLM-provider — implementationsplan

**Datum:** 2026-04-18
**Syfte:** Lägga till Google Gemini som valbar LLM-provider (likt Claude/Ollama/Groq) med
 **Google Search Grounding** aktiverat — Gemini's motsvarighet till Claude's `web_search`-tool, men ofta skarpare på realtidsdata eftersom det bygger direkt på Google Search.

**Git-state vid planens tillkomst:** `main` @ `c68e9c4`, alla installationsfas-fixar
pushade. Appen är installerad som MSI med fyra providers (Auto/Claude/Ollama/Groq).

---

## Kontext

### Varför Gemini?
- Claude 4.5 är "lat" med tool-use. Även med `tool_choice=tool` och tydlig system-prompt väljer Claude ofta att svara från träningsdata istället för att anropa `web_search`.
- Gemini 2.5 Flash/Pro har inbyggt Google Search-grounding som är djupt integrerat: modellen gör sökningen och svarar med inline-citeringar + käll-URL:er i `grounding_metadata`. Mer reliabelt för väder/aktier/nyheter.
- Gemini Flash är dessutom billigare per token än Claude Sonnet.

### Vad vill användaren
Kunna välja **Gemini** som `action_llm_provider` (och/eller `dictation_llm_provider`) via
 Settings → Action-LLM. När Gemini är vald + action-PTT i query-mode → request med
 `tools: [{google_search: {}}]` → svar med källor visas i popup.

---

## Befintlig arkitektur (läs innan du börjar)

### LLM-crate: `src-tauri/crates/llm/`
Alla providers impl:ar `trait LlmProvider`:
```rust
// crates/llm/src/provider.rs
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError>;
}
```
- `LlmRequest { system, turns, temperature, max_tokens }` (turns = Vec<TurnContent>)
- `LlmStream = Pin<Box<dyn Stream<Item = Result<String, LlmError>>>>`

Befintliga implementationer att använda som mall:
- **`crates/llm/src/anthropic.rs`** — SSE-streaming, Claude. Komplexast.
- **`crates/llm/src/groq.rs`** — OpenAI-kompatibel SSE. Enklast. **Starta här som template.**
- **`crates/llm/src/ollama.rs`** — NDJSON-streaming. Bra för att förstå chunked parsing.

Agentic/tool-use är separat från LlmProvider-traiten och lever i
 **`crates/llm/src/tools.rs`** (`tool_step` / `tool_step_with_choice` —
 Anthropic-specifik JSON-format). Gemini-grounding får en **egen path** i
 `src-tauri/src/agentic.rs` eftersom Gemini's tool-request är annorlunda
 (se "Grounding-path" nedan).

### Settings-crate: `src-tauri/crates/settings/src/lib.rs`
- `enum LlmProvider { Auto, Claude, Ollama, Groq }` — lägg till `Gemini`.
- `Settings`-struct har `action_llm_provider` + `dictation_llm_provider`
  (separerade i c68e9c4). Plus `groq_llm_model`, `ollama_model` etc.
- Default: `LlmProvider::Auto`.

### Secrets-crate: `src-tauri/crates/secrets/src/lib.rs`
Windows Credential Manager wrapper. Tre nycklar idag:
- `anthropic_api_key` → `get_anthropic_key`, `set_anthropic_key`, `clear_anthropic_key`, `has_anthropic_key`
- `groq_api_key` → `get_groq_key`, `set_groq_key`, `clear_groq_key`, `has_groq_key`
- `google_refresh_token` → för OAuth (annat flöde — inget att göra med Gemini-API-nyckel)

Lägg till **gemini_api_key** i samma mönster. `service = "svoice-v3"`, `username = "gemini_api_key"`.

### IPC-crate: `src-tauri/crates/ipc/src/commands.rs`
Tauri-commands för frontend. Befintliga secret-helpers:
- `set_anthropic_key`, `clear_anthropic_key`, `has_anthropic_key`
- `set_groq_key`, `clear_groq_key`, `has_groq_key`

Lägg till: `set_gemini_key`, `clear_gemini_key`, `has_gemini_key`.

### Provider-selection: `src-tauri/src/lib.rs:1180+`
```rust
async fn select_llm_provider(
    choice: ProviderChoice,
    settings: &Settings,
    anthropic_key: Option<&str>,
) -> Option<Arc<dyn LlmProvider>>
```
Lägg till en `ProviderChoice::Gemini`-gren + `build_gemini()`-closure som läser
 gemini-nyckel från secrets och konstruerar `GeminiClient`.

### Agentic-flow: `src-tauri/src/agentic.rs`
`run_agentic` är Anthropic-specifik (använder `svoice_llm::tool_step_with_choice`
 som anropar Anthropic's messages-endpoint). För Gemini behövs en parallell
 `run_agentic_gemini`-funktion. Se "Grounding-path" nedan.

Wire-up är i `src-tauri/src/lib.rs` där `handle_action_released` väljer
 agentic-path:
```rust
if !is_follow_up && mode == "query" && heuristic_hit && prep.is_some() {
    // idag kör run_agentic (Claude)
}
```

### Frontend: `src/windows/Settings.tsx` + `src/lib/settings-api.ts`
- `LlmProviderChoice = "auto" | "claude" | "ollama" | "groq"` → lägg till `"gemini"`.
- `PROVIDER_LABELS` record → lägg till `gemini: "Gemini"`.
- Settings UI har **två segmented controls** (en per LLM-use-case). Båda får automatiskt
 den nya providern.
- Lägg till API-nyckel-fält + modell-val i samma struktur som Anthropic/Groq.

---

## Gemini API — tekniska detaljer

### Autentisering
- API-nyckel via `x-goog-api-key`-header eller `?key=<KEY>`-query-param.
- Hämtas från https://aistudio.google.com/apikey (gratis tier räcker för personlig use).

### Endpoint
```
POST https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:streamGenerateContent?alt=sse
```
Default-modell: `gemini-2.5-flash` (billig, snabb, stödjer grounding).
Alternativ: `gemini-2.5-pro` (dyrare, smartare).

### Request body (utan grounding)
```json
{
  "contents": [
    { "role": "user", "parts": [{"text": "Hej"}] },
    { "role": "model", "parts": [{"text": "Hej!"}] },
    { "role": "user", "parts": [{"text": "Vad är vädret?"}] }
  ],
  "systemInstruction": {
    "parts": [{"text": "Du är en svensk assistent..."}]
  },
  "generationConfig": {
    "temperature": 0.3,
    "maxOutputTokens": 1024
  }
}
```

### Request body (med Google Search grounding)
Lägg till:
```json
{
  "tools": [{"googleSearch": {}}]
}
```

### Streaming-format
SSE, samma som Claude. Varje chunk:
```
data: {"candidates":[{"content":{"parts":[{"text":"..."}]},"finishReason":null}],"usageMetadata":{...}}
```
Sista chunken har `finishReason: "STOP"`.

Grounding-metadata kommer i en separat chunk-del:
```json
"candidates": [{
  "groundingMetadata": {
    "webSearchQueries": ["väder Stockholm"],
    "groundingChunks": [
      { "web": { "uri": "https://smhi.se/...", "title": "SMHI" } }
    ],
    "groundingSupports": [
      { "segment": { "startIndex": 0, "endIndex": 42, "text": "..." },
        "groundingChunkIndices": [0], "confidenceScores": [0.95] }
    ]
  }
}]
```

### Rate limits (free tier)
- gemini-2.5-flash: 10 RPM, 250k TPM, 250 RPD
- Mer än tillräckligt för personlig use. Betald tier: 2000 RPM.

---

## Plan — implementeras i 5 faser

### Fas 1: `GeminiClient` (basic complete_stream utan grounding)
**Mål:** Kunna välja Gemini som provider för vanlig text-LLM (ingen websökning).
**Tid:** ~30 min.

#### Steg
1. Kopiera `crates/llm/src/groq.rs` till ny fil `crates/llm/src/gemini.rs`.
2. Byt alla Groq-referenser → Gemini. Nyckel-detaljer:
   - Base URL: `https://generativelanguage.googleapis.com/v1beta`
   - Authentication: `x-goog-api-key: {API_KEY}`-header
   - Request-format: Gemini `contents[].parts[].text` istället för OpenAI `messages`
   - System-prompt i separat `systemInstruction`-fält (inte som en `role: "system"`)
   - Rol-mapping: `Role::User` → `"user"`, `Role::Assistant` → `"model"`
   - Parse SSE-chunks: `candidates[0].content.parts[0].text`
3. Lägg till dep i `crates/llm/Cargo.toml` (reqwest finns redan, inget nytt behövs).
4. Exportera från `crates/llm/src/lib.rs`:
   ```rust
   pub use gemini::GeminiClient;
   ```

#### Exempel-skeleton
```rust
// crates/llm/src/gemini.rs
pub struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::Client,
    enable_grounding: bool,  // används i Fas 2
}

impl GeminiClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "gemini-2.5-flash".into(),
            http: reqwest::Client::new(),
            enable_grounding: false,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_grounding(mut self, enabled: bool) -> Self {
        self.enable_grounding = enabled;
        self
    }
}

#[async_trait::async_trait]
impl LlmProvider for GeminiClient {
    async fn complete_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
            self.model
        );
        let mut body = serde_json::json!({
            "contents": turns_to_gemini_contents(&req.turns),
            "generationConfig": {
                "temperature": req.temperature,
                "maxOutputTokens": req.max_tokens
            }
        });
        if let Some(sys) = req.system {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": sys}]
            });
        }
        if self.enable_grounding {
            body["tools"] = serde_json::json!([{"googleSearch": {}}]);
        }
        // ... reqwest post, SSE parsing, chunk → String stream ...
    }
}

fn turns_to_gemini_contents(turns: &[TurnContent]) -> Vec<serde_json::Value> {
    turns.iter().map(|t| {
        let role = match t.role {
            Role::User => "user",
            Role::Assistant => "model",
        };
        serde_json::json!({
            "role": role,
            "parts": [{"text": &t.text}]
        })
    }).collect()
}
```

#### Verifiering Fas 1
- `cargo check -p svoice-llm` passar.
- Enhets-test: skriv `#[tokio::test]` som mockar HTTP och parsar streaming-chunks.

---

### Fas 2: Settings + secrets + provider-selection
**Mål:** Marcus kan välja Gemini i Settings-UI och nyckeln sparas i keyring.
**Tid:** ~20 min.

#### Steg
1. `crates/settings/src/lib.rs`:
   - Lägg till `Gemini` i `enum LlmProvider`.
   - Lägg till `pub gemini_model: String` i Settings-struct.
   - Default: `"gemini-2.5-flash".into()`.
2. `crates/secrets/src/lib.rs`:
   - Mirra anthropic-helpers. Använd username `"gemini_api_key"`.
3. `crates/ipc/src/commands.rs`:
   - `set_gemini_key(key: String)`, `clear_gemini_key()`, `has_gemini_key()`.
   - Exportera från `crates/ipc/src/lib.rs`.
4. `src-tauri/src/lib.rs`:
   - Registrera de tre nya IPC-commands i `invoke_handler![...]`.
   - I `select_llm_provider`, lägg till:
     ```rust
     let build_gemini = || -> Option<Arc<dyn LlmProvider>> {
         svoice_secrets::get_gemini_key()
             .ok()
             .flatten()
             .filter(|k| !k.is_empty())
             .map(|key| {
                 Arc::new(GeminiClient::new(key).with_model(settings.gemini_model.clone()))
                     as Arc<dyn LlmProvider>
             })
     };
     ```
   - Lägg till `ProviderChoice::Gemini => build_gemini()` i match.
   - I `Auto`-grenen: prioritet Ollama → Anthropic → Gemini → Groq (eller någon bra ordning).
5. Frontend `src/lib/settings-api.ts`:
   - `LlmProviderChoice` type: lägg till `"gemini"`.
   - Lägg till `gemini_model: string` i `Settings` interface.
6. Frontend `src/windows/Settings.tsx`:
   - `PROVIDER_LABELS.gemini = "Gemini"`.
   - Nytt block "Gemini (Google AI Studio)" med:
     - API-nyckel-fält (mirra `anthropic`-blocket, samma keyring-mönster).
     - Modell-dropdown: `gemini-2.5-flash` / `gemini-2.5-pro`.

#### Verifiering Fas 2
- `cargo check --workspace`, `pnpm tsc -b` passar.
- Dev-läge: Settings → Action-LLM visar 5 providers i segmented. Klicka Gemini, spara API-nyckel, kör Insert-PTT med enkel fråga ("berätta om rymden kort") — svar ska streamas.
- Loggen ska visa `action-LLM: använder Gemini (gemini-2.5-flash)`.

---

### Fas 3: Grounding-path i agentic
**Mål:** När Gemini är vald + query-mode + realtids-heuristik → kör med `googleSearch`-tool och visa "Söker på nätet"-chip i popup.
**Tid:** ~30 min.

#### Steg
1. I `src-tauri/src/agentic.rs`, lägg till `run_agentic_gemini(app, command, api_key, model, ev_token, ev_done)`:
   - Bygg `GeminiClient::new(key).with_model(model).with_grounding(true)`.
   - Skicka request med grounding aktiverat.
   - Emit `action_tool_call` med name="web_search", status="running" direkt vid start.
   - Streama chunks → emit `action_llm_token` som vanligt.
   - Vid done: parsa `groundingMetadata` (sista chunk) → emit `action_tool_call` med status="done" + summary=URL:er.
   - Append assistant-svar till `svoice_ipc::ACTIVE_CONVERSATION` (samma som Anthropic-path).
2. I `src-tauri/src/lib.rs::handle_action_released`:
   - Efter `let prep = ...`, kolla om `settings.action_llm_provider == LlmProvider::Gemini`:
     ```rust
     if action_provider == LlmProvider::Gemini && !is_follow_up && mode == "query" && heuristic_hit {
         let gemini_key = svoice_secrets::get_gemini_key().ok().flatten();
         if let Some(key) = gemini_key {
             rt.spawn(async move {
                 if let Err(e) = agentic::run_agentic_gemini(
                     &app_clone, &command_clone, key, model, EV_ACTION_LLM_TOKEN, EV_ACTION_LLM_DONE
                 ).await {
                     // ... error emit ...
                 }
             });
             return Ok(());
         }
     }
     ```
   - Behåll Anthropic-path för andra providers.
3. UI: `TOOL_LABELS["web_search"]` i `ActionPopup.tsx` funkar redan — `run_agentic_gemini` emit:ar samma event-name.

#### Gotchas
- Gemini SSE-chunks har annat format än Claude. Tolka `candidates[0].content.parts[0].text` plus eventuellt `groundingMetadata` i SAMMA response-kropp, inte separat event.
- `groundingMetadata.groundingChunks[].web.uri` innehåller `vertexaisearch.cloud.google.com/...` **redirect-URL:er**, inte final-domäner. Använd `title` för visningen ("enligt SMHI") istället för URI.
- Gemini kan ta 2-5 sek före första token vid grounding (söker först). Popup ska visa "lyssnar..." → "söker på nätet..." tydligt.

#### Verifiering Fas 3
- Insert-PTT med "vädret i Stockholm just nu" när action-provider=Gemini:
  - Popup visar "⏳ Söker på nätet" chip
  - Svar kommer med käll-hänvisning i texten (ex. "enligt SMHI: ...")
  - Chip byter till "✓ Söker på nätet"
- Testa också generell fråga ("vad är 2+2") — ska INTE trigga grounding (heuristic matchar inte), Gemini svarar direkt utan chip.

---

### Fas 4: Follow-up med Gemini
**Mål:** Håll Insert/Space i popup → följdfråga ska använda Gemini igen med konversations-context.
**Tid:** ~15 min.

#### Steg
`ACTIVE_CONVERSATION.turns` innehåller `TurnContent`-objekt med `Role::User/Assistant`.
 När `is_follow_up=true` bygger `handle_action_released` LLM-request från de turns:
```rust
let llm_req = if is_follow_up {
    svoice_ipc::append_user_turn(command.clone());
    let (system, turns) = svoice_ipc::snapshot_conversation().unwrap();
    LlmRequest { system, turns, temperature: 0.3, max_tokens: 1024 }
} else { ... };
```

`GeminiClient::complete_stream(llm_req)` måste hantera flera turns korrekt via
 `turns_to_gemini_contents` (alternerande `user`/`model`-roles). Gemini's API
 kräver att turns alternerar — vi kan inte skicka två user-turns i följd.
 Kolla om `ACTIVE_CONVERSATION` har alternerande roles. Om första turnen var
 agentic (user → assistant_accum saved), bör det alltid gå bra. Men om stream
 failade utan assistant-turn kan det bli [user, user]. I så fall: merge second
 user-turn med första innan skickas.

#### Verifiering Fas 4
- Insert "vädret i Stockholm", håll Space "och i Göteborg" → Gemini ska komma ihåg väder-kontexten.

---

### Fas 5: Rebuild MSI + commit
**Mål:** Installerad app har Gemini-support.
**Tid:** ~15 min.

1. Verifiera workspace-check + pnpm tsc.
2. `cargo tauri build`.
3. Hitta gamla ProductCode via:
   ```powershell
   Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*" | Where-Object { $_.DisplayName -like "*SVoice*" }
   ```
4. Uninstall + install med UAC-elevation.
5. Uppdatera autostart-registry-path (eller fixa `sync_autostart`-bug i samma runda — se roadmap).
6. Commit + push.

---

## Kritiska filer (i ordning som de rörs)

1. `src-tauri/crates/llm/src/gemini.rs` — NY fil, GeminiClient-impl
2. `src-tauri/crates/llm/src/lib.rs` — pub use gemini::GeminiClient
3. `src-tauri/crates/settings/src/lib.rs` — Gemini-variant + gemini_model field
4. `src-tauri/crates/secrets/src/lib.rs` — get/set/clear/has_gemini_key
5. `src-tauri/crates/ipc/src/commands.rs` — tre IPC commands
6. `src-tauri/crates/ipc/src/lib.rs` — exports
7. `src-tauri/src/lib.rs` — select_llm_provider, invoke_handler, routing i handle_action_released
8. `src-tauri/src/agentic.rs` — run_agentic_gemini
9. `src/lib/settings-api.ts` — LlmProviderChoice + gemini_model
10. `src/windows/Settings.tsx` — PROVIDER_LABELS + UI-block för nyckel/modell

## Återanvänd

- `AnthropicClient::with_model()` — samma builder-pattern för `GeminiClient`.
- `GroqClient` — closest template eftersom båda använder SSE.
- Befintliga secrets-helpers (`get_groq_key` etc) — identisk struktur.
- Befintlig `TOOL_LABELS["web_search"] = "Söker på nätet"` i ActionPopup.tsx.
- Befintlig `append_assistant_turn()` / `snapshot_conversation()` från svoice_ipc för follow-up-kontext.

## Verifiering (end-to-end)

1. **Dev-läge**: Settings → Action-LLM → välj Gemini. Spara API-nyckel. Insert-PTT → enkel fråga → svar streamas.
2. **Grounding**: Insert-PTT "vädret i Stockholm just nu" → chip "Söker på nätet" → svar med källa.
3. **Follow-up**: Insert → svar → håll Space → uppföljningsfråga → Gemini minns kontexten.
4. **Blandat**: sätt dictation_provider=Groq, action_provider=Gemini. RightCtrl-diktering ska polera via Groq; Insert via Gemini.
5. **MSI**: rebuild + install, alla flöden fungerar från `C:\Program Files\SVoice 3\`.

## Known gotchas

- **Gemini 2.5 Pro vs Flash**: Flash är default (snabb+billig), Pro är smartare men långsammare + dyrare. Låt user välja via dropdown.
- **Grounding redirect-URL:s**: Gemini returnerar `vertexaisearch.cloud.google.com/grounding-api-redirect/...` istället för riktiga käll-URL:er. Använd `groundingChunks[].web.title` för visning, inte URI.
- **Rate-limits**: free tier 10 RPM. Om user triggar många requests snabbt → 429. Visa tydligt error-toast.
- **Content filter**: Gemini blockerar ibland on safety-grounds (`finishReason: "SAFETY"`). Handla det som ett fel med tydligt meddelande.
- **`systemInstruction` är optional men rekommenderat** för att få svenskt svar. Använd samma system-prompt som `agentic::system_prompt()` (eller en förenklad variant för Gemini).
- **Token-count-skillnader**: Gemini räknar annorlunda än Claude. Samma max_tokens ger olika resultat. Testa och justera.
