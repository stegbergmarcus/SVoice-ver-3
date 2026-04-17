# SVoice 3

Svensk dikterings-app för Windows. Under aktiv utveckling — iter 1 levererar ett
walking skeleton där höger Ctrl används som push-to-talk och injicerar en
testtext i valfri Windows-app.

## Status

- **Iter 1** (walking skeleton + STT-spike): merged, tag `iter1-complete`.
- **Iter 2** (riktig STT + settings-UI): merged, tag `iter2-complete`. KB-Whisper via
  Python-sidecar, CUDA-inferens ~300-700 ms, Settings-UI med live mic-meter.
- **Iter 2.5** (voice-oval overlay): merged, tag `iter2.5-complete`. SV-monogram
  logotyp + live waveform + indeterminate progress-bar vid transkribering.
- **Iter 3** (action-LLM popup): merged, tag `iter3-complete`. Höger Alt öppnar en
  kontextmedveten popup som transformerar markerad text eller svarar på frågor via
  Anthropic Claude med SSE-streaming. **Kräver manuell verifiering med API-nyckel.**
- **Iter 4** (Google tool-calls + Ollama + keyring): nästa.

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
pnpm install
# För release-build (MSI): kör även bundle-python.ps1 innan cargo tauri build
cargo tauri dev
```

Setup-skriptet installerar Rust, Node.js, pnpm, CMake och Tauri CLI via winget,
kör ett första cargo build och förbereder allt för `cargo tauri dev`.

**Systemkrav för dev-mode:**
- Python 3.11 installerat (`py -3.11 --version` ska fungera).
- `pip install faster-whisper numpy nvidia-cublas-cu12 nvidia-cudnn-cu12 nvidia-cuda-runtime-cu12 nvidia-cuda-nvrtc-cu12` i Python 3.11-miljön (vänner som redan har det från spike kan skippa).
- NVIDIA GPU med CUDA-stöd (fallback till CPU möjlig via Settings → Beräkningsläge).

### Bundla Python-runtime för release-build

För att bygga en fristående MSI där Python följer med:

```powershell
.\scripts\bundle-python.ps1    # laddar ner ~2 GB Python + CUDA
cargo tauri build              # bygger MSI med bundlad runtime
```

Utan bundlat runtime använder appen systemets Python 3.11 via `py -3.11`.

### Manuell setup

1. **Rust**: `winget install Rustlang.Rustup` (eller [rustup.rs](https://rustup.rs)).
2. **Node.js LTS**: `winget install OpenJS.NodeJS.LTS`.
3. **pnpm**: `npm install -g pnpm`.
4. **Tauri CLI**: `cargo install tauri-cli --version "^2.0" --locked`.
5. **Python 3.11**: `winget install Python.Python.3.11`.
6. **CMake** (valfritt, endast för STT-spike): `winget install Kitware.CMake`.

Sen:

```powershell
pnpm install
py -3.11 -m pip install faster-whisper numpy nvidia-cublas-cu12 nvidia-cudnn-cu12 nvidia-cuda-runtime-cu12 nvidia-cuda-nvrtc-cu12
cargo tauri dev
```

## Användning

När appen kör:

- **Huvudfönstret** visar Settings-vyn (mörkt tema med vänster wordmark + höger inställningspanel) — mikrofon, STT-modell, beräkningsläge, tystnadströskel med live mic-meter.
- **Overlay-pill** i övre vänstra hörnet visar PTT-state (Redo / Spelar in… / Transkriberar…).
- **Tray-ikon** i Windows systemfält visar aktuell state och har en Avsluta-meny.
- **Push-to-talk:** håll **höger Ctrl** i valfri Windows-app och säg något på svenska. Släpp Ctrl och transkriptet injiceras där fokus är. Första PTT-tryck efter start laddar modellen (~1-2 s), sen cachad.

## Arkitektur

- **Frontend:** React 18 + TypeScript + Vite (multi-window via Tauri 2). Design-språk: editorial minimalism × pro-audio studio (Fraunces display-serif, Instrument Sans body, JetBrains Mono värden, charcoal + bärnsten-amber).
- **Backend:** Rust (Tauri 2) uppdelad i separata crates:
  - `svoice-hotkey` — LowLevelKeyboardHook för höger Ctrl PTT + PTT state machine
  - `svoice-inject` — clipboard-paste + SendInput Unicode-fallback
  - `svoice-audio` — WASAPI-capture, ringbuffer, linjär resample till 16 kHz mono, energi-VAD, live RMS-emit
  - `svoice-stt` — async Python-sidecar-driver (faster-whisper), JSON-protokoll över stdin/stdout
  - `svoice-settings` — JSON-backed settings i `%APPDATA%/svoice-v3/settings.json`
  - `svoice-ipc` — Tauri-kommandon (get_settings, set_settings) och event-typer
  - `svoice-llm`, `svoice-integrations` — stubs tills iter 3+
- **Python-sidecar:** `src-tauri/resources/python/stt_sidecar.py` — long-living subprocess med faster-whisper, UTF-8-stdio, CUDA-DLL PATH-injection.
- **Spike-binär:** `svoice-stt-spike` + Python-script för STT-benchmark.

## Licens

Proprietary — ej för distribution.
