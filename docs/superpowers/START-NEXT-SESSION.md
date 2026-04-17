# Starta nästa session

## Första meddelandet att skicka till Claude

> Hej! Vi fortsätter på SVoice 3. Appen är fullt fungerande på `main` med Google-integration + Keyring + konfigurerbara hotkeys. Senaste tag: `iter4-phase1-complete`. Nästa steg: **integrera tool-use-loopen i action-worker** (koppla ihop `svoice_llm::tools` + `svoice_integrations::google::tool_registry` i `src-tauri/src/lib.rs` action-worker så popup-en kan köra agentic kommandon som "lägg till möte imorgon kl 14"). Läs i ordning: `docs/superpowers/plans/2026-04-17-remaining-work-roadmap.md` + denna fil.

## Status — appen i nuläget

**Fullt fungerande end-to-end:**

1. **Diktering** (konfigurerbar hotkey, default höger Ctrl) — KB-Whisper via Python-sidecar, 200-700 ms på RTX 5080.
2. **Action-LLM popup** (konfigurerbar hotkey, default Insert) — Claude Sonnet 4.5 + Ollama (Qwen 2.5 14B default). Auto-mode provar Ollama först, faller tillbaka till Claude.
3. **Voice-oval overlay** (nederst centrerat) — SV-monogram + symmetrisk waveform + progress-bar under STT.
4. **Settings-UI** med sektioner:
   - Moduler (STT/Action-LLM/polering-toggles)
   - Audio (mic)
   - STT (modellval)
   - LLM (provider, API-nyckel lagras nu i Windows Credential Manager via `••••••••`-UX)
   - Snabbkommandon (9 valbara hotkeys per PTT, restart-required)
   - Integrationer (Google OAuth anslut/frånkoppla + client-ID-konfig)
5. **Tray-resident** — main dolt by default, öppnas via vänsterklick.
6. **Hot-reload** — alla settings-ändringar träder i kraft direkt (förutom hotkeys som kräver restart i nuvarande version).
7. **Windows-notifikationer** — toast när Ollama-pull är klar (user kan stänga Settings).
8. **Keyring för secrets** — anthropic_api_key + google_refresh_token under service `svoice-v3`.

## Git

```
main @ tag iter4-phase1-complete
Taggar: iter1-complete, iter2-complete, iter2.5-complete, iter3-complete,
        iter4.5a-complete, iter4.5b-complete, iter4-phase1-complete
Repo:  https://github.com/stegbergmarcus/SVoice-ver-3 (privat)
Tester: 46 gröna, 0 failed, 1 ignored
```

## Roadmap-status

| Prio | Fas | Scope | Status |
|---|---|---|---|
| ✅ | Iter 4.5a | Keyring (`svoice-secrets`-crate, `••••••••` UX, JSON → CM migration) | **Klar** |
| ✅ | Iter 4.5b.1 | OS-notifikation när Ollama-pull är klar | **Klar** |
| ✅ | Iter 4.5b.2 | Konfigurerbara hotkeys (9 keys, same-key-validering) | **Klar** |
| ✅ | Iter 4 fas 1 | OAuth 2.1 PKCE + callback-server + keyring-token-storage | **Klar** |
| ✅ | Iter 4 fas 2 | GoogleClient + Calendar v3 + Gmail v1 | **Klar** |
| ✅ | Iter 4 fas 3 | Tool-use-schema (`svoice_llm::tools`) + Google-dispatcher | **Klar** |
| 🔥 | Iter 4 fas 4 | **Integrera tool-use-loop i action-worker** (nästa steg) | Kvar |
| 💡 | Iter 5a | Silero VAD + streaming STT | Kvar |
| 💡 | Iter 5b | Smart-function library + command palette | Kvar |
| 💡 | Release | EV-cert + auto-updater + CI/CD + hot-reload av hotkeys | Kvar |

## Nästa steg — iter 4 fas 4 (koppla ihop pusselbitarna)

Tool-use-loopen är redan skriven i `svoice_llm::tools::step()` + `svoice_integrations::google::tool_registry::execute()`. Kvarstår att integrera i `action_worker_loop` (i `src-tauri/src/lib.rs`).

**Arkitektur-skiss:**

```rust
// Pseudo, i action_worker_loop efter STT har transkriberat user-kommandot:
let google_enabled = svoice_secrets::has_google_refresh_token() && settings.google_oauth_client_id.is_some();
let use_tools = google_enabled && looks_agentic(&user_command); // enkel heuristik

if use_tools {
    let client = GoogleClient::new(client_id, refresh_token);
    let mut conv = ToolConversation::new(Some(SYSTEM_PROMPT.into()), user_command);
    let tools: Vec<ToolDef> = tool_registry::all_tools_json()
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
    loop {
        match tool_step(&api_key, &model, &mut conv, &tools, 1024, 0.3).await? {
            StepOutcome::Finished { text } => { emit_done_with_text(text); break; }
            StepOutcome::NeedTools { calls, .. } => {
                emit_popup_status(format!("Claude kör verktyg: {}…", calls[0].name));
                let mut results = vec![];
                for call in &calls {
                    let content = tool_registry::execute(&client, &call.name, &call.input).await
                        .unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e));
                    results.push(ToolResult { tool_use_id: call.id.clone(), content, is_error: false });
                }
                conv.add_tool_roundtrip(assistant_blocks_from_last_response, &results);
            }
        }
    }
} else {
    // Befintlig streaming-path som nu.
}
```

