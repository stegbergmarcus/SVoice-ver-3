# Starta nästa session

## Första meddelandet att skicka till Claude

> Hej! Vi fortsätter på SVoice 3. Nu ska vi in i **installationsfas** — jag vill kunna köra appen som riktig Windows-app (MSI-installer) istället för bara `cargo tauri dev`. Läs `docs/superpowers/START-NEXT-SESSION.md` + `plans/2026-04-17-remaining-work-roadmap.md`. Kritiskt: kolla att senaste commit (palette-close-fix) är pushad innan vi börjar. Sen kör vi release-bundle.

## Status just nu (2026-04-18 sen eftermiddag)

**Senaste lokala commit:** `2241157` (fix: backend palette-close)

**⚠ VIKTIGT VID SESSION-START:** Verifiera att `2241157` är pushad till GitHub:
```bash
git log origin/main..HEAD --oneline
# Om raden visar commits → kör: git push origin main
```
Senaste pushförsöket failade med connection-timeout. Commit finns lokalt.

**Senaste tag:** `iter8-complete` (pushad)

**Git-historik (senaste 10):**
```
2241157 fix(palette): backend-driven hide         ← lokal, ej pushad ännu
039c9ae fix(migrate): #[ignore] keyring-tester
9009334 merge: iter 8 — command palette + Gmail write + toasts
3252e5d fix(settings-tabs): pill-style tabs
88f136f fix(settings-tabs): tydligare kontrast
402766e merge: iter 7 — Settings-flikar
caaabcc merge: iter 6 — Groq + web_search + språk + onboarding
dfdc0d4 merge: iter 4 fas 1-3 Google OAuth + REST
d2f9d53 merge: iter 4 fas 4 — tool-use-loop (iter 4 complete)
aded07b merge: iter 4.5a — Keyring för API-nycklar
```

## Vad som fungerar idag

**Fullt fungerande end-to-end:**

1. **STT** — lokal KB-Whisper ELLER Groq Whisper (gratis-tier, ~100× snabbare)
2. **LLM** — Claude (Sonnet/Opus/Haiku 4.x), Ollama (6 modeller), Groq (Llama 3.3, GPT-OSS etc), alla med Auto-fallback
3. **Action-popup** (Insert) — streaming tokens, transform/query-mode
4. **Agentic action** — automatisk tool-use för Google Calendar + Gmail + web_search via Claude
5. **Gmail draft-skrivning** — `draft_email` + `draft_reply` (skickas ALDRIG automatiskt, user granskar i Gmail)
6. **Command palette** (Ctrl+Shift+Space) — snabbmeny för 5 inbyggda smart-functions
7. **Settings-UI** — 5 flikar: Översikt, Ljud & STT, Action-LLM, Integrationer, Snabbkommandon
8. **Hot-reload** — alla settings inkl. hotkeys
9. **Keyring** — Windows Credential Manager för alla secrets (`anthropic_api_key`, `groq_api_key`, `google_refresh_token`)
10. **CI** — GitHub Actions (fmt, build, check, clippy)

## Kända issues att hantera

### 1. `cargo test --workspace` **raderar** användarens Anthropic-nyckel

Tre tester i `src-tauri/src/migrate.rs` är nu `#[ignore]`-markerade men orsaken är en **arkitektur-brist**:
`svoice_secrets::SERVICE` använder `cfg(test)` som inte propagerar till dep-användare. När andra crates kör tester som rör secrets, använder de PROD-keyring.

**Långsiktig fix:** Extrahera `Backend`-trait i `svoice-secrets` så tester kan injicera in-memory mock. Se [issue-not-yet-created].

Tills dess: **kör ALDRIG `cargo test --workspace -- --include-ignored`** utan att backup:a Anthropic-nyckeln först.

### 2. Settings-fönstret stängs med X → tray-click visade inget

Fixat i iter 7 — `CloseRequested` intercept:as, window:n hides istället för destroys. Om problem återkommer, kolla `src-tauri/src/lib.rs` setup-closure.

### 3. Palette lämnade svart rektangel vid Esc

Fixat i commit `2241157` (backend-driven close). **Verifiera fixen är pushad.**

### 4. Groq STT + LLM inte testad med riktig API-nyckel

All kod byggd + enhetstestad. Saknar bara att Marcus fyller i Groq-nyckel och verifierar end-to-end.

## Installationsfas — riktig Windows-app

### Steg 1: Bundla Python-runtime (~2.3 GB)

Python STT-sidecaren behöver ha Python embed:ad:

```powershell
.\scripts\bundle-python.ps1
```

