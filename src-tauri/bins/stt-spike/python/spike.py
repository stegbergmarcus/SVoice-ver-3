#!/usr/bin/env python3
"""
STT-spike för SVoice 3 iter 1.

Kör faster-whisper på en WAV-fil, mäter cold/warm inference-latens och VRAM,
och dokumenterar om modellen kan laddas på RTX 5080 + CUDA 13.2.

Används pga Windows Smart App Control blockerar Rust build-scripts för
ct2rs/whisper-rs på SAC-aktiva dev-maskiner. Python (signerad av PSF) kör
problemfritt.

Usage:
    python spike.py <wav-path> [--model <repo_or_path>] [--device cuda|cpu]

Exempel:
    python spike.py ../testdata/sv-test.wav
    python spike.py ../testdata/sv-test.wav --model KBLab/kb-whisper-medium
"""

import argparse
import json
import os
import site
import sys
import time
from pathlib import Path


def _add_nvidia_dll_dirs():
    """Python 3.8+ kräver os.add_dll_directory() för att ladda tredjeparts-DLLs.
    faster-whisper's ct2 runtime söker cublas64_12.dll. Om nvidia-cublas-cu12
    är installerat via pip, ligger DLLs i site-packages/nvidia/*/bin — vi
    måste registrera dem som DLL-sökbara kataloger före import."""
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


def main():
    parser = argparse.ArgumentParser(description="SVoice STT spike")
    parser.add_argument("wav", type=Path, help="WAV-fil att transkribera")
    parser.add_argument(
        "--model",
        default="KBLab/kb-whisper-medium",
        help="HF repo eller lokal path (default: KBLab/kb-whisper-medium)",
    )
    parser.add_argument(
        "--device",
        default="cuda",
        choices=["cuda", "cpu"],
        help="compute device (default: cuda)",
    )
    parser.add_argument(
        "--compute-type",
        default="float16",
        help="float16 | int8_float16 | int8 (default: float16)",
    )
    parser.add_argument(
        "--language",
        default="sv",
        help="språk-kod (default: sv)",
    )
    parser.add_argument(
        "--beam-size",
        type=int,
        default=3,
        help="beam size (default: 3)",
    )
    args = parser.parse_args()

    if not args.wav.exists():
        print(f"FEL: WAV finns inte: {args.wav}", file=sys.stderr)
        sys.exit(2)

    print(f">> Importar faster-whisper...")
    t0 = time.perf_counter()
    from faster_whisper import WhisperModel  # type: ignore
    import_ms = int((time.perf_counter() - t0) * 1000)
    print(f"  import tog {import_ms} ms")

    vram_before = sample_vram_mb()
    if vram_before is not None:
        print(f"VRAM före load: {vram_before} MB")

    print(f">> Laddar modell '{args.model}' ({args.device}, {args.compute_type})...")
    t0 = time.perf_counter()
    try:
        model = WhisperModel(
            args.model,
            device=args.device,
            compute_type=args.compute_type,
        )
    except Exception as e:
        print(f"FEL: kunde inte ladda modellen: {e}", file=sys.stderr)
        sys.exit(3)
    load_ms = int((time.perf_counter() - t0) * 1000)
    print(f"  load tog {load_ms} ms")

    vram_after = sample_vram_mb()
    if vram_before is not None and vram_after is not None:
        print(f"VRAM efter load: {vram_after} MB (delta +{vram_after - vram_before} MB)")

    # Cold inference
    print(">> Cold inference...")
    t0 = time.perf_counter()
    segments, info = model.transcribe(
        str(args.wav), language=args.language, beam_size=args.beam_size
    )
    cold_text = " ".join(s.text for s in segments).strip()
    cold_ms = int((time.perf_counter() - t0) * 1000)
    print(f"  cold: \"{cold_text}\" ({cold_ms} ms)")

    # Warm inference
    print(">> Warm inference...")
    t0 = time.perf_counter()
    segments, _ = model.transcribe(
        str(args.wav), language=args.language, beam_size=args.beam_size
    )
    warm_text = " ".join(s.text for s in segments).strip()
    warm_ms = int((time.perf_counter() - t0) * 1000)
    print(f"  warm: \"{warm_text}\" ({warm_ms} ms)")

    # Korrekthetscheck
    expected_path = args.wav.parent / "sv-test.expected.txt"
    expected = expected_path.read_text(encoding="utf-8").strip() if expected_path.exists() else None

    print("\n=== Sammanfattning ===")
    result = {
        "model": args.model,
        "device": args.device,
        "compute_type": args.compute_type,
        "wav": str(args.wav),
        "wav_duration_s": wav_duration(args.wav),
        "language": info.language if info else args.language,
        "language_probability": info.language_probability if info else None,
        "import_ms": import_ms,
        "load_ms": load_ms,
        "cold_inference_ms": cold_ms,
        "warm_inference_ms": warm_ms,
        "vram_used_before_mb": vram_before,
        "vram_used_after_mb": vram_after,
        "vram_delta_mb": (vram_after - vram_before) if (vram_before and vram_after) else None,
        "transcript_cold": cold_text,
        "transcript_warm": warm_text,
        "expected": expected,
        "has_swedish_chars": all(c in cold_text for c in "åäö"),
    }
    print(json.dumps(result, indent=2, ensure_ascii=False))


def sample_vram_mb():
    """Returnera used VRAM i MB via nvidia-smi. None om misslyckas."""
    try:
        import subprocess
        out = subprocess.check_output(
            ["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
            text=True,
            timeout=5,
        )
        return int(out.strip().split("\n")[0])
    except Exception:
        return None


def wav_duration(path: Path) -> float:
    """Returnera längd i sekunder (approximativt via header)."""
    try:
        with open(path, "rb") as f:
            header = f.read(44)
        channels = int.from_bytes(header[22:24], "little")
        rate = int.from_bytes(header[24:28], "little")
        bits = int.from_bytes(header[34:36], "little")
        data_size = int.from_bytes(header[40:44], "little")
        bytes_per_sample = (bits // 8) * channels
        if bytes_per_sample == 0 or rate == 0:
            return 0.0
        return data_size / (rate * bytes_per_sample)
    except Exception:
        return 0.0


if __name__ == "__main__":
    main()
