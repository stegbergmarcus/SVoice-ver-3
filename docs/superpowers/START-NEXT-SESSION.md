# Starta nästa session

## Första meddelandet att skicka till Claude

> Hej! Vi fortsätter på SVoice 3. Iter 1 är klar och merged till main (tag `iter1-complete`). Läs `plan.md` för övergripande vision, `docs/superpowers/specs/2026-04-16-stt-spike-report.md` för STT-val, `docs/superpowers/plans/2026-04-17-iter2-real-stt.md` för nästa iterations plan. Kör enligt iter 2-planen, börja med Fas A (Python-sidecar-protokoll). Använd `superpowers:subagent-driven-development` för task-exekvering.

## Snabb orientering för framtida-Claude

- **Hårdvara:** RTX 5080 (16 GB), CUDA 13.2-toolkit installerat. CUDA 12 DLLs finns i `C:\Users\marcu\AppData\Local\Programs\Python\Python311\Lib\site-packages\nvidia\*\bin\`.
- **Python:** `py -3.11` är target-versionen för sidecar. `faster-whisper` är installerat.
- **Smart App Control:** av på dev-maskinen. Builds fungerar fritt.
- **Distribution-scope:** användaren + vänner. Ingen EV-cert planerad. Vänner får antingen bundlad MSI (SAC måste vara av hos dem) eller klonar repot + kör `scripts/setup-dev.ps1` för att bygga själva.

## Verifierade fakta

- Höger Ctrl som PTT fungerar via LowLevelKeyboardHook (konsumerar eventet, så target ser inte Ctrl-tryck).
- Clipboard-paste injektion är primär, SendInput Unicode är fallback.
- Volume-overlay animeras live (fixat via `src-tauri/capabilities/default.json` + object-payload).
- GPU-inferens: 303 ms warm, 701 ms cold för 5 s audio med kb-whisper-medium fp16.
- Iter 1 har 13 unit-tester som alla passerar.

## Arbetsflöde

1. Starta med att läsa `docs/superpowers/plans/2026-04-17-iter2-real-stt.md` från början.
2. Checkout ny branch: `git checkout -b iter2/real-stt`.
3. Följ planen task-för-task. Commit per task enligt commit-message-formatet i planen.
4. När Fas G är klar: merge till main, tag `iter2-complete`.

## Bygga / köra

```powershell
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"
cargo tauri dev           # utveckling
cargo tauri build         # release MSI
cd src-tauri
cargo test --workspace    # alla tester
```

## Git-state vid senaste commit

- Branch: `main` vid `iter1-complete`
- Alla iter 1-fixes + volume-meter-fix + capabilities ingår
- Känd begränsning: inga som blockerar iter 2

## Lycka till!
