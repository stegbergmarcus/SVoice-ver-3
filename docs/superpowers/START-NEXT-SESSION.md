# Starta nästa session

## Första meddelandet att skicka till Claude

> Hej! Vi fortsätter på SVoice 3. Appen är fullt fungerande på `main` och vi har en roadmap att följa. Läs i ordning: `docs/superpowers/plans/2026-04-17-remaining-work-roadmap.md` (komplett plan), `plan.md` (ursprunglig vision), `docs/superpowers/specs/` (specs). Git-state: senaste tag `iter4.5a-complete`, branch `main`. Rekommenderat nästa steg enligt roadmap: **Iter 4.5b** (notifikationer + konfigurerbara hotkeys) eller direkt på **Google-integration iter 4**. Använd `superpowers:subagent-driven-development` för större implementations-arbete.

## Status — appen i nuläget

**Fullt fungerande end-to-end:**

1. **Diktering** (höger Ctrl) — KB-Whisper via Python-sidecar, 200-700 ms på RTX 5080, auto-cachad i HF.
2. **Action-LLM popup** (Insert) — Claude Sonnet 4.5 + Ollama (Qwen 2.5 14B default). Auto-mode provar Ollama först, faller tillbaka till Claude.
3. **Voice-oval overlay** (nederst centrerat) — SV-monogram + symmetrisk waveform + progress-bar under STT.
4. **Settings-UI** — moduler-toggles (STT av/på, Action-LLM av/på, LLM-polering), modell-dropdowns med ✓/↓ cache-status, Ollama-download med progress-bar, API-nyckel-fält.
5. **Tray-resident** — main dolt by default, öppnas via vänsterklick.
6. **Hot-reload** — alla settings-ändringar träder i kraft direkt, även STT-modell-byte.

**Kritiska bug-fixar landade:**
- Tangentbord-hang vid paste (key-repeat-consumption + HWND save/restore + own-process guard)
- Simultan PTT-lockout (Ctrl+Insert samtidigt ignoreras)
- Popup öppnas tidigt så STT/LLM-fel syns

## Git

```
main @ tag iter4.5a-complete
Taggar: iter1-complete, iter2-complete, iter2.5-complete, iter3-complete, iter4.5a-complete
Repo: https://github.com/stegbergmarcus/SVoice-ver-3 (privat)
```

## Vad som är kvar — läs roadmap-filen

**`docs/superpowers/plans/2026-04-17-remaining-work-roadmap.md`** innehåller den kompletta planen. Kortfattat:

| Prio | Fas | Scope |
|---|---|---|
| ✅ | Iter 4.5a | Keyring (klar — nyckel i Windows Credential Manager, `svoice-secrets`-crate) |
| 🔥 | Iter 4.5b | Notifikationer när download klar + konfigurerbara hotkeys |
| ⚠️ | Iter 4 | Google-integration: OAuth 2.1 + Gmail/Calendar + tool-use-loop |
| 💡 | Iter 5a | Silero VAD + streaming STT |
| 💡 | Iter 5b | Smart-function library + command palette |
| 💡 | Release | EV-cert + auto-updater + CI/CD |

Total tidsestimat till publik-produktionsklar: **6-8 veckor fokuserat arbete**.

## Viktiga filer

| Fil | Roll |
|---|---|
| `plan.md` | Ursprunglig vision (Fas 1-4) |
| `docs/superpowers/plans/2026-04-17-remaining-work-roadmap.md` | Komplett roadmap från denna punkt |
| `src-tauri/src/lib.rs` | Setup + workers (audio, dictation, action) |
| `src-tauri/crates/*/src/` | Pure logic-crates |
| `src-tauri/resources/python/stt_sidecar.py` | Python-sidecar |
| `src/windows/Settings.tsx` | Main settings-UI |
| `src/windows/ActionPopup.tsx` | Action-popup |
| `src/overlays/RecordingIndicator.tsx` | Voice-oval overlay |
| `src/components/SVoiceLogo.tsx` | Logo-komponent |

## Arbetsflöde

1. Checkout branch: `git checkout -b iter<N>/<scope>`
2. Läs roadmap-filen för valt scope
3. Brainstorma med `superpowers:brainstorming` om scope är stort (Google-integration är det)
4. Skriv plan → execute via `superpowers:subagent-driven-development` → merge → tag
5. Uppdatera `START-NEXT-SESSION.md` efter merge

## Bygga / köra

```powershell
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"
cargo tauri dev                      # utveckling (settings läses live)
cd src-tauri && cargo test --workspace   # ~22 tester gröna

# Release-bygg (om MSI önskas):
.\scripts\bundle-python.ps1          # ~2.3 GB Python-runtime
cargo tauri build                    # MSI med bundled runtime
```

## Design-principer att följa

- **Editorial × pro-audio studio**: charcoal/ivory/amber, Fraunces + Instrument Sans + JetBrains Mono
- **Wow-känsla obligatorisk** — inga generiska AI-UI:er
- **Privacy-first default** — cloud är opt-in
- **Hot-reload alltid** — user ska aldrig behöva starta om appen
- **Tangentbord får aldrig fastna** — symmetriskt konsumera PTT-events, spara target-HWND

## Lycka till!
