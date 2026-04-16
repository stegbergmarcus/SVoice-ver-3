# Ny STT — Svensk dikterings-app för Windows

## Context

En fristående Windows-applikation för svensk tal-till-text (STT) med integrerad LLM-funktionalitet. Byggs från scratch (ingen gammal kod återanvänds) för att adressera bristerna i befintliga lösningar: ingen stark svensk STT-app finns på marknaden i april 2026. Primär målhårdvara är RTX 5080 + 7800X3D, men appen ska fungera även på svagare datorer via modellbyte.

**Mål:** lokalt-först, snabb push-to-talk-diktering i valfri Windows-applikation, med valbar LLM-poleringsmängd, smarta funktioner (sammanfatta, översätt, svara mail), och opt-in Google-integration för agentiska flöden.

**Bekräftade val:**
- **Stack:** Tauri 2.x + Rust backend + React/TypeScript frontend
- **Aktivering:** Global hotkey, push-to-talk (håll-för-att-prata)
- **Privacy:** Lokalt STT + LLM som default. API-leverantörer är opt-in per profil.
- **Integration:** Google via direkt OAuth 2.1 i opt-in modul (fas 3). MCP skjuts till framtida fas när lokala modellers tool-calling mognat.
- **LLM är helt fristående från STT.** Användaren ska kunna köra STT utan LLM, STT + LLM för polering, eller LLM *endast för actions* (utan att LLM rör dikteringsutdata). Se "LLM-lägen" nedan.

---

## LLM-lägen (LLM-oberoende)

STT-pipen och LLM-pipen är **två separata system** som kopplas samman via användarval. Tre huvudlägen + per-smart-function override:

| Läge | STT-output | LLM används för |
|---|---|---|
| **STT-only** | Råtext från whisper injiceras direkt. | Inget. LLM-modul inaktiverad. |
| **STT + polering** | LLM-polerad text injiceras (grammatik, punktuering, ev. ord-byten). | Post-processing av varje transkript före injektion. |
| **STT + actions-only** | Råtext från whisper injiceras direkt (som STT-only). | LLM triggas *endast* när användaren explicit anropar en smart-function (command palette / hotkey). Rör inte dikteringsflödet. |

**Implikationer:**
- `stt::engine` levererar alltid råtranskript till en central `transcript_bus`.
- `llm::polish` är en *prenumerant* som kan kopplas av/på i settings.
- `llm::smart_fn` triggas alltid manuellt via command palette eller dedikerad hotkey — oberoende av om polering är på.
- "STT + actions-only" är default för användare som vill ha ren diktering men ändå nå LLM-kraften för explicita uppgifter (sammanfatta, översätt, svara mail etc).

**Settings-exponering:**
- Profil-nivå: `llm_polish_enabled: bool` + `llm_smart_fns_enabled: bool` (oberoende togglar).
- Per smart-function: `llm_provider_override`, `temperature_override` som idag.
- UI: tydlig on/off-switch för LLM-polering i settings, separat från smart-function-modulen. Tray visar ett litet LLM-ikon bara när LLM faktiskt används.

**Konsekvenser för modellhantering:**
- LLM-modeller (Ollama, API-nycklar) behöver aldrig konfigureras om användaren kör STT-only. Första start kräver inte Ollama-install.
- Ollama-sidecar spawnar *lazy* — först när ett LLM-läge aktiveras eller en smart function körs.
- API-LLM-nycklar kan vara tomma utan att appen kraschar.

---

## Arkitektur

### Processtopologi

```
┌──────────────────────────────────────────────┐
│  Tauri main process (Rust)                   │
│   - Global hotkey listener                   │
│   - Audio capture (WASAPI)                   │
│   - STT pipeline (faster-whisper FFI)        │
│   - LLM orkestrering                         │
│   - SendInput text-injektion                 │
│   - Settings + keyring                       │
│   - Integrations (Google OAuth)              │
└──────────────────────────────────────────────┘
        │ IPC (tauri::invoke)        │ HTTP localhost:11434
        ▼                            ▼
┌─────────────────────┐      ┌─────────────────────┐
│  React WebView UI   │      │  Ollama sidecar     │
│  - Settings         │      │  (detected eller    │
│  - Model Center     │      │   spawnad av appen) │
│  - Recording pill   │      └─────────────────────┘
│  - Command palette  │
└─────────────────────┘
```

### Dataflöde — diktering (push-to-talk)

1. Användare håller `Win+Alt+Space` → `tauri-plugin-global-shortcut` fires key-down.
2. `hotkey::ptt_state` → `Recording`; `audio::Capturer` öppnar WASAPI shared-mode @ 16 kHz mono.
3. Samplings in i lock-free ringbuffer (`ringbuf` crate). Silero VAD taggar tal-segment.
4. Key-up → `stt::engine` flushar buffer till faster-whisper (batch för korta, streaming med 30s chunks + 1s overlap för längre).
5. Transkript → valfri `llm::polish`-pass (svensk grammatikkorrektur).
6. `inject::send_input` skriver Unicode via `SendInput`; clipboard-paste som fallback.

