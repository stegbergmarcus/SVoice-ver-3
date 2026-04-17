# SVoice 3 Iter 2 Implementation Plan — Riktig STT

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ersätt dummy-STT med faktisk svensk tal-till-text via Python-subprocess (kb-whisper-medium). Levererar end-to-end: håll höger Ctrl → prata → släpp → transkriptet injiceras i fokuserat fönster.

**Architecture:** Python-sidecar som körs som long-living subprocess, kommunicerar via stdin (audio bytes) + stdout (JSON transcript). WASAPI audio capture via `cpal` i ringbuffer, VAD trimmer tystnad i början/slutet, audio skickas till sidecar vid key-up. Sidecar unloads modellen efter 2 min idle.

**Tech Stack:** Befintligt (Rust 1.95, Tauri 2, React/TS/Vite, cpal, windows-rs) + `ringbuf` crate för audio-ringbuffer, Python 3.11 embeddable, `faster-whisper`, bundlade CUDA 12 DLLs från `nvidia-*-cu12`-pip-paketen.

---

## Kontext — var iter 1 slutade

- **Branch:** `main` vid tag `iter1-complete` (commit hash: se `git log iter1-complete`).
- **Walking skeleton fungerar:** höger Ctrl → `dummy_transcribe()` returnerar hårdkodad testtext → clipboard-paste-injektion. Verifierat i Notepad, browser, Teams.
- **Volume-meter overlay fungerar:** mic RMS animeras live under PTT (grön/gul/röd gradient). Tauri capabilities konfigurerade i `src-tauri/capabilities/default.json`.
- **STT-spike bevisad:** kb-whisper-medium via faster-whisper på RTX 5080 + CUDA 13.2 levererar **303 ms warm, 701 ms cold** för 5 s audio, **1870 MB VRAM** för fp16. CPU-fallback fungerar (2.6 s). Kräver CUDA 12 DLLs på PATH — se `docs/superpowers/specs/2026-04-16-stt-spike-report.md`.
- **Distribution:** privat användning + vänner. SAC-stängd på dev-maskinen. Se `docs/superpowers/specs/2026-04-16-sac-mitigation-plan.md`.

### Nyckelfiler att känna till

| Fil | Roll |
|---|---|
| `plan.md` | Högnivå-vision (4 faser, iter 2 = del av fas 1-avslut + hela fas 2-STT) |
| `src-tauri/crates/hotkey/src/ll_hook.rs` | LowLevelKeyboardHook för höger Ctrl — konsumerar eventet (`return LRESULT(1)`) så target inte ser Ctrl nedtryckt |
| `src-tauri/crates/hotkey/src/ptt_state.rs` | 3-state machine (Idle → Recording → Processing → Idle) med unit-tests |
| `src-tauri/crates/audio/src/volume.rs` | Befintlig cpal-wrapper för volym-mätning; iter 2 utvidgar den till full audio-capture |
| `src-tauri/crates/inject/src/` | Clipboard-paste primär, SendInput Unicode-fallback |
| `src-tauri/crates/stt/src/lib.rs` | Stub med `dummy_transcribe()` — iter 2 ersätter med Python-subprocess-wrapper |
| `src-tauri/src/lib.rs` | Tauri builder + ptt_worker_loop som kopplar ihop allt |
| `src-tauri/bins/stt-spike/python/spike.py` | Referens-Python för faster-whisper-anrop — gör om till långsam sidecar i iter 2 |

### Bygga + köra

```powershell
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"
cargo tauri dev   # kör
cargo tauri build # release-build + MSI
```

Alla tester: `cd src-tauri && cargo test --workspace` (13 tester passerar i iter 1-baseline).

---

## Iter 2-scope

### In-scope

1. Python-sidecar-infrastruktur (spawn, stdin/stdout protokoll, lazy start, idle-unload)
2. WASAPI audio capture utbyggd från `volume.rs` till full ringbuffer
3. VAD (Silero via ONNX-runtime, eller WebRTC som enklare fallback)
4. STT-pipeline som ersätter `dummy_transcribe()`
5. CUDA 12 DLL + embeddable Python bundling i resursmappen
6. GPU-detektion vid start med automatisk CPU-fallback
7. Minimal settings-UI: välj mic-enhet, modellstorlek, compute (auto/CPU/GPU)
8. Timeout + error-hantering när STT tar för länge eller failar

### Out-of-scope (iter 3+)

- LLM-polering av transkript
- Smart functions / command palette
- Google-integration
- Streaming partials i overlay under inspelning
- Konfigurerbar hotkey (höger Ctrl är hårdkodad i iter 2)
- Model Center med nedladdnings-UI (modeller cachas av faster-whisper automatiskt via HF)

---

## Filstruktur som skapas i iter 2

### Nya Rust-filer

