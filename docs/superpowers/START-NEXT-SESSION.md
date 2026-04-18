# Starta nästa session

## Första meddelandet att skicka till Claude

> Hej! Vi fortsätter på SVoice 3. Iter 1–5 klara. Appen är produktionsklar nog för privat användning. Senaste tag `iter5-complete`. Kvar: (1) command palette UI för smart-functions, (2) Silero VAD för bättre STT, (3) EV-code-signing + auto-updater för publik release. Se `docs/superpowers/plans/2026-04-17-remaining-work-roadmap.md` + denna fil.

## Status — appen i nuläget

**Fullt fungerande end-to-end:**

1. **Diktering** (konfigurerbar hotkey, default höger Ctrl) — KB-Whisper via Python-sidecar, 200-700 ms på RTX 5080.
2. **Action-LLM popup** (konfigurerbar hotkey, default Insert) — Claude + Ollama. Auto-fallback.
3. **Agentic action-LLM** — när Google är ansluten och kommandot innehåller kalender/mail-keywords kör appen tool-use-loop (list/create calendar-events, search/read mail). Live-status i popup.
4. **Voice-oval overlay** (nederst centrerat) — SV-monogram + symmetrisk waveform + progress-bar under STT.
5. **Settings-UI** med 8 sektioner (Moduler, Audio, STT, LLM, Snabbkommandon, Integrationer, Smart-functions, Tystnadströskel).
6. **Tray-resident** — main dolt by default.
7. **Hot-reload** av ALLA settings — inkl. hotkeys (live rebind), STT-model, LLM-provider, API-nyckel.
8. **Windows OS-notifikationer** — toast när Ollama-pull är klar.
9. **Keyring för secrets** — `anthropic_api_key` + `google_refresh_token` under service `svoice-v3`.
10. **Smart-functions** — 5 sv-defaults seedas i `%APPDATA%/svoice-v3/smart_functions/`. Användaren kan redigera JSON-filer direkt. Command palette för snabbtriggning kommer senare.

## Git

```
main @ tag iter5-complete
Taggar: iter1-complete, iter2-complete, iter2.5-complete, iter3-complete,
        iter4.5a-complete, iter4.5b-complete,
        iter4-phase1-complete, iter4-complete, iter5-complete
Repo:  https://github.com/stegbergmarcus/SVoice-ver-3 (privat)
CI:    .github/workflows/ci.yml (Windows runner, fmt + tsc + build + check + clippy)
Tester: 58 gröna, 0 failed, 1 ignored
```

## Roadmap-status

| Prio | Fas | Scope | Status |
|---|---|---|---|
| ✅ | Iter 1 | Walking skeleton (PTT + clipboard + tray) | Klar |
| ✅ | Iter 2 | KB-Whisper STT + settings | Klar |
| ✅ | Iter 2.5 | Voice-oval overlay + SV-logo | Klar |
| ✅ | Iter 3 | Action-LLM popup infra | Klar |
| ✅ | Iter 4.5a | Keyring för API-nyckel | Klar |
| ✅ | Iter 4.5b | Notifikationer + konfigurerbara hotkeys | Klar |
| ✅ | Iter 4 | Google-integration (OAuth, Calendar, Gmail, tool-use) | Klar |
| ✅ | Iter 5 | Hotkey hot-reload, smart-functions, CI | Klar |
| 🔥 | Next | **Command palette UI för smart-functions** | Kvar |
| 🔥 | Next | **Silero VAD** för bättre röstdetektion i brus | Kvar |
| 💡 | Future | Streaming STT med partials | Kvar |
| 💡 | Future | Model Center UI + custom vocab | Kvar |
| 💡 | Future | Fler integrationer (Outlook, Slack, Notion) | Kvar |
| 💡 | Future | Per-app-profiler | Kvar |
| 💡 | Release | EV-cert (köp ~$300/år) + auto-updater + MSI-bundle | Kräver Marcus |

## Setup för Google-integration (Marcus måste göra MANUELLT)

Innan agentic flow fungerar end-to-end:

1. Gå till https://console.cloud.google.com/apis/credentials
2. Skapa OAuth-client av typ **Desktop app**.
3. Under "Authorized redirect URIs" lägg till `http://127.0.0.1/callback`.
4. Aktivera Calendar API + Gmail API i https://console.cloud.google.com/apis/library.
5. Kopiera client-ID:n → paste i Settings → Integrationer → Google OAuth client-ID → Spara.
6. Klicka "Anslut Google-konto" — browser öppnas med consent-sida.
7. Godkänn. Refresh-token sparas i keyring.
8. Testa: säg "vad har jag i kalendern idag" i action-popup.

## Kvarvarande arbete — prioriterat

### 1. Command palette UI (smart-functions)

Scope:
- Ny hotkey `Ctrl+Shift+Space` (registrera via `register_with_role("palette", ...)`)
- Nytt Tauri-window `palette.html` — liten centrerad modal.
- Search-input + scrollbar lista av smart-functions från `list_smart_functions()`.
- Val triggar samma flow som action-popup: om mode=transform, capture selection; om mode=query, ta user-input som command.
- Kör LLM med function.system som system-prompt + interpolerad user_template.