### Prestandaprofiler

Profiler bundlar STT-inställningar + *förslag* på LLM-konfiguration. LLM-polering är alltid oberoende toggle (se "LLM-lägen" ovan) — profilen dikterar bara defaults.

| Profil | STT | Beam | Föreslagen LLM-polering | Streaming | VRAM (STT) |
|---|---|---|---|---|---|
| **Fast** | kb-whisper-base int8 | 1 | av (default) | partials live | ~1 GB |
| **Balanced** | kb-whisper-medium fp16 | 3 | lokal 7B om användare slagit på | partials live | ~4 GB |
| **Quality** | kb-whisper-large fp16 | 5 | API (Claude/GPT) om användare slagit på | batch | ~6 GB |

Profiler kan bytas via tray eller `Ctrl+Alt+1/2/3`. Varje smart funktion kan override:a profil. LLM-polering och smart-functions togglas *separat* i settings.

### Idle-beteende

Appen bor i tray; ljudenhet stängd; Whisper-modell unloadas efter 2 min idle (konfigurerbart); hotkey-lyssnaren alltid aktiv. Första keypress pre-warmar modellen (~300–800 ms) medan användaren börjar tala.

---

## Modulstruktur

### Rust-workspace (`src-tauri/crates/*`)

| Crate | Nyckelfiler | Syfte |
|---|---|---|
| `audio` | `capture.rs`, `ring.rs`, `vad.rs`, `resample.rs` | WASAPI-input, ringbuffer, Silero VAD via ONNX, 48→16 kHz resampling |
| `stt` | `engine.rs` (trait), `faster_whisper.rs` (ct2rs FFI), `whisper_cpp.rs` (whisper-rs fallback), `streaming.rs`, `model_manager.rs` | Model-agnostisk STT-pipeline |
| `llm` | `client.rs` (trait), `ollama.rs`, `openai_compat.rs`, `anthropic.rs`, `gemini.rs`, `prompts.rs`, `smart_fn.rs` | Multi-provider LLM med smart-function runner |
| `hotkey` | `register.rs`, `ptt_state.rs` | Global shortcut + PTT state machine |
| `inject` | `send_input.rs`, `clipboard.rs`, `target_detect.rs` | Unicode-injektion via winapi + fallbacks |
| `settings` | `profile.rs`, `store.rs`, `secrets.rs` (keyring), `manifest.rs` | Config + Windows Credential Manager |
| `integrations/google` | `oauth.rs`, `gmail.rs`, `calendar.rs`, `tools.rs` | OAuth 2.1 PKCE + REST + function-schema |
| `ipc` | `commands.rs` | Tauri-kommandon exponerade till UI |

### React/TypeScript (`src/`)

| Område | Filer |
|---|---|
| Huvudfönster | `windows/Main.tsx` (tabbad shell) |
| Settings | `windows/Settings/{Profiles,Hotkeys,Models,LLMProviders,Integrations}.tsx` |
| Model Center | `windows/ModelCenter.tsx` |
| Overlays | `overlays/{RecordingIndicator,TranscriptOverlay,CommandPalette}.tsx` |
| Delad logik | `lib/ipc.ts` (typad invoke), `lib/store.ts` (Zustand) |

---

## Modellhantering

- **Katalog:** `src-tauri/resources/manifest.json` listar modeller: id, HF-repo, kvantisering, disk/VRAM-kostnad, språk.
- **Nedladdning:** `hf-hub` crate eller `reqwest` direkt, med SHA-256 verifiering och resumable downloads.
- **Lagring:** `%APPDATA%/ny-stt/models/<engine>/<model-id>/`.
- **Auto-val vid första start:** GPU-detektering via `nvml-wrapper` (NVIDIA) eller `wgpu::Adapter`:
  - ≥12 GB VRAM → kb-whisper-large fp16, Balanced default
  - 6–12 GB → kb-whisper-medium fp16
  - <6 GB eller ingen CUDA → kb-whisper-base int8 på whisper.cpp
- **Shippade modeller i Model Center:** kb-whisper-base/medium/large, Whisper-v3-turbo, NVIDIA Parakeet 1.1B (valfritt).

---

## Smart-funktioner

**Datamodell** (`%APPDATA%/ny-stt/smart_functions/*.json`):

```json
{
  "id": "reply-to-email",
  "name": "Svara på mail (svenska)",
  "input": "selection|clipboard|last-transcript",
  "prompt": "Du är en assistent... {{input}}",
  "llm_preference": "api-preferred",
  "output": "insert|replace|copy|show",
  "temperature": 0.3
}
```

