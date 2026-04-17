# Starta nästa session

## Första meddelandet att skicka till Claude

> Hej! Vi fortsätter på SVoice 3. Iter 2 är klar och merged till main (tag `iter2-complete`).
> Läs `plan.md` för övergripande vision — Fas 1.5 (overlay-polish + logo) är nästa småfix, sen Fas 2 (action-LLM popup — unified röstdriven UX som slår ihop selection-transform, Q&A och agentic tool-calls till en enda popup). Välj vad du vill börja med: `iter2.5/overlay-polish` eller direkt på `iter3/action-llm-popup`. Använd `superpowers:brainstorming` om vi ska designa action-popup-flödet innan implementation.

## Status iter 2

Iter 2 levererade:

- **Riktig svensk STT** via Python-sidecar + faster-whisper (kb-whisper-medium, 300-700 ms på RTX 5080).
- **Audio-pipeline**: WASAPI-capture med ringbuffer, linjär resampler till 16 kHz mono, energi-VAD.
- **Settings-UI** i React med editorial-minimalism × pro-audio studio-estetik, live mic-meter med tröskelmarkering, JSON-persistens till `%APPDATA%/svoice-v3/settings.json`.
- **Python-runtime-bundling** via `scripts/bundle-python.ps1` (~2.3 GB, körs vid behov för MSI-build).
- **End-to-end verifierat** i dev-mode. MSI-build deferred enligt friends-kloning-strategi (SAC-block + 2 GB runtime).

25 commits mellan `iter1-complete` och `iter2-complete`. Alla tester gröna, 0 warnings.

## Verifierade fakta

- Höger Ctrl PTT fungerar via LowLevelKeyboardHook.
- STT-pipeline: audio-capture (cpal WASAPI, persistent, !Send-hanteras via per-thread spawn) → ringbuffer → drain vid key-up → VAD-trim → PythonStt::transcribe → clipboard-paste injection.
- Python-sidecar på `py -3.11` (launcher-flagga, `py` default är 3.14 på dev-maskinen). Release-build använder bundlat embeddable Python via tauri resource_dir.
- CUDA 12 DLLs hittas via os.environ["PATH"] injection i Python (inte enbart add_dll_directory — ctranslate2 är C-extension som letar via conventional PATH).
- UTF-8 stdio måste reconfiguras explicit på Windows Python (default cp1252 bryter Rust read_line på svenska tecken).
- Settings läses vid app-start och appliceras på SttConfig.model, device, compute_type, vad_threshold. Hot-reload av modell kräver restart i iter 2.
- Live mic-meter i Settings använder AudioCapture's inbyggda rate-limited RMS-callback (~30 Hz) — ingen dubbel cpal-stream.

## Nyckelfiler att känna till

| Fil | Roll |
|---|---|
| `plan.md` | Vision: Fas 1 (iter 2) klar, Fas 1.5 nästa, Fas 2 = action-LLM popup |
| `src-tauri/src/lib.rs` | Tauri setup, PTT-worker-loop, Settings → runtime wiring |
| `src-tauri/crates/stt/src/{protocol,sidecar,engine}.rs` | JSON-protokoll, async driver, PythonStt |
| `src-tauri/resources/python/stt_sidecar.py` | Python-sidecar (faster-whisper, CUDA PATH, UTF-8 stdio) |
| `src-tauri/crates/audio/src/{capture,ringbuffer,resample,vad}.rs` | Audio pipeline |
| `src-tauri/crates/settings/src/lib.rs` | JSON settings med ComputeMode |
| `src/windows/Settings.tsx`, `Settings.css`, `theme.css` | React Settings-view + design-tokens |
| `src/lib/settings-api.ts` | Typad Tauri-invoke-wrapper för settings |
| `scripts/bundle-python.ps1` | Embeddable Python + pip deps för release-MSI |

## Iter 2.5 — Overlay-polish (om det blir först)

Befintlig overlay (`src/overlays/RecordingIndicator.tsx` + `overlay.html`) är basic. Plan säger:
- Oval "voice-oval" med två zoner: vänster SV-monogram (logotyp), höger live waveform (~30 Hz, reagerar på volym).
- Transcribing-state: waveform → indeterminate progress-streck.
- Micro-animationer mellan state-byten (ease-in/out).
- Designa en riktig SVoice-logotyp (monogram SV i samma visuella språk som settings).

Designtokens finns redan i `src/theme.css` — återanvänd dem. Overlay har egen window-label i tauri.conf.json (transparent, always-on-top).

## Iter 3 — Action-LLM popup

**Kärnvision** (project-memory-fångad):
- Ny PTT-hotkey (t.ex. höger Alt) öppnar kontextmedveten popup.
- Selection-detection (Win32 UI Automation eller clipboard-snapshot) styr läge:
  - Markerad text + röstkommando → LLM transformerar → preview i popup → Enter ersätter original.
  - Ingen markering → Q&A-popup, streamad LLM-svar.
  - Agentic kommando (iter 4) → Google-tool-call.
- Ollama-sidecar infrastruktur återanvänder iter 2-sidecar-pattern.
- Popup = separat Tauri-window (always-on-top, center-of-screen, blur-backdrop).

**Börja med `superpowers:brainstorming`** för att spec:a UX-flödet innan implementation.

## Arbetsflöde

1. Checkout ny branch: `git checkout -b iter2.5/overlay-polish` eller `iter3/action-llm-popup`.
2. Brainstorma om scope är stort (action-popup ja, overlay-polish nej).
3. Skriv plan → executa → merge → tag.

## Bygga / köra

```powershell
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"
cargo tauri dev           # utveckling
cargo tauri build         # release MSI (kräver bundle-python.ps1 först)
cd src-tauri
cargo test --workspace    # alla tester
```

## Git-state vid senaste commit

- Branch: `main` vid tag `iter2-complete`
- Senaste commit: `388383f` (merge från iter2/real-stt)
- Klass: 0 warnings, 22 tests grön (+ 1 ignored integration-test för sidecar)

## Lycka till!
