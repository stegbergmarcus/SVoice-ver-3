# Distribution-readiness — designdokument

**Datum:** 2026-04-18
**Status:** Design godkänd, väntar på implementationsplan.
**Mål:** Göra SVoice 3 redo att delas med vänner som en fristående installer. Fyra
 oberoende förbättringar, körs som sekventiella faser i en gemensam plan
 eftersom de alla rör samma "post-Gemini"-paket och delar verifierings-
 infrastruktur.

---

## Kontext

Post-Gemini-implementationen (commit `8570a42`) ger en fungerande app, men
 tre friktioner stoppar oss från att dela med kompisar:

1. **MSI:n är 1,4 GB** eftersom KB-Whisper-modeller (~1,2 GB) bundlas med
   Tauri's resources. Enkel distribution (Drive/Dropbox/USB) blir klumpig.
2. **Inget sätt att upptäcka uppdateringar** — Marcus blir manuell update-
   kurir för varje kompis.
3. **Osäker autostart** — registry-entry kan peka på fel path efter
   reinstall, så appen startar inte alltid vid inloggning.

Därutöver finns en mindre UX-friktion: click-outside på action-popup stänger
 omedelbart, även mitt i streaming. Enkel fix med stort UX-värde.

---

## Feature 1 — Click-outside grace-period på action-popup

### Problem
`WindowEvent::Focused(false)` hide:ar popupen direkt (se
 [`src-tauri/src/lib.rs:490`](../../src-tauri/src/lib.rs)). Om user råkar
 klicka utanför popupen medan Gemini fortfarande streamar svaret försvinner
 både popupen och det halvfärdiga svaret.

### Design
Pausa click-outside-hide så länge popupen aktivt streamar eller just
 levererat ett nytt svar. Explicit close (Esc, "Applicera", "Avbryt",
 Insert-follow-up) påverkas inte.

**Mekanism:**
- Ny atomisk flagga `ACTION_POPUP_STREAMING: AtomicBool` i `lib.rs`.
- Sätts `true` när `action_llm_token` första gången emittas för en ny
  popup-session. Återställs `false` 500 ms efter `action_llm_done` (så
  user hinner reagera och klicka bort om de vill).
- I `WindowEvent::Focused(false)`-handlern: skippa hide om flaggan är
  `true`. Efter flaggan blir `false` fungerar click-outside som idag.
- Flaggan rensas också vid `action_apply`, `action_cancel` och när
  användaren trycker Insert för follow-up.

### Avgränsningar
- Ingen UI-indikator — grace-periodens beteende är implicit.
- Inget användarbart toggle. YAGNI.

### Verifiering
- Insert-PTT med lång query ("förklara kvantfysik i detalj") → klicka
  på skrivbordet mitt i streaming → popup stannar.
- Samma fråga → vänta tills klar → klicka utanför → popup stängs (efter
  500 ms grace).

---

## Feature 2 — Update-check mot GitHub Releases

### Problem
Ingen väg för user (eller kompis) att veta om en ny MSI finns. Marcus blir
 manuell distributör.

### Design

**Auto-check + manuell knapp** — båda använder samma backend.

**Backend (`crates/ipc/src/commands.rs` + ny modul `updates`):**
```rust
#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateStatus, String>;

pub struct UpdateStatus {
    pub current_version: String,       // env!("CARGO_PKG_VERSION")
    pub latest_version: Option<String>, // från GitHub release tag
    pub available: bool,                // latest > current (semver)
    pub download_url: Option<String>,   // första .msi-asset
    pub release_notes: Option<String>,  // markdown body (trunkerad)
    pub checked_at: i64,                // unix timestamp
}
```

- Hämtar `https://api.github.com/repos/stegbergmarcus/SVoice-ver-3/releases/latest`
  utan autentisering (public repo, 60 req/h rate limit per IP).
- Parse:ar `tag_name` (tar bort ev. `v`-prefix), jämför semver mot
  `CARGO_PKG_VERSION`. Crate: `semver` (lätt, redan i Cargo ekosystem).
- 404 / nätfel / "inga releases" → returnera `available: false` utan fel-
  toast (första kompisen som installerar har inga releases att jämföra mot).

**Auto-check:**
- Kör vid app-start, 10 sek efter setup är klar (så det inte stör första-
  start-UX).
