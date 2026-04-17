# SVoice 3 — kvarvarande arbete & roadmap till full produkt

**Senast uppdaterad:** 2026-04-17
**Git-state:** `main` @ `51b3167`, taggar `iter1-complete` → `iter3-complete`

Dokumentet är en handoff-sammanfattning. Claude i nästa session ska kunna läsa detta + `plan.md` + `START-NEXT-SESSION.md` och direkt veta exakt var att fortsätta.

---

## Sammanfattning av levererat (kort)

**Fullständigt fungerande idag:**
- Svensk tal-till-text via KB-Whisper + Python-sidecar + faster-whisper, CUDA GPU-inferens 200-700 ms på RTX 5080.
- Push-to-talk (höger Ctrl) injicerar transkript i valfri Windows-app via clipboard-paste.
- Action-LLM popup (Insert) med selection-transform / Q&A. Anthropic Claude + Ollama lokal, Auto-fallback.
- Settings-UI med editorial × studio-design: STT/LLM av/på-toggles, modellval, mic-devices, API-key, Ollama-pull med progress-bar, STT cache-status ✓/↓.
- Voice-oval overlay centrerad nederst med SV-logo + symmetrisk live-waveform + progress-bar under transkribering.
- Hot-reload av alla settings utan app-restart (inkl STT-modell-byte via reload_config → sidecar-respawn).
- Simultan-PTT-lockout, key-repeat-consumption, HWND-focus-restore, own-process-guard — alla tangentbord-hang-issues lösta.
- Tray-resident: main-window dolt by default, öppnas via vänsterklick eller "Visa inställningar"-menyn.

**Git-taggar:**
- `iter1-complete`: walking skeleton (PTT + clipboard-paste + tray)
- `iter2-complete`: riktig STT + settings + audio-pipeline
- `iter2.5-complete`: voice-oval overlay + SV-logotyp
- `iter3-complete`: action-LLM popup infrastruktur
- **otaggat (senaste main)**: alla iter 3.5-fixar, Ollama, hot-reload, moduler-toggles, download-progress

**Branch-rekommendation för nästa session:**
`git checkout -b iter4/google-integration` (eller whatever scope-namn passar)

---

## Iter 4 — Google-integration (3-4 veckor) · **NÄSTA STORA STEG**

**Mål:** Action-LLM popup ska kunna exekvera agentic kommandon via Google-verktyg. T.ex. "lägg till detta i kalendern på fredag kl 14" eller "svara på mailet från Marcus med bekräftelse".

### Fas 4A — OAuth 2.1-infrastruktur

Nytt: `svoice-integrations/google`-crate.

- **OAuth 2.1 PKCE**: `oauth2 = "5"` crate, S256-challenge, loopback-redirect `http://127.0.0.1:<ephemeral>/callback`.
- **Tokens lagras i keyring** (se Iter 4.5 — keyring behöver landa först eller parallellt).
- **Inkrementell consent**: en scope per feature (gmail.modify, calendar.events).
- **Refresh-token-hantering**: auto-refresh före expiry.

### Fas 4B — Gmail + Calendar REST-wrappers

Tunna `reqwest`-wrappers (ingen tung SDK).

Första verktyg:
1. `list_calendar_events(start, end)` — GET /calendar/v3/calendars/primary/events
2. `create_calendar_event(summary, start, end, description)` — POST
3. `search_emails(query, max_results)` — GET /gmail/v1/users/me/messages?q=...
4. `read_email(message_id)` — GET /gmail/v1/users/me/messages/{id}
5. `draft_reply(thread_id, body)` — POST /gmail/v1/users/me/drafts

### Fas 4C — Tool-use-loop i Anthropic

Claude stödjer tool-use redan. Ändra `AnthropicClient::complete_stream` till att hantera tool_use-stop i stream:
1. Vid `stop_reason == "tool_use"`: extrahera tool-calls
2. Dispatch till rätt Rust-handler (Google-verktyg)
3. Lägg tool_result i messages, kör request igen
4. Loop tills stop_reason == "end_turn"

Ollama har tool-use i vissa modeller (Qwen 2.5, Llama 3.1) men kvalitet-osäker — acceptera fallback till Anthropic för tool-kommandon.

### Fas 4D — UI: Integrations-sektion

Ny sektion i Settings:
- "Anslut Google-konto"-knapp (öppnar OAuth-flow i browser)
- Lista kopplade scopes
- "Koppla från"-knapp
- Tydlig text om vilka verktyg som kräver vilka scopes

### Exit-kriterier iter 4
- [ ] OAuth-flow fungerar, refresh-token sparas
- [ ] Alla 5 verktyg testade end-to-end via curl-mockade Claude-responses
- [ ] Agentic action-popup: "lägg till möte imorgon" → Claude tool-callar → event skapas
- [ ] Popup visar "verktyg körs: create_calendar_event…" → "✓ möte skapat"
- [ ] Error-handling: expirerad token → auto-refresh, permission denied → tydligt felmeddelande