| Fil | Syfte |
|---|---|
| `src-tauri/crates/audio/src/capture.rs` | Full WASAPI input-capture med persistent stream + ringbuffer |
| `src-tauri/crates/audio/src/ringbuffer.rs` | Lock-free ringbuffer wrapper runt `ringbuf`-crate, PTT-aware |
| `src-tauri/crates/audio/src/resample.rs` | Linjär resample till 16 kHz mono (matchar Whisper-input) |
| `src-tauri/crates/audio/src/vad.rs` | VAD-wrapper; v1 använder enkel energi-tröskel, v2 Silero |
| `src-tauri/crates/stt/src/sidecar.rs` | Spawn + drive Python-subprocess via stdin/stdout |
| `src-tauri/crates/stt/src/protocol.rs` | JSON-protokoll mellan Rust ↔ Python (request/response-typer) |
| `src-tauri/crates/stt/src/engine.rs` | `trait Stt` + `PythonStt`-impl som orkestrerar sidecar-anrop |
| `src-tauri/crates/settings/src/lib.rs` | Load/save settings till `%APPDATA%/svoice-v3/settings.json` |
| `src-tauri/resources/python/stt_sidecar.py` | Långsam sidecar-implementation (baserad på spike.py) |
| `src-tauri/resources/python/README.md` | Hur Python-runtimen bundlas |
| `scripts/bundle-python.ps1` | Hämtar Python embeddable + faster-whisper + CUDA 12 DLLs till `src-tauri/resources/python-runtime/` |
| `scripts/verify-cuda-load.ps1` | Test-script som verifierar att CUDA 12 DLLs laddas |

### Nya React-filer

| Fil | Syfte |
|---|---|
| `src/windows/Settings.tsx` | Minimal settings-view (mic-val, modell, compute-läge) |
| `src/lib/settings-api.ts` | Wrapper runt Tauri invoke för settings |
| `src/components/TranscriptPreview.tsx` | Visar senaste transkript i overlay (valfritt) |

### Modifierade filer

- `src-tauri/src/lib.rs` — ersätt `dummy_transcribe()`-anropet med riktig STT-anrop
- `src-tauri/crates/stt/src/lib.rs` — ersätt stub med module-deklarationer
- `src-tauri/crates/audio/src/lib.rs` — re-export capture + ringbuffer + VAD
- `src-tauri/tauri.conf.json` — extra resources + ev. permissions
- `src-tauri/capabilities/default.json` — lägg till FS-permissions för settings + Python-resources

---

## Faser

1. **Fas A:** Python-sidecar-protokoll (Rust ↔ Python stdin/stdout JSON-loop)
2. **Fas B:** Audio capture + ringbuffer + resample
3. **Fas C:** VAD (enkel energi-tröskel först, Silero senare)
4. **Fas D:** Wire-up: ersätt `dummy_transcribe()` med riktig STT
5. **Fas E:** Python-runtime-bundling + CUDA 12 DLL-distribution
6. **Fas F:** Settings-UI (minimal)
7. **Fas G:** Exit verification + release-test

---

# Fas A — Python-sidecar-protokoll

## Task A1: Designa JSON-protokoll mellan Rust och Python

**Files:**
- Create: `src-tauri/crates/stt/src/protocol.rs`

