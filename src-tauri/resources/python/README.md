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
