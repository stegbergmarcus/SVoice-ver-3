# SVoice 3 — Iter 1 Design (Walking Skeleton + STT-spike)

**Datum:** 2026-04-16
**Scope:** Första iterationen av Fas 1 från `plan.md`.
**Teknisk identifierare:** `svoice-v3`
**Display-namn:** SVoice 3

## Kontext

SVoice 3 är en fristående Windows-applikation för svensk tal-till-text med valbar LLM-polering och smarta funktioner. `plan.md` beskriver hela visionen (4 faser, 9–15 veckors kalendertid). Detta dokument specificerar **endast iter 1** av Fas 1 — det minsta steget som bevisar att arkitekturen är körbar.

Iter 1 består av två parallella spår:
- **Walking skeleton:** ett end-to-end-flöde utan riktig ljud/STT, som bevisar att hotkey → state machine → text-injektion fungerar mot Notepad och andra Windows-mål.
- **STT-spike:** standalone-binary som verifierar att `ct2rs` + kb-whisper-medium kan köras på target-hårdvaran (RTX 5080, CUDA 13.2).

När båda är klara har vi (a) ett fungerande skelett att plugga in riktig STT i, och (b) ett evidensbaserat val av STT-backend. Iter 2 (egen spec) ersätter dummy-STT:n med riktig STT + WASAPI audio capture.

## Workspace-struktur

```
SVoice ver 3/
├─ plan.md
├─ docs/superpowers/
│  ├─ specs/
│  └─ plans/
├─ .gitignore
├─ package.json                     pnpm workspace root
├─ pnpm-workspace.yaml
├─ vite.config.ts
├─ tsconfig.json
├─ index.html
├─ src/                             React frontend
│  ├─ main.tsx
│  ├─ windows/
│  ├─ overlays/
│  └─ lib/
└─ src-tauri/
   ├─ Cargo.toml                    workspace root
   ├─ tauri.conf.json
   ├─ build.rs
   ├─ src/main.rs                   Tauri builder
   ├─ crates/
   │  ├─ audio/                     stub
   │  ├─ stt/                       stub
   │  ├─ hotkey/                    aktiv
   │  ├─ inject/                    aktiv
   │  ├─ llm/                       tom
   │  ├─ settings/                  stub
   │  ├─ ipc/                       aktiv
   │  └─ integrations/              tom
   ├─ resources/
   │  ├─ manifest.json
   │  └─ smart_functions/
   └─ bins/
      └─ stt-spike/                 fristående spike-binary
```

**Designval:**
- **pnpm** över npm — snabbare, bättre workspace-stöd. Installeras globalt via `npm i -g pnpm`.
- **Cargo workspace med separata crates** — matchar planens modulstruktur; ger kompilerings-isolering.
- **`bins/stt-spike/`** som bin-target i samma workspace — spiken delar dependencies med huvudappen men kan köras isolerat via `cargo run -p stt-spike`.

## Walking Skeleton — Scope

Mål: bevisa hela injektions-röret end-to-end utan ljudkedja.

**Komponenter som byggs:**

1. **Tauri 2 scaffold** — React + TypeScript + Vite frontend, Rust backend, bygger och startar i dev- och release-läge.
2. **Global hotkey** — `Win+Alt+Space` som push-to-talk (key-down = start, key-up = stop) via `tauri-plugin-global-shortcut`. Fallback till `Ctrl+Alt+Space` om primärregistrering failar.
3. **PTT state machine** (`src-tauri/crates/hotkey/src/ptt_state.rs`) — tillstånden `Idle → Recording → Processing → Idle`. Tauri-events emitteras till frontend vid varje övergång.
4. **Dummy STT** — på key-up returneras hårdkodad svensk teststräng: `"Hej, det här är ett test med å, ä och ö."` (Validerar Unicode-path utan att bygga ljudkedjan.)
5. **Text-injektion** (`src-tauri/crates/inject/src/send_input.rs`) — via `windows`-crate, skriver Unicode-text till fokuserad app via `SendInput` med `KEYEVENTF_UNICODE`.
6. **Clipboard-paste-fallback** (`src-tauri/crates/inject/src/clipboard.rs`) — om `SendInput` misslyckas (t.ex. UIP/Electron-klient), läggs texten på clipboard och `Ctrl+V` skickas.
7. **Tray-ikon** — grå (idle), röd (recording). Minimal meny: "Quit".
8. **Recording-pill** — liten always-on-top-overlay (`src/overlays/RecordingIndicator.tsx`) som visar `Recording…` / `Transcribing…`. Verifierar Tauri multi-window-setup.