- [ ] **Step 1:** Skapa filen med request/response-typer:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttRequest {
    /// Be sidecar att ladda modell. Skickas en gång vid första STT-användning.
    Load { model: String, device: String, compute_type: String, language: String },
    /// Skicka audio för transkription. Audio följer som raw f32 little-endian på stdin
    /// direkt efter JSON-raden, `audio_samples` många floats.
    Transcribe { audio_samples: u32, sample_rate: u32, beam_size: u32 },
    /// Stäng ner sidecar gracefully.
    Shutdown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttResponse {
    Ready,
    Loaded { load_ms: u64, vram_used_mb: Option<u64> },
    Transcript { text: String, inference_ms: u64, language: String, confidence: f32 },
    Error { message: String, recoverable: bool },
}
```

- [ ] **Step 2:** Uppdatera `crates/stt/src/lib.rs` så modulen exporteras:

```rust
pub mod engine;
pub mod protocol;
pub mod sidecar;

pub use engine::{PythonStt, Stt, SttError};
pub use protocol::{SttRequest, SttResponse};
```

(`engine` + `sidecar` skapas i kommande tasks.)

- [ ] **Step 3:** Uppdatera `crates/stt/Cargo.toml`:

```toml
[dependencies]
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
```

- [ ] **Step 4:** `cargo build -p svoice-stt` — verifiera kompilation. Commit: `feat(stt): JSON protocol for Python sidecar`.

## Task A2: Implementera Python-sidan av protokollet

**Files:**
- Create: `src-tauri/resources/python/stt_sidecar.py`
- Create: `src-tauri/resources/python/README.md`

- [ ] **Step 1:** Skapa `stt_sidecar.py` som läser JSON-requester från stdin, binär audio efter `Transcribe`-requests, svarar JSON på stdout:

```python
#!/usr/bin/env python3
"""SVoice 3 STT sidecar. Kör faster-whisper i long-living-process.
Rust pratar via stdin/stdout. En JSON per rad; Transcribe-request följs av
<audio_samples * 4 bytes> f32-le audio direkt efter JSON-raden.
"""
import json
import os
import site
import struct
import sys
import time
from pathlib import Path


def _add_nvidia_dll_dirs():
    if not hasattr(os, "add_dll_directory"):
        return
    for base in site.getsitepackages() + [site.getusersitepackages()]:
        nvidia_root = Path(base) / "nvidia"
        if not nvidia_root.exists():
            continue
        for sub in nvidia_root.iterdir():
            bin_dir = sub / "bin"
            if bin_dir.exists():
                try:
                    os.add_dll_directory(str(bin_dir))
                except OSError:
                    pass


_add_nvidia_dll_dirs()


def send(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def read_request():
    line = sys.stdin.readline()
    if not line:
        return None
    return json.loads(line)


def read_audio(n_samples: int):
    data = sys.stdin.buffer.read(n_samples * 4)
    if len(data) != n_samples * 4:
        raise IOError(f"unexpected audio length: got {len(data)}, expected {n_samples * 4}")
    return struct.unpack(f"<{n_samples}f", data)


def main():
    send({"type": "ready"})
    model = None
    language = "sv"
    beam_size = 3

    while True:
        req = read_request()
        if req is None:
            break
        t = req.get("type")
        try:
            if t == "load":
                from faster_whisper import WhisperModel
                t0 = time.perf_counter()
                model = WhisperModel(
                    req["model"],
                    device=req["device"],
                    compute_type=req["compute_type"],
                )
                load_ms = int((time.perf_counter() - t0) * 1000)
                language = req.get("language", "sv")
                vram = _query_vram_mb()
                send({"type": "loaded", "load_ms": load_ms, "vram_used_mb": vram})
            elif t == "transcribe":
                if model is None:
                    send({"type": "error", "message": "model not loaded", "recoverable": True})
                    continue
                audio = read_audio(req["audio_samples"])
                beam_size = req.get("beam_size", beam_size)
                t0 = time.perf_counter()
                segments, info = model.transcribe(
                    audio, language=language, beam_size=beam_size
                )
                text = " ".join(s.text for s in segments).strip()
                infer_ms = int((time.perf_counter() - t0) * 1000)
                send({
                    "type": "transcript",
                    "text": text,
                    "inference_ms": infer_ms,
                    "language": info.language if info else language,
                    "confidence": float(info.language_probability or 0.0),
                })
            elif t == "shutdown":
                break
            else:
                send({"type": "error", "message": f"unknown request: {t}", "recoverable": False})
        except Exception as e:
            send({"type": "error", "message": str(e), "recoverable": True})


def _query_vram_mb():
    try:
        import subprocess
        out = subprocess.check_output(
            ["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
            text=True, timeout=3,
        )
        return int(out.strip().split("\n")[0])
    except Exception:
        return None


if __name__ == "__main__":
    main()
```

- [ ] **Step 2:** Skapa `README.md`:

```markdown
# SVoice 3 Python STT sidecar

Long-living-subprocess som tar emot JSON-requests från Rust och kör faster-whisper.

## Protokoll

Rust ↔ Python kommunicerar via stdin/stdout.

Varje meddelande från Rust till Python är en JSON-rad (`\n`-terminerad). `transcribe`
följs direkt av raw f32-little-endian-audio, `audio_samples * 4` bytes.

Python svarar alltid med en JSON-rad. Sekvens:
1. Sidecar startar → `{"type":"ready"}`
2. Rust skickar `{"type":"load",...}` → Python svarar `{"type":"loaded","load_ms":N}`
3. Rust skickar `{"type":"transcribe",...} + audio bytes` → Python svarar `{"type":"transcript",...}`
4. Rust skickar `{"type":"shutdown"}` → Python avslutar

Se `src-tauri/crates/stt/src/protocol.rs` för Rust-sidans typer.

## Kör manuellt för test

```bash
py -3.11 src-tauri/resources/python/stt_sidecar.py
# (förvänta {"type":"ready"} på stdout; skicka JSON-rader på stdin)
```
```

- [ ] **Step 3:** Manuellt test av sidecar ifrån kommandoraden för att verifiera att `ready`-eventet kommer och att `load`-request kan skickas. Commit: `feat(stt): Python sidecar implementation`.

## Task A3: Rust-sidans sidecar-driver

**Files:**
- Create: `src-tauri/crates/stt/src/sidecar.rs`

- [ ] **Step 1:** Implementera sidecar med `tokio::process::Command`:

```rust
use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::protocol::{SttRequest, SttResponse};

#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("kunde inte spawna Python-sidecar: {0}")]
    Spawn(String),
    #[error("sidecar stängde oväntat")]
    Closed,
    #[error("protokoll-fel: {0}")]
    Protocol(String),
    #[error("IO-fel: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-fel: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct Sidecar {
    child: Child,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
}

impl Sidecar {
    pub async fn spawn(python_path: &PathBuf, script_path: &PathBuf) -> Result<Self, SidecarError> {
        let mut child = Command::new(python_path)
            .arg(script_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| SidecarError::Spawn(e.to_string()))?;

        let stdin = child.stdin.take().ok_or(SidecarError::Closed)?;
        let stdout = BufReader::new(child.stdout.take().ok_or(SidecarError::Closed)?);
        let this = Self {
            child,
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(stdout),
        };

        // Vänta på ready-svar
        match this.read_response().await? {
            SttResponse::Ready => Ok(this),
            other => Err(SidecarError::Protocol(format!("förväntade Ready, fick {other:?}"))),
        }
    }

    pub async fn send_request(&self, req: &SttRequest) -> Result<(), SidecarError> {
        let mut stdin = self.stdin.lock().await;
        let line = serde_json::to_string(req)? + "\n";
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn send_audio(&self, samples: &[f32]) -> Result<(), SidecarError> {
        let mut stdin = self.stdin.lock().await;
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        stdin.write_all(&bytes).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn read_response(&self) -> Result<SttResponse, SidecarError> {
        let mut stdout = self.stdout.lock().await;
        let mut line = String::new();
        let n = stdout.read_line(&mut line).await?;
        if n == 0 {
            return Err(SidecarError::Closed);
        }
        let resp: SttResponse = serde_json::from_str(line.trim())?;
        Ok(resp)
    }

    pub async fn shutdown(mut self) -> Result<(), SidecarError> {
        let _ = self.send_request(&SttRequest::Shutdown).await;
        let _ = self.child.wait().await;
        Ok(())
    }
}
```

- [ ] **Step 2:** Commit: `feat(stt): async sidecar driver with spawn + request/response`.

## Task A4: Integration-test av sidecar-protokollet

**Files:**
- Create: `src-tauri/crates/stt/tests/sidecar_test.rs`

- [ ] **Step 1:** Skriv ett integration-test som spawnar sidecar och verifierar `ready`:

```rust
use std::path::PathBuf;

use svoice_stt::{Sidecar, SttRequest, SttResponse};

#[tokio::test]
#[ignore] // kräver systempython — kör manuellt med `cargo test -p svoice-stt -- --ignored`
async fn sidecar_responds_ready_on_spawn() {
    let python = PathBuf::from("py");
    let script = PathBuf::from("../../resources/python/stt_sidecar.py");
    let sidecar = Sidecar::spawn(&python, &script).await.expect("spawn");
    // Om vi nådde hit utan panic har vi fått Ready.
    sidecar.shutdown().await.expect("shutdown");
}
```

- [ ] **Step 2:** Kör `cargo test -p svoice-stt -- --ignored`. Commit: `test(stt): sidecar spawn integration test`.

---

# Fas B — Audio capture + ringbuffer

## Task B1: Ringbuffer-wrapper

**Files:**
- Create: `src-tauri/crates/audio/src/ringbuffer.rs`
- Modify: `src-tauri/crates/audio/Cargo.toml`

- [ ] **Step 1:** Lägg till `ringbuf = "0.4"` i audio-crate Cargo.toml.

- [ ] **Step 2:** Implementera thread-safe ringbuffer för f32-samples:

```rust
use ringbuf::{traits::*, HeapRb};
use std::sync::Arc;

pub struct AudioRing {
    producer: Arc<std::sync::Mutex<ringbuf::HeapProd<f32>>>,
    consumer: Arc<std::sync::Mutex<ringbuf::HeapCons<f32>>>,
}

impl AudioRing {
    /// `capacity` i sekunder vid 16 kHz (t.ex. 30s → 480 000 samples).
    pub fn new(capacity_samples: usize) -> Self {
        let rb = HeapRb::<f32>::new(capacity_samples);
        let (producer, consumer) = rb.split();
        Self {
            producer: Arc::new(std::sync::Mutex::new(producer)),
            consumer: Arc::new(std::sync::Mutex::new(consumer)),
        }
    }

    /// Skriv samples från audio-callback-tråd. Returnerar antal faktiskt skrivna
    /// (om buffer är full skrivs färre).
    pub fn push_samples(&self, samples: &[f32]) -> usize {
        let mut p = self.producer.lock().unwrap();
        p.push_slice(samples)
    }

    /// Läs ut alla tillgängliga samples (drainar buffer).
    pub fn drain(&self) -> Vec<f32> {
        let mut c = self.consumer.lock().unwrap();
        let len = c.occupied_len();
        let mut out = vec![0.0; len];
        c.pop_slice(&mut out);
        out
    }

    /// Klär buffer utan att returnera innehållet.
    pub fn clear(&self) {
        let mut c = self.consumer.lock().unwrap();
        while c.try_pop().is_some() {}
    }

    pub fn len(&self) -> usize {
        self.consumer.lock().unwrap().occupied_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_drain_roundtrip() {
        let ring = AudioRing::new(100);
        let written = ring.push_samples(&[0.1, 0.2, 0.3]);
        assert_eq!(written, 3);
        let drained = ring.drain();
        assert_eq!(drained, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn overflow_discards_excess() {
        let ring = AudioRing::new(3);
        let written = ring.push_samples(&[0.1, 0.2, 0.3, 0.4, 0.5]);
        assert_eq!(written, 3);
    }
}
```

- [ ] **Step 3:** `cargo test -p svoice-audio`. Commit: `feat(audio): AudioRing thread-safe ringbuffer`.

## Task B2: Resampler

**Files:**
- Create: `src-tauri/crates/audio/src/resample.rs`

- [ ] **Step 1:** Enkel linjär resample till 16 kHz mono (baserat på spike-koden):

```rust
pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz {
        return input.to_vec();
    }
    let ratio = from_hz as f32 / to_hz as f32;
    let out_len = ((input.len() as f32) / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_idx = i as f32 * ratio;
        let i0 = src_idx.floor() as usize;
        let i1 = (i0 + 1).min(input.len().saturating_sub(1));
        let frac = src_idx - i0 as f32;
        out.push(input[i0] * (1.0 - frac) + input[i1] * frac);
    }
    out
}

/// Om input är stereo/multi-channel, mixa ner till mono genom genomsnitt.
pub fn mix_to_mono(input: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return input.to_vec();
    }
    let ch = channels as usize;
    input
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_identity_when_same_rate() {
        let s = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_linear(&s, 16000, 16000), s);
    }

    #[test]
    fn mix_stereo_to_mono_averages() {
        let stereo = vec![1.0, 3.0, 2.0, 4.0]; // L R L R
        assert_eq!(mix_to_mono(&stereo, 2), vec![2.0, 3.0]);
    }
}
```

- [ ] **Step 2:** `cargo test -p svoice-audio`. Commit: `feat(audio): linear resampler + mono downmix`.

## Task B3: Bygg ut `capture.rs` med persistent stream

**Files:**
- Create: `src-tauri/crates/audio/src/capture.rs`
- Modify: `src-tauri/crates/audio/src/lib.rs`

- [ ] **Step 1:** Skapa `capture.rs` baserat på `volume.rs`-mönstret men med ringbuffer:

```rust
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::resample::{mix_to_mono, resample_linear};
use crate::ringbuffer::AudioRing;

pub struct AudioCapture {
    _stream: cpal::Stream,
    pub ring: Arc<AudioRing>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("ingen input-enhet")]
    NoDevice,
    #[error("oförväntat sample format: {0:?}")]
    UnsupportedFormat(cpal::SampleFormat),
    #[error("cpal-fel: {0}")]
    Cpal(String),
}

impl AudioCapture {
    /// Skapar stream som kontinuerligt pushar INTO ringbufferen. Stream stängs
    /// när AudioCapture drop:s.
    pub fn start(ring: Arc<AudioRing>) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(CaptureError::NoDevice)?;
        let config = device
            .default_input_config()
            .map_err(|e| CaptureError::Cpal(e.to_string()))?;

        let sample_format = config.sample_format();
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let stream_cfg = config.into();
        let err_cb = |err| tracing::error!("audio capture error: {err}");

        let ring_cb = ring.clone();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &stream_cfg,
                    move |data: &[f32], _| {
                        let mono = mix_to_mono(data, channels);
                        let resampled = resample_linear(&mono, sample_rate, 16000);
                        ring_cb.push_samples(&resampled);
                    },
                    err_cb,
                    None,
                )
                .map_err(|e| CaptureError::Cpal(e.to_string()))?,
            cpal::SampleFormat::I16 => {
                let norm = i16::MAX as f32;
                device
                    .build_input_stream(
                        &stream_cfg,
                        move |data: &[i16], _| {
                            let f: Vec<f32> = data.iter().map(|&s| s as f32 / norm).collect();
                            let mono = mix_to_mono(&f, channels);
                            let resampled = resample_linear(&mono, sample_rate, 16000);
                            ring_cb.push_samples(&resampled);
                        },
                        err_cb,
                        None,
                    )
                    .map_err(|e| CaptureError::Cpal(e.to_string()))?
            }
            other => return Err(CaptureError::UnsupportedFormat(other)),
        };
        stream.play().map_err(|e| CaptureError::Cpal(e.to_string()))?;

        Ok(Self { _stream: stream, ring, sample_rate, channels })
    }
}
```

- [ ] **Step 2:** Uppdatera `lib.rs`:

```rust
pub mod capture;
pub mod resample;
pub mod ringbuffer;
pub mod volume;

