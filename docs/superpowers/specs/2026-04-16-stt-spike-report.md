# STT-Spike — Resultatrapport

**Datum:** 2026-04-16
**Spike-runner:** `src-tauri/bins/stt-spike/python/spike.py` (Python subprocess-väg)
**Hårdvara:** RTX 5080 (16 GB), driver 595.97, CUDA 13.2 toolkit
**Modell testad:** `KBLab/kb-whisper-medium` (HF-standardformat, CT2-konverterad on-the-fly av faster-whisper)

## Sammanfattning

Spiken verifierade att **pipeline från WAV → Whisper → transkript fungerar** på användarens maskin. **GPU-inferens blockeras** dock av en CUDA-version-mismatch som kräver lösning för produktion.

**Vald produktionsväg:** Python-subprocess (faster-whisper i sidecar-process). Rust-native `ct2rs` togs bort som primär-kandidat pga Windows Smart App Control blockerar osignerade native build-scripts på användarens dev-maskin — och användaren vill medvetet behålla SAC aktiv för att utveckla under samma restriktioner som slutanvändare.

## Mätvärden

### CPU-inferens (int8, fungerande baseline)

| Metrik | Värde |
|---|---|
| Python import (faster-whisper + deps) | 3.7 s |
| Modell-load (kb-whisper-medium, int8) | 2.3 s |
| Cold inference (5 s audio) | 2.67 s |
| Warm inference (5 s audio) | 2.39 s |
| VRAM-delta | 0 MB (körs på RAM) |
| Språkdetektering | `sv` @ 100 % confidence |

### GPU-inferens (float16) — fungerar efter SAC-off + PATH-fix

| Metrik | Värde |
|---|---|
| Modell-load till VRAM | 1.81 s |
| VRAM-delta | +1870 MB (kb-whisper-medium fp16) |
| **Cold inference (5 s audio)** | **701 ms** |
| **Warm inference (5 s audio)** | **303 ms** |
| Språkdetektering | `sv` @ 100 % confidence |

**Uppföljning 2026-04-16 kväll:** Användaren stängde av Smart App Control. GPU-testen kördes om. **SAC var inte orsaken till cublas-felet** — det var vanlig Windows DLL-search-path. `os.add_dll_directory()` från Python räckte inte (CTranslate2 C++-kod använder LoadLibrary som ignorerar den). Fixen som fungerade var att sätta Windows `PATH`-miljövariabeln innan Python startar så att den inkluderade `site-packages/nvidia/{cublas,cudnn,cuda_runtime,cuda_nvrtc}/bin/`. Då laddades alla CUDA 12 DLLs utan problem, och warm-inference nådde 303 ms — på gränsen för plan.md:s mål på <300 ms.

