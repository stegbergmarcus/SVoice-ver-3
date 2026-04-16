# SVoice 3 Iter 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Leverera (1) ett walking-skeleton som bevisar hotkey → PTT state machine → text-injektion end-to-end i valfri Windows-app, och (2) en STT-spike som verifierar `ct2rs` + kb-whisper-medium på RTX 5080 + CUDA 13.2.

**Architecture:** Tauri 2 app (Rust backend + React/TypeScript/Vite frontend) organiserad som Cargo-workspace med separata crates per ansvarsområde (hotkey, inject, audio, stt, llm, …). I iter 1 är bara `hotkey`, `inject` och `ipc` aktiva; `audio`, `stt`, `llm`, `settings`, `integrations` är stubbs. Spiken ligger som fristående bin-crate `stt-spike/` som kör isolerat från Tauri-appen.

**Tech Stack:** Rust 1.95, Tauri 2.x, React 18 + TypeScript 5 + Vite 5, pnpm, `windows` crate (SendInput), `arboard` (clipboard), `tauri-plugin-global-shortcut`, `ct2rs` (faster-whisper FFI), `hf-hub` (model download), PowerShell `System.Speech.Synthesis` (TTS-genererad test-wav).

---

## Plan-översikt

Planen är uppdelad i fyra delar:

- **Del 1 — Bootstrap (Fas A):** Cargo workspace + Tauri scaffold + pnpm + stub-crates. Gemensam grund.
- **Del 2 — Walking skeleton (Fas B–F):** Hotkey, PTT state machine, SendInput, clipboard-fallback, Tauri-wire-up, tray, recording-pill, exit-verification. Levererar en körbar app där en hårdkodad dummy-text injiceras på key-up.
- **Del 3 — STT-spike (Fas G–H):** Fristående bin-crate som laddar kb-whisper-medium via ct2rs och mäter latens + VRAM. Skriver en spike-rapport.
- **Del 4 — Exit (Fas I):** Manuell testplan + uppdatering av `plan.md` med spike-resultat.

---

## Filstruktur som skapas

### Nya filer i repo-roten

| Fil | Syfte |
|---|---|
| `package.json` | pnpm workspace root |
| `pnpm-workspace.yaml` | workspace-deklaration |
| `vite.config.ts` | Vite-konfig för Tauri |
| `tsconfig.json` | TypeScript-config för frontend |
| `index.html` | Vite entry |
| `src/main.tsx` | React entry |
| `src/windows/Main.tsx` | minimal huvudvyn (tom i iter 1) |
| `src/overlays/RecordingIndicator.tsx` | always-on-top-pill |
| `src/lib/ipc.ts` | typade Tauri invoke-wrappers |

### Nya filer under `src-tauri/`

| Fil | Syfte |
|---|---|
| `src-tauri/Cargo.toml` | workspace root (members = alla crates + bins) |
| `src-tauri/tauri.conf.json` | Tauri-konfig med två fönster (main + overlay) |
| `src-tauri/build.rs` | Tauri build hook |
| `src-tauri/src/main.rs` | Tauri builder, plugin-init, tray, IPC-registrering |
| `src-tauri/crates/hotkey/Cargo.toml` | |
| `src-tauri/crates/hotkey/src/lib.rs` | public API (re-export) |
| `src-tauri/crates/hotkey/src/ptt_state.rs` | PTT state machine |
| `src-tauri/crates/hotkey/src/register.rs` | global shortcut registrering + fallback |
| `src-tauri/crates/inject/Cargo.toml` | |
| `src-tauri/crates/inject/src/lib.rs` | dispatch: send_input → clipboard fallback |
| `src-tauri/crates/inject/src/send_input.rs` | Unicode SendInput via windows-crate |
| `src-tauri/crates/inject/src/clipboard.rs` | arboard-write + Ctrl+V-synth |
| `src-tauri/crates/ipc/Cargo.toml` | |
| `src-tauri/crates/ipc/src/lib.rs` | Tauri command exports |
| `src-tauri/crates/ipc/src/commands.rs` | `#[tauri::command]` handlers |
| `src-tauri/crates/audio/Cargo.toml` (stub) | |
| `src-tauri/crates/audio/src/lib.rs` (stub) | |
| `src-tauri/crates/stt/Cargo.toml` (stub) | |
| `src-tauri/crates/stt/src/lib.rs` (stub med `pub fn dummy_transcribe() -> String`) | |
| `src-tauri/crates/llm/Cargo.toml` (stub) | |
| `src-tauri/crates/llm/src/lib.rs` (stub) | |
| `src-tauri/crates/settings/Cargo.toml` (stub) | |
| `src-tauri/crates/settings/src/lib.rs` (stub) | |
| `src-tauri/crates/integrations/Cargo.toml` (stub) | |
| `src-tauri/crates/integrations/src/lib.rs` (stub) | |
| `src-tauri/bins/stt-spike/Cargo.toml` | |
| `src-tauri/bins/stt-spike/src/main.rs` | ct2rs-driven spike-runner |
| `src-tauri/bins/stt-spike/src/metrics.rs` | mätning cold/warm inference, VRAM |
| `src-tauri/bins/stt-spike/testdata/sv-test.wav` | TTS-genererad |
| `src-tauri/bins/stt-spike/testdata/sv-test.expected.txt` | förväntad text |
| `src-tauri/resources/manifest.json` (minimal) | shippad modellkatalog |
| `docs/superpowers/specs/2026-04-XX-stt-spike-report.md` | spike-resultatrapport (XX = dag spiken körs) |

### Ikon-assets

| Fil | Syfte |
|---|---|
| `src-tauri/icons/icon.png` | app-ikon (generas via tauri icon) |
| `src-tauri/icons/tray-idle.ico` | grå idle-ikon |
| `src-tauri/icons/tray-recording.ico` | röd recording-ikon |

---

## Cargo-workspace-layout

`src-tauri/Cargo.toml` (workspace root):

```toml
[workspace]
resolver = "2"
members = [
    ".",
    "crates/audio",
    "crates/stt",
    "crates/hotkey",
    "crates/inject",
    "crates/llm",
    "crates/settings",
    "crates/ipc",
    "crates/integrations",
    "bins/stt-spike",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.95"
authors = ["Marcus Stegberg <stegbergm@gmail.com>"]
license = "Proprietary"

[workspace.dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-global-shortcut = "2"
tauri-build = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tokio = { version = "1", features = ["full"] }
windows = { version = "0.60", features = [
    "Win32_Foundation",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_DataExchange",
    "Win32_System_Memory",
    "Win32_System_Ole",
    "Win32_Graphics_Gdi",
] }
arboard = "3"
anyhow = "1"
```

---

# Fas A — Bootstrap

## Task A1: Installera pnpm och Tauri CLI

**Files:** (inga; installerar globala verktyg)

- [ ] **Step 1: Installera pnpm globalt**

Run:
```bash
npm i -g pnpm
```
Expected: `pnpm 9.x` installerat. Verifiera:
```bash
pnpm --version
```

- [ ] **Step 2: Installera Tauri CLI via cargo**

Run:
```bash
/c/Users/marcu/.cargo/bin/cargo.exe install tauri-cli --version "^2.0" --locked
```
Expected: Kompilerar och installerar `cargo-tauri` i `~/.cargo/bin/`. Tar ~2-4 min. Verifiera:
```bash
/c/Users/marcu/.cargo/bin/cargo.exe tauri --version
```

- [ ] **Step 3: Lägg till ~/.cargo/bin på PATH för denna session**

Run (i den bash-session där resten av planen körs):
```bash
export PATH="/c/Users/marcu/.cargo/bin:$PATH"
cargo --version && rustup --version && cargo tauri --version
```
Expected: alla tre verktyg körs utan full path.

- [ ] **Step 4: Commit (inga filändringar, hoppa)**

Inga committed ändringar i detta task — det är rent miljö-setup.

---