---

## Iter 4.5 — Säkerhet + UX-polish (1-2 veckor) · **BÖR GÅ PARALLELLT ELLER PRE-ITER 4**

Mindre features som förbättrar upplevelsen markant.

### Keyring för API-nycklar
- `keyring = "3"` crate → Windows Credential Manager
- Flytta `anthropic_api_key` från `settings.json` → Credential Manager med nyckel `svoice:anthropic_api_key`
- Settings-UI: fältet visar `••••••••` om keyring har värde, annars tomt
- Bakåt-kompatibilitet: vid migrering, läs klartext, skriv till keyring, radera ur JSON

### Notifikationer när download klar
User-wish: "Bra om det går att stänga inställningsrutan i väntan med notis när den är klar och installerad"
- Lägg till `tauri-plugin-notification` dep
- Ollama pull_done → toast-notification "Qwen 2.5 14B nedladdad"
- STT-modell (triggered vid sidecar Load): liknande
- Fallback: tray-ikon blinkar

### Konfigurerbara hotkeys
- Ny Settings-sektion "Snabbkommandon"
- Dropdown/key-capture för diktering-hotkey och action-hotkey
- Validering: inte samma som standard-systemkeys
- `register_hotkey(HotKey::Custom(vk))` i svoice-hotkey

### Multi-monitor overlay-positioning
- Lyssna på monitor-change-event i Tauri
- Re-calculate overlay-position när user flyttar primär-display
- Eller: låt user välja monitor i Settings

### Error-toasts i main-window
- `action-popup-error` → visa i main via toast om popup inte synlig
- STT-fel → toast + tray-ikon orange

---

## Iter 5 — Framtid (opinionerat, senare)

### STT-kvalitetsförbättringar
- **Silero VAD** via ONNX runtime — bättre än energi-tröskel vid brus
- **Streaming STT** — partials visas i overlay under inspelning (kräver chunked pipeline + LocalAgreement-2 dedup)
- **Model Center** — UI för att ladda ner/byta STT-modeller utan att gå via Settings-dropdown (thumbnails, size, storage-used)
- **Custom-vocab** — user kan lägga till uttryckliga ord (namn, tekniska termer) för bias

### Smart-function library
Plan.md beskrev smart-function-system — JSON-filer i `%APPDATA%/svoice-v3/smart_functions/*.json`:
- `correct-grammar-sv`, `summarize`, `translate-sv-en`, `rewrite-formal`, `reply-to-email`
- Command palette (`Ctrl+Shift+Space`) som listar alla
- Hot-reload via `notify`-crate

### Per-app-profiler
- Detektera foreground-window-process
- Mappa process → profil (Fast/Balanced/Quality, olika VAD, olika LLM)
- T.ex. Slack → mer polering, Notepad → ren diktering, Code-editor → mindre polering

### Andra integrationer
- Outlook (Graph API, motsvarande Gmail)
- Slack (Bot Token + user-auth)
- Notion (API v1 för append/edit)
- Linear, Jira via MCP?

### Wake-word + VAD always-on
- Istället för PTT: säg "SVoice" för att börja lyssna
- Porcupine (Picovoice) eller open-source wake-word
- Mer komplex audio-pipeline

### MCP-klient
- Model Context Protocol för tool-integration utanför Google
- SVoice som MCP-host som kan ansluta till filesystem, GitHub, etc

### Custom-prompts + historik
- Varje smart-function får user-redigerbara prompts via UI
- Action-popup historik ("senaste 10 kommandon") med "kör igen"-knapp

---

## Release & distribution

### EV-code-signing
- Köp EV-certifikat (~$300/år från Sectigo/DigiCert)
- Signera MSI med `signtool`
- Första publika release — Smart App Control ska acceptera

### Auto-updater
- `tauri-plugin-updater`
- Self-hosted update-server eller GitHub Releases som feed
- Signaturverifiering

### CI/CD
- GitHub Actions: build + sign + release på tag-push
- Matrix-build för Python-runtime-varianter (CPU-only, CUDA)

### MSI-test
- G2 från iter 2-planen var deferred (2.3 GB bundled runtime, friends-kloning-strategi)
- När EV-cert finns: faktisk MSI-release med bundled runtime verifieras

---

## Kända buggar / limitations att hålla koll på

| Prio | Issue | Kontext |
|---|---|---|
| Låg | Multi-monitor overlay-pos | Overlay-y beräknas bara primär vid app-start |
| Låg | Notifikation när download klar | User önskade stänga Settings under pull |
| Låg | LLM-polering har ingen streaming-preview | Polering är blockerande, user ser ingen progress |
| Info | action-popup "loading..."-state | Visas kort innan första LLM-token; kan polishas |
| Info | STT sidecar har inte download-progress | faster-whisper visar progress på stderr → kan parsas och emittas framåt |

