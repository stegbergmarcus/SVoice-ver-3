# SVoice 3

Röststyrd produktivitetssvit för Windows. Diktera var som helst, ställ frågor
och transformera markerad text via AI, hantera kalender och mail via röst —
allt med två tangenter.

---

## ⚠️ Begränsad distribution

**Den här appen delas för närvarande endast personligt med inbjudna
användare.** Sprid inte installer-länken vidare utan att först kontakta
Marcus. Koden publiceras för granskning, inte för vidarespridning eller
kommersiellt bruk. Se `LICENSE` för detaljer.

---

## Installation (Windows 10/11, x64)

1. Ladda ner `SVoice 3_0.1.0_x64_en-US.msi` från den privata release-länken
   du fått från Marcus.
2. Dubbelklicka MSI-filen.

### Windows SmartScreen-varning

Eftersom MSI:n är **osignerad** (inget $200+/år code-signing-certifikat i
den här fasen) visar Windows SmartScreen en varning:

> **Windows protected your PC**
> Microsoft Defender SmartScreen prevented an unrecognized app from starting.

**Detta är ofarligt.** Installern är byggd direkt från
[denna repo](https://github.com/stegbergmarcus/SVoice-ver-3) och innehåller
ingen malware — du kan granska all källkod själv. Så här fortsätter du:

1. Klicka **"More info"** (lite text längst ner i varningen).
2. Klicka **"Run anyway"**.
3. Windows UAC frågar om admin-rättigheter → **"Ja"** (MSI-installer kräver
   det för att skriva till `C:\Program Files\`).

Efter installation startar SVoice automatiskt i system-tray (SV-monogram).
Vänsterklicka på tray-ikonen för att öppna Settings.

---

## Första uppsättning

Öppna appen och gå till **Settings → Hjälp-fliken**. Där finns fullständig
setup-guide:

- Välj mikrofon + STT-modell (KB-Whisper Base laddas automatiskt ~150 MB
  vid första start)
- Skaffa API-nyckel för din valda AI-provider (Claude, Gemini, Groq eller
  lokal Ollama)
- (Valfritt) Koppla Google-konto för kalender + mail via röst
- Tangenterna: höger Ctrl = diktering, Insert = AI-popup, Ctrl+Shift+Space
  = smart-function-palette

---

## Vad appen gör

### Diktering (höger Ctrl)
Håll höger Ctrl i valfri Windows-app, prata på svenska, släpp. Texten
injiceras där markören står.

### Action-popup (Insert)
Håll Insert, ge ett röstkommando, släpp. En popup öppnas med AI-svar:

- **Utan markerad text** = Q&A-läge ("Vad är vädret i Stockholm just nu?",
  "Boka möte imorgon 14", "Sök mail från Anna")
- **Med markerad text** = transformation ("Gör detta mer formellt",
  "Översätt till engelska", "Rätta grammatiken")

Enter eller klick på "Applicera" klistrar in svaret där du var.

### AI-providers
- **Anthropic Claude** — tyngst resonemang, bäst för agentic tool-use
- **Google Gemini** — inbyggd Google Search-grounding, function-calling
  för kalender/mail, gratis-tier räcker för vardag
- **Groq** — snabb + billig, bra för grammatikpolering
- **Ollama (lokalt)** — privat/offline, ingen molnkoppling
- **Auto** — lokal först, cloud-fallback

### Google-integration
Efter anslutning via Settings → Integrationer:
- Lista/skapa kalenderhändelser
- Söka/läsa/skapa mail (ingen auto-send — bara utkast)

---

## Kända begränsningar

- **Windows-only.** macOS + Linux stöds inte.
- **Osignerad MSI.** Windows SmartScreen varnar (se ovan).
- **Gemini preview-modeller** (3 Pro, 3 Flash, 3.1 Pro) har mycket snäva
  gratis-kvoter (ofta 5-25 RPD). Stabila `gemini-2.5-flash` har rymligare
  kvot. Vid 429-fel: byt modell eller vänta tills midnatt PT (~09:00 SE).
- **STT första-start tar 30-60 sek** medan Base-modellen laddas ner från
  Hugging Face. Tydlig tray-notifikation visar status.
- **Ollama kräver separat installation** ([ollama.com](https://ollama.com))
  om du vill köra lokalt.
- **Mail skickas aldrig automatiskt** — AI:n skapar bara utkast, du måste
  klicka Skicka själv i Gmail.
- **Sidecar-lock under STT-modell-download**: om du manuellt laddar ner
  en större modell (Medium/Large) samtidigt som du försöker diktera,
  blockeras diktering tills download är klar.
- **Gemini-kvot-gräns** slås hårt på preview-modeller. Kolla din usage på
  [ai.dev/rate-limit](https://ai.dev/rate-limit).
- **Inget cloud-sync** av settings. Varje installation är lokal.

---

## Dataskydd

- **Audio stannar i RAM** — skrivs aldrig till disk.
- **API-nycklar** lagras i Windows Credential Manager, krypterade per
  Windows-konto.
- **Ingen telemetri.** Appen pratar bara med de providers du själv valt
  (Anthropic/Google/Groq) + Hugging Face (för modell-download).
- **Lokal STT** (KB-Whisper) skickar inget ljud till nätet. Om du byter
  STT-provider till Groq Whisper skickas komprimerad audio dit via HTTPS.

---

## Uppdateringar

Appen kollar automatiskt GitHub Releases en gång per dygn för nya versioner
och visar en tray-notifikation. Settings → Översikt → Version-kortet har
också en manuell "Sök uppdateringar"-knapp.

Ny version = ny MSI. Ladda ner från release-länken, avinstallera gamla via
Inställningar → Appar, installera den nya. Settings + API-nycklar bevaras.

---

## Utveckling (för source-kod-granskning)

Om du vill kompilera själv från källkoden:

```powershell
# Krav: Rust, Node.js LTS, pnpm, Python 3.11, NVIDIA GPU med CUDA (valfritt)
.\scripts\setup-dev.ps1
pnpm install
cargo tauri dev       # dev-mode
cargo tauri build     # fresh MSI
```

Detaljerad arkitektur: se `docs/superpowers/specs/` för per-iter specs.

---

## Licens

Copyright © Marcus Stegberg. All rights reserved.

Koden publiceras för granskning. Ingen del av SVoice 3 får redistribueras,
modifieras för distribution eller användas kommersiellt utan skriftligt
medgivande från upphovspersonen. Se [`LICENSE`](LICENSE) för fullständiga
villkor.