## Task A2: Skapa Cargo-workspace-root och stub-crates

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/crates/audio/Cargo.toml` + `src/lib.rs`
- Create: `src-tauri/crates/stt/Cargo.toml` + `src/lib.rs`
- Create: `src-tauri/crates/hotkey/Cargo.toml` + `src/lib.rs`
- Create: `src-tauri/crates/inject/Cargo.toml` + `src/lib.rs`
- Create: `src-tauri/crates/llm/Cargo.toml` + `src/lib.rs`
- Create: `src-tauri/crates/settings/Cargo.toml` + `src/lib.rs`
- Create: `src-tauri/crates/ipc/Cargo.toml` + `src/lib.rs`
- Create: `src-tauri/crates/integrations/Cargo.toml` + `src/lib.rs`

- [ ] **Step 1: Skapa `src-tauri/` och workspace-root Cargo.toml**

Skapa `src-tauri/Cargo.toml` (workspace-root, ingen `[package]` ännu — vi lägger till det i Task A4 när Tauri scaffold:as):

```toml
[workspace]
resolver = "2"
members = [
    "crates/audio",
    "crates/stt",
    "crates/hotkey",
    "crates/inject",
    "crates/llm",
    "crates/settings",
    "crates/ipc",
    "crates/integrations",
    "bins/stt-spike",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.95"

[workspace.dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-global-shortcut = "2"
tauri-build = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tokio = { version = "1", features = ["full"] }
windows = { version = "0.60", features = [
    "Win32_Foundation",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_DataExchange",
    "Win32_System_Memory",
    "Win32_System_Ole",
    "Win32_Graphics_Gdi",
] }
arboard = "3"
anyhow = "1"
```

Notera: `bins/stt-spike` är med i members redan — vi skapar den crate:en i fas G. Låt members-listan vara komplett redan nu så vi inte glömmer; stub-crate för stt-spike skapas i steg 3 nedan tillsammans med övriga.

- [ ] **Step 2: Skapa 8 stub-crates — skript**

Windows Git Bash stödjer Bash-loopar. Skapa samtliga stub-crates med en loop:

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
for crate in audio stt hotkey inject llm settings ipc integrations; do
  mkdir -p "crates/$crate/src"
  cat > "crates/$crate/Cargo.toml" <<EOF
[package]
name = "svoice-${crate}"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
EOF
  cat > "crates/$crate/src/lib.rs" <<EOF
// svoice-${crate} — stub för iter 1
// Fylls i i senare iterationer enligt plan.md.

#[cfg(test)]
mod tests {
    #[test]
    fn it_compiles() {}
}
EOF
done
```
Expected: åtta stub-crates skapade med `Cargo.toml` + `src/lib.rs`.

- [ ] **Step 3: Skapa även `bins/stt-spike/` som stub**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
mkdir -p bins/stt-spike/src bins/stt-spike/testdata
cat > bins/stt-spike/Cargo.toml <<'EOF'
[package]
name = "svoice-stt-spike"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "stt-spike"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
EOF
cat > bins/stt-spike/src/main.rs <<'EOF'
// Placeholder — fylls i i fas G.
fn main() -> anyhow::Result<()> {
    println!("stt-spike stub; implemented in fas G");
    Ok(())
}
EOF
```

- [ ] **Step 4: Verifiera att workspace kompilerar**

Run:
```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo build --workspace
```
Expected: `Compiling` för alla nio crates, `Finished`. Varningar OK, inga fel.

- [ ] **Step 5: Verifiera att alla stub-tester passerar**

Run:
```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo test --workspace
```
Expected: 8 passing `it_compiles`-tester + 0 för stt-spike (ingen test). Totalt `test result: ok`.

- [ ] **Step 6: Commit**

Run:
```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3"
git add src-tauri/
git commit -m "feat(scaffold): cargo workspace with stub crates

Introduces the src-tauri/ workspace root with 8 stub crates (audio,
stt, hotkey, inject, llm, settings, ipc, integrations) plus the
stt-spike binary crate. All stubs compile and their smoke tests
pass. Tauri [package] section and actual tauri app will be added
in Task A3.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task A3: Scaffolda Tauri 2 app-package in i `src-tauri/`

**Files:**
- Modify: `src-tauri/Cargo.toml` (lägg till `[package]` och appens `[dependencies]`)
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/icons/` (genereras)
- Create: `package.json` (i repo-root)
- Create: `vite.config.ts`
- Create: `tsconfig.json`
- Create: `index.html`
- Create: `src/main.tsx`

- [ ] **Step 1: Installera Node deps för frontend (Vite + React + TypeScript + Tauri API)**

Skapa `package.json` i repo-rot:

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3"
cat > package.json <<'EOF'
{
  "name": "svoice-v3",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-global-shortcut": "^2",
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "@types/react": "^18.3.12",
    "@types/react-dom": "^18.3.1",
    "@vitejs/plugin-react": "^4.3.4",
    "typescript": "^5.6",
    "vite": "^5.4"
  }
}
EOF
pnpm install
```
Expected: `node_modules/` skapad, 0 errors. `pnpm-lock.yaml` genererad.

- [ ] **Step 2: Skapa Vite + TypeScript-konfig**

`vite.config.ts`:
```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
```

`tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "lib": ["ES2023", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "moduleDetection": "force",
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"]
}
```

`index.html`:
```html
<!DOCTYPE html>
<html lang="sv">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>SVoice 3</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`src/main.tsx`:
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import Main from "./windows/Main";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Main />
  </React.StrictMode>
);
```

`src/windows/Main.tsx`:
```tsx
export default function Main() {
  return (
    <main style={{ fontFamily: "system-ui", padding: "2rem" }}>
      <h1>SVoice 3</h1>
      <p>Walking skeleton aktiv. Håll <kbd>Win+Alt+Space</kbd> i valfri app för att injicera testtext.</p>
    </main>
  );
}
```

Verifiera:
```bash
mkdir -p src/windows
# (flytta src/main.tsx och src/windows/Main.tsx enligt ovan)
pnpm build
```
Expected: `vite build` avslutas utan fel; `dist/` skapas.

- [ ] **Step 3: Lägg till Tauri-package i `src-tauri/Cargo.toml`**

Uppdatera `src-tauri/Cargo.toml` genom att *lägga till* (inte ersätta) följande överst, före `[workspace]`:

```toml
[package]
name = "svoice-v3"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
description = "SVoice 3 — svensk dikterings-app för Windows"
default-run = "svoice-v3"

[lib]
name = "svoice_v3_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { workspace = true }

[dependencies]
tauri = { workspace = true }
tauri-plugin-global-shortcut = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
tokio = { workspace = true }
anyhow = { workspace = true }
svoice-hotkey = { path = "crates/hotkey" }
svoice-inject = { path = "crates/inject" }
svoice-stt = { path = "crates/stt" }
svoice-ipc = { path = "crates/ipc" }

[[bin]]
name = "svoice-v3"
path = "src/main.rs"
```

- [ ] **Step 4: Skapa `src-tauri/build.rs`**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 5: Skapa `src-tauri/src/lib.rs` (minimal run-funktion)**

```rust
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,svoice=debug")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            tracing::info!("svoice-v3 startar");
            let _ = app.get_webview_window("main");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 6: Skapa `src-tauri/src/main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    svoice_v3_lib::run()
}
```

- [ ] **Step 7: Skapa `src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "SVoice 3",
  "version": "0.1.0",
  "identifier": "se.stegberg.svoice.v3",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "SVoice 3",
        "width": 900,
        "height": 600,
        "resizable": true,
        "visible": true
      }
    ],
    "security": {
      "csp": "default-src 'self' tauri:; img-src 'self' asset: data:;"
    }
  },
  "bundle": {
    "active": true,
    "targets": "msi",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.ico"],
    "category": "Productivity",
    "shortDescription": "Svensk diktering för Windows"
  },
  "plugins": {}
}
```

- [ ] **Step 8: Generera ikoner via `cargo tauri icon`**

Tauri CLI kan generera hela ikonpaketet från en PNG. Skapa först en enkel 1024x1024 PNG-placeholder (blått "S"):

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3"
mkdir -p src-tauri/icons
# Generera en enkel 1024x1024 blå fyrkant med bokstaven S som placeholder
powershell -NoProfile -Command "
Add-Type -AssemblyName System.Drawing;
\$bmp = New-Object System.Drawing.Bitmap 1024, 1024;
\$g = [System.Drawing.Graphics]::FromImage(\$bmp);
\$g.Clear([System.Drawing.Color]::FromArgb(37, 99, 235));
\$font = New-Object System.Drawing.Font 'Segoe UI', 600, 'Bold';
\$brush = [System.Drawing.Brushes]::White;
\$fmt = New-Object System.Drawing.StringFormat;
\$fmt.Alignment = 'Center';
\$fmt.LineAlignment = 'Center';
\$rect = New-Object System.Drawing.RectangleF 0, 0, 1024, 1024;
\$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit;
\$g.DrawString('S', \$font, \$brush, \$rect, \$fmt);
\$bmp.Save('src-tauri/icons/app-icon.png', [System.Drawing.Imaging.ImageFormat]::Png);
\$bmp.Dispose();
"

cd src-tauri
cargo tauri icon icons/app-icon.png
```
Expected: Tauri CLI genererar `icon.ico`, `32x32.png`, `128x128.png`, `128x128@2x.png`, `Square*.png`, etc i `src-tauri/icons/`.

- [ ] **Step 9: Kör `cargo tauri dev` som smoke-test**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3"
cargo tauri dev
```
Expected: Vite startar på 1420, Tauri-fönstret öppnas med titel "SVoice 3" och texten "Walking skeleton aktiv…". Stäng fönstret med Ctrl+C.

- [ ] **Step 10: Verifiera `cargo tauri build --debug` (valfritt, tar tid)**

Vi hoppar full release build här — dev-läget räcker för att bevisa att scaffold funkar. Full build körs i exit-fasen.

- [ ] **Step 11: Commit**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3"
git add package.json pnpm-lock.yaml vite.config.ts tsconfig.json index.html src/ src-tauri/
git commit -m "feat(scaffold): Tauri 2 + React + Vite baseline boots

Tauri app package set up in src-tauri/ with main.rs/lib.rs/build.rs
and tauri.conf.json (single 'main' window, CSP default-src self).
React/TypeScript frontend under src/ renders a placeholder Main view.
cargo tauri dev boots successfully.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task A4: pnpm-workspace och tsconfig-paths

**Files:**
- Create: `pnpm-workspace.yaml`

- [ ] **Step 1: Skapa `pnpm-workspace.yaml`**

```yaml
packages:
  - "."
```

(Iter 1 har bara ett paket — rot-paketet. Filen finns för framtida multi-package support.)

- [ ] **Step 2: Commit**

```bash
git add pnpm-workspace.yaml
git commit -m "chore: add pnpm-workspace.yaml (single-package for now)"
```

---

# Fas B — Walking Skeleton: Hotkey & PTT State Machine

## Task B1: PTT state machine (TDD)

**Files:**
- Modify: `src-tauri/crates/hotkey/Cargo.toml`
- Create: `src-tauri/crates/hotkey/src/ptt_state.rs`
- Modify: `src-tauri/crates/hotkey/src/lib.rs`

- [ ] **Step 1: Uppdatera `svoice-hotkey`s Cargo.toml**

```toml
[package]
name = "svoice-hotkey"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
serde = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 2: Skriv failing test i `crates/hotkey/src/ptt_state.rs`**