- Caching: senaste check:en sparas i `%APPDATA%/svoice-v3/update-check.json`
  med timestamp. Ny auto-check bara om >24 h sedan senaste.
- Om `available: true`: tray-balloon-notification en gång per ny version.
  Användaren kan också se statusen i Settings.

**UI (Settings → Översikt-fliken, ny sektion "Version"):**
- Card med "Version 0.1.0" + status-rad:
  - ✓ "Du kör senaste versionen" (grön)
  - ⚠ "Ny version 0.2.0 tillgänglig" + knapp "Ladda ner" (öppnar
    `download_url` i default-browser)
  - ○ "Kunde inte kontrollera — prova igen" (om nätfel)
- **"Sök uppdateringar nu"-knapp** under statusraden. Kör check synkront
  och uppdaterar kortet.
- Release-notes visas som expanderbar "Visa detaljer"-sektion om tillgängliga.

### Avgränsningar
- **Ingen auto-install** — bara notifiera + öppna download-URL i browser.
  Tauri-updater är overkill och kräver signing-infrastruktur.
- **Ingen channel-väljare** (stable/beta). YAGNI.
- Rate-limits hanteras graciöst (ingen retry-storm).

### Verifiering
- Starta appen → 10 sek senare ska logg säga "update-check OK, no update".
- Manuell knapp → samma resultat.
- Skapa en dummy-release på GitHub med tag `v0.99.0` → starta appen →
  ska detektera ny version och visa kort/toast.

---

## Feature 3 — Fixa autostart-bugg

### Problem
Okänd i detalj. Misstänkt orsak:
 `tauri-plugin-autostart` skriver `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SVoice 3`
 med install-path vid `enable()`. Efter reinstall till ny path uppdateras
 inte registret om nyckeln redan finns med samma namn.

### Utredning (måste göras först)
1. Kolla registret: `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v "SVoice 3"`
2. Jämför värdet mot faktisk install-path (`C:\Program Files\SVoice 3\svoice-v3.exe`).
3. Granska `svoice_ipc::commands::sync_autostart` + `tauri-plugin-autostart`-
   beteende vid `enable()`: skriver den över eller hoppar den om entry
   finns?

### Hypotetisk design (bekräftas efter utredning)

**Om autostart-plugin skriver idempotent:** problem ligger i att registret
 behåller en dangling reference när Windows sparar pathen i en "legacy"
 format. Fix: expandera `sync_autostart` till att alltid skriva rätt path
 vid app-start, oavsett plugin-rapport.

**Om plugin inte skriver över:** skriv registret manuellt med
 `winreg`-crate. Force:a rätt värde vid varje `sync_autostart`-anrop
 (idempotent: skriv bara om värdet skiljer sig). Plugin fortfarande
 används för `is_enabled`-query.

### Avgränsningar
- Design fastställs efter utredning. Spec noterar bara att fixen ska:
  - Vara idempotent (no-op om registret redan är rätt).
  - Köras vid varje app-start (`sync_autostart` i `setup`) — inte bara när
    user ändrar toggle.
  - Logga orsak till eventuell ändring på `info`-nivå.

### Verifiering
1. Slå på autostart i Settings → spara → reboot → SVoice ska starta i tray.
2. Avinstallera + installera om från ny path → autostart-toggle fortfarande på
   → reboot → SVoice ska starta från den nya path:en.
3. Om bugg inte kan reproduceras: lämna nuvarande kod som den är, logga
   diagnostikläget och stäng feature som "ej reproducerbar".

---

## Feature 4 — Lazy-download av KB-Whisper

### Problem
Bundling av alla KB-Whisper-snapshots gör MSI:n 1,4 GB. De flesta kompisar
 behöver bara en variant (Large), och en del kanske inte ens använder
 STT (bara Gemini/Claude för action-popup).

### Design

**1. Ta bort modellerna från Tauri-bundle.**
- `tauri.conf.json` slutar inkludera `KBLab/kb-whisper-*`-snapshots.
- Python-runtime bundlas fortfarande (150 MB — behövs för att ens köra
  sidecaret utan systemwide Python).
- Förväntad MSI: ~200-250 MB.

**2. Ingen auto-download av standardmodellen.**
Vid första app-start visar Settings → Ljud & STT att ingen modell är
 cachad. User måste explicit klicka **"Ladda ner"** bredvid en modell i
 dropdownen (matchar befintligt Ollama-mönster).