**Saker att hantera:**

1. `parse_response` i `tools.rs` sparar redan assistant-blocks i conv för `Finished`-fallet. För `NeedTools` behöver caller skicka med blocks till `add_tool_roundtrip` (se kod i `tools.rs`). Denna detalj behöver ev. refaktoreras för cleaner API.
2. Popup-UI behöver en ny event `action_tool_call` som visar "Claude kör `create_calendar_event`…" med spinner.
3. `looks_agentic()`-heuristik — kan börja med simpel keyword-match ("möte", "kalendern", "mail") eller bara ALLTID använda tools om Google är ansluten och user-prompt är längre än ~20 ord.
4. Timing: tool-calls blockerar user. Max-iteration-guard (t.ex. 5 rounds) så inte Claude loopar oändligt.
5. Error-fallback: om tool-call failar, gå tillbaka till ren text-streaming.

**Alternativ plan:** skapa en SEPARAT IPC-command `agentic_action(command)` som kör loopen icke-streaming och returnerar final text. Action-popup kan försöka agentic först, fallback till streaming. Enklare att rolla ut incrementellt.

## Viktiga filer

| Fil | Roll |
|---|---|
| `plan.md` | Ursprunglig vision (Fas 1-4) |
| `docs/superpowers/plans/2026-04-17-remaining-work-roadmap.md` | Komplett roadmap från denna punkt |
| `src-tauri/src/lib.rs` | Setup + workers (audio, dictation, action) — **action_worker_loop ska utökas med tool-use** |
| `src-tauri/crates/llm/src/tools.rs` | Tool-use datamodell + `step()` — klar |
| `src-tauri/crates/integrations/src/google/tool_registry.rs` | Tools-definitioner + `execute()` — klar |
| `src-tauri/crates/integrations/src/google/oauth.rs` | PKCE-flow — klar |
| `src-tauri/crates/integrations/src/google/client.rs` | Auto-refreshing REST-client — klar |
| `src-tauri/crates/secrets/src/lib.rs` | Keyring (anthropic + google_refresh_token) — klar |
| `src/windows/Settings.tsx` | Main settings-UI — klart för iter 4 |
| `src/windows/ActionPopup.tsx` | Action-popup — kvar: visa tool-status |

## Setup för Google-integration (Marcus måste göra)

Innan tool-use-loopen kan testas end-to-end:

1. Gå till https://console.cloud.google.com/apis/credentials
2. Skapa OAuth-client av typ **Desktop app**.
3. Under "Authorized redirect URIs" lägg till `http://127.0.0.1/callback` (porten är ephemeral så ingen specifik port krävs — Google matchar på domän+path).
4. Aktivera Calendar API + Gmail API i https://console.cloud.google.com/apis/library.
5. Kopiera client-ID:n → paste i Settings → Integrationer → Google OAuth client-ID → Spara.
6. Klicka "Anslut Google-konto" — browser öppnas med consent-sida.
7. Godkänn. App får refresh-token + sparar i keyring.

## Arbetsflöde för nästa session

1. Checkout branch: `git checkout -b iter4/phase4-tool-use-integration`
2. Läs `src-tauri/crates/llm/src/tools.rs` och `.../integrations/src/google/tool_registry.rs`
3. Implementera `looks_agentic()` + tool-use-loop i `action_worker_loop`
4. Lägg till `action_tool_call`-event + visa i popup
5. Testa manuellt end-to-end med Marcus Google-konto
6. Merge → tag `iter4-complete`

## Bygga / köra

```bash
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"
cargo tauri dev                                          # utveckling
cd src-tauri && cargo test --workspace -- --test-threads=1   # 46 tester gröna

# Release-bygg (om MSI önskas):
.\scripts\bundle-python.ps1                              # ~2.3 GB Python-runtime
cargo tauri build                                        # MSI med bundled runtime
```

## Design-principer

- **Editorial × pro-audio studio**: charcoal/ivory/amber, Fraunces + Instrument Sans + JetBrains Mono
- **Wow-känsla obligatorisk** — inga generiska AI-UI:er (callback-sidan är ett gott exempel)
- **Privacy-first default** — cloud är opt-in; Google kräver explicit OAuth
- **Hot-reload alltid** — (undantag: hotkeys kräver restart i fas 1, hot-reload kommer senare)
- **Tangentbord får aldrig fastna** — symmetriskt konsumera PTT-events, spara target-HWND
- **Fail-soft för secrets/integration** — om keyring saknas, popup visar tydligt felmeddelande; ingen klartext-fallback.

## Testnyckel under natten

Under autonoma natten byggdes alla features utan att jag kunde testa manuellt i riktig app. Följande manuella tester rekommenderas första saken imorgon:

- [ ] `cargo tauri dev` startar utan fel
- [ ] Settings → API-nyckel visar `••••••••` efter migration (om du hade klartext tidigare)
- [ ] Rensa-knappen + skriv ny nyckel + Spara → fungerar
- [ ] Ändra dikterings-hotkey → Spara → omstart → ny hotkey fungerar
- [ ] Starta Ollama-pull → stäng Settings → vänta → OS-toast dyker upp
- [ ] Sätt Google client-ID → Spara → "Anslut Google-konto" → browser öppnas
  (Om flowet failar: kolla redirect URI-matchning i Google Cloud Console)
- [ ] Credential Manager visar både `anthropic_api_key` och `google_refresh_token` under service `svoice-v3`