Detta:
- Laddar ner Python 3.11 embeddable build
- Installerar `faster-whisper` + deps
- Paketer in i `src-tauri/resources/python-runtime/`

Kolla att scriptet fortfarande fungerar (inte rört på ett tag).

### Steg 2: Bygg release-MSI

```powershell
cd src-tauri
cargo tauri build
```

Output: `src-tauri/target/release/bundle/msi/SVoice 3_0.1.0_x64_en-US.msi`

**Observera:** Utan EV-certifikat kommer Windows SmartScreen + Defender blockera installationen. För privat-installation går det att bypassa (Mer info → Kör ändå). Publik distribution kräver EV-code-signing (~$300/år från Sectigo/DigiCert).

### Steg 3: Testa installation

1. Kopiera MSI:n till annan Windows-dator eller efter `cargo tauri dev` stängts
2. Dubbelklicka → installera
3. Start-menyn → "SVoice 3"
4. Första start: user måste:
   - Fylla i Anthropic-nyckel i Settings → Action-LLM
   - ELLER installera Ollama + dra ner qwen2.5:14b
   - ELLER fylla i Groq-nyckel (gratis via console.groq.com/keys)
   - Paste:a Google OAuth client_id + secret (om kalender/mail önskas)

### Steg 4: Auto-updater (senare)

När MSI fungerar lokalt:

1. Lägg till `tauri-plugin-updater`
2. Generera signing keys: `cargo tauri signer generate`
3. Publikt repo-release-feed: GitHub Releases med `latest.json`
4. Bygg signed MSI i CI (kräver EV-cert i GitHub secrets)
5. App checkar uppdateringar vid start

Det här är en full iter (3-5 dagar) när release-pipeline är klar.

## Rekommenderad sekvens för nästa session

1. **Push lokal commit till GitHub** (om ej pushad)
2. **Testa bundle-python.ps1 + cargo tauri build** på utvecklar-maskin
3. **Installera MSI lokalt** och verifiera att alla features fortfarande funkar efter install
4. **Fix eventuella bundle-problem** (path-issues, saknade resurser)
5. **Skriv install-instruktioner för Marcus vänner** (README + onboarding i appen)
6. Senare: EV-cert + auto-updater (kräver inköp)

## Features kvar på roadmapen (lägre prio än install)

### Nice-to-have (skippade under natten pga scope)
- **Silero VAD** — bättre röst-trim i brus (ONNX runtime, ~2 MB modell)
- **Streaming STT med partials** — live-text i overlay under inspelning
- **Custom prompts-historik** — senaste 10 action-popup-kommandon
- **Första-start-wizard** — guide genom API-nycklar + Google-setup
- **Per-app-profiler** — olika settings beroende på aktiv app
- **Fler integrationer** — Outlook, Slack, Notion via MCP
- **Wake-word** — "SVoice" istället för PTT
- **Mail-skicka** (inte bara draft) — kräver gmail.send-scope + säkerhetsöverväganden

### Skippad i natten
- **Command palette historik** — krävde för mycket UI-arbete för marginell nytta
- **Theme-toggle** (ljust/mörkt) — inte nämnt av Marcus

## Setup för att komma igång (för Marcus i nästa session)

```bash
cd "C:\Users\marcu\Documents\Programmering hemma\Temp\SVoice ver 3"

# Verifiera remote är i sync:
git fetch origin
git log origin/main..HEAD --oneline

# Om pushbehov:
git push origin main

# Starta dev för testing:
cargo tauri dev

# När klar för release:
.\scripts\bundle-python.ps1
cd src-tauri && cargo tauri build
```

## Secrets du har konfigurerat (Windows Credential Manager → svoice-v3)

- `anthropic_api_key` — Claude
- `groq_api_key` — Groq STT + LLM (om konfigurerad)
- `google_refresh_token` — Google OAuth (med Calendar + Gmail modify-scope)

Samt i `%APPDATA%/svoice-v3/settings.json`:
- `google_oauth_client_id`
- `google_oauth_client_secret`
- Alla övriga preferenser

## Design-principer att respektera

- **Editorial × pro-audio studio**: charcoal/ivory/amber, Fraunces + Instrument Sans + JetBrains Mono
- **Wow-känsla obligatorisk** — inga generiska AI-UI:er
- **Privacy-first default** — lokal STT/LLM föredras; cloud är opt-in
- **Hot-reload alltid** — inga "kräver omstart"-varningar
- **Tangentbord får ALDRIG fastna** — symmetriskt konsumera PTT-events
- **Fail-soft för secrets** — popup visar felmeddelanden tydligt
- **Drafts > sends** — mail skapas aldrig automatiskt utan granskning