**3. Dropdown-rendering.**
Nuvarande `MODELS`-lista i `Settings.tsx` har redan VRAM-notis
 (`"snabbast · ~1 GB VRAM"` etc). Utöka den per användarens önskemål:
- Lägg till **rekommenderat VRAM** som tydlig kolumn i field-help:en,
  inte bara i dropdown-raderna. Blir synligt bredvid "Lokal modell"-
  dropdownen, stil: "💡 Minst rekommenderat VRAM: Base 1 GB / Medium 4 GB
  / Large 6 GB. CPU-fallback funkar men är 5-10× långsammare."
- Visa också **diskstorlek** (hur mycket download:en är). Base ~150 MB,
  Medium ~1,5 GB, Large ~3 GB.

**4. Download-flöde.**
Återanvänd Ollama-mönstret:
- Ny IPC: `download_stt_model(model: String)` som spawnar Python-sidecar
  i download-only-mode (eller använder `huggingface_hub.snapshot_download`
  direkt).
- Progress-events `stt_model_download_progress` och `stt_model_download_done`
  till frontend.
- Progressbar i Settings under modell-dropdownen, precis som Ollama-pull.
- OS-notifikation vid klar.

**5. Edge-cases.**
- Nätverksfel under download: tydligt error-toast, delad download
  stannar i HF-cache-folder (kan fortsätta nästa gång — HF-lib hanterar
  resume).
- User ändrar modell medan download pågår: avbryt nuvarande, starta ny.
  Cancellation-token via atomisk flagga.
- STT-start innan modell cachad: emit "modell ej nedladdad, ladda ner
  först"-fel + blink på Settings-knappen.

### Avgränsningar
- **Python-runtime bundlas kvar.** Att separera även Python innebär
  att alla kompisar måste installera systemwide Python — för stor
  UX-regression.
- **KB-Whisper är endast nedladdningsbart.** Ingen eskortering till
  andra Whisper-varianter ([faster-whisper](https://github.com/SYSTRAN/faster-whisper)
  multilingual etc). YAGNI — kan läggas till senare.
- **Groq STT** (cloud) är redan lazy och opåverkad.

### Verifiering
1. Bygg MSI → verifiera storlek ≤ 300 MB.
2. Avinstallera → installera MSI → öppna Settings → Ljud & STT → modell-
   dropdown visar alla med ↓-prefix (ej cachad).
3. Klicka "Ladda ner" på Large → progressbar → klar → prefix blir ✓.
4. Höger-Ctrl-PTT → STT fungerar med nedladdad modell.
5. Försök höger-Ctrl-PTT UTAN nedladdad modell → tydligt fel, ingen
   krasch, Settings-knappen blinkar.

---

## Ordning

1. **Feature 1** (grace-period) — snabb, inga externa beroenden.
2. **Feature 2** (update-check) — fristående, kan testas mot dummy-release.
3. **Feature 3** (autostart) — utredning + fix, risk för blackbox-debug.
4. **Feature 4** (lazy-download) — störst scope, rör Python-sidecar +
   Tauri-bundling + UI. Påverkar MSI-storlek, så den sista MSI-builden
   ska inkludera denna.

Varje feature är sin egen fas i implementationsplanen. Alla faser har
 egna commits så vi kan rulla tillbaka utan att ta med de andra.

## Icke-mål

- Ingen telemetri/crash-reporting. Privacy-first-load stannar.
- Ingen auto-update-installation (bara notify + öppna download).
- Inget CI/CD för att bygga MSI:er på push — detta är fortfarande en
  "Marcus bygger och taggar manuellt"-process.
- Ingen signering av MSI:n (Windows SmartScreen kommer att varna
  kompisar — fix i senare roadmap).
- Ingen refaktor av `Settings.tsx`. Sparas till när filen faktiskt
  blir ohanterlig.

## Återanvänd

- Ollama-download-mönster: `pull_ollama_model`, `ollama_pull_progress`,
  `ollama_pull_done`. Mirra för STT-download.
- `check_hf_cached` finns redan — använd för dropdown-rendering.
- `sync_autostart` finns redan — utöka för #3.
- `tauri-plugin-notification` finns — använd för update + download-klar.
