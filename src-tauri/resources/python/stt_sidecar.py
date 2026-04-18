#!/usr/bin/env python3
"""SVoice 3 STT sidecar. Kör faster-whisper i long-living-process.
Rust pratar via stdin/stdout. En JSON per rad; Transcribe-request följs av
<audio_samples * 4 bytes> f32-le audio direkt efter JSON-raden.
"""
import json
import os
import site
import sys
import time
from pathlib import Path

import numpy as np

# Windows default-stdout är cp1252; tvinga UTF-8 utan CRLF-translation så att
# Rust's line-reader kan parsa svenska transkript.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
if hasattr(sys.stdin, "reconfigure"):
    sys.stdin.reconfigure(encoding="utf-8", newline="\n")


def _add_nvidia_dll_dirs():
    if not hasattr(os, "add_dll_directory"):
        return
    added = []
    for base in site.getsitepackages() + [site.getusersitepackages()]:
        nvidia_root = Path(base) / "nvidia"
        if not nvidia_root.exists():
            continue
        for sub in nvidia_root.iterdir():
            bin_dir = sub / "bin"
            if bin_dir.exists():
                try:
                    os.add_dll_directory(str(bin_dir))
                    added.append(str(bin_dir))
                except OSError:
                    pass
    # ctranslate2 är en C-extension; den letar cublas64_12.dll via Windows
    # conventional PATH, inte os.add_dll_directory. Injicera därför även i
    # PATH innan `faster_whisper` importeras.
    if added:
        os.environ["PATH"] = os.pathsep.join(added) + os.pathsep + os.environ.get("PATH", "")


_add_nvidia_dll_dirs()


def send(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def read_request():
    line = sys.stdin.readline()
    if not line:
        return None
    try:
        return json.loads(line)
    except json.JSONDecodeError as e:
        return {"type": "__decode_error__", "message": str(e)}


def read_audio(n_samples: int):
    data = sys.stdin.buffer.read(n_samples * 4)
    if len(data) != n_samples * 4:
        raise IOError(f"unexpected audio length: got {len(data)}, expected {n_samples * 4}")
    # faster-whisper kräver numpy-array (eller str/Path), inte tuple.
    return np.frombuffer(data, dtype="<f4").copy()


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
            if t == "__decode_error__":
                send({"type": "error", "message": f"json decode: {req.get('message')}", "recoverable": True})
                continue
            elif t == "load":
                if model is not None:
                    del model
                    model = None
                    import gc
                    gc.collect()
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
                if req.get("sample_rate", 16000) != 16000:
                    send({"type": "error",
                          "message": f"unsupported sample_rate: {req.get('sample_rate')} (sidecar expects 16000)",
                          "recoverable": False})
                    continue
                try:
                    audio = read_audio(req["audio_samples"])
                except (IOError, OSError) as e:
                    send({"type": "error", "message": f"audio read failed: {e}", "recoverable": False})
                    break
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
            elif t == "download_model":
                repo_id = req.get("model")
                if not repo_id:
                    send({"type": "error", "message": "download_model saknar 'model'", "recoverable": False})
                    continue
                try:
                    from huggingface_hub import snapshot_download
                    send({"type": "download_started", "model": repo_id})
                    t0 = time.perf_counter()
                    # allow_patterns trimmar bort README/exempel-filer som inte
                    # behövs för inferens → minst 10-20% mindre download per modell.
                    snapshot_download(
                        repo_id=repo_id,
                        allow_patterns=[
                            "*.bin",
                            "*.safetensors",
                            "*.pt",
                            "*.json",
                            "*.txt",
                            "*.model",
                            "tokenizer*",
                            "vocab*",
                        ],
                    )
                    elapsed_ms = int((time.perf_counter() - t0) * 1000)
                    send({"type": "downloaded", "model": repo_id, "elapsed_ms": elapsed_ms})
                except ImportError as e:
                    send({
                        "type": "error",
                        "message": f"huggingface_hub saknas i Python-runtime: {e}",
                        "recoverable": False,
                    })
                except Exception as e:
                    send({
                        "type": "error",
                        "message": f"download failed: {e}",
                        "recoverable": True,
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
