# SVoice 3 — Smart App Control Mitigation Plan

**Datum:** 2026-04-16
**Status:** Planerande dokument. Uppdateras med empiriska resultat från release-build-test.

## Problem

Windows 11 (Home/Pro) kör **Smart App Control (SAC)** som en säkerhetsmekanism som blockerar osignerade körbara filer. Under iter 1 STT-spike stötte vi på SAC-blockering av Rust build-scripts (`os error 4551 — programkontrollprincip har blockerat den här filen`). Detta ledde till att vi bytte från `ct2rs` till Python-subprocess-väg.

Frågan är nu: **kommer SAC blockera appen också för slutanvändare?**

## Riskmatris för produktionsdistribution

| Distribution-väg | SAC-beteende | Rekommendation |
|---|---|---|
| Osignerad MSI från vår webbsajt | Blockerad på SAC-on-maskiner (ca 40-60% av Windows 11-installationer 2026) | ❌ Otillräckligt |
| MSI signerad med billig OV-cert | SmartScreen-varning initialt. Får "reputation" över tid. SAC kan fortfarande blockera i "Enforced" mode. | ⚠️ Delvis — kräver uppbyggd reputation |
| MSI signerad med **EV-cert** | Godkänd direkt av SmartScreen + SAC för kända utgivare | ✅ Rekommenderat för webbsdistribution |
| Microsoft Store-distribution | Signerad automatiskt av MS, SAC-trust inbakat | ✅ Enklast, men MS Store-policy för native apps |
| Bundle direct (sysadmin + AD GPO) | Miljövis hanterat via IT-policy | ✅ För enterprise-kunder |

## Tre lager vi måste klara

1. **Installer-lagret (MSI).** MSI måste vara signerad så Windows SmartScreen tillåter körning utan varning.
2. **App-binärer.** `SVoice 3.exe` + alla `.dll`-beroenden måste vara signerade eller ligga i app-katalog som är trusted via installern.
3. **Python-subprocess-sidecar.** Python-tolken är PSF-signerad (trust-bar). Men faster-whisper/CTranslate2 Python-wheels lägger egna `.dll`-filer (`libctranslate2.dll`, CUDA-deps). Dessa är normalt signerade av sina utgivare, men kan SAC-blockeras om de ligger i user-writeable %APPDATA%.

## Strategi — hantering per lager

### Installer (MSI)

- **Signera MSI med EV-cert.** Kostnad ~$400-600/år hos DigiCert, Sectigo eller liknande. Kräver företagsverifiering (4-6 veckor första gången). En gång och gjort.
- Om EV är för dyrt för v1: OV-cert (~$100/år) räcker för att få SmartScreen-varning att bli "känd utgivare" efter några nedladdningar. Men på aggressiva SAC-maskiner blockeras även OV-signerade appar ibland.
- Signering sker via `signtool.exe` (Windows SDK) i CI efter `cargo tauri build`.

### App-binärer

- Tauri 2 bundler genererar MSI som kopierar alla .dll:er till `C:\Program Files\SVoice 3\`. Filer i Program Files är trusted av Windows — SAC blockerar normalt inte DLL-load därifrån, även om individuell DLL inte är signerad.
- **Sanity-krav:** `cargo tauri build` måste kunna köra på dev-maskinen. Om SAC blockerar build själv, behöver dev-maskinen Defender-exceptions (redan i place) eller SAC avstängt på byggmaskin/CI.
- **CI-rekommendation:** bygg i GitHub Actions Windows-runner (inga SAC-problem där), signera med vår EV-cert via secrets, släpp signerade artifacts.

### Python-subprocess-sidecar (kritiskt)

Här är det svåraste lagret. Vi ska bundla:
1. Embeddable Python (t.ex. `python-3.11-embed-amd64.zip` från python.org — **signerad** av PSF).
2. faster-whisper Python-paket + beroenden (i site-packages i bundlen).
3. CUDA 12 runtime DLLs (`cublas64_12.dll`, `cudart64_12.dll`, `cudnn64_9.dll` etc.) — **signerade av NVIDIA**.

**Viktigt:** alla dessa filer måste läggas i `C:\Program Files\SVoice 3\python\` av den signerade installern. Då är de "trusted via installern" och SAC lastar dem utan att varna.

Om vi däremot laddar ner Python runtime vid första start (som vissa appar gör) och placerar i `%LOCALAPPDATA%`, blir de user-writeable → SAC kan blockera. **Skippa den designen.**

### GPU-detektion vid start

Appen ska detektera vid start:
1. Finns NVIDIA GPU? (`nvml-wrapper` eller kolla `nvidia-smi`)
2. Kan CUDA 12-libs laddas? (test-load `cublas64_12.dll` från vår bundlade path)
3. Om ja: kör faster-whisper med `device=cuda, compute_type=float16`
4. Om nej: fall tillbaka till `device=cpu, compute_type=int8`

Det gör att appen fungerar på alla Windows 11-maskiner:
- NVIDIA GPU + vårt bundlade CUDA 12 → GPU-speed
- NVIDIA GPU som råkar ha cuBLAS 12 i system-PATH → GPU-speed
- CPU-only / Intel GPU / AMD GPU → CPU-fallback (långsammare men funkar)

## Utvecklings-ergonomi

Användarens dev-maskin har SAC **aktiv medvetet** för att utveckla under samma villkor som slutanvändare. Det fungerar:

- `cargo tauri dev` bygger ny `svoice-v3.exe` varje gång → Defender-exception för projekt-mappen räcker för Windows Defender, men SAC kan fortfarande rycka till.
- **Observerat:** vid `cargo build` av komplexa deps (`ct2rs`, `num-traits`) blockerade SAC enskilda build-scripts. Påverkar inte Tauri-dev-flödet (vi byggde `svoice-v3` tusen gånger utan SAC-problem).
- **Om SAC börjar blockera dev-flödet mer aggressivt:** överväga en tillfällig byggmaskin i WSL2 eller VM där SAC inte gäller, eller ge upp och slå av SAC på dev-maskinen (men den möjligheten vill användaren undvika).

## Empiriska test som återstår

- [ ] `cargo tauri build --debug` på dev-maskinen → fungerar det? Eller blockerar SAC?
- [ ] Installera resulterande MSI → SAC-varning eller tyst installation?
- [ ] Kör installerad app → fungerar hotkey, injection, overlay som i dev?
- [ ] Prova kopiera MSI:n till en annan Windows 11-maskin med SAC-on → samma test

Dessa tester utförs **direkt efter detta dokument skrivs** så vi har konkret data.

## Beslutpunkter för iter 2

1. **Certifieringsbeslut:** EV eller OV? Påverkar timeline (EV tar 4-6 veckor att få; OV några dagar).
2. **Distributionskanal:** Direktnedladdning från vår sajt? Microsoft Store-parallell? Båda?
3. **Signerings-pipeline:** GitHub Actions med cert i secrets, eller lokal build-maskin med USB-token?
4. **Bundle-storlek:** inkludera CUDA 12-deps by default (~500 MB MSI) eller erbjuda "slim" (CPU-only) + "full" (GPU) installer-varianter?

Dessa beslut tas vid iter 2-start. För tillfället fortsätter vi med iter 1 avslut + walking skeleton verifieringstest som baseline.

## Nästa steg

1. Kör `cargo tauri build --debug` nu och logga utfallet.
2. Om det blockeras av SAC: dokumentera hur vi ska lösa för dev-maskinen (permanent exclusion? WSL-build?).
3. Om det går igenom: installera MSI:n, verifiera att walking skeleton fungerar installerat.
4. Uppdatera detta dokument med resultaten.