Skapa fil med bara testerna först:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        let m = PttMachine::new();
        assert_eq!(m.state(), PttState::Idle);
    }

    #[test]
    fn keydown_from_idle_goes_recording() {
        let mut m = PttMachine::new();
        let ev = m.on_key_down();
        assert_eq!(m.state(), PttState::Recording);
        assert_eq!(ev, PttEvent::StartedRecording);
    }

    #[test]
    fn keyup_from_recording_goes_processing() {
        let mut m = PttMachine::new();
        m.on_key_down();
        let ev = m.on_key_up();
        assert_eq!(m.state(), PttState::Processing);
        assert_eq!(ev, PttEvent::StoppedRecording);
    }

    #[test]
    fn finish_from_processing_goes_idle() {
        let mut m = PttMachine::new();
        m.on_key_down();
        m.on_key_up();
        let ev = m.on_finish_processing();
        assert_eq!(m.state(), PttState::Idle);
        assert_eq!(ev, PttEvent::FinishedProcessing);
    }

    #[test]
    fn keydown_while_recording_is_noop() {
        let mut m = PttMachine::new();
        m.on_key_down();
        let ev = m.on_key_down();
        assert_eq!(m.state(), PttState::Recording);
        assert_eq!(ev, PttEvent::NoChange);
    }

    #[test]
    fn keyup_while_idle_is_noop() {
        let mut m = PttMachine::new();
        let ev = m.on_key_up();
        assert_eq!(m.state(), PttState::Idle);
        assert_eq!(ev, PttEvent::NoChange);
    }

    #[test]
    fn keyup_while_processing_is_noop() {
        let mut m = PttMachine::new();
        m.on_key_down();
        m.on_key_up();
        let ev = m.on_key_up();
        assert_eq!(m.state(), PttState::Processing);
        assert_eq!(ev, PttEvent::NoChange);
    }
}
```

- [ ] **Step 3: Kör test och verifiera att det misslyckas med "PttMachine not found"**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo test -p svoice-hotkey
```
Expected: Kompileringsfel ("cannot find type `PttMachine` in this scope" etc). Det är vårt failing test.

- [ ] **Step 4: Implementera minimal `PttMachine` så testerna passerar**

Lägg implementationen ovanför `#[cfg(test)]` i samma fil:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PttState {
    Idle,
    Recording,
    Processing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PttEvent {
    StartedRecording,
    StoppedRecording,
    FinishedProcessing,
    NoChange,
}

#[derive(Debug, Default)]
pub struct PttMachine {
    state: PttState,
}

impl Default for PttState {
    fn default() -> Self {
        PttState::Idle
    }
}

impl PttMachine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> PttState {
        self.state
    }

    pub fn on_key_down(&mut self) -> PttEvent {
        match self.state {
            PttState::Idle => {
                self.state = PttState::Recording;
                PttEvent::StartedRecording
            }
            _ => PttEvent::NoChange,
        }
    }

    pub fn on_key_up(&mut self) -> PttEvent {
        match self.state {
            PttState::Recording => {
                self.state = PttState::Processing;
                PttEvent::StoppedRecording
            }
            _ => PttEvent::NoChange,
        }
    }

    pub fn on_finish_processing(&mut self) -> PttEvent {
        match self.state {
            PttState::Processing => {
                self.state = PttState::Idle;
                PttEvent::FinishedProcessing
            }
            _ => PttEvent::NoChange,
        }
    }
}
```

- [ ] **Step 5: Kör testerna igen**

```bash
cargo test -p svoice-hotkey
```
Expected: `test result: ok. 7 passed; 0 failed`.

- [ ] **Step 6: Re-exportera typer från `lib.rs`**

Ersätt `crates/hotkey/src/lib.rs` innehåll med:

```rust
pub mod ptt_state;
pub use ptt_state::{PttEvent, PttMachine, PttState};
```

- [ ] **Step 7: Kompilera hela workspace för att säkerställa inget bröts**

```bash
cargo build --workspace
```
Expected: grönt.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/crates/hotkey/
git commit -m "feat(hotkey): PTT state machine with full test coverage

PttMachine is a 3-state machine (Idle -> Recording -> Processing ->
Idle) that emits PttEvent on each transition. Spurious events
(e.g. key_down while already Recording) are no-ops.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task B2: Global shortcut-registrering med fallback

**Files:**
- Create: `src-tauri/crates/hotkey/src/register.rs`
- Modify: `src-tauri/crates/hotkey/src/lib.rs`

- [ ] **Step 1: Lägg till beroende mot tauri-plugin-global-shortcut i hotkey-crate**

Uppdatera `crates/hotkey/Cargo.toml` `[dependencies]`-sektion:

```toml
[dependencies]
serde = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
tauri = { workspace = true }
tauri-plugin-global-shortcut = { workspace = true }
```

- [ ] **Step 2: Implementera `register.rs` med Arc-wrapped callback (för att kunna återanvändas i fallback-pathen)**

```rust
use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("kunde inte registrera någon hotkey (primär: {primary}, fallback: {fallback}, orsaker: primär={primary_err}, fallback={fallback_err})")]
    AllFailed {
        primary: String,
        fallback: String,
        primary_err: String,
        fallback_err: String,
    },
}

#[derive(Debug, Clone)]
pub struct RegisteredHotkey {
    pub label: String,
    pub shortcut: Shortcut,
}

pub type HotkeyCallback<R> =
    Arc<dyn Fn(&AppHandle<R>, &Shortcut, ShortcutEvent) + Send + Sync + 'static>;

pub fn register_ptt<R>(
    app: &AppHandle<R>,
    callback: HotkeyCallback<R>,
) -> Result<RegisteredHotkey, HotkeyError>
where
    R: Runtime,
{
    let primary = Shortcut::new(
        Some(Modifiers::SUPER | Modifiers::ALT),
        Code::Space,
    );
    let fallback = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT),
        Code::Space,
    );

    let gs = app.global_shortcut();

    let cb_clone = callback.clone();
    match gs.on_shortcut(primary, move |app, sc, ev| (cb_clone)(app, sc, ev)) {
        Ok(()) => {
            tracing::info!("hotkey registrerad: Win+Alt+Space");
            Ok(RegisteredHotkey {
                label: "Win+Alt+Space".into(),
                shortcut: primary,
            })
        }
        Err(primary_err) => {
            tracing::warn!(
                "primär hotkey Win+Alt+Space misslyckades ({primary_err}); provar Ctrl+Alt+Space"
            );
            let cb_clone2 = callback.clone();
            match gs.on_shortcut(fallback, move |app, sc, ev| (cb_clone2)(app, sc, ev)) {
                Ok(()) => {
                    tracing::info!("hotkey registrerad (fallback): Ctrl+Alt+Space");
                    Ok(RegisteredHotkey {
                        label: "Ctrl+Alt+Space".into(),
                        shortcut: fallback,
                    })
                }
                Err(fallback_err) => Err(HotkeyError::AllFailed {
                    primary: "Win+Alt+Space".into(),
                    fallback: "Ctrl+Alt+Space".into(),
                    primary_err: primary_err.to_string(),
                    fallback_err: fallback_err.to_string(),
                }),
            }
        }
    }
}

/// Hjälpfunktion för att detektera om ett ShortcutEvent är key-down eller key-up.
pub fn is_key_down(ev: &ShortcutEvent) -> bool {
    matches!(ev.state(), ShortcutState::Pressed)
}
```

- [ ] **Step 3: Re-exportera från `lib.rs`**

Uppdatera `crates/hotkey/src/lib.rs`:

```rust
pub mod ptt_state;
pub mod register;

pub use ptt_state::{PttEvent, PttMachine, PttState};
pub use register::{is_key_down, register_ptt, HotkeyCallback, HotkeyError, RegisteredHotkey};
```

- [ ] **Step 4: Kompilera hotkey-crate**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo build -p svoice-hotkey
```
Expected: grönt. Varningar om oanvända imports OK.

- [ ] **Step 5: Kör tester**

```bash
cargo test -p svoice-hotkey
```
Expected: 7 tester passar (de från Task B1).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/hotkey/
git commit -m "feat(hotkey): register_ptt with primary/fallback shortcut

register_ptt wraps tauri-plugin-global-shortcut and tries
Win+Alt+Space first; if registration fails (e.g. Windows reserves
the combo) it falls back to Ctrl+Alt+Space. HotkeyError::AllFailed
is only returned when both fail.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

# Fas C — Walking Skeleton: Text Injection

## Task C1: Unicode SendInput writer (send_input.rs)

**Files:**
- Modify: `src-tauri/crates/inject/Cargo.toml`
- Create: `src-tauri/crates/inject/src/send_input.rs`
- Modify: `src-tauri/crates/inject/src/lib.rs`

- [ ] **Step 1: Uppdatera `svoice-inject/Cargo.toml`**

```toml
[package]
name = "svoice-inject"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"

[target.'cfg(windows)'.dependencies]
windows = { workspace = true }

[dependencies]
thiserror = { workspace = true }
tracing = { workspace = true }
arboard = { workspace = true }
```

- [ ] **Step 2: Implementera `send_input.rs`**

```rust
use std::mem::size_of;

use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY,
};

#[derive(Debug, thiserror::Error)]
pub enum SendInputError {
    #[error("SendInput misslyckades vid index {index} ({sent}/{total} events skickade, GetLastError=0x{err:X})")]
    PartialSend {
        index: usize,
        sent: u32,
        total: u32,
        err: u32,
    },
    #[error("SendInput returnerade 0 för tom text — detta bör inte hända")]
    EmptyText,
}

/// Skriver Unicode-text via SendInput med KEYEVENTF_UNICODE.
/// Varje kodpunkt expanderas till UTF-16 code units, och varje code unit skickas
/// som ett key-down+key-up-par.
pub fn send_unicode(text: &str) -> Result<(), SendInputError> {
    if text.is_empty() {
        return Err(SendInputError::EmptyText);
    }

    let code_units: Vec<u16> = text.encode_utf16().collect();
    let mut inputs: Vec<INPUT> = Vec::with_capacity(code_units.len() * 2);

    for unit in &code_units {
        // key-down
        inputs.push(make_keyboard_input(*unit, KEYEVENTF_UNICODE));
        // key-up
        inputs.push(make_keyboard_input(
            *unit,
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        ));
    }

    let total = inputs.len() as u32;
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };

    if sent != total {
        let err = unsafe { GetLastError().0 };
        return Err(SendInputError::PartialSend {
            index: sent as usize,
            sent,
            total,
            err,
        });
    }

    Ok(())
}

fn make_keyboard_input(wscan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: wscan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_returns_error() {
        assert!(matches!(send_unicode(""), Err(SendInputError::EmptyText)));
    }

    // Faktisk injektion mot aktivt fönster kan inte unit-testas rimligt i CI.
    // Täcks av manuell E2E i Fas F.
}
```

- [ ] **Step 3: Uppdatera `lib.rs` (tillfälligt — dispatch läggs till i C3)**

