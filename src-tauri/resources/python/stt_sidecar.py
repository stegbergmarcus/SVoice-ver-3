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