**Medvetet uteslutet:**
- WASAPI audio capture
- Riktig STT (väntar på spike)
- Settings-UI (hårdkodad config)
- Model Center, LLM, smart functions, tray-profilbyte

**Exit-test:** Öppna Notepad. Håll `Win+Alt+Space` i ~1 sekund. Släpp. Texten `"Hej, det här är ett test med å, ä och ö."` ska dyka upp med korrekta svenska tecken. Sekundärtest mot Edge/Chrome URL-bar samt Teams chat-input för injektionsfallback.

## STT-Spike — Scope

Mål: verifiera att `ct2rs` + kb-whisper-medium fungerar på RTX 5080 + CUDA 13.2, innan arkitekturen commitas.

**Placering:** `src-tauri/bins/stt-spike/` som fristående bin-target. Ingen Tauri, ingen UI — bara `cargo run -p stt-spike -- <wav-path>`.

**Flöde:**
1. Ladda kb-whisper-medium (CTranslate2-konverterad) från `KBLab/kb-whisper-medium` via `hf-hub`-crate till `%APPDATA%/svoice-v3/models/kb-whisper-medium/`.
2. Läs pre-inspelad WAV (16 kHz mono, 3–5 s svensk text) som input.
3. Kör transkription; logga:
   - Cold model load-tid
   - Cold inference-tid (första körningen)
   - Warm inference-tid (andra körningen)
   - VRAM-användning (via `nvml-wrapper`)
   - Transkriberad text

**Testfil:** Genereras automatiskt via Windows TTS i PowerShell (`System.Speech.Synthesis`) från en känd mening, sparas som `testdata/sv-test.wav`. Förväntad text är känd → enkel korrekthetscheck.

**Success-kriterier:**
- Transkription producerar korrekt svensk text med å/ä/ö
- Warm inference <300 ms för 5 s klipp (budget för 1.5 s end-to-end på lång monolog enligt `plan.md`)
- VRAM <6 GB
- Ingen CUDA-kraschslog

**Fallback-stege om ct2rs failar — dokumenteras i spike-rapport:**

| Steg | Åtgärd | Konsekvens för arkitektur |
|---|---|---|
| 1 | `ct2rs` i CPU-mode | Bekräftar att FFI-bindning funkar; isolerar CUDA som orsak |
| 2 | `whisper-rs` (whisper.cpp) med CUDA 13.2 | whisper.cpp har färskare CUDA-stöd; arkitektur oförändrad |
| 3 | Python-subprocess med `faster-whisper`-paketet | Garanterat fungerande; +80 MB i distribution, extra process-overhead; STT-crate blir "extern process"-wrapper |

**Exit:** Spike-rapport på `docs/superpowers/specs/2026-04-XX-stt-spike-report.md` med mätvärden, vald backend-väg, och uppdaterad riskbedömning för `plan.md`'s "Riskområden"-sektion.

## Förutsättningar

Måste vara på plats innan implementation kan börja:

| # | Åtgärd | Ansvar | Tid |
|---|---|---|---|
| 1 | Installera Rust via `winget install Rustlang.Rustup` | Claude | ~5 min |
| 2 | `cargo install tauri-cli --version "^2.0"` | Claude | ~2 min |
| 3 | `npm i -g pnpm` | Claude | <1 min |
| 4 | Generera `testdata/sv-test.wav` via Windows TTS | Claude | <1 min |
| 5 | `git init` + `.gitignore` | Claude | <1 min |

**Redan verifierat i miljön:**
- Node 24.12 + npm 11.6
- Git 2.52
- RTX 5080 (16 GB VRAM), driver 595.97
- CUDA toolkit 13.2
- Ollama 0.20.7 (används först i fas 2)
- Python 3.11 + 3.14 (redo om Python-fallback blir aktuell)

## Exit-criteria för Iter 1

Iter 1 räknas som klar när:

1. Walking-skeleton-testet passerar i Notepad, Browser URL-bar och Teams. Clipboard-fallback aktiveras korrekt där `SendInput` failar.
2. Spike-rapport existerar, STT-backend-väg vald och dokumenterad. `plan.md`'s riskområde för `ct2rs` är uppdaterat med spike-resultatet.
3. All kod committad, grön `cargo build`, grön `pnpm build`, alla tester gröna.

Efter iter 1 startar iter 2 (separat spec): ersätt dummy-STT med riktig STT + WASAPI audio capture + VAD.

## Öppna frågor

Inga som blockerar start. Beslut som tas löpande under implementation (hotkey-fallback-tangent, Tauri fönster-konfig, etc.) noteras i implementationsplanen.