**Inbyggda funktioner (fas 2):**
- `correct-grammar-sv` — svensk grammatikkorrektur av senaste transkript
- `summarize` — sammanfatta markerad text
- `translate-sv-en` / `translate-en-sv`
- `rewrite-formal` — skriv om till formell ton
- `reply-to-email` — föreslå svar på mailtråd

**Runner:** CommandPalette (`Ctrl+Shift+Space`) → `ipc::run_smart_fn` → `llm::smart_fn::execute` → streamar tokens tillbaka via Tauri events → UI visar preview eller injicerar direkt. Hot-reload av användarändrade JSON via `notify` crate.

---

## Google-integration (opt-in, fas 3)

- **Auth:** `oauth2` crate, PKCE S256, loopback-redirect `http://127.0.0.1:<ephemeral>/callback`, inkrementell consent (scopes per feature).
- **Token-lagring:** `keyring-rs` → Windows Credential Manager, nyckel `ny-stt:google:<account>`.
- **API-klienter:** tunna `reqwest`-wrappers över Gmail v1 + Calendar v3 (ingen tung SDK).
- **Tool-exponering:** `integrations/google/tools.rs` definierar OpenAI-kompatibla JSON-schemas. LLM-orkestraturen fångar tool-call i response, dispatchar till Rust-handlers, loopar tills finish.
- **V1-verktyg:** `list_calendar_events`, `create_calendar_event`, `search_emails`, `read_email`, `draft_reply`.
- **UI-gating:** Integrations-flik disabled tills API-LLM är konfigurerad. Tydlig text: *"Lokala LLM:er har opålitlig tool-calling i 2026 — integrationer kräver API-LLM i v1."*

---

## Säkerhet & privacy

- **Noll telemetri, ingen crash-reporter** i v1.
- `keyring-rs` för alla hemligheter (OAuth-tokens, API-nycklar). Config-JSON innehåller bara referenser: `"api_key_ref": "ny-stt:openai"`.
- Ljud hålls endast i RAM; skrivs aldrig till disk om inte "Save recordings" är på (default av, DPAPI-krypterat).
- **Tray-indikator-färger:** grå (idle), röd (lokalt recording), orange (cloud STT), lila (cloud LLM).
- CSP för webview: `default-src 'self' tauri:`.
- EV-signerad binär i produktion.

---

## Faserad leverans

### Fas 1 — MVP (4–5 veckor) — STT-only
Tauri-scaffold, WASAPI-capture, kb-whisper-medium via faster-whisper (CUDA), push-to-talk, SendInput-injektion, minimal tray + settings, Model Center med 3 KB Whisper-storlekar. **Ingen LLM-kod kompileras in som hårt beroende** — `llm::*` modulerna finns som stubs.
**Exit:** diktera i valfri Windows-app med <1.5 s end-to-end latens på RTX 5080, utan att någon LLM-komponent är installerad eller konfigurerad.

### Fas 2 — LLM som valbart tillägg (3–4 veckor)
Ollama-sidecar (lazy-spawn), API-providers (OpenAI-compat + Anthropic), smart-function runner, command palette, 5 inbyggda funktioner, prestandaprofiler, keyring-backad nyckelhantering, whisper.cpp-fallback för svaga datorer. Oberoende togglar: `llm_polish_enabled`, `llm_smart_fns_enabled`. "STT + actions-only" blir default för nya profiler i denna fas.

### Fas 3 — Google-integration (2–3 veckor)
OAuth 2.1 PKCE, Gmail/Calendar-klienter, tool-call-loop i Anthropic + OpenAI-compat, Integrations-UI, 5 första verktyg, cloud-aktivitets-indikatorer.

### Fas 4 — Framtid
VAD always-on med wake-word, MCP-klient, fler integrationer (Slack, Outlook, Notion), streaming STT från Deepgram/ElevenLabs, per-app-profiler (detektera foreground-app → byt profil automatiskt).

---

## Riskområden — bygg spikes först

| Risk | Spike |
|---|---|
| `ct2rs` Windows + CUDA 12+ mognad för RTX 50-serien | 1-dags spike: ladda kb-whisper-medium, mät cold/warm latens. Fallback: Python-subprocess med `faster-whisper` (~80 MB extra). |
| Silero VAD via ONNX Runtime på Windows | Verifiera DirectML/CUDA EP-tillgång. WebRTC VAD räcker för PTT om Silero strular. |
| SendInput mot Electron/UIA-tunga appar (Teams, Slack) | Testa top-10 mål-appar; tuna clipboard-fallback-heuristik. |
| Streaming-dedup vid chunk-gränser med KB Whisper | Empirisk tuning; överväg `whisper-streaming` LocalAgreement-2-algoritm. |
| Ollama tool-calling för svenska prompts | Liten eval mot Qwen 2.5 32B + Gemma 3 innan det ruckas ut helt från integrationer. |

