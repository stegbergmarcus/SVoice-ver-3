# SVoice 3

Svensk dikterings-app för Windows. Under aktiv utveckling — iter 1 levererar ett
walking skeleton där höger Ctrl används som push-to-talk och injicerar en
testtext i valfri Windows-app.

## Status

- **Iter 1 (walking skeleton + STT-spike):** klar på `iter1/walking-skeleton-spike`.
- **Iter 2 (riktig STT via Python-subprocess + WASAPI-capture + VAD):** planeras.

Se `plan.md` för övergripande vision och `docs/superpowers/specs/` för detaljerade
iter-specifikationer och spike-rapporter.

## Distribution

Appen är i tidig utveckling och distribueras inte som osignerad installer —
**Windows 11 Smart App Control blockerar osignerade MSI/NSIS**. Se
[docs/superpowers/specs/2026-04-16-sac-mitigation-plan.md](docs/superpowers/specs/2026-04-16-sac-mitigation-plan.md)
för detaljer.

Initial användning: kör från source via `cargo tauri dev`.

## Komma igång

### Kortversionen

```powershell
git clone <repo-url> "SVoice ver 3"
cd "SVoice ver 3"
.\scripts\setup-dev.ps1
cargo tauri dev
```

Setup-skriptet installerar Rust, Node.js, pnpm, CMake och Tauri CLI via winget,
kör ett första cargo build och förbereder allt för `cargo tauri dev`.

### Manuell setup

1. **Rust**: `winget install Rustlang.Rustup` (eller [rustup.rs](https://rustup.rs)).
2. **Node.js LTS**: `winget install OpenJS.NodeJS.LTS`.
3. **pnpm**: `npm install -g pnpm`.
4. **Tauri CLI**: `cargo install tauri-cli --version "^2.0" --locked`.
5. **CMake** (valfritt, endast för STT-spike): `winget install Kitware.CMake`.

Sen:

```powershell
pnpm install
cargo tauri dev
```

## Användning

När appen kör:

- **Huvudfönstret** visar en minimal statusvy.
- **Overlay-pill** i övre vänstra hörnet visar PTT-state (Redo / Spelar in… / Transkriberar…).
- **Tray-ikon** i Windows systemfält visar aktuell state och har en Avsluta-meny.
- **Push-to-talk:** håll **höger Ctrl** i valfri Windows-app och släpp. I iter 1
  injiceras en fast testtext; i iter 2 ersätts det med riktig tal-till-text.

## Arkitektur

- **Frontend:** React 18 + TypeScript + Vite (multi-window via Tauri 2).
- **Backend:** Rust (Tauri 2) uppdelad i separata crates:
  - `svoice-hotkey` — LowLevelKeyboardHook för höger Ctrl PTT + PTT state machine
  - `svoice-inject` — clipboard-paste + SendInput Unicode-fallback
  - `svoice-audio` — cpal-wrapper för mikrofon volym-mätning
  - `svoice-stt` — dummy-transcribe (iter 1), Python-subprocess-wrapper (iter 2)
  - `svoice-ipc` — Tauri-kommandon och event-typer
  - `svoice-llm`, `svoice-settings`, `svoice-integrations` — stubs tills iter 2+
- **Spike-binär:** `svoice-stt-spike` + Python-script för STT-benchmark.

## Licens

Proprietary — ej för distribution.