pub use capture::{AudioCapture, CaptureError};
pub use ringbuffer::AudioRing;
pub use volume::{VolumeMeter, VolumeMeterError};
```

- [ ] **Step 3:** `cargo build -p svoice-audio && cargo test -p svoice-audio`. Commit: `feat(audio): persistent capture pipeline with 16kHz mono resample`.

---

# Fas C — VAD

## Task C1: Enkel energi-VAD som v1

**Files:**
- Create: `src-tauri/crates/audio/src/vad.rs`

- [ ] **Step 1:** Implementera enkel VAD som trimmar tystnad i början/slutet:

```rust
/// Hittar index för första och sista "icke-tysta" samplen baserat på RMS över 20ms-fönster.
/// Returnerar (start, end) i samples. Om allt är tyst returneras (0, 0).
pub fn trim_silence(samples: &[f32], sample_rate: u32, energy_threshold: f32) -> (usize, usize) {
    let window = (sample_rate as usize / 50).max(1); // 20ms
    let mut first = None;
    let mut last = 0;
    for (i, chunk) in samples.chunks(window).enumerate() {
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        if rms > energy_threshold {
            if first.is_none() {
                first = Some(i * window);
            }
            last = i * window + chunk.len();
        }
    }
    (first.unwrap_or(0), last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_silence_returns_zero_range() {
        let samples = vec![0.0; 16000];
        assert_eq!(trim_silence(&samples, 16000, 0.01), (0, 0));
    }

    #[test]
    fn trims_leading_and_trailing_silence() {
        let mut samples = vec![0.0; 16000];
        for i in 4000..8000 {
            samples[i] = 0.5;
        }
        let (start, end) = trim_silence(&samples, 16000, 0.01);
        assert!(start <= 4000 && start >= 3000);
        assert!(end >= 8000 && end <= 9000);
    }
}
```

- [ ] **Step 2:** `cargo test -p svoice-audio`. Commit: `feat(audio): simple energy-based VAD (v1)`.

## Task C2: (Valfritt) Silero VAD via ONNX

Skjut till iter 3 eller senare om v1 räcker. Silero kräver ONNX runtime + modellnedladdning.

---

# Fas D — Wire-up: ersätt dummy-STT

## Task D1: STT-engine-abstraktion

**Files:**
- Create: `src-tauri/crates/stt/src/engine.rs`

- [ ] **Step 1:** Skapa trait + Python-impl:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::protocol::{SttRequest, SttResponse};
use crate::sidecar::{Sidecar, SidecarError};

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error("modell ej laddad")]
    NotLoaded,
    #[error("sidecar svarade med fel: {0}")]
    Remote(String),
    #[error("oväntat svar: {0}")]
    Unexpected(String),
}

#[derive(Debug, Clone)]
pub struct SttConfig {
    pub model: String,
    pub device: String,
    pub compute_type: String,
    pub language: String,
    pub beam_size: u32,
    pub python_path: PathBuf,
    pub script_path: PathBuf,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            model: "KBLab/kb-whisper-medium".into(),
            device: "cuda".into(),
            compute_type: "float16".into(),
            language: "sv".into(),
            beam_size: 3,
            python_path: PathBuf::from("py"),
            script_path: PathBuf::from("src-tauri/resources/python/stt_sidecar.py"),
        }
    }
}

pub struct PythonStt {
    sidecar: Arc<Mutex<Option<Sidecar>>>,
    config: SttConfig,
}

impl PythonStt {
    pub fn new(config: SttConfig) -> Self {
        Self { sidecar: Arc::new(Mutex::new(None)), config }
    }

    async fn ensure_loaded(&self) -> Result<(), SttError> {
        let mut guard = self.sidecar.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let sc = Sidecar::spawn(&self.config.python_path, &self.config.script_path).await?;
        sc.send_request(&SttRequest::Load {
            model: self.config.model.clone(),
            device: self.config.device.clone(),
            compute_type: self.config.compute_type.clone(),
            language: self.config.language.clone(),
        })
        .await?;
        match sc.read_response().await? {
            SttResponse::Loaded { load_ms, vram_used_mb } => {
                tracing::info!(
                    "STT-modell laddad på {} ms (VRAM: {:?} MB)",
                    load_ms, vram_used_mb
                );
            }
            SttResponse::Error { message, .. } => return Err(SttError::Remote(message)),
            other => return Err(SttError::Unexpected(format!("{other:?}"))),
        }
        *guard = Some(sc);
        Ok(())
    }

    pub async fn transcribe(&self, audio: &[f32]) -> Result<String, SttError> {
        self.ensure_loaded().await?;
        let guard = self.sidecar.lock().await;
        let sc = guard.as_ref().ok_or(SttError::NotLoaded)?;
        sc.send_request(&SttRequest::Transcribe {
            audio_samples: audio.len() as u32,
            sample_rate: 16000,
            beam_size: self.config.beam_size,
        })
        .await?;
        sc.send_audio(audio).await?;
        match sc.read_response().await? {
            SttResponse::Transcript { text, inference_ms, .. } => {
                tracing::info!("STT: {} ms → \"{}\"", inference_ms, text);
                Ok(text)
            }
            SttResponse::Error { message, .. } => Err(SttError::Remote(message)),
            other => Err(SttError::Unexpected(format!("{other:?}"))),
        }
    }
}
```

- [ ] **Step 2:** `cargo build -p svoice-stt`. Commit: `feat(stt): PythonStt engine with lazy-spawn and model caching`.

## Task D2: Koppla in i ptt_worker_loop

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml` (säkerställ tokio runtime)

- [ ] **Step 1:** Ersätt `dummy_transcribe()`-anropet i `perform_inject` med riktig STT. Worker måste ha tokio-runtime eftersom STT är async.

Konceptuell patch:
- Lägg en `AudioCapture` + `AudioRing` som starts vid app-start (persistent stream, lågt overhead).
- Vid `Pressed`: rensa ringbuffer (så vi bara fångar NY audio).
- Vid `Released`: drain ringbuffer, applicera VAD, skicka till `PythonStt::transcribe`, inject resultatet.

```rust
use svoice_audio::{AudioCapture, AudioRing};
use svoice_audio::vad::trim_silence;
use svoice_stt::{PythonStt, SttConfig};

// I setup-closure:
let ring = Arc::new(AudioRing::new(16000 * 30)); // 30s buffer
let _capture = AudioCapture::start(ring.clone())?;
let stt = Arc::new(PythonStt::new(SttConfig::default()));
let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

// I worker:
// Pressed:
ring.clear();

// Released → Processing:
let audio = ring.drain();
let (start, end) = trim_silence(&audio, 16000, 0.005);
let segment = &audio[start..end];
if segment.is_empty() {
    tracing::warn!("inget tal detekterat (allt under VAD-tröskel)");
    // skip inject
} else {
    match rt.block_on(stt.transcribe(segment)) {
        Ok(text) => {
            match inject(&text) { ... }
        }
        Err(e) => tracing::error!("STT-fel: {e}"),
    }
}
```

- [ ] **Step 2:** Bygg. Fixa ev. beroende-fel. Commit: `feat(lib): real STT pipeline replaces dummy_transcribe`.

## Task D3: Manuell end-to-end-verifiering

- [ ] **Step 1:** Kör `cargo tauri dev`.
- [ ] **Step 2:** Öppna Notepad. Håll höger Ctrl, säg "Hej, det här är ett test", släpp.
- [ ] **Step 3:** Verifiera att transkriptet skrivs i Notepad inom ~1-2 s.
- [ ] **Step 4:** Om OK: commit som "iter2-stt-working"-tag och fortsätt till Fas E. Annars: felsök (sidecar-logg, audio-length, VAD-tröskel).

---

# Fas E — Python-runtime-bundling

## Task E1: Script som laddar ner embeddable Python + CUDA DLLs

**Files:**
- Create: `scripts/bundle-python.ps1`

- [ ] **Step 1:** Scripta nedladdning av Python embeddable + pip-paket + CUDA DLLs till `src-tauri/resources/python-runtime/`:

```powershell
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$runtime = Join-Path $root "src-tauri/resources/python-runtime"

New-Item -ItemType Directory -Force -Path $runtime | Out-Null

# 1. Embeddable Python
$embed = "python-3.11.9-embed-amd64.zip"
$url = "https://www.python.org/ftp/python/3.11.9/$embed"
Invoke-WebRequest $url -OutFile "$runtime/$embed"
Expand-Archive "$runtime/$embed" -DestinationPath "$runtime/python" -Force
Remove-Item "$runtime/$embed"

# 2. pip bootstrap (embeddable saknar pip som standard)
Invoke-WebRequest https://bootstrap.pypa.io/get-pip.py -OutFile "$runtime/get-pip.py"
& "$runtime/python/python.exe" "$runtime/get-pip.py"
Remove-Item "$runtime/get-pip.py"

# 3. Installera faster-whisper + CUDA 12 runtime
& "$runtime/python/python.exe" -m pip install faster-whisper nvidia-cublas-cu12 nvidia-cudnn-cu12 nvidia-cuda-runtime-cu12 nvidia-cuda-nvrtc-cu12 hf_xet

Write-Host "Python-runtime bundlad till $runtime"
Write-Host "Storlek: $((Get-ChildItem $runtime -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB) MB"
```

- [ ] **Step 2:** Kör scriptet. Verifiera storlek (<600 MB). Commit: `chore: bundle-python script`.

## Task E2: Tauri-konfig för Python-resursen

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1:** Lägg till `"resources"` i `bundle`-sektionen så Python-runtime följer med i MSI:

```json
"bundle": {
  "resources": {
    "resources/python-runtime": "python-runtime",
    "resources/python/stt_sidecar.py": "python/stt_sidecar.py"
  },
  ...
}
```

- [ ] **Step 2:** Uppdatera `SttConfig::default()` att använda bundlat python via `tauri::path::resource_dir()` i runtime.

- [ ] **Step 3:** `cargo tauri build` → verifiera att MSI-storlek inkluderar Python (+500 MB). Commit: `feat(bundle): ship Python runtime + CUDA DLLs`.

## Task E3: PATH-setup för CUDA 12 DLLs

**Files:**
- Modify: `src-tauri/crates/stt/src/sidecar.rs`

- [ ] **Step 1:** Sätt `PATH` i child-process env vid spawn så CUDA 12 DLLs hittas:

```rust
let python_dir = python_path.parent().unwrap();
let cuda_bins: Vec<String> = glob_cuda_bin_dirs(python_dir);
let path_add = cuda_bins.join(";");
let current_path = std::env::var("PATH").unwrap_or_default();

let mut child = Command::new(python_path)
    .env("PATH", format!("{path_add};{current_path}"))
    .arg(script_path)
    // ...
```

där `glob_cuda_bin_dirs` letar upp `site-packages/nvidia/*/bin/`-mappar.

- [ ] **Step 2:** Testa att GPU-inferens fungerar utan system-PATH-tweak. Commit: `feat(stt): set CUDA PATH in sidecar env`.

---

# Fas F — Settings-UI (minimal)

## Task F1: Settings-crate skeleton

**Files:**
- Modify: `src-tauri/crates/settings/src/lib.rs`

- [ ] **Step 1:** Enkel JSON-persistens:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub mic_device: Option<String>,
    pub stt_model: String,
    pub stt_compute_mode: ComputeMode, // auto | cpu | gpu
    pub vad_threshold: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mic_device: None,
            stt_model: "KBLab/kb-whisper-medium".into(),
            stt_compute_mode: ComputeMode::Auto,
            vad_threshold: 0.005,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComputeMode { Auto, Cpu, Gpu }

impl Settings {
    pub fn path() -> PathBuf {
        let appdata = std::env::var("APPDATA").expect("APPDATA");
        PathBuf::from(appdata).join("svoice-v3").join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }
}
```

- [ ] **Step 2:** Tester + commit: `feat(settings): JSON-backed settings with defaults`.

## Task F2: Tauri-kommandon för settings

**Files:**
- Modify: `src-tauri/crates/ipc/src/commands.rs`

- [ ] **Step 1:** Lägg till `get_settings` och `set_settings` som `#[tauri::command]`. Anropas från React.

- [ ] **Step 2:** Commit: `feat(ipc): settings commands for frontend`.

## Task F3: Settings-view i React

**Files:**
- Create: `src/windows/Settings.tsx`
- Modify: `src/windows/Main.tsx` (routing eller "Settings"-knapp)

- [ ] **Step 1:** Enkel vy med dropdowns för mic, modell, compute-mode. Använder Tauri invoke för get/set.

- [ ] **Step 2:** Commit: `feat(ui): minimal settings window`.

---

# Fas G — Exit verification

## Task G1: End-to-end-test i dev

- [ ] **Step 1:** Kör `cargo tauri dev` från ren build.
- [ ] **Step 2:** Fyll i testprotokoll (se `docs/superpowers/specs/2026-04-16-iter1-walking-skeleton-verification.md` för mall — skapa iter 2-variant):
  - (a) Håll höger Ctrl i Notepad, säg kort mening → transkript injiceras inom 1.5 s
  - (b) Lång mening (15 s) → streaming ej i scope, men full transkript kommer när key-up
  - (c) Inget tal (tystnad) → ingen injection, warning i logg
  - (d) Smoke-test i Edge URL-bar (clipboard-fallback-path)
  - (e) Växla mic i Settings → ny mic används direkt utan restart
  - (f) Sätt compute-mode = cpu → bekräfta långsammare men fungerande
  - (g) Sätt compute-mode = gpu och CUDA 12 DLLs saknas → ska fall tillbaka till CPU med warning

## Task G2: Release-build-verifiering

- [ ] **Step 1:** `cargo tauri build` → MSI i `target/release/bundle/msi/`.
- [ ] **Step 2:** Installera MSI (SAC är av). Starta från Start-menyn.
- [ ] **Step 3:** Verifiera alla (a)-(g) från G1 fungerar i installerad app.
- [ ] **Step 4:** Verifiera MSI-storlek (~500-800 MB med bundlad Python + CUDA).

## Task G3: Merge + tag

- [ ] **Step 1:** Merge branch → main. Tag `iter2-complete`.
- [ ] **Step 2:** Uppdatera README.md med ny status. Commit: `docs: update README for iter 2 completion`.

---

## Exit-criteria iter 2

- [x] Riktig svensk STT med kb-whisper-medium
- [x] End-to-end på RTX 5080 <1.5 s för kort mening (mål från plan.md)
- [x] CPU-fallback fungerar
- [x] Python-runtime bundlad i MSI
- [x] Settings-UI för mic + modell + compute-mode
- [x] Alla tester gröna, warnings 0
- [x] Installerbar MSI verifierad på dev-maskinen

## Framtid (iter 3+)

- LLM-polering (Ollama-sidecar + API-providers)
- Smart functions / command palette
- Google-integration
- Silero VAD (ersätter energi-VAD)
- Streaming partials
- Konfigurerbar hotkey
- Model Center med nedladdnings-UI
- EV-signerad distribution