Filer att röra:
- `src-tauri/tauri.conf.json` — ny window-config `"palette"`.
- `src-tauri/src/lib.rs` — registrera palette-hotkey + öppna/stänga-IPC.
- `src-tauri/crates/ipc/src/commands.rs` — `run_smart_function(id, selection, command)`.
- `src/windows/Palette.tsx` + `.css` — UI.
- `src/main.tsx` eller motsvarande — routing för palette-window.

Uppskattat scope: 3-4 timmar.

### 2. Silero VAD

Scope: Ersätt `trim_silence` (enkel RMS-tröskel) i `src-tauri/crates/audio/src/vad.rs` med Silero ONNX-modell för bättre röstdetektion i brus (fan, tangentbord, etc.).

Filer:
- `src-tauri/crates/audio/Cargo.toml` — lägg `ort = "2"` (ONNX runtime).
- `src-tauri/crates/audio/src/vad_silero.rs` — ny modul.
- `src-tauri/resources/models/silero_vad.onnx` — ladda ner modellen (~2 MB).
- Behåll gammal `trim_silence` som fallback om ORT-init failar.

Uppskattat scope: 4-5 timmar (mest ORT-setup + Windows-kompatibilitet).

### 3. Streaming STT med partials

Scope: Live-transkribering under PTT-hold. Visa partials i overlay.

Stort scope (chunked pipeline + LocalAgreement-2 dedup + UI-partial-state). Uppskattat 1-2 dagar.

### 4. Publik release

- **EV-cert**: Marcus köper ~$300/år cert från Sectigo/DigiCert. signeras MSI via `signtool`.
- **Auto-updater**: `tauri-plugin-updater`. Self-hosted update-server eller GitHub Releases som feed. Signaturverifiering.
- **MSI-bundle-test**: `scripts/bundle-python.ps1` drar ner ~2.3 GB Python-runtime; `cargo tauri build` skapar MSI.

## Viktiga filer

| Fil | Roll |
|---|---|
| `plan.md` | Ursprunglig vision |
| `docs/superpowers/plans/2026-04-17-remaining-work-roadmap.md` | Roadmap |
| `src-tauri/src/lib.rs` | Setup + workers |
| `src-tauri/src/agentic.rs` | Tool-use-loop-integration |
| `src-tauri/src/migrate.rs` | Settings-migration |
| `src-tauri/crates/secrets/` | Keyring-wrapper |
| `src-tauri/crates/hotkey/` | LowLevelKeyboardHook + role-based rebind |
| `src-tauri/crates/llm/src/tools.rs` | Anthropic tool-use data + step |
| `src-tauri/crates/integrations/src/google/` | OAuth + Calendar + Gmail + tool_registry |
| `src-tauri/crates/smart-functions/` | JSON-baserade prompts |
| `src/windows/Settings.tsx` | Main UI |
| `src/windows/ActionPopup.tsx` | Action-popup med tool-status-rader |
| `.github/workflows/ci.yml` | CI-pipeline |

## Bygga / köra

```bash
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"
cargo tauri dev                                            # utveckling
cd src-tauri && cargo test --workspace -- --test-threads=1 # 58 tester gröna

# Release (privat):
cargo tauri build                                          # bygger .exe
# Release (publikt med MSI):
.\scripts\bundle-python.ps1                                # ~2.3 GB Python
cargo tauri build                                          # MSI med bundle
# (Signering kräver EV-cert — se "Publik release" ovan.)
```

## Design-principer (etablerade)

- **Editorial × pro-audio studio**: charcoal/ivory/amber, Fraunces + Instrument Sans + JetBrains Mono
- **Wow-känsla obligatorisk** — inga generiska AI-UI:er
- **Privacy-first default** — cloud opt-in; Google kräver explicit OAuth
- **Hot-reload alltid** — inga "kräver omstart"-varningar kvar
- **Tangentbord får aldrig fastna** — symmetriskt konsumera PTT-events, spara target-HWND
- **Fail-soft för secrets/integration** — popup visar felmeddelanden; ingen klartext-fallback
- **Konservativ agentic-heuristik** — föredrar vanlig streaming över onödig tool-loop

## Manuell testchecklist för Marcus första saken nästa dag

- [ ] `cargo tauri dev` startar utan fel, tray-ikon syns
- [ ] Dictation med Ctrl höger funkar
- [ ] Action-popup med Insert funkar
- [ ] Settings: ändra dictation-hotkey → Spara → NYA hotkey funkar direkt (utan restart!)
- [ ] Smart-functions-sektion visar 5 defaults; "Öppna mapp" öppnar Explorer
- [ ] Google-setup enligt instruktioner ovan → "Anslut Google-konto" → fungerar
- [ ] Efter Google ansluten: säg "vad har jag i kalendern idag" → agentic tool-call → svar
- [ ] Credential Manager visar `svoice-v3` med `anthropic_api_key` + `google_refresh_token`
- [ ] `%APPDATA%/svoice-v3/smart_functions/` innehåller 5 .json-filer