---

## Kritiska filer att skapa

### Rust
- `src-tauri/Cargo.toml` — workspace root
- `src-tauri/src/main.rs` — Tauri builder, plugins, sidecar-spawn
- `src-tauri/tauri.conf.json` — fönsterdef (main, overlay, palette), permissions
- `src-tauri/crates/audio/src/capture.rs` — WASAPI input capture
- `src-tauri/crates/audio/src/vad.rs` — Silero VAD wrapper
- `src-tauri/crates/stt/src/engine.rs` — STT-trait + dispatch
- `src-tauri/crates/stt/src/faster_whisper.rs` — ct2rs FFI
- `src-tauri/crates/stt/src/streaming.rs` — chunked streaming + overlap dedup
- `src-tauri/crates/stt/src/model_manager.rs` — HF-nedladdning + integritet
- `src-tauri/crates/hotkey/src/ptt_state.rs` — PTT state machine
- `src-tauri/crates/inject/src/send_input.rs` — Unicode-injektor
- `src-tauri/crates/inject/src/clipboard.rs` — paste-fallback
- `src-tauri/crates/llm/src/ollama.rs` — lokal klient
- `src-tauri/crates/llm/src/openai_compat.rs` — delad OpenAI-formad klient
- `src-tauri/crates/llm/src/anthropic.rs` — Claude med tool-use-loop
- `src-tauri/crates/llm/src/smart_fn.rs` — smart-function runner
- `src-tauri/crates/settings/src/secrets.rs` — keyring-wrapper
- `src-tauri/crates/settings/src/manifest.rs` — modell-katalog-loader
- `src-tauri/crates/integrations/google/src/oauth.rs` — PKCE + loopback
- `src-tauri/crates/integrations/google/src/tools.rs` — function-calling-schemas
- `src-tauri/resources/manifest.json` — shippad modell-katalog
- `src-tauri/resources/smart_functions/*.json` — inbyggda funktioner

### React
- `src/windows/Main.tsx` — React-shell
- `src/windows/Settings/Models.tsx` — nedladdnings-center
- `src/windows/Settings/Integrations.tsx` — Google-konnektor UI
- `src/overlays/RecordingIndicator.tsx` — always-on-top-pill
- `src/overlays/CommandPalette.tsx` — smart-function-launcher
- `src/lib/ipc.ts` — typade invoke-wrappers

---

## Verifiering / manuell testplan

1. **Hotkey + injektion:** Öppna Notepad, håll `Win+Alt+Space`, säg "Hej, det här är ett test." — text hamnar rätt med å/ä/ö.
2. **Fallback-paste:** Browser-lösenordsfält → verifiera clipboard-fallback vid aktivering.
3. **Modellbyte:** Tray Fast → Quality, diktera igen; modell-reload <5 s, kvaliteten ökar.
4. **Lång monolog:** 45 s tal → streaming partials i overlay, korrekt dedup vid chunk-gränser.
5. **Smart function — grammar:** Markera svensk paragraf → palette → "Correct grammar" → ersättning.
6. **Smart function — reply email (API LLM):** Klistra in mail-body → palette → "Svara på mail" → artigt svenskt svar.
7. **Google-integration:** Connect Google → palette → "Vad har jag för möten i morgon?" → OAuth + tool-call + resultat inserterat.
8. **Privacy-indikator:** Byt till Quality (API STT) → orange tray + cloud-pill på overlay.
9. **Svag-dator-path:** Disable CUDA-flagga → verifiera whisper.cpp Vulkan på iGPU.
10. **Idle:** Kör 1 h idle → modell unloadad, RAM till baseline.
11. **Secret-rotation:** Ta bort API-nyckel i UI → Credential Manager-entry borttaget.
12. **Autostart:** Aktivera autostart, boota om, tray syns, första hotkey funkar <2 s.
13. **STT-only-läge:** Stäng av både `llm_polish_enabled` och `llm_smart_fns_enabled`; verifiera att Ollama aldrig spawnar, ingen API-trafik, råtranskript injiceras direkt.
14. **Actions-only-läge:** `llm_polish_enabled=false`, `llm_smart_fns_enabled=true`. Diktera → råtext in i Notepad. Markera → palette → "Sammanfatta" → LLM svarar. Verifiera att ingen LLM-trafik sker under själva dikteringen.
15. **Lazy Ollama-spawn:** Starta appen med LLM-togglar av → Task Manager visar ingen Ollama-process. Slå på polering → Ollama-process dyker upp innan nästa diktering.