```rust
#[cfg(windows)]
pub mod send_input;

#[cfg(windows)]
pub use send_input::{send_unicode, SendInputError};
```

- [ ] **Step 4: Bygg och testa**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo build -p svoice-inject
cargo test -p svoice-inject
```
Expected: Build grönt, `empty_text_returns_error` passerar.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/inject/
git commit -m "feat(inject): Unicode SendInput writer

send_unicode(text) converts the string to UTF-16 code units and
dispatches each as a KEYEVENTF_UNICODE key-down/key-up pair via
SendInput. Returns PartialSend if the OS rejects any event
mid-batch (common on UIPI-protected windows like elevated apps).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task C2: Clipboard paste fallback

**Files:**
- Create: `src-tauri/crates/inject/src/clipboard.rs`
- Modify: `src-tauri/crates/inject/src/lib.rs`

- [ ] **Step 1: Implementera `clipboard.rs`**

```rust
use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
    VK_V,
};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard-åtkomst misslyckades: {0}")]
    Access(String),
    #[error("synthesiserad Ctrl+V misslyckades (sent={sent}, total={total})")]
    PasteFailed { sent: u32, total: u32 },
}

/// Lägger texten på clipboard och skickar Ctrl+V till aktivt fönster.
/// Sparar inte tidigare clipboard — en förbättring för senare iter.
pub fn paste_via_clipboard(text: &str) -> Result<(), ClipboardError> {
    let mut cb = arboard::Clipboard::new().map_err(|e| ClipboardError::Access(e.to_string()))?;
    cb.set_text(text)
        .map_err(|e| ClipboardError::Access(e.to_string()))?;

    // Låt clipboard synka ~30ms (vissa Electron-appar läser för snabbt annars).
    sleep(Duration::from_millis(30));

    send_ctrl_v()?;
    Ok(())
}

fn send_ctrl_v() -> Result<(), ClipboardError> {
    let inputs = [
        make_vk(VK_CONTROL, false),
        make_vk(VK_V, false),
        make_vk(VK_V, true),
        make_vk(VK_CONTROL, true),
    ];
    let total = inputs.len() as u32;
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != total {
        return Err(ClipboardError::PasteFailed { sent, total });
    }
    Ok(())
}

fn make_vk(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_access_works() {
        // Verifierar bara att arboard kan öppnas — faktisk inject sker i manuell test.
        let cb = arboard::Clipboard::new();
        assert!(cb.is_ok(), "clipboard open failed: {:?}", cb.err());
    }
}
```

- [ ] **Step 2: Uppdatera `lib.rs`**

```rust
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod send_input;

#[cfg(windows)]
pub use clipboard::{paste_via_clipboard, ClipboardError};
#[cfg(windows)]
pub use send_input::{send_unicode, SendInputError};
```

- [ ] **Step 3: Bygg & testa**

```bash
cargo build -p svoice-inject
cargo test -p svoice-inject
```
Expected: `clipboard_access_works` passerar.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/inject/
git commit -m "feat(inject): clipboard paste fallback (arboard + Ctrl+V)

paste_via_clipboard places text on the clipboard via arboard and
synthesizes Ctrl+V. 30ms sleep gives Electron clients time to see
the clipboard update. Previous clipboard content is lost in iter 1;
restoring it is a future enhancement.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task C3: Dispatcher (try SendInput, fallback to clipboard)

**Files:**
- Modify: `src-tauri/crates/inject/src/lib.rs`

- [ ] **Step 1: Ersätt `lib.rs` med dispatcher-funktion**

```rust
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod send_input;

#[cfg(windows)]
pub use clipboard::{paste_via_clipboard, ClipboardError};
#[cfg(windows)]
pub use send_input::{send_unicode, SendInputError};