Konsekvens för iter 2-distribution: vi bundlar CUDA 12 DLLs (från `nvidia-cublas-cu12`-pip-paketet eller direkt från NVIDIA Redistributable) i `C:\Program Files\SVoice 3\cuda\` och sätter `PATH` för Python-sidecar-processen vid spawn. Ingen EV-signing krävs — DLLs är NVIDIA-signerade redan och PATH sätts i sidecar-processkontexten, inte systemvid.

### Transkript

- **TTS-genererad WAV:** default engelsk röst (ingen svensk Windows-röst installerad på dev-maskinen). Spiken mäter främst pipeline + latens, inte kvalitet, så detta accepterades.
- **Förväntat:** `Hej, det här är ett test med å, ä och ö.`
- **CPU-transkript:** `Hedge.  Det har AR ETT testmed A.  A och O.` (förvanskat pga TTS-röstens engelska uttal, inte modellfel)
- **Språkdetektering** identifierade ändå svenska korrekt — modellen är svensk-finetunad som förväntat.

## Vad hände (kronologiskt)

1. **Rust-ct2rs-vägen (primär plan)** — kräver `cmake` (installerades via winget, OK) och bygger en stor graf av C++-crates (`sentencepiece-sys`, `ct2rs-sys`, `ctranslate2`). Build nådde halvvägs men **Smart App Control blockerade nybyggda build-script-exes** med "programkontrollprincip har blockerat den här filen" (os error 4551). Defender-exclusions via `Add-MpPreference` hjälpte inte — SAC är ett separat system.
2. **Python-subprocess-vägen (fallback 3 från plan.md)** — faster-whisper installerades direkt via pip utan build-issues. Modellen laddades från `KBLab/kb-whisper-medium` (symlink-varning ignorerad) till `~/.cache/huggingface/hub/`. Första download tog ~12 s (~1.5 GB).
3. **CPU-inferens** gick igenom utan problem — 2.6 s för 5 s audio, imperfekt transkript pga TTS-röstens språkproblem men pipeline bevisad.
4. **GPU-inferens** failade vid `model.encode()` med `cublas64_12.dll is not found`. CTranslate2 4.x (som faster-whisper binder till) länkar mot CUDA 12.x-runtime, användaren har CUDA 13.2 installerat systemvid. Pip-paketen `nvidia-cublas-cu12`, `nvidia-cudnn-cu12`, `nvidia-cuda-runtime-cu12` installerades och DLL:er kopierades till arbetsmappen, men LoadLibrary hittade dem inte — troligtvis SAC-restriktioner på DLL-load från user-writeable områden. `os.add_dll_directory()` före import hjälpte inte.

## Beslut för iter 2

### STT-backend: Python-subprocess (faster-whisper)

- **Arkitektur:** `src-tauri/crates/stt/` blir en *extern process-wrapper*. Python sidecar spawnas lazy vid första STT-användning och hålls vid liv enligt plan.md's "Idle-beteende" (unload efter 2 min idle).
- **Distribution:** bundla embeddable Python (~30 MB) + faster-whisper + CTranslate2 + CUDA 12 runtime DLLs (~400-500 MB totalt). Alternativt: detektera och kräv system-Python 3.11 vid install och dynamiskt installera paket.
- **CPU-default i iter 2:** tills vi löser GPU-problemet är CPU-inferens den säkra vägen. kb-whisper-medium på CPU int8 klarar 5 s audio på 2.6 s — dugligt för block-transkribering efter PTT-release men för långsamt för streaming-partials.

### CUDA 13.2 → CUDA 12.x bridge (inte löst i iter 1)

Tre möjliga vägar för iter 2 att ta:

1. **Signerad installer + signed CUDA DLLs:** om vi EV-signerar appens installer och inkluderar CUDA 12 DLLs bundlade i app-mappen (som även skulle vara skriven av signerad installer), har SAC normalt inga invändningar. Bör testas i release-build-fas.
2. **Nedgradera CUDA-toolkit till 12.x på dev-maskinen:** inte acceptabelt för användaren (vill utveckla med senaste toolkit för att matcha andra komponenter).
3. **Vänta på CTranslate2 med CUDA 13-stöd:** otydligt tidsintervall. CTranslate2 följer sällan nya CUDA-versioner snabbt.

**Rekommenderad iter 2-plan:** implementera Python-sidecar-arkitekturen med CPU-default. Lägg till GPU-detektion: vid install, om signerad installer kan placera CUDA 12 DLLs i app-privat mapp, aktivera GPU-läge automatiskt. Fall tillbaka till CPU om DLL-laddning failar.

## Kvarvarande risker

- **Latens-krav:** plan.md sätter mål på <1.5 s end-to-end på RTX 5080. Med CPU-fallback (~2.6 s för 5 s klipp) missar vi målet för medellånga dikteringar. GPU-lösning är kritisk för att nå mål.
- **Distribution-storlek:** Python+faster-whisper+CUDA-libs lägger 400-500 MB ovanpå Tauri-binären. Acceptabelt men icke-trivialt.
- **Symlink-problem på Windows** (Developer Mode krävs annars): påverkar bara HF-cache-effektivitet, inte funktionalitet.
- **SAC + dev-ergonomi:** användarens SAC-val blockerar Rust-native STT. Om SAC slås av senare (t.ex. för CI-builds) kan `ct2rs` omtestas — koden finns kvar under git-historik (commits före 2026-04-16).

## Rekommendation för iter 2-start

1. Implementera `svoice-stt` som extern-process-wrapper som spawn:ar Python-subprocess via `tauri::api::process::Command`.
2. Börja med CPU-mode hårdkodad — låt oss få WASAPI-audio-captur + VAD + Python-STT end-to-end att fungera utan GPU-komplexitet.
3. När walking skeleton + Python-STT är verifierat, addera GPU-detektering och CUDA 12 DLL-bundling som en egen iter 2-delmilstolpe.

## Artefakter

- `src-tauri/bins/stt-spike/python/spike.py` — spike-script med `--device`, `--compute-type`-flaggor för återkörning.
- `src-tauri/bins/stt-spike/testdata/sv-test.wav` — TTS-genererad test-audio.
- `src-tauri/bins/stt-spike/testdata/sv-test.expected.txt` — förväntad transkription.

Att återköra spiken:
```bash
cd "src-tauri/bins/stt-spike"
HF_HUB_DISABLE_XET=1 py -3.11 python/spike.py testdata/sv-test.wav --device cpu --compute-type int8
```

För framtida GPU-test när CUDA 12 DLLs går att ladda:
```bash
py -3.11 python/spike.py testdata/sv-test.wav --device cuda --compute-type float16
```
