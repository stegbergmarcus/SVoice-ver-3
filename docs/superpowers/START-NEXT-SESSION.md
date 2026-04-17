# Starta nästa session

## Första meddelandet att skicka till Claude

> Hej! Vi fortsätter på SVoice 3. Iter 1 → 3 är mergade till main (taggar iter1-complete, iter2-complete, iter2.5-complete, iter3-complete). Iter 3 (action-LLM popup) är kompilerad men INTE manuellt verifierad — högsta prio är att testa end-to-end med en Anthropic API-nyckel. Läs `plan.md` och `docs/superpowers/specs/` för kontext. Iter 4 scope: Google-integration via tool-calls, Ollama lokal LLM, keyring för API-nycklar.

## Status översikt

| Iter | Tagg | Levererat |
|---|---|---|
| 1 | `iter1-complete` | Walking skeleton: PTT hook, clipboard-paste, tray |
| 2 | `iter2-complete` | Riktig STT (KB-Whisper via Python-sidecar), settings-UI, audio-pipeline |
| 2.5 | `iter2.5-complete` | Voice-oval overlay med SV-monogram + live waveform |
| 3 | `iter3-complete` | Action-LLM popup: höger Alt → selection-transform / Q&A via Anthropic |

## Iter 3 — vad måste verifieras manuellt

Backend kompilerar grön. Frontend bundlas klar. **Men LLM-flödet är inte manuellt testat** pga API-nyckel-krav. Första prioritet för nästa session:

1. Starta appen: `cargo tauri dev`.
2. Öppna Settings-vyn → lägg in Anthropic API-nyckel → spara.
3. **Starta om appen** (settings läses bara vid start).
4. Markera text i Notepad → håll höger Alt + säg "gör detta till en punktlista" → släpp.
5. Popup ska öppnas → selection + kommando visas → Claude streamar svar.
6. Enter → markerad text ersätts.
7. Ingen markering → höger Alt + "vad är huvudstaden i Island" → popup → Q&A-svar.

**Kända unverified edge-cases:**
- Popup window `visible: false` + `.show()`-flödet (Tauri 2).
- Focus-hantering: popup tar focus via `setFocus()` — men hur Enter-baserad paste hamnar i rätt target-window efter popup hide är osäkert. Kan behöva spara `GetForegroundWindow()`-hwnd innan popup visas och restore före paste_and_restore.
- Samtidigt pressed RightCtrl + RightAlt → odefinierat beteende (delad AudioRing).
- LLM-prompten är svenska — Haiku/Sonnet hanterar det bra, men formatet ska testas.

## Nyckelfiler (iter 3)

| Fil | Roll |
|---|---|
| `src-tauri/src/lib.rs` | Setup + 3 worker-trådar (audio-owner, ptt, action) |
| `src-tauri/crates/hotkey/src/ll_hook.rs` | Multi-key LowLevelKeyboardHook (RCtrl + RAlt) |
| `src-tauri/crates/llm/src/{provider,anthropic}.rs` | LLM-trait + Anthropic SSE-streaming |
| `src-tauri/crates/inject/src/clipboard.rs` | capture_selection + paste_and_restore |
| `src-tauri/crates/ipc/src/commands.rs` | action_apply / action_cancel IPC |
| `src/windows/ActionPopup.tsx` + CSS | Popup-UI med editorial-minimalism |
| `src/overlays/action-popup.html` | Popup-entry + fonts |

## Iter 4-scope (framtid)

**Prioritet 1 — Google tool-calls:**
- Anthropic tool-use-loop i action-popup
- `integrations/google/` crate: OAuth 2.1 PKCE + Gmail/Calendar/Drive-wrappers
- Fem första verktyg: `list_calendar_events`, `create_calendar_event`, `search_emails`, `read_email`, `draft_reply`
- Agentic commands i popup: "lägg till detta i kalendern på fredag" → Claude → tool-call → utför

**Prioritet 2 — Ollama lokal LLM:**
- `svoice-llm` utökad med `OllamaClient` (HTTP mot localhost:11434, SSE-streaming)
- Settings: toggle "Använd lokal LLM när tillgänglig"
- Fallback-ordning: Ollama → Anthropic → error

**Prioritet 3 — Säkerhet + polish:**
- Flytta API-nyckel från settings.json till Windows Credential Manager via `keyring-rs`
- Simultan-keypress-konflikt: global AtomicBool som blockerar andra worker när en är aktiv
- Focus-hantering: spara target-hwnd före popup visas, restore före paste

**Prioritet 4 — UX:**
- Hot-reload av STT-modell (byt modell i Settings → ring sidecar shutdown + respawn med ny model)
- Konfigurerbar hotkey (ersätt hårdkodad RCtrl/RAlt)
- Model Center för STT-modeller (nedladdning + byte)

## Arbetsflöde

1. `git checkout -b iter4/<scope>`
2. Brainstorma med `superpowers:brainstorming` om scope är stort (Google-integration är det).
3. Skriv plan → execute → merge → tag.

## Bygga / köra

```powershell
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"
cargo tauri dev                      # utveckling
cd src-tauri && cargo test --workspace  # alla tester (19 passed)

# Release-bygg (om distributionsbundle önskas):
./scripts/bundle-python.ps1          # laddar ner ~2.3 GB Python-runtime
cargo tauri build                    # bygger MSI (friends-distribution)
```

## Git-state

- Branch: `main`
- Senaste tagg: `iter3-complete`
- Antal commits sedan `iter1-complete`: ~45
- Alla tester gröna, 0 warnings.

## Lycka till!