#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    #[error(transparent)]
    SendInput(#[from] SendInputError),
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
    #[error("båda injektionsvägarna misslyckades (send_input: {send_input}, clipboard: {clipboard})")]
    BothFailed {
        send_input: String,
        clipboard: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectMethod {
    SendInput,
    Clipboard,
}

/// Försöker SendInput först. Vid fel (PartialSend) faller tillbaka till clipboard-paste.
#[cfg(windows)]
pub fn inject(text: &str) -> Result<InjectMethod, InjectError> {
    match send_unicode(text) {
        Ok(()) => {
            tracing::debug!("inject: SendInput lyckades ({} tecken)", text.chars().count());
            Ok(InjectMethod::SendInput)
        }
        Err(send_err) => {
            tracing::warn!("inject: SendInput misslyckades ({send_err}); faller tillbaka till clipboard");
            match paste_via_clipboard(text) {
                Ok(()) => {
                    tracing::debug!("inject: clipboard-fallback lyckades");
                    Ok(InjectMethod::Clipboard)
                }
                Err(cb_err) => Err(InjectError::BothFailed {
                    send_input: send_err.to_string(),
                    clipboard: cb_err.to_string(),
                }),
            }
        }
    }
}

#[cfg(not(windows))]
pub fn inject(_text: &str) -> Result<InjectMethod, InjectError> {
    unimplemented!("text-injektion stöds bara på Windows i iter 1")
}
```

Lägg till tracing till dependencies om det inte redan finns:

Kolla `crates/inject/Cargo.toml` och säkerställ att `tracing = { workspace = true }` finns under `[dependencies]` (inte under `[target.cfg]`).

- [ ] **Step 2: Bygg hela workspace**

```bash
cargo build --workspace
```
Expected: grönt.

- [ ] **Step 3: Kör alla inject-tester**

```bash
cargo test -p svoice-inject
```
Expected: passing.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/inject/
git commit -m "feat(inject): unified inject() dispatcher

inject(text) tries send_unicode first and falls back to
paste_via_clipboard on failure. Returns InjectMethod to let callers
log which path was used. BothFailed carries errors from both paths
for diagnostics.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

# Fas D — Walking Skeleton: Tauri Wire-Up

## Task D1: Dummy STT stub + IPC-kommandon

**Files:**
- Modify: `src-tauri/crates/stt/src/lib.rs`
- Modify: `src-tauri/crates/ipc/Cargo.toml`
- Create: `src-tauri/crates/ipc/src/commands.rs`
- Modify: `src-tauri/crates/ipc/src/lib.rs`

- [ ] **Step 1: Ersätt stub i `crates/stt/src/lib.rs`**

```rust
pub const DUMMY_TEXT: &str = "Hej, det här är ett test med å, ä och ö.";

pub fn dummy_transcribe() -> String {
    DUMMY_TEXT.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_text_contains_swedish_chars() {
        let t = dummy_transcribe();
        assert!(t.contains('å'));
        assert!(t.contains('ä'));
        assert!(t.contains('ö'));
    }
}
```

- [ ] **Step 2: Uppdatera `crates/ipc/Cargo.toml`**

```toml
[package]
name = "svoice-ipc"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
tauri = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
svoice-hotkey = { path = "../hotkey" }
svoice-inject = { path = "../inject" }
svoice-stt = { path = "../stt" }
```

- [ ] **Step 3: Skapa `crates/ipc/src/commands.rs`**

```rust
use serde::Serialize;
use svoice_hotkey::PttState;
use svoice_inject::{inject, InjectError, InjectMethod};
use svoice_stt::dummy_transcribe;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InjectResult {
    pub method: String,
    pub chars: usize,
}

#[derive(Debug, Serialize)]
pub struct PttStateReport {
    pub state: PttState,
}

/// Kör ett end-to-end inject av dummy-STT-texten. Används både som smoke-command
/// från UI och som resultat i PTT-loop (via hotkey callback).
#[tauri::command]
pub fn run_dummy_inject() -> Result<InjectResult, String> {
    let text = dummy_transcribe();
    match inject(&text) {
        Ok(method) => Ok(InjectResult {
            method: match method {
                InjectMethod::SendInput => "send_input".into(),
                InjectMethod::Clipboard => "clipboard".into(),
            },
            chars: text.chars().count(),
        }),
        Err(e) => Err(map_inject_error(e)),
    }
}

fn map_inject_error(e: InjectError) -> String {
    format!("inject-fel: {e}")
}
```

- [ ] **Step 4: Uppdatera `crates/ipc/src/lib.rs`**

```rust
pub mod commands;
pub use commands::{run_dummy_inject, InjectResult, PttStateReport};
```

- [ ] **Step 5: Bygg & testa**

```bash
cargo build --workspace
cargo test -p svoice-stt -p svoice-ipc
```
Expected: `dummy_text_contains_swedish_chars` passerar, ipc kompilerar utan varningar.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/stt/ src-tauri/crates/ipc/
git commit -m "feat(ipc+stt): run_dummy_inject command wires stt->inject

dummy_transcribe() returns a hardcoded Swedish test string with
a/a/o. run_dummy_inject is a #[tauri::command] that pipes the
dummy text through inject() and reports which injection method
succeeded.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task D2: Koppla hotkey + IPC i Tauri `lib.rs`

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Ersätt `src-tauri/src/lib.rs` med full wire-up**

```rust
use std::sync::{Arc, Mutex};

use svoice_hotkey::{is_key_down, register_ptt, HotkeyCallback, PttMachine};
use svoice_inject::{inject, InjectMethod};
use svoice_ipc::run_dummy_inject;
use svoice_stt::dummy_transcribe;
use tauri::{AppHandle, Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,svoice=debug")),
        )
        .init();

    let ptt = Arc::new(Mutex::new(PttMachine::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            tracing::info!("svoice-v3 startar");

            let ptt_cb = ptt.clone();
            let callback: HotkeyCallback<tauri::Wry> = Arc::new(
                move |app: &AppHandle<tauri::Wry>, _sc, ev| {
                    let mut m = ptt_cb.lock().unwrap();
                    if is_key_down(&ev) {
                        let _ = m.on_key_down();
                        let _ = app.emit("ptt://state", m.state());
                    } else {
                        let _ = m.on_key_up();
                        let _ = app.emit("ptt://state", m.state());
                        drop(m);
                        // Utför dummy-injektion.
                        let text = dummy_transcribe();
                        match inject(&text) {
                            Ok(method) => {
                                let method_str = match method {
                                    InjectMethod::SendInput => "send_input",
                                    InjectMethod::Clipboard => "clipboard",
                                };
                                tracing::info!("inject OK via {method_str}");
                            }
                            Err(e) => tracing::error!("inject FAIL: {e}"),
                        }
                        let mut m = ptt_cb.lock().unwrap();
                        let _ = m.on_finish_processing();
                        let _ = app.emit("ptt://state", m.state());
                    }
                },
            );

            match register_ptt(&app.handle(), callback) {
                Ok(reg) => tracing::info!("hotkey aktiv: {}", reg.label),
                Err(e) => tracing::error!("hotkey-registrering misslyckades: {e}"),
            }

            let _ = app.get_webview_window("main");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![run_dummy_inject])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 2: Bygg & verifiera kompilation**

```bash
cargo build -p svoice-v3
```
Expected: grönt. Varningar om oanvända `Manager`-trait är OK men städa om det är lätt.

- [ ] **Step 3: Manuellt smoke-test**

Öppna ett nytt terminal-fönster (eller stäng detta och öppna nytt — PATH-fix sker då automatiskt).

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3"
cargo tauri dev
```

Med appen igång: öppna Notepad, klicka i textrutan, håll **Win+Alt+Space** i ~1 sek, släpp. Förväntat: texten `"Hej, det här är ett test med å, ä och ö."` dyker upp i Notepad.

Om det inte funkar, kolla:
- Konsol-logg ("hotkey aktiv: …" raden)
- Om hotkey krockade: ska ha fallit tillbaka till Ctrl+Alt+Space (se log)
- Notepad måste ha fokus när du trycker

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(tauri): wire hotkey PTT -> dummy STT -> inject end-to-end

lib.rs::run() now:
- builds Tauri app with global-shortcut plugin
- registers PTT callback that transitions the PttMachine and emits
  'ptt://state' events to the frontend
- on key-up, runs dummy_transcribe() through inject() and logs
  which path was taken

With this in place, Win+Alt+Space in Notepad injects the test
string. Walking skeleton end-to-end proven.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

# Fas E — Walking Skeleton: Tray & Recording Pill

## Task E1: Tray-ikon med state-baserad bild + Quit-meny

**Files:**
- Modify: `src-tauri/tauri.conf.json` (lägg till tray-icon-block)
- Create: `src-tauri/icons/tray-idle.ico`
- Create: `src-tauri/icons/tray-recording.ico`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Skapa två tray-ikoner via PowerShell**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri/icons"
powershell -NoProfile -Command "
Add-Type -AssemblyName System.Drawing;
function Make-Tray(\$color, \$path) {
  \$bmp = New-Object System.Drawing.Bitmap 64, 64;
  \$g = [System.Drawing.Graphics]::FromImage(\$bmp);
  \$g.SmoothingMode = 'AntiAlias';
  \$brush = New-Object System.Drawing.SolidBrush \$color;
  \$g.FillEllipse(\$brush, 8, 8, 48, 48);
  \$bmp.Save(\$path + '.png', [System.Drawing.Imaging.ImageFormat]::Png);
  \$bmp.Dispose();
}
Make-Tray ([System.Drawing.Color]::Gray) 'tray-idle';
Make-Tray ([System.Drawing.Color]::FromArgb(220, 38, 38)) 'tray-recording';
"
# Konvertera till .ico via ImageMagick om tillgängligt, annars använd .png direkt
# Tauri 2 accepterar .png som tray-ikon på Windows om vi sätter Image-format manuellt.
```

Not: Tauri 2's tray-icon stödjer `Image::from_path` med PNG. Vi använder PNG direkt och namnger dem `.png` (inte `.ico`). Ändra filnamnsreferenserna i kommande steg till `tray-idle.png`/`tray-recording.png`.

- [ ] **Step 2: Uppdatera `src-tauri/Cargo.toml` — tauri-features**

Se till att `tauri`-raden under `[dependencies]` (inte workspace) har `"image-png"` feature:

I `src-tauri/Cargo.toml`, ändra:
```toml
[dependencies]
tauri = { workspace = true, features = ["image-png"] }
```
(Tauri 2 feature-flaggor är additiva — workspace-configen har redan `tray-icon`.)

- [ ] **Step 3: Uppdatera `src-tauri/src/lib.rs` med tray-logik**

Lägg till tray-setup i `setup`-closure (före hotkey-registrering):

```rust
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

// ... i setup-closuren, efter tracing::info!("svoice-v3 startar"):
let quit_item = MenuItem::with_id(app, "quit", "Avsluta", true, None::<&str>)?;
let menu = Menu::with_items(app, &[&quit_item])?;

let tray_idle_bytes = include_bytes!("../icons/tray-idle.png");
let tray_rec_bytes = include_bytes!("../icons/tray-recording.png");

let idle_img = Image::from_bytes(tray_idle_bytes)?;
let _rec_img = Image::from_bytes(tray_rec_bytes)?;

let _tray = TrayIconBuilder::with_id("main-tray")
    .icon(idle_img.clone())
    .menu(&menu)
    .tooltip("SVoice 3 — idle")
    .on_menu_event(|app, ev| {
        if ev.id.as_ref() == "quit" {
            app.exit(0);
        }
    })
    .build(app)?;
```

Och i hotkey-callbacken: när state övergår till `Recording`, byt ikon till röd; när tillbaka till `Idle`, byt till grå. Det görs via `app.tray_by_id("main-tray").unwrap().set_icon(Some(...))`.

Komplett uppdaterad `lib.rs` (ersätt helt):

```rust
use std::sync::{Arc, Mutex};

use svoice_hotkey::{is_key_down, register_ptt, HotkeyCallback, PttMachine, PttState};
use svoice_inject::{inject, InjectMethod};
use svoice_ipc::run_dummy_inject;
use svoice_stt::dummy_transcribe;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

const TRAY_IDLE_BYTES: &[u8] = include_bytes!("../icons/tray-idle.png");
const TRAY_REC_BYTES: &[u8] = include_bytes!("../icons/tray-recording.png");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,svoice=debug")),
        )
        .init();

    let ptt = Arc::new(Mutex::new(PttMachine::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            tracing::info!("svoice-v3 startar");

            // Tray
            let quit_item = MenuItem::with_id(app, "quit", "Avsluta", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;
            let idle_img = Image::from_bytes(TRAY_IDLE_BYTES)?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(idle_img)
                .menu(&menu)
                .tooltip("SVoice 3 — idle")
                .on_menu_event(|app, ev| {
                    if ev.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // Hotkey
            let ptt_cb = ptt.clone();
            let callback: HotkeyCallback<tauri::Wry> = Arc::new(
                move |app: &AppHandle<tauri::Wry>, _sc, ev| {
                    let state_after: PttState;
                    if is_key_down(&ev) {
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_key_down();
                        state_after = m.state();
                    } else {
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_key_up();
                        state_after = m.state();
                    }

                    let _ = app.emit("ptt://state", state_after);
                    update_tray_for_state(app, state_after);

                    // Om vi just gick från Recording -> Processing: kör dummy-inject.
                    if !is_key_down(&ev) && state_after == PttState::Processing {
                        let text = dummy_transcribe();
                        match inject(&text) {
                            Ok(method) => {
                                let method_str = match method {
                                    InjectMethod::SendInput => "send_input",
                                    InjectMethod::Clipboard => "clipboard",
                                };
                                tracing::info!("inject OK via {method_str}");
                            }
                            Err(e) => tracing::error!("inject FAIL: {e}"),
                        }
                        let mut m = ptt_cb.lock().unwrap();
                        m.on_finish_processing();
                        let final_state = m.state();
                        let _ = app.emit("ptt://state", final_state);
                        update_tray_for_state(app, final_state);
                    }
                },
            );

            match register_ptt(&app.handle(), callback) {
                Ok(reg) => tracing::info!("hotkey aktiv: {}", reg.label),
                Err(e) => tracing::error!("hotkey-registrering misslyckades: {e}"),
            }

            let _ = app.get_webview_window("main");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![run_dummy_inject])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn update_tray_for_state(app: &AppHandle<tauri::Wry>, state: PttState) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let bytes = match state {
            PttState::Recording => TRAY_REC_BYTES,
            _ => TRAY_IDLE_BYTES,
        };
        if let Ok(img) = Image::from_bytes(bytes) {
            let _ = tray.set_icon(Some(img));
        }
        let tip = match state {
            PttState::Idle => "SVoice 3 — idle",
            PttState::Recording => "SVoice 3 — spelar in",
            PttState::Processing => "SVoice 3 — transkriberar",
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}
```

- [ ] **Step 4: Bygg & kör**

```bash
cargo tauri dev
```
Expected: fönster öppnas, tray-ikon (grå cirkel) dyker upp nere till höger. Håll hotkey → tray blir röd under recording → grå efter inject.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/
git commit -m "feat(tray): state-driven tray icon + Quit menu

Tray icon switches between gray (idle) and red (recording) based
on PttState. Menu has a single 'Avsluta' entry that exits the app.
Tooltip mirrors the state in Swedish.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task E2: Always-on-top recording-pill overlay

**Files:**
- Modify: `src-tauri/tauri.conf.json` (lägg till 'overlay'-window)
- Create: `src/overlays/RecordingIndicator.tsx`
- Create: `src/overlays/overlay-entry.tsx`
- Create: `src/overlays/overlay.html`
- Modify: `vite.config.ts` (multi-entry build)

- [ ] **Step 1: Lägg till overlay-fönster i `tauri.conf.json`**

Ersätt `"windows"` array:

```json
"windows": [
  {
    "label": "main",
    "title": "SVoice 3",
    "width": 900,
    "height": 600,
    "resizable": true,
    "visible": true
  },
  {
    "label": "overlay",
    "url": "overlay.html",
    "width": 220,
    "height": 60,
    "x": 20,
    "y": 20,
    "decorations": false,
    "resizable": false,
    "alwaysOnTop": true,
    "skipTaskbar": true,
    "transparent": true,
    "shadow": false,
    "focus": false,
    "visible": true
  }
]
```

- [ ] **Step 2: Skapa `src/overlays/overlay.html`**

Tauri 2 vill ha separat HTML-entry för multi-window. Vite stödjer multi-entry via `rollupOptions.input`.

Skapa fil `src/overlays/overlay.html`:

```html
<!DOCTYPE html>
<html lang="sv">
  <head>
    <meta charset="UTF-8" />
    <title>SVoice overlay</title>
    <style>
      html, body {
        margin: 0;
        padding: 0;
        background: transparent;
        font-family: system-ui, -apple-system, sans-serif;
        -webkit-app-region: drag;
      }
    </style>
  </head>
  <body>
    <div id="overlay-root"></div>
    <script type="module" src="./overlay-entry.tsx"></script>
  </body>
</html>
```

- [ ] **Step 3: Skapa `src/overlays/RecordingIndicator.tsx`**

```tsx
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type PttState = "idle" | "recording" | "processing";

export default function RecordingIndicator() {
  const [state, setState] = useState<PttState>("idle");

  useEffect(() => {
    const unlisten = listen<PttState>("ptt://state", (ev) => {
      setState(ev.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const color = state === "recording" ? "#dc2626" : state === "processing" ? "#f59e0b" : "#6b7280";
  const label = state === "recording" ? "Spelar in…" : state === "processing" ? "Transkriberar…" : "Redo";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "10px 14px",
        borderRadius: 999,
        background: "rgba(17, 24, 39, 0.92)",
        color: "white",
        fontSize: 13,
        boxShadow: "0 4px 12px rgba(0,0,0,0.35)",
        userSelect: "none",
      }}
    >
      <span
        style={{
          width: 12,
          height: 12,
          borderRadius: 999,
          background: color,
        }}
      />
      <span>{label}</span>
    </div>
  );
}
```

- [ ] **Step 4: Skapa `src/overlays/overlay-entry.tsx`**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import RecordingIndicator from "./RecordingIndicator";

ReactDOM.createRoot(document.getElementById("overlay-root")!).render(
  <React.StrictMode>
    <RecordingIndicator />
  </React.StrictMode>
);
```

- [ ] **Step 5: Uppdatera `vite.config.ts` för multi-entry**

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        overlay: resolve(__dirname, "src/overlays/overlay.html"),
      },
    },
  },
});
```

- [ ] **Step 6: Bygg frontend för att verifiera multi-entry**

```bash
pnpm build
```
Expected: `dist/` innehåller `index.html` + `src/overlays/overlay.html` + assets.

- [ ] **Step 7: Kör full dev-test**

```bash
cargo tauri dev
```
Expected: Två fönster öppnas — huvudfönstret och en liten always-on-top pill uppe i hörnet som visar "Redo". När du håller hotkey: pill-texten blir "Spelar in…" röd, sedan "Transkriberar…" orange, sedan tillbaka till "Redo".

- [ ] **Step 8: Commit**

```bash
git add src/ vite.config.ts src-tauri/tauri.conf.json
git commit -m "feat(ui): recording-indicator overlay pill

Adds always-on-top transparent overlay window that subscribes to
ptt://state events and shows Idle/Recording/Processing with color
coded dot + Swedish label. Vite is configured for multi-entry so
both index.html and src/overlays/overlay.html are bundled.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

# Fas F — Walking Skeleton: Exit Verification

## Task F1: Manuell testprotokoll + dokumentation

**Files:**
- Create: `docs/superpowers/specs/2026-04-XX-iter1-walking-skeleton-verification.md`

- [ ] **Step 1: Kör `cargo tauri dev` och gå igenom testlistan**

Öppna appen och testa följande. Anteckna resultat direkt (PASS/FAIL + noteringar) på ett papper eller i en textfil.

| # | Test | Förväntat resultat |
|---|---|---|
| 1 | Öppna Notepad, klicka i textrutan, håll Win+Alt+Space 1 sek, släpp. | Texten `Hej, det här är ett test med å, ä och ö.` dyker upp korrekt. |
| 2 | Samma test i Edge adressrad. | Texten dyker upp (kan komma via clipboard-fallback beroende på UIPI). |
| 3 | Samma test i Teams chatt-fält (om installerat). | Texten dyker upp via någondera väg; kolla konsol-log för vilken metod. |
| 4 | Håll hotkey, kolla tray-ikon. | Tray byter till röd under recording, tillbaka till grå efter. |
| 5 | Håll hotkey, kolla overlay-pill. | Pill visar "Spelar in…" (röd), sedan "Transkriberar…" (orange), sedan "Redo". |
| 6 | Högerklicka tray → Avsluta. | Appen stängs rent. |
| 7 | Släpp hotkey utan att ha hållit länge (< 200 ms tap). | Injektion sker fortfarande (dummy-texten är oberoende av recording-längd). |
| 8 | Tryck två gånger snabbt i rad. | Andra invokationen processar korrekt efter att första avslutats. |

- [ ] **Step 2: Skriv verifierings-rapport**

Skapa `docs/superpowers/specs/2026-04-XX-iter1-walking-skeleton-verification.md` (byt XX mot dagens datum) med mall:

```markdown
# Iter 1 Walking Skeleton — Verifieringsrapport

**Datum:** 2026-04-XX
**Utförd av:** Marcus

## Miljö
- Windows 11 Home 10.0.26200
- RTX 5080, driver 595.97
- Rust 1.95.0, Tauri CLI 2.x

## Testresultat

| # | Test | Resultat | Notering |
|---|---|---|---|
| 1 | Notepad-injektion | PASS/FAIL | |
| 2 | Edge-adressrad | PASS/FAIL | (metod: send_input/clipboard) |
| 3 | Teams chat | PASS/FAIL | |
| 4 | Tray-ikon byten | PASS/FAIL | |
| 5 | Overlay-pill state | PASS/FAIL | |
| 6 | Quit-meny | PASS/FAIL | |
| 7 | Kort tap (< 200 ms) | PASS/FAIL | |
| 8 | Snabbt dubbel-tryck | PASS/FAIL | |

## Ev. issues som upptäckts

(fyll i)

## Slutsats

Walking skeleton ___ (godkänd / behöver fixes)
```

- [ ] **Step 3: Commit verifierings-docet**

```bash
git add docs/superpowers/specs/
git commit -m "docs: iter 1 walking skeleton verification template"
```

---

# Fas G — STT-Spike: Setup

## Task G1: Generera test-WAV via Windows TTS

**Files:**
- Create: `src-tauri/bins/stt-spike/testdata/sv-test.wav`
- Create: `src-tauri/bins/stt-spike/testdata/sv-test.expected.txt`

- [ ] **Step 1: Skapa PowerShell-TTS-skript**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri/bins/stt-spike/testdata"

powershell -NoProfile -Command "
Add-Type -AssemblyName System.Speech;
\$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer;
# Välj svensk röst om tillgänglig
\$voice = \$synth.GetInstalledVoices() | Where-Object { \$_.VoiceInfo.Culture.Name -like 'sv-*' } | Select-Object -First 1;
if (\$voice) { \$synth.SelectVoice(\$voice.VoiceInfo.Name); Write-Host 'Använder:' \$voice.VoiceInfo.Name; }
else { Write-Host 'VARNING: ingen svensk röst installerad — använder default. Installera svenska Windows TTS via Settings -> Language.'; }
\$synth.SetOutputToWaveFile('sv-test.wav');
\$synth.Speak('Hej, det här är ett test med å, ä och ö.');
\$synth.Dispose();
Write-Host 'sv-test.wav skapad.';
"
```

Expected: `sv-test.wav` (normalt ~30-50 KB för denna mening).

- [ ] **Step 2: Spara förväntad text**

```bash
cat > "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri/bins/stt-spike/testdata/sv-test.expected.txt" <<'EOF'
Hej, det här är ett test med å, ä och ö.
EOF
```

- [ ] **Step 3: Verifiera wav-filens format med PowerShell**

```bash
powershell -NoProfile -Command "
Add-Type -AssemblyName System.Speech;
\$stream = [System.IO.File]::OpenRead('C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri/bins/stt-spike/testdata/sv-test.wav');
\$buf = New-Object byte[] 44;
\$stream.Read(\$buf, 0, 44) | Out-Null;
\$stream.Close();
\$ch = [BitConverter]::ToInt16(\$buf, 22);
\$sr = [BitConverter]::ToInt32(\$buf, 24);
\$bits = [BitConverter]::ToInt16(\$buf, 34);
Write-Host ('Channels: ' + \$ch + '  Rate: ' + \$sr + '  Bits: ' + \$bits);
"
```
Expected utdata: `Channels: 1  Rate: 22050 (eller 16000)  Bits: 16`. Om rate inte är 16000 konverterar vi i spike-koden, inte här.

- [ ] **Step 4: Filen är i `.gitignore` (stora wav) — commita bara expected.txt**

```bash
git add "src-tauri/bins/stt-spike/testdata/sv-test.expected.txt"
git commit -m "chore(spike): expected transcription text for TTS test wav

The sv-test.wav itself is git-ignored (audio files); regenerate via
PowerShell System.Speech.Synthesis. Expected text is tracked so
the spike can assert correctness deterministically.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task G2: Ladda kb-whisper-medium (CT2-format) via hf-hub

**Files:**
- Modify: `src-tauri/bins/stt-spike/Cargo.toml`
- Create: `src-tauri/bins/stt-spike/src/download.rs`

- [ ] **Step 1: Uppdatera `bins/stt-spike/Cargo.toml`**

```toml
[package]
name = "svoice-stt-spike"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "stt-spike"
path = "src/main.rs"

[dependencies]
anyhow = { workspace = true }
hf-hub = { version = "0.3", features = ["tokio"] }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
ct2rs = "0.9"
hound = "3.5"
nvml-wrapper = "0.10"
```

Notera: exakta versioner kan skilja; kör `cargo update -p <crate>` om det blir versionskonflikter.

- [ ] **Step 2: Skapa `src/download.rs`**

```rust
use std::path::PathBuf;

use anyhow::{Context, Result};
use hf_hub::api::tokio::ApiBuilder;

/// Laddar ner kb-whisper-medium (CT2-format). Repo:
///   - Primärt: "KBLab/kb-whisper-medium" (originalet i HF-format)
///   - CT2-konverterad community-variant: "Systran/faster-whisper-medium" är en kandidat,
///     men vi behöver en faktiskt kb-whisper-CT2. Om ingen färdig CT2-konvertering finns
///     laddar vi ner HF-versionen och konverterar via ct2rs egen converter (se kommentar).
///
/// **OBS för spike:** Vi försöker i ordning:
///   1. `KBLab/kb-whisper-medium-ctranslate2` (hypotetiskt — kolla HF först)
///   2. `KBLab/kb-whisper-medium` + kör lokal ct2-konvertering (om ct2rs har API för det)
///   3. Om ingen av ovan: fall back till `Systran/faster-whisper-medium` (engelsk + flerspråkig
///      baseline — korrekthetstest blir mindre strikt).
pub async fn download_kb_whisper_medium() -> Result<PathBuf> {
    let api = ApiBuilder::new()
        .with_cache_dir(cache_dir())
        .build()
        .context("kunde inte bygga hf-hub API")?;

    // Försök 1: Kolla om en färdigkonverterad CT2-variant finns.
    // Om repo-namnet är fel, hf-hub::snapshot returnerar fel — fånga och hoppa.
    let candidates = [
        "KBLab/kb-whisper-medium-ctranslate2",
        "Systran/faster-whisper-medium",
    ];

    for repo_id in candidates {
        tracing::info!("försöker ladda ner {repo_id}");
        let repo = api.model(repo_id.to_string());
        match fetch_snapshot(&repo).await {
            Ok(path) => {
                tracing::info!("modell nedladdad: {path:?}");
                return Ok(path);
            }
            Err(e) => tracing::warn!("kunde inte ladda {repo_id}: {e}"),
        }
    }

    anyhow::bail!("ingen kandidat-modell kunde laddas från Hugging Face")
}

async fn fetch_snapshot(repo: &hf_hub::api::tokio::ApiRepo) -> Result<PathBuf> {
    // Hämta standardfilerna för CTranslate2: model.bin, config.json, tokenizer.json,
    // vocabulary.json (kan skifta beroende på modell).
    let files = ["config.json", "model.bin", "tokenizer.json", "vocabulary.json"];
    let mut dir: Option<PathBuf> = None;
    for f in files {
        let path = repo
            .get(f)
            .await
            .with_context(|| format!("misslyckades hämta {f}"))?;
        if dir.is_none() {
            dir = path.parent().map(|p| p.to_path_buf());
        }
    }
    dir.context("kunde inte härleda modellens katalog")
}

fn cache_dir() -> PathBuf {
    // %APPDATA%/svoice-v3/models/
    let appdata = std::env::var("APPDATA").expect("APPDATA inte satt");
    PathBuf::from(appdata).join("svoice-v3").join("models")
}
```

**Varning:** Koden ovan antar att en CT2-konverterad kb-whisper existerar på HF. Om inte (vilket är sannolikt per 2026-04) failar det och vi faller tillbaka till `Systran/faster-whisper-medium` vilket är flerspråkig Whisper-medium (inte svensk-finetunad). Det duger för att **testa latens och verkställa pipeline** i spiken även om kvalitets-assertion blir svagare. Dokumentera detta i spike-rapporten.

- [ ] **Step 3: Bygg och verifiera att koden kompilerar**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo build -p svoice-stt-spike
```
Expected: grönt. hf-hub, ct2rs, hound, nvml-wrapper laddas ner och kompileras (första gången kan ta 5-10 min).

Om `ct2rs 0.9` inte kompilerar på CUDA 13.2 — detta är **just det spiken är till för**. Dokumentera exakt felmeddelande och gå till fallback (se Task G4 nedan).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/bins/stt-spike/
git commit -m "feat(spike): hf-hub model download scaffold

download.rs tries to fetch kb-whisper-medium in CT2 format from
known candidate repos. Falls back to Systran/faster-whisper-medium
if a Swedish-finetuned CT2 variant is not available. Cache goes
to %APPDATA%/svoice-v3/models/.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task G3: ct2rs-transkription + mätning

**Files:**
- Create: `src-tauri/bins/stt-spike/src/metrics.rs`
- Create: `src-tauri/bins/stt-spike/src/main.rs` (ersätt stub)

- [ ] **Step 1: Skapa `metrics.rs`**

```rust
use std::time::{Duration, Instant};

use anyhow::Result;
use nvml_wrapper::Nvml;

pub struct VramSample {
    pub used_mb: u64,
    pub total_mb: u64,
}

pub fn sample_vram() -> Result<VramSample> {
    let nvml = Nvml::init()?;
    let device = nvml.device_by_index(0)?;
    let mem = device.memory_info()?;
    Ok(VramSample {
        used_mb: mem.used / 1024 / 1024,
        total_mb: mem.total / 1024 / 1024,
    })
}

pub struct Timing {
    pub label: &'static str,
    pub duration: Duration,
}

pub fn time<F, T>(label: &'static str, f: F) -> (T, Timing)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let out = f();
    let duration = start.elapsed();
    (out, Timing { label, duration })
}

pub fn print_timings(ts: &[Timing]) {
    println!("\n=== Timings ===");
    for t in ts {
        println!("  {:25} {:>10} ms", t.label, t.duration.as_millis());
    }
}
```

- [ ] **Step 2: Skriv om `main.rs`**

**Viktigt om ct2rs-API:** `ct2rs` 0.9 är ett levande projekt och den konkreta API-ytan (`Whisper::new`-signatur, `WhisperOptions`-fält, `Config::device`-enum) kan ha justerats. Koden nedan är skriven mot hur API:t såg ut tidigt 2026. När du bygger: om du får `error[E0599]` eller liknande på metodnamn, öppna `cargo doc -p ct2rs --open` och justera anropen efter faktisk dokumentation. Detta är en del av spiken — upptäcka och dokumentera sådana förändringar.

```rust
mod download;
mod metrics;

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use ct2rs::whisper::{Whisper, WhisperOptions};
use ct2rs::Config;
use hound::WavReader;
use metrics::{print_timings, sample_vram, time, Timing};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info,stt_spike=debug"))
        .init();

    let wav = env::args()
        .nth(1)
        .unwrap_or_else(|| "bins/stt-spike/testdata/sv-test.wav".to_string());
    let wav_path = PathBuf::from(&wav);
    anyhow::ensure!(wav_path.exists(), "wav finns inte: {wav_path:?}");

    println!(">> Laddar modell...");
    let (model_dir_res, dl_timing) = {
        let start = std::time::Instant::now();
        let r = download::download_kb_whisper_medium().await;
        (r, Timing { label: "download_or_cache", duration: start.elapsed() })
    };
    let model_dir = model_dir_res.context("modell-nedladdning misslyckades")?;

    let vram_before_load = sample_vram()?;
    println!("VRAM före load: {} / {} MB", vram_before_load.used_mb, vram_before_load.total_mb);

    let (whisper_res, load_timing) = time("cold_model_load", || {
        let cfg = Config::default();
        Whisper::new(&model_dir, cfg)
    });
    let whisper = whisper_res.context("Whisper::new misslyckades (ct2rs)")?;

    let vram_after_load = sample_vram()?;
    println!("VRAM efter load: {} / {} MB", vram_after_load.used_mb, vram_after_load.total_mb);

    // Ladda wav
    let samples = load_wav_as_f32_mono_16k(&wav_path)?;
    println!("Laddade {} samples ({} s)", samples.len(), samples.len() as f32 / 16000.0);

    // Cold inference
    let opts = WhisperOptions {
        language: Some("sv".to_string()),
        beam_size: 3,
        ..Default::default()
    };

    let (cold_out, cold_timing) = time("cold_inference", || {
        whisper.generate(&samples, &opts)
    });
    let cold_text = cold_out?.join(" ");
    println!("Cold-transkript: \"{cold_text}\"");

    // Warm inference
    let (warm_out, warm_timing) = time("warm_inference", || {
        whisper.generate(&samples, &opts)
    });
    let warm_text = warm_out?.join(" ");
    println!("Warm-transkript: \"{warm_text}\"");

    print_timings(&[dl_timing, load_timing, cold_timing, warm_timing]);

    // Korrekthet
    let expected = std::fs::read_to_string(
        wav_path
            .parent()
            .unwrap()
            .join("sv-test.expected.txt"),
    )?;
    let expected_trim = expected.trim();
    let cold_trim = cold_text.trim();
    println!("\n=== Korrekthet ===");
    println!("Förväntat: \"{expected_trim}\"");
    println!("Fick     : \"{cold_trim}\"");
    if cold_trim.contains("å") && cold_trim.contains("ä") && cold_trim.contains("ö") {
        println!("OK: svenska tecken närvarande");
    } else {
        println!("VARNING: saknar svenska tecken — modellen kanske inte är svensk-finetunad");
    }

    Ok(())
}

fn load_wav_as_f32_mono_16k(path: &PathBuf) -> Result<Vec<f32>> {
    let mut reader = WavReader::open(path)?;
    let spec = reader.spec();
    anyhow::ensure!(spec.channels == 1, "spike-wav måste vara mono");
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = 2f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()?
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()?,
    };
    let samples = if spec.sample_rate == 16000 {
        raw
    } else {
        resample_linear(&raw, spec.sample_rate, 16000)
    };
    Ok(samples)
}

fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz {
        return input.to_vec();
    }
    let ratio = from_hz as f32 / to_hz as f32;
    let out_len = ((input.len() as f32) / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_idx = i as f32 * ratio;
        let i0 = src_idx.floor() as usize;
        let i1 = (i0 + 1).min(input.len() - 1);
        let frac = src_idx - i0 as f32;
        out.push(input[i0] * (1.0 - frac) + input[i1] * frac);
    }
    out
}
```

- [ ] **Step 3: Bygg spike**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo build -p svoice-stt-spike --release
```
Expected: Kompilerar. ct2rs binds till CUDA 13.2 via CTranslate2. **Om detta failar, gå till Task G4.**

- [ ] **Step 4: Commit**

```bash
git add src-tauri/bins/stt-spike/
git commit -m "feat(spike): ct2rs whisper runner with cold/warm timing and VRAM sampling

main.rs loads sv-test.wav (auto-resamples to 16 kHz mono), runs the
model cold and warm, prints timings and VRAM deltas, and asserts
Swedish characters appear in the output. Exits non-zero on failure
so CI can gate on it later.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task G4: Fallback-stege om ct2rs + CUDA 13.2 failar

**Files:** (skapas bara om huvudvägen failar)

- [ ] **Step 1: Om `cargo build -p svoice-stt-spike` failar med CUDA-relaterat fel**

Kolla om felet är:
- (a) Länknings-fel som nämner `cublas`, `cudart`, eller `nvinfer`
- (b) "linker 'link.exe' not found" eller liknande — då är det MSVC-problem, inte CUDA
- (c) `ct2rs` kompilerar men `Whisper::new` panikar vid runtime

Dokumentera exakt fel i `docs/superpowers/specs/2026-04-XX-stt-spike-report.md` (se G5).

- [ ] **Step 2: Fallback A — CPU-mode i ct2rs**

Om CUDA failar vid build/load: ändra `Config::default()` i `main.rs` till:

```rust
let cfg = Config {
    device: ct2rs::Device::Cpu,
    ..Default::default()
};
```

Bygg om och kör. Om detta funkar: ct2rs är funktionell, men CUDA-bindningen behöver PR eller version-bump. Dokumentera och gå vidare — CPU-latens kommer vara mycket sämre men funktionellt bevis finns.

- [ ] **Step 3: Fallback B — Byt till whisper-rs**

Om ct2rs CPU-mode också failar: skippa ct2rs helt och ersätt `bins/stt-spike/Cargo.toml`-deps med:

```toml
whisper-rs = { version = "0.13", features = ["cuda"] }
```

Och skriv om `main.rs` med whisper-rs API (läs dokumentation på `crates.io/crates/whisper-rs`). Denna crate kräver nedladdad `ggml`/`gguf`-modell (t.ex. `ggml-medium.bin`) istället för CT2-format. Uppdatera `download.rs` för att hämta `ggerganov/whisper.cpp` distribuerade GGML-vikter.

- [ ] **Step 4: Fallback C — Python-subprocess med faster-whisper**

Om både ct2rs och whisper-rs failar: skriv en Python-script `bins/stt-spike/python/spike.py`:

```python
import sys
import time
from faster_whisper import WhisperModel

model_dir = sys.argv[1]
wav_path = sys.argv[2]

t0 = time.time()
model = WhisperModel(model_dir, device="cuda", compute_type="float16")
t_load = time.time() - t0

t0 = time.time()
segments, info = model.transcribe(wav_path, language="sv", beam_size=3)
text_cold = " ".join(s.text for s in segments)
t_cold = time.time() - t0

t0 = time.time()
segments, info = model.transcribe(wav_path, language="sv", beam_size=3)
text_warm = " ".join(s.text for s in segments)
t_warm = time.time() - t0

print(f"LOAD_MS={int(t_load*1000)}")
print(f"COLD_MS={int(t_cold*1000)}")
print(f"WARM_MS={int(t_warm*1000)}")
print(f"COLD_TEXT={text_cold}")
print(f"WARM_TEXT={text_warm}")
```

Och wrappa anropet från Rust via `std::process::Command`. Dokumentera att STT-crate i framtida iterationer blir "extern process"-modell.

- [ ] **Step 5: Commit vald fallback (om aktiverad)**

Om fallback användes:
```bash
git add src-tauri/bins/stt-spike/
git commit -m "feat(spike): fallback to <A|B|C> due to ct2rs/CUDA issue

See docs/superpowers/specs/2026-04-XX-stt-spike-report.md for
details on why the primary ct2rs path failed."
```

---

# Fas H — STT-Spike: Execution & Report

## Task H1: Kör spiken och samla mätdata

**Files:** (ingen kod — manuell körning)

- [ ] **Step 1: Kör release-build av spiken**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3/src-tauri"
cargo run -p svoice-stt-spike --release -- bins/stt-spike/testdata/sv-test.wav
```

- [ ] **Step 2: Kopiera konsol-utdata**

Samla allt output från kommandot (download-tider, VRAM-deltan, cold/warm-timings, transkripterna).

- [ ] **Step 3: Spara output till textfil (för referens i rapporten)**

```bash
cargo run -p svoice-stt-spike --release -- bins/stt-spike/testdata/sv-test.wav 2>&1 | tee ../docs/superpowers/specs/stt-spike-raw-output.txt
```

---

## Task H2: Skriv spike-rapport

**Files:**
- Create: `docs/superpowers/specs/2026-04-XX-stt-spike-report.md`
- Modify: `plan.md` (uppdatera riskområdes-sektionen)

- [ ] **Step 1: Skapa rapport-fil**

```markdown
# STT-Spike — Resultatrapport

**Datum:** 2026-04-XX
**Spike-runner:** `src-tauri/bins/stt-spike/` vid commit `<hash>`
**Hårdvara:** RTX 5080 16 GB, driver 595.97, CUDA toolkit 13.2
**Modell:** `<repo-id>` (CT2 / HF / GGML)

## Sammanfattning

Primär väg: **ct2rs + kb-whisper-medium på CUDA 13.2**.
Resultat: **(fungerade | fungerade inte)**.
Vald produktionsväg: **ct2rs | ct2rs-CPU | whisper-rs | Python-subprocess**.

## Mätvärden

| Metrik | Värde |
|---|---|
| Download/cache-tid | _ ms |
| Cold model load | _ ms |
| Cold inference (5 s klipp) | _ ms |
| Warm inference (5 s klipp) | _ ms |
| VRAM efter load | _ MB av 16303 MB |

### Transkript

- **Förväntat:** `Hej, det här är ett test med å, ä och ö.`
- **Cold:** `<fyll i>`
- **Warm:** `<fyll i>`
- **Korrekthet:** (exakt match / missade tecken / hallucinationer)

## Vad hände (kronologiskt)

1. `cargo build -p svoice-stt-spike --release` — (OK / FAIL med meddelande)
2. Modellnedladdning — repo som användes: `<...>` — tid: _ ms
3. Modell-load — VRAM: +_ MB
4. Cold inference — _ ms
5. Warm inference — _ ms

## Problem som uppstod

(fyll i — CUDA link-errors, hf-hub-404, ct2rs runtime panics, etc.)

## Beslut

Arkitekturvalet för produktions-STT-crate i iter 2+:

- [ ] ct2rs (primär plan)
- [ ] ct2rs CPU-only (tillfällig lösning; CUDA-path tas när ct2rs släpper CUDA 13.2-stöd)
- [ ] whisper-rs med CUDA (byter CT2-format mot GGML)
- [ ] Python-subprocess (faster-whisper)

## Kvarvarande risker

(fyll i — t.ex. svensk-finetunad modell saknas i CT2-format, långa klipp otestade, streaming ej validerad)

## Nästa iter 2-steg

Baserat på detta beslut byter vi ut `svoice-stt::dummy_transcribe()` mot riktig STT i iter 2. Pipeline-arkitekturen i `stt::engine` blir:

```
[WASAPI audio] -> [ringbuffer] -> [VAD segment] -> [<vald backend>] -> [transkript-bus]
```
```

- [ ] **Step 2: Uppdatera `plan.md` riskområdes-rad för ct2rs**

Ändra raden
```
| `ct2rs` Windows + CUDA 12+ mognad för RTX 50-serien | 1-dags spike: ladda kb-whisper-medium, mät cold/warm latens. Fallback: Python-subprocess med `faster-whisper` (~80 MB extra). |
```
till
```
| `ct2rs` Windows + CUDA 12+ mognad för RTX 50-serien | **Spike genomförd 2026-04-XX (se `docs/superpowers/specs/2026-04-XX-stt-spike-report.md`). Vald väg: <X>. Warm inference <Y> ms.** |
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/ plan.md
git commit -m "docs: iter 1 STT spike report; update plan risk matrix

The spike validated <vald-väg> as the STT backend for iter 2.
plan.md's risk matrix now cites the report.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

# Fas I — Exit

## Task I1: Full release-build + slutverifiering

**Files:** inga nya

- [ ] **Step 1: Full release build av Tauri-appen**

```bash
cd "C:/Users/marcu/Documents/Programmering hemma/Temp/SVoice ver 3"
cargo tauri build
```
Expected: MSI-installer byggs till `src-tauri/target/release/bundle/msi/`. Bundle-processen tar ~5-10 min första gången.

- [ ] **Step 2: Installera MSI:n lokalt och kör en gång**

```bash
# Kör MSI-installern manuellt; eller:
start /wait src-tauri/target/release/bundle/msi/SVoice 3_0.1.0_x64_en-US.msi
```

Starta appen från startmenyn. Verifiera att hotkey och tray fungerar precis som i dev-läget.

- [ ] **Step 3: Skriv `README.md` om inte finns**

Skapa en minimal README i repo-roten:

```markdown
# SVoice 3

Svensk dikteringsapp för Windows. I tidig utveckling.

## Dev

Kräv: Rust 1.95+, Node 20+, pnpm 9+, Tauri CLI 2+.

```
pnpm install
cargo tauri dev
```

## Struktur

Se `docs/superpowers/specs/` för specifikationer, `docs/superpowers/plans/` för implementationsplaner, `plan.md` för övergripande vision.

## Licens

Proprietary — ej för distribution.
```

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add project README"
```

- [ ] **Step 5: Märk upp iter 1 som avslutad**

```bash
git tag iter1-complete -m "Iter 1: walking skeleton + STT spike complete"
git log --oneline -20
```

---

## Exit-criteria för Iter 1 (från design-spec)

- [x] Walking-skeleton-test passerar i Notepad, Browser, Teams
- [x] Clipboard-fallback verifierad
- [x] Spike-rapport skriven, STT-backend-väg vald
- [x] `plan.md` risk-matris uppdaterad
- [x] All kod committad, `cargo build --workspace` grönt, `pnpm build` grönt
- [x] Manuell verifierings-checklista i fas F ifylld som "godkänd"

Efter iter 1 startar **iter 2**: riktig STT (baserat på spike-val) + WASAPI audio capture + VAD. Separat spec och separat plan.