---

## Arkitektur-översikt (för snabb-orientering)

```
┌──────────────────────────────────────────┐
│ Rust (Tauri 2)                           │
│  ├── svoice-audio    (cpal + WASAPI)     │
│  │     ├── capture.rs (ring + mic-level) │
│  │     ├── vad.rs (energi-trim)          │
│  │     └── devices.rs (mic enumeration)  │
│  ├── svoice-stt      (Python-sidecar)    │
│  │     ├── protocol.rs (JSON wire)       │
│  │     ├── sidecar.rs (tokio async)      │
│  │     └── engine.rs (PythonStt + reload)│
│  ├── svoice-llm      (trait + providers) │
│  │     ├── anthropic.rs (SSE streaming)  │
│  │     └── ollama.rs (NDJSON + pull)     │
│  ├── svoice-hotkey   (LowLevelKbHook)    │
│  │     └── multi-key + repeat-consume    │
│  ├── svoice-inject   (clipboard/SendIn)  │
│  │     ├── paste_and_restore (HWND save) │
│  │     └── capture_selection (Ctrl+C)    │
│  ├── svoice-settings (JSON + reload)     │
│  ├── svoice-ipc      (Tauri commands)    │
│  └── svoice-integrations/google (iter 4) │
│
│ Python sidecar (src-tauri/resources/)    │
│  └── stt_sidecar.py (faster-whisper)     │
│      + UTF-8 stdio + CUDA PATH injection │
│
│ React/TS (src/)                          │
│  ├── windows/Settings.tsx (main UI)      │
│  ├── windows/ActionPopup.tsx (LLM popup) │
│  ├── overlays/RecordingIndicator.tsx     │
│  ├── components/SVoiceLogo.tsx           │
│  └── lib/settings-api.ts (typed invoke)  │
└──────────────────────────────────────────┘
```

**IPC-commands (27 stycken vid tiden för denna plan):**
- `get_settings`, `set_settings`
- `action_apply`, `action_cancel`
- `list_mic_devices`
- `list_ollama_models`, `pull_ollama_model`
- `check_hf_cached`

**Events (frontend lyssnar på):**
- `ptt_state`, `ptt_volume`, `mic_level`
- `action_popup_open`, `action_llm_token`, `action_llm_done`, `action_llm_error`
- `ollama_pull_progress`, `ollama_pull_done`

**Hot-reload-pattern som ska följas genomgående:**
Varje PTT-release (eller settings-change-IPC) läser `Settings::load()` från disk och bygger fresh klienter/config. Inga "build-once"-patterns — state i disk är source of truth.

---

## Rekommenderad sekvens för att avsluta projektet

1. **Keyring** (iter 4.5a, 2-3 dagar) — enklast, ger säkerhet-förtroende innan vi delar appen bredare.
2. **Notifikationer + konfigurerbara hotkeys** (iter 4.5b, 3-4 dagar) — UX-polish som hör ihop.
3. **Google-integration Fas 4A+4B** (iter 4a, 2 veckor) — OAuth + REST-wrappers
4. **Tool-use-loop + action-popup integration** (iter 4b, 1 vecka) — koppla ihop
5. **Silero VAD + streaming STT** (iter 5a, 1 vecka) — kvalitets-kickstart
6. **Smart-function-library** (iter 5b, 1 vecka) — command palette
7. **EV-cert + auto-updater** (release-prep, 1 vecka) — förbereder publik distribution

**Total tidsestimat till "fullständig produkt":** 6-8 veckor fokuserat arbete.

---

## Att komma ihåg

**Design-principer som etablerats:**
- Editorial × pro-audio studio. Warm charcoal, ivory, amber (VU-meter-färger).
- Fraunces display-serif + Instrument Sans body + JetBrains Mono värden.
- Wow-känsla är uttalat mål. Inga generiska AI-UI:er.
- Privacy-first default, cloud är opt-in.
- Hot-reload över app-restart — user ska aldrig behöva starta om.

**Felhanterings-principer:**
- Varje error ska kunna visas i popup eller toast.
- Tangentbord får ALDRIG fastna. LowLevelKeyboardHook måste konsumera registrerade keys symmetriskt (key-down + key-up + repeats).
- Target-HWND sparas alltid innan vår egen webview stjäl focus.

**Kod-mönster:**
- `anyhow::Result` för high-level errors, `thiserror` för library-lokala.
- Async workers kör via `tokio::runtime::Runtime::new()` (blocking `block_on` från std-thread är OK).
- Settings hot-reload: `Settings::load()` vid varje PTT, bygg fresh klienter.
- IPC-commands är tunna, all business-logic i crate-moduler.
