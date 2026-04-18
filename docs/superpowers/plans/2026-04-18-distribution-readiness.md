# Distribution-readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fyra oberoende UX/distribution-förbättringar som gör SVoice 3 redo att delas med kompisar — click-outside grace-period, update-check mot GitHub Releases, autostart-reinforce, lazy-download av KB-Whisper.

**Architecture:** Alla fyra rör lib.rs / ipc-crate / frontend Settings och körs som sekventiella faser. Fas 1-3 är backend-tyngda; Fas 4 rör både backend (Python-sidecar + Tauri-bundling) och frontend (dropdown + progress). Varje fas avslutas med commit + (Fas 4) MSI-rebuild.

**Tech Stack:** Rust (Tauri 2, tokio, winreg, semver, reqwest), TypeScript (React), Python (huggingface_hub för modell-download).

**Spec:** `docs/superpowers/specs/2026-04-18-distribution-readiness-design.md`

**Git-state:** `main` @ `a042f6c` (spec commit). Arbetar direkt på main med user:s explicit consent (samma setup som Gemini-plan).

---

## Fas 1: Click-outside grace-period på action-popup

**Mål:** Popupen försvinner inte om user råkar klicka utanför medan den streamar.

**Kritiska filer:**
- Modifiera: [`src-tauri/src/lib.rs`](../../../src-tauri/src/lib.rs) — popup-streaming-flagga, focus-lost-handler, reset-punkter

### Task 1.1: Lägg till streaming-flagga + start/end-hooks

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Lägg till atomisk flagga i lib.rs top-level statics**

Lägg till efter `PALETTE_SELECTION`-statisken:
```rust
/// Sätts `true` från att första `action_llm_token` emittas tills 500 ms efter
/// `action_llm_done`. Under denna period skippas click-outside-hide så user
/// inte tappar ett pågående (eller nyss-levererat) svar genom att oavsiktligt
/// klicka på skrivbordet.
static ACTION_POPUP_STREAMING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
```

- [ ] **Step 2: Wrap `emit_event` för action_llm_token så flaggan sätts**

I `handle_action_released` (both fresh + follow-up paths) samt `run_agentic_gemini` + `run_agentic`: byt direkta `emit_event(..., EV_ACTION_LLM_TOKEN, ...)`-anrop i lib.rs → en ny helper:
```rust
fn emit_action_token(app: &AppHandle, text: String) {
    ACTION_POPUP_STREAMING.store(true, std::sync::atomic::Ordering::SeqCst);
    emit_event(app, EV_ACTION_LLM_TOKEN, ActionToken { text });
}
```

Ersätt alla 3 förekomster i `lib.rs` (i streaming-path i `handle_action_released`, i `run_smart_function`) av `emit_event(..., EV_ACTION_LLM_TOKEN, ActionToken { text })` → `emit_action_token(&app_clone, text)`.

Agentic-pathen i `agentic.rs` emitter direkt via `app.emit(ev_token, json)`. Där: importera `ACTION_POPUP_STREAMING` från `crate::ACTION_POPUP_STREAMING` + sätt till true innan emit. Eller enklare: gör flaggan till `pub` så agentic.rs kan nå den.

Välj `pub static ACTION_POPUP_STREAMING: AtomicBool` och uppdatera i agentic.rs:

```rust
// agentic.rs, i run_agentic inuti StepOutcome::Finished och NeedTools text-grenar
// samt run_agentic_gemini GeminiEvent::Text-grenen:
crate::ACTION_POPUP_STREAMING.store(true, std::sync::atomic::Ordering::SeqCst);
let _ = app.emit(ev_token, serde_json::json!({ "text": text }));
```

- [ ] **Step 3: Schemalägg flag-clear 500 ms efter action_llm_done**

Kritiskt: `emit_event(&app_clone, EV_ACTION_LLM_DONE, ())` används på flera ställen. Ersätt med en helper som schemalägger clear:

```rust
// I lib.rs:
fn emit_action_done(app: &AppHandle) {
    emit_event(app, EV_ACTION_LLM_DONE, ());
    let app_clone = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        ACTION_POPUP_STREAMING.store(false, std::sync::atomic::Ordering::SeqCst);
        // Ingen explicit hide här — vi låter nästa focus-lost eller user-action hide:a.
        let _ = app_clone;
    });
}
```

Ersätt alla `emit_event(&app_clone, EV_ACTION_LLM_DONE, ())` i lib.rs med `emit_action_done(&app_clone)`. I agentic.rs — lätta vägen är att bara stora flaggan till `false` innan `app.emit(ev_done, ())`:

```rust
// agentic.rs, i båda run_agentic_* precis före final emit:
let _ = app.emit(ev_done, ());
// Grace-period hanteras i lib.rs — här kan vi låta lib.rs timing ta över
// eftersom ev_done är EV_ACTION_LLM_DONE som lib.rs också wrap:ar via
// emit_action_done vid vanlig streaming. För agentic: emit done + spawn
// clear-timer direkt:
let app_clone = app.clone();
tokio::spawn(async move {
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    crate::ACTION_POPUP_STREAMING.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = app_clone;
});
```

- [ ] **Step 4: Clear flaggan vid action_apply / action_cancel / follow-up-start**

`action_apply` och `action_cancel` är IPC-commands i `crates/ipc/src/commands.rs` — de rör inte lib.rs-statics direkt. Enklaste vägen: exponera en pub funktion i lib.rs som ipc-cratet kan inte importera (cross-crate). Istället, lägg till clear-logic där state rensas i lib.rs.

Lägg till en `pub fn clear_action_popup_streaming()` i lib.rs:
```rust
pub fn clear_action_popup_streaming() {
    ACTION_POPUP_STREAMING.store(false, std::sync::atomic::Ordering::SeqCst);
}
```

Men ipc-cratet kan inte importera från main-binary-cratet. Lösning: flytta statics till `crates/ipc/src/commands.rs` istället. Ändra:

```rust
// crates/ipc/src/commands.rs, top-level:
pub static ACTION_POPUP_STREAMING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
```

Exportera från `crates/ipc/src/lib.rs`:
```rust
pub use commands::{
    // ... existing exports ...
    ACTION_POPUP_STREAMING,
};
```

Uppdatera `action_apply` och `action_cancel` i commands.rs att rensa flaggan:
```rust
pub async fn action_apply(app: tauri::AppHandle, result: String) -> Result<(), String> {
    // ... befintlig kod ...
    clear_active_conversation();
    ACTION_POPUP_STREAMING.store(false, std::sync::atomic::Ordering::SeqCst);
    tracing::info!("action-popup: result applied via clipboard, conversation cleared");
    Ok(())
}

pub fn action_cancel(app: tauri::AppHandle) {
    // ... befintlig kod ...
    clear_active_conversation();
    ACTION_POPUP_STREAMING.store(false, std::sync::atomic::Ordering::SeqCst);
    tracing::debug!("action-popup: cancelled by user, conversation cleared");
}
```

Uppdatera alla lib.rs + agentic.rs-anrop att använda `svoice_ipc::ACTION_POPUP_STREAMING` istället för `crate::ACTION_POPUP_STREAMING` (eftersom flaggan flyttats).

- [ ] **Step 5: Gate focus-lost-handler på flaggan**

I `lib.rs setup()` där action-popup window-event-handler registreras (runt rad 490):

```rust
if let Some(popup) = app.get_webview_window("action-popup") {
    let popup_clone = popup.clone();
    popup.on_window_event(move |ev| {
        if let tauri::WindowEvent::Focused(false) = ev {
            // Skippa click-outside-hide medan popupen aktivt streamar eller
            // precis levererat ett svar (500 ms grace-period). Utan gate:en
            // kan user tappa ett pågående svar genom att oavsiktligt klicka
            // på skrivbordet.
            if svoice_ipc::ACTION_POPUP_STREAMING
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                tracing::debug!(
                    "action-popup: focus-lost ignorerad — streaming pågår"
                );
                return;
            }
            if popup_clone.is_visible().ok().unwrap_or(false) {
                let _ = popup_clone.hide();
                svoice_ipc::clear_active_conversation();
                tracing::debug!(
                    "action-popup: stängd via click-outside (focus lost)"
                );
            }
        }
    });
}
```

- [ ] **Step 6: Kompilerar + workspace-check**

Run: `cd src-tauri && cargo check --workspace`
Expected: `Finished` utan fel.

- [ ] **Step 7: Manuell verifiering**

Run: `pnpm tauri dev`
Manuella tester:
1. Insert-PTT + "förklara kvantfysik kort" → klicka på skrivbordet mitt i streaming → popup ska **stanna kvar**.
2. Vänta tills streaming klar → klicka utanför → popup ska stängas (efter 500 ms grace).
3. Insert-PTT → Esc → popup stängs direkt (grace ska inte påverka explicit close).
4. Insert-PTT → "Applicera" → popup stängs, result pastas. Verifiera att följande popup-session fungerar normalt.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/crates/ipc/src/commands.rs \
        src-tauri/crates/ipc/src/lib.rs \
        src-tauri/src/lib.rs \
        src-tauri/src/agentic.rs
git commit -m "feat(action-popup): grace-period för click-outside under streaming"
```

---

## Fas 2: Update-check mot GitHub Releases

**Mål:** Användare kan se/manuell-checka om en ny SVoice-version finns, auto-check vid app-start med 24 h cache.

**Kritiska filer:**
- Skapa: `src-tauri/crates/updates/` (ny crate)
- Modifiera: `src-tauri/crates/ipc/src/commands.rs` (ny IPC-command), `src-tauri/src/lib.rs` (spawn auto-check + invoke_handler), `src-tauri/Cargo.toml` + workspace members
- Modifiera frontend: `src/lib/settings-api.ts`, `src/windows/Settings.tsx` (ny sektion)

### Task 2.1: Skapa `svoice-updates`-crate

**Files:**
- Create: `src-tauri/crates/updates/Cargo.toml`
- Create: `src-tauri/crates/updates/src/lib.rs`

- [ ] **Step 1: Skapa crate-struktur**

Skapa `src-tauri/crates/updates/Cargo.toml`:
```toml
[package]
name = "svoice-updates"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
semver = "1"
chrono = { version = "0.4", default-features = false, features = ["clock", "serde"] }
```

Lägg till i `src-tauri/Cargo.toml` workspace members (efter `crates/stt`):
```toml
    "crates/updates",
```

Lägg till som dep i `src-tauri/Cargo.toml` root-package (efter `svoice-smart-functions`):
```toml
svoice-updates = { path = "crates/updates" }
```

- [ ] **Step 2: Skriv UpdateStatus + check-funktion**

Skapa `src-tauri/crates/updates/src/lib.rs`:
```rust
//! GitHub Releases update-checker för SVoice 3.
//!
//! Hämtar senaste release via GitHub's public API (ingen auth → 60 req/h
//! per IP), jämför semver mot `CARGO_PKG_VERSION` och returnerar en
//! `UpdateStatus` som UI:n renderar som card i Settings → Översikt.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const REPO_OWNER: &str = "stegbergmarcus";
const REPO_NAME: &str = "SVoice-ver-3";
const USER_AGENT: &str = concat!(
    "SVoice3-UpdateCheck/",
    env!("CARGO_PKG_VERSION")
);
/// Minsta tid mellan automatiska checker (manuell check respekterar inte).
const AUTO_CHECK_COOLDOWN_HOURS: i64 = 24;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("HTTP-fel: {0}")]
    Http(String),
    #[error("GitHub API-fel {status}: {body}")]
    Api { status: u16, body: String },
    #[error("kunde inte tolka version: {0}")]
    InvalidVersion(String),
    #[error("cache-fel: {0}")]
    Cache(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub available: bool,
    pub download_url: Option<String>,
    pub release_notes: Option<String>,
    pub checked_at: i64,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn cache_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(appdata)
        .join("svoice-v3")
        .join("update-check.json")
}

/// Returnerar Some(cached) om senaste check är inom cooldown, annars None.
pub fn cached_recent() -> Option<UpdateStatus> {
    let body = std::fs::read_to_string(cache_path()).ok()?;
    let cached: UpdateStatus = serde_json::from_str(&body).ok()?;
    let now = chrono::Utc::now().timestamp();
    let age_hours = (now - cached.checked_at) / 3600;
    if age_hours < AUTO_CHECK_COOLDOWN_HOURS {
        Some(cached)
    } else {
        None
    }
}

fn save_cache(status: &UpdateStatus) -> Result<(), UpdateError> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| UpdateError::Cache(e.to_string()))?;
    }
    let json = serde_json::to_string_pretty(status)
        .map_err(|e| UpdateError::Cache(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| UpdateError::Cache(e.to_string()))
}

/// Trimma ev. `v`-prefix från GitHub-tag ("v0.2.0" → "0.2.0").
fn normalize_tag(tag: &str) -> &str {
    tag.trim_start_matches(|c: char| c == 'v' || c == 'V')
}

pub async fn check_latest() -> Result<UpdateStatus, UpdateError> {
    let url = format!(
        "https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"
    );
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| UpdateError::Http(e.to_string()))?;

    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?;

    let status = resp.status();
    let current = env!("CARGO_PKG_VERSION").to_string();

    // 404 = "no releases yet" — inte ett fel, bara "available: false".
    if status.as_u16() == 404 {
        let st = UpdateStatus {
            current_version: current,
            latest_version: None,
            available: false,
            download_url: None,
            release_notes: None,
            checked_at: chrono::Utc::now().timestamp(),
        };
        let _ = save_cache(&st);
        return Ok(st);
    }

    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UpdateError::Api {
            status: status.as_u16(),
            body,
        });
    }

    let rel: GithubRelease = resp
        .json()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?;

    let latest_raw = normalize_tag(&rel.tag_name).to_string();
    let cur_sem = semver::Version::parse(&current)
        .map_err(|e| UpdateError::InvalidVersion(format!("current: {e}")))?;
    let lat_sem = semver::Version::parse(&latest_raw)
        .map_err(|e| UpdateError::InvalidVersion(format!("latest: {e}")))?;

    let available = lat_sem > cur_sem;
    let download_url = rel
        .assets
        .into_iter()
        .find(|a| a.name.to_lowercase().ends_with(".msi"))
        .map(|a| a.browser_download_url);

    let notes = rel.body.map(|b| {
        // Trunca långa release-notes till 2000 tecken.
        if b.len() > 2000 {
            format!("{}…", &b[..2000])
        } else {
            b
        }
    });

    let st = UpdateStatus {
        current_version: current,
        latest_version: Some(latest_raw),
        available,
        download_url,
        release_notes: notes,
        checked_at: chrono::Utc::now().timestamp(),
    };
    let _ = save_cache(&st);
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_v_prefix() {
        assert_eq!(normalize_tag("v0.1.0"), "0.1.0");
        assert_eq!(normalize_tag("V0.1.0"), "0.1.0");
        assert_eq!(normalize_tag("0.1.0"), "0.1.0");
    }

    #[test]
    fn cache_path_ends_with_json() {
        assert!(cache_path().to_string_lossy().ends_with("update-check.json"));
    }
}
```

- [ ] **Step 3: Workspace-check**

Run: `cd src-tauri && cargo check -p svoice-updates`
Expected: `Finished` utan fel.

- [ ] **Step 4: Kör enhetstester**

Run: `cd src-tauri && cargo test -p svoice-updates --lib`
Expected: 2 tests passed.

### Task 2.2: IPC-command + invoke_handler-registrering

**Files:**
- Modify: `src-tauri/crates/ipc/src/commands.rs`
- Modify: `src-tauri/crates/ipc/src/lib.rs`
- Modify: `src-tauri/crates/ipc/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Lägg till svoice-updates dep i ipc-cratet**

I `src-tauri/crates/ipc/Cargo.toml`, lägg till under `[dependencies]`:
```toml
svoice-updates = { path = "../updates" }
```

- [ ] **Step 2: Lägg till IPC-command i commands.rs**

Lägg till i slutet av `src-tauri/crates/ipc/src/commands.rs`:
```rust
// ───────── Update-check ─────────

/// Query GitHub Releases API efter senaste publicerade version av SVoice 3.
/// Jämför semver mot `CARGO_PKG_VERSION` och returnerar struktur med
/// `available: bool` + download-URL. Resultatet cachas 24 h i
/// `%APPDATA%/svoice-v3/update-check.json`.
#[tauri::command]
pub async fn check_for_updates() -> Result<svoice_updates::UpdateStatus, String> {
    svoice_updates::check_latest()
        .await
        .map_err(|e| format!("update-check: {e}"))
}

/// Variant av `check_for_updates` som returnerar cachat resultat om det finns
/// inom cooldown-fönstret, annars hämtar färskt. Anropas vid app-start för att
/// undvika att trigga GitHub-API varje gång appen öppnas.
#[tauri::command]
pub async fn check_for_updates_cached() -> Result<svoice_updates::UpdateStatus, String> {
    if let Some(cached) = svoice_updates::cached_recent() {
        return Ok(cached);
    }
    svoice_updates::check_latest()
        .await
        .map_err(|e| format!("update-check: {e}"))
}
```

- [ ] **Step 3: Exportera från lib.rs**

Uppdatera `src-tauri/crates/ipc/src/lib.rs`:
```rust
pub use commands::{
    action_apply, action_cancel, action_followup_start, action_followup_stop, append_assistant_turn,
    append_user_turn, check_for_updates, check_for_updates_cached, check_hf_cached,
    clear_active_conversation, clear_anthropic_key, clear_gemini_key, clear_groq_key,
    get_settings, google_connect, google_connection_status, google_disconnect, has_anthropic_key,
    has_gemini_key, has_groq_key, list_mic_devices, list_ollama_models, list_smart_functions,
    open_smart_functions_dir, pull_ollama_model, set_active_conversation, set_anthropic_key,
    set_gemini_key, set_groq_key, set_settings, snapshot_conversation, sync_autostart,
    ACTION_POPUP_STREAMING, ActiveConversation, GoogleStatus, InjectResult, PttStateReport,
    FOLLOWUP_START_REQUESTED, FOLLOWUP_STOP_REQUESTED,
};
```

- [ ] **Step 4: Registrera commands i invoke_handler**

I `src-tauri/src/lib.rs`, i `invoke_handler![...]`-listan, lägg till (sorterat):
```rust
svoice_ipc::check_for_updates,
svoice_ipc::check_for_updates_cached,
```

- [ ] **Step 5: Kompilerar**

Run: `cd src-tauri && cargo check --workspace`
Expected: `Finished` utan fel.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml \
        src-tauri/crates/updates/ \
        src-tauri/crates/ipc/Cargo.toml \
        src-tauri/crates/ipc/src/commands.rs \
        src-tauri/crates/ipc/src/lib.rs \
        src-tauri/src/lib.rs
git commit -m "feat(updates): update-check mot GitHub Releases (backend)"
```

### Task 2.3: Auto-check vid app-start

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Spawn auto-check 10 sek efter setup**

Lägg till efter `audio-owner`-thread spawn men innan `let ptt_cb: LlCallback = ...` i `setup()`-closuren:

```rust
// Auto-check för ny version 10 sek efter setup. Använder cached resultat
// om senaste check är <24 h gammal så vi inte hamrar GitHub API vid varje
// app-start. Bara trayballon-notifikation — aldrig blockande UI.
let update_app = app_handle.clone();
rt.spawn(async move {
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    match svoice_updates::check_latest_cached_fallback().await {
        Ok(status) if status.available => {
            if let Some(latest) = &status.latest_version {
                tracing::info!("ny version {latest} tillgänglig");
                use tauri_plugin_notification::NotificationExt;
                if let Err(e) = update_app
                    .notification()
                    .builder()
                    .title("SVoice 3 — uppdatering tillgänglig")
                    .body(format!("Version {latest} är nu släppt. Öppna Settings för nedladdning."))
                    .show()
                {
                    tracing::debug!("update-notis failade: {e}");
                }
            }
        }
        Ok(_) => tracing::debug!("update-check: du kör senaste versionen"),
        Err(e) => tracing::debug!("update-check misslyckades (no-op): {e}"),
    }
});
```

- [ ] **Step 2: Lägg till `check_latest_cached_fallback` helper i updates-crate**

I `src-tauri/crates/updates/src/lib.rs`, lägg till efter `cached_recent()`:

```rust
/// Returnerar cached om recent, annars hämtar färskt och returnerar det.
/// Används av auto-check-pathen för att undvika att blanda cache-logik i
/// caller-koden.
pub async fn check_latest_cached_fallback() -> Result<UpdateStatus, UpdateError> {
    if let Some(cached) = cached_recent() {
        return Ok(cached);
    }
    check_latest().await
}
```

- [ ] **Step 3: Lägg till svoice-updates dep i root Cargo.toml om ej redan**

Verifiera att `svoice-updates = { path = "crates/updates" }` finns under `[dependencies]` i `src-tauri/Cargo.toml`. Om inte: lägg till.

- [ ] **Step 4: Kompilerar + worspace-test**

Run: `cd src-tauri && cargo check --workspace && cargo test -p svoice-updates --lib`
Expected: allt passerar.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/updates/src/lib.rs src-tauri/src/lib.rs
git commit -m "feat(updates): auto-check 10s efter app-start med 24h cache"
```

### Task 2.4: Frontend Settings-sektion

**Files:**
- Modify: `src/lib/settings-api.ts`
- Modify: `src/windows/Settings.tsx`

- [ ] **Step 1: Lägg till TS-bindningar**

I `src/lib/settings-api.ts`, lägg till efter `GoogleStatus`-interfacet:
```ts
export interface UpdateStatus {
  current_version: string;
  latest_version: string | null;
  available: boolean;
  download_url: string | null;
  release_notes: string | null;
  checked_at: number;
}

export async function checkForUpdates(): Promise<UpdateStatus> {
  return invoke<UpdateStatus>("check_for_updates");
}

export async function checkForUpdatesCached(): Promise<UpdateStatus> {
  return invoke<UpdateStatus>("check_for_updates_cached");
}
```

- [ ] **Step 2: Lägg till state + effect i Settings.tsx**

I `src/windows/Settings.tsx`, lägg till efter importen av `setGroqKey`:
```ts
import {
  // ... existing imports ...
  checkForUpdatesCached,
  checkForUpdates,
  type UpdateStatus,
} from "../lib/settings-api";
```

Lägg till state i `SettingsView`-komponenten efter `smartFns`-state:
```ts
const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
const [updateChecking, setUpdateChecking] = useState(false);
const [updateError, setUpdateError] = useState<string | null>(null);
```

Lägg till effect i `useEffect(() => { ... }, [])` (första one):
```ts
checkForUpdatesCached()
  .then(setUpdateStatus)
  .catch((e) => console.debug("[settings] update-check (cached) failed:", e));
```

Lägg till handler efter `handleGoogleDisconnect`:
```ts
async function handleCheckUpdates() {
  setUpdateChecking(true);
  setUpdateError(null);
  try {
    const status = await checkForUpdates();
    setUpdateStatus(status);
  } catch (e) {
    setUpdateError(String(e));
  } finally {
    setUpdateChecking(false);
  }
}
```

- [ ] **Step 3: Rendera version-card på Översikt-fliken**

I `Settings.tsx`, i `{activeTab === "overview" && (<>...</>)}`-blocket, efter `Kom igång`-sektionen men före `Moduler`-sektionen, lägg till:

```tsx
<article className="settings-section">
  <div className="settings-section-label">
    <h2>Version</h2>
    <p>
      SVoice 3 uppdateras via nya MSI-installer från GitHub Releases.
      Auto-check körs en gång per dygn.
    </p>
  </div>
  <div className="settings-section-body">
    <div
      style={{
        padding: "14px 16px",
        background: updateStatus?.available
          ? "rgba(212, 169, 85, 0.08)"
          : "rgba(243, 237, 227, 0.02)",
        border: updateStatus?.available
          ? "1px solid rgba(212, 169, 85, 0.28)"
          : "1px solid rgba(243, 237, 227, 0.06)",
        borderRadius: 12,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          marginBottom: 6,
        }}
      >
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: 5,
            background: updateStatus?.available
              ? "#d4a955"
              : updateStatus
                ? "#7bd37e"
                : "rgba(243, 237, 227, 0.3)",
          }}
        />
        <div style={{ fontWeight: 500 }}>
          {updateStatus?.available
            ? `Ny version ${updateStatus.latest_version} tillgänglig`
            : updateStatus
              ? `Version ${updateStatus.current_version} (senaste)`
              : "Kontrollerar version…"}
        </div>
        <div style={{ flex: 1 }} />
        {updateStatus?.available && updateStatus.download_url && (
          <a
            href={updateStatus.download_url}
            target="_blank"
            rel="noreferrer"
            className="btn btn-primary btn-compact"
            style={{ textDecoration: "none" }}
          >
            Ladda ner
          </a>
        )}
        <button
          type="button"
          className="btn btn-ghost btn-compact"
          onClick={handleCheckUpdates}
          disabled={updateChecking}
        >
          {updateChecking ? "Söker…" : "Sök uppdateringar"}
        </button>
      </div>
      {updateError && (
        <div
          style={{
            fontSize: 12,
            color: "var(--danger)",
            marginTop: 6,
          }}
        >
          {updateError}
        </div>
      )}
      {updateStatus?.release_notes && updateStatus.available && (
        <details style={{ marginTop: 10 }}>
          <summary
            style={{
              fontSize: 12,
              color: "var(--ink-tertiary)",
              cursor: "pointer",
            }}
          >
            Visa release-notes
          </summary>
          <pre
            style={{
              marginTop: 6,
              fontSize: 12,
              lineHeight: 1.5,
              whiteSpace: "pre-wrap",
              color: "var(--ink-secondary)",
              fontFamily: "var(--font-sans)",
            }}
          >
            {updateStatus.release_notes}
          </pre>
        </details>
      )}
    </div>
  </div>
</article>
```

- [ ] **Step 4: Frontend-typecheck**

Run: `pnpm tsc -b`
Expected: inga fel.

- [ ] **Step 5: Manuell verifiering (utan ny release)**

Run: `pnpm tauri dev`
1. Öppna Settings → Översikt → "Version"-kortet syns med "Version 0.1.0 (senaste)" + grön dot.
2. Klicka "Sök uppdateringar" → ska visa samma (eftersom ingen release finns än).
3. Skapa dummy-release på GitHub via web-UI:
   - Tag: `v0.99.0` (pre-release OK så kompisar inte ser den)
   - Asset: ladda upp en slumpmässig .msi-fil (eller hoppa över — då blir download_url null)
4. I appen: klicka "Sök uppdateringar" → ska nu visa "Ny version 0.99.0 tillgänglig" + Ladda ner-knapp (om asset finns).
5. Ta bort dummy-releasen från GitHub efter test.

- [ ] **Step 6: Commit**

```bash
git add src/lib/settings-api.ts src/windows/Settings.tsx
git commit -m "feat(updates): version-card på Settings-Översikt med auto + manuell check"
```

---

## Fas 3: Autostart-reinforce

**Mål:** Registry-entry för autostart är alltid konsistent med faktisk install-path, även efter reinstall från annan location.

**Kritiska filer:**
- Modifiera: `src-tauri/crates/ipc/src/commands.rs` (utvidga `sync_autostart`)
- Modifiera: `src-tauri/crates/ipc/Cargo.toml` (winreg-dep)

### Task 3.1: Diagnos + hypotes

**Files:**
- Ingen kodändring — dokumentation.

- [ ] **Step 1: Läs nuvarande registry-state**

Run: `powershell -NoProfile -Command "Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' | Select-Object 'SVoice 3' | Format-List"`

Förväntat: värdet är `"C:\Program Files\SVoice 3\svoice-v3.exe"` eller liknande. Dokumentera faktisk sträng.

- [ ] **Step 2: Granska `sync_autostart`-implementation**

Läs `src-tauri/crates/ipc/src/commands.rs:173-191` (`sync_autostart`). Bekräfta:
- `is_enabled()` returnerar bool oavsett path (plugin kollar bara entry-existens).
- Early-return `if currently == desired return Ok()` skippar rewrite om state redan matchar.
- Detta betyder: om registry-entryn existerar med fel path + settings.autostart=true → sync hoppar över och path stannar fel.

**Slutsats:** bug är latent. Triggas vid reinstall till ny path. Fix: force-rewrite när desired=true, oavsett is_enabled-state.

- [ ] **Step 3: Skriv ut hypotes i commit-body för Task 3.2**

(Ingen commit här — hypotesen dokumenteras i commit-message för nästa task.)

### Task 3.2: Reinforce-logik

**Files:**
- Modify: `src-tauri/crates/ipc/src/commands.rs`

- [ ] **Step 1: Refaktorera sync_autostart till force-rewrite**

Ersätt `sync_autostart` i `src-tauri/crates/ipc/src/commands.rs`:

```rust
/// Synka Windows startup-registret mot `desired`. När `desired=true` kör vi
/// alltid disable + enable (force-rewrite) även om plugin rapporterar
/// `is_enabled=true` — för annars kan registry-entryn peka på en gammal
/// install-path (från tidigare installation till annan mapp) och SVoice
/// startar inte vid inloggning efter reinstall. `enable()` på
/// `tauri-plugin-autostart` skriver fullständig path till nuvarande exe,
/// så rewrite garanterar aktuell path.
///
/// Exponerad på crate-nivå så lib.rs setup kan anropa den vid app-start
/// (idempotent: no-op om desired=false och registry redan är tom).
pub fn sync_autostart(app: &AppHandle, desired: bool) -> Result<(), String> {
    let mgr = app.autolaunch();
    let currently = mgr
        .is_enabled()
        .map_err(|e| format!("autolaunch is_enabled: {e}"))?;

    if !desired {
        // User vill ha autostart av — disable om entry finns, annars no-op.
        if currently {
            mgr.disable()
                .map_err(|e| format!("autolaunch disable: {e}"))?;
            tracing::info!("autostart inaktiverad i Windows registret");
        }
        return Ok(());
    }

    // desired=true: force-rewrite så registry alltid pekar på aktuell exe-path.
    // Om registry redan är rätt är detta en ~1ms no-op.
    if currently {
        // disable först så enable nedan skriver en fresh entry med rätt path.
        mgr.disable()
            .map_err(|e| format!("autolaunch disable för rewrite: {e}"))?;
    }
    mgr.enable()
        .map_err(|e| format!("autolaunch enable: {e}"))?;
    tracing::info!(
        "autostart aktiverad/reinforce:ad i Windows registret (currently_was={currently})"
    );
    Ok(())
}
```

- [ ] **Step 2: Kompilerar**

Run: `cd src-tauri && cargo check --workspace`
Expected: `Finished` utan fel.

- [ ] **Step 3: Manuell verifiering**

Run: `pnpm tauri dev`
1. Settings → Översikt → Moduler → slå PÅ "Starta automatiskt med Windows" → Spara.
2. Verifiera registry:
   `powershell -NoProfile -Command "Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' 'SVoice 3'"`
   Förväntat: värdet pekar på aktuell dev-build-exe (`target\debug\svoice-v3.exe`) eller prod-exe.
3. Stäng appen. Ändra registry-värdet manuellt till en fake path:
   `powershell -NoProfile -Command "Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' 'SVoice 3' 'C:\fake\path.exe'"`
4. Starta appen igen (tauri dev eller installerad) → sync_autostart:s reinforce-logik ska skriva över.
5. Verifiera registry igen → ska vara aktuell path, INTE `C:\fake\path.exe`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/ipc/src/commands.rs
git commit -m "$(cat <<'EOF'
fix(autostart): force-rewrite registry-path vid varje app-start

Tidigare early-return i sync_autostart (if currently == desired → no-op)
innebar att registry-entryn kunde peka på gammal install-path efter
reinstall till annan mapp — appen startade inte vid inloggning.

Nu: om desired=true kör vi alltid disable→enable-cycle så
tauri-plugin-autostart skriver fresh entry med aktuell exe-path. Om
värdet redan stämmer är det en ~1ms no-op. Loggning på info-nivå visar
"currently_was=true/false" för senare diagnos om problemet återkommer.

Manuellt verifierat genom att peta registry-värdet till fake-path och
starta om appen — värdet skrivs över korrekt.
EOF
)"
```

---

## Fas 4: Lazy-download av KB-Whisper

**Mål:** MSI-storlek från 1,4 GB → ~200 MB. Användare väljer modell i Settings och klickar "Ladda ner" explicit (ingen auto-download).

**Kritiska filer:**
- Modifiera: `src-tauri/tauri.conf.json` (ta bort modeller från resources)
- Modifiera: `src-tauri/resources/python/stt_sidecar.py` (ny download-only mode)
- Skapa: ny IPC-command `download_stt_model` + progress-events
- Modifiera: `src/windows/Settings.tsx` (ladda-ner-knapp + progressbar)
- Modifiera: `src/lib/settings-api.ts` (TS-bindningar)

### Task 4.1: Kontext (förhandsgranskad)

**Sidecar-protokoll** (bekräftat vid plan-skrivning):

- **Sidecar är persistent**: `Sidecar::spawn` startar Python-process, Python skickar `{"type":"ready"}` när redo.
- **Protocol**: `SttRequest`/`SttResponse` som tagged enums i `src-tauri/crates/stt/src/protocol.rs`. Python-sidecar:en i `src-tauri/resources/python/stt_sidecar.py` hanterar `load`, `transcribe`, `shutdown`.
- **Modeller cachas** via `faster_whisper.WhisperModel(repo_id)` som internt kallar `huggingface_hub.snapshot_download`. Cache ligger i `~/.cache/huggingface/hub/models--<org>--<model>/`.
- **IPC-mönster**: Rust skickar JSON-rad via stdin, läser JSON-rad från stdout. Audio går direkt som f32le-bytes efter `transcribe`-JSON.

**Design-beslut för download-action:**

- **Ny enum-variant**: `SttRequest::DownloadModel { model: String }` + `SttResponse::Downloaded { model, elapsed_ms }`.
- **Python använder `snapshot_download`** (faster-whisper stöder det via `WhisperModel(repo_id)` men utan rengöring — rent `snapshot_download` är tydligare semantik för "ladda ner men ladda inte i VRAM").
- **Progress**: `snapshot_download` saknar granular progress-callback, så vi emittar bara `download_start` innan och `Downloaded` efter. UI visar indeterminate progressbar.
- **Ingen VRAM-allocation** under download: bara disk-write, så det kan ske medan en annan modell är laddad.

(Ingen commit.)

### Task 4.2: Download-mode i Python-sidecar + protocol

**Files:**
- Modify: `src-tauri/crates/stt/src/protocol.rs`
- Modify: `src-tauri/resources/python/stt_sidecar.py`

- [ ] **Step 1: Utöka SttRequest/SttResponse med download-varianter**

I `src-tauri/crates/stt/src/protocol.rs`, lägg till varianter:

```rust
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttRequest {
    Load {
        model: String,
        device: String,
        compute_type: String,
        language: String,
    },
    Transcribe {
        audio_samples: u32,
        sample_rate: u32,
        beam_size: u32,
    },
    /// Be sidecar att ladda ner en HF-modell till disk-cache utan att
    /// ladda den i VRAM. Idempotent: no-op om modellen redan är komplett
    /// cachad. Efter download kan user byta till modellen via Load.
    DownloadModel {
        model: String,
    },
    Shutdown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SttResponse {
    Ready,
    Loaded {
        load_ms: u64,
        vram_used_mb: Option<u64>,
    },
    Transcript {
        text: String,
        inference_ms: u64,
        language: String,
        confidence: f32,
    },
    /// Sidecar har startat download — använd för UI "startar..."-status.
    /// Skickas direkt efter DownloadModel-request mottagen.
    DownloadStarted {
        model: String,
    },
    /// Download klar. `elapsed_ms` = tid från request till klar.
    Downloaded {
        model: String,
        elapsed_ms: u64,
    },
    Error {
        message: String,
        recoverable: bool,
    },
}
```

- [ ] **Step 2: Implementera download_model i Python-sidecar**

I `src-tauri/resources/python/stt_sidecar.py`, lägg till nytt case i `main()`-loopens `if/elif/else`-kedja. Efter `transcribe`-blocket, före `shutdown`:

```python
elif t == "download_model":
    repo_id = req.get("model")
    if not repo_id:
        send({"type": "error", "message": "download_model saknar 'model'", "recoverable": False})
        continue
    try:
        from huggingface_hub import snapshot_download
        send({"type": "download_started", "model": repo_id})
        t0 = time.perf_counter()
        # allow_patterns trimmar bort README/exempel-filer som inte behövs
        # för inferens → minst 10-20% mindre download per modell.
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
```

- [ ] **Step 3: Verifiera att huggingface_hub finns i bundled Python**

Run: `powershell -NoProfile -Command "& 'C:\Program Files\SVoice 3\python-runtime\python\python.exe' -c 'import huggingface_hub; print(huggingface_hub.__version__)'"`
Expected: en version-sträng (typiskt `0.20+`). Om `ModuleNotFoundError`: `huggingface_hub` är inte bundlat och måste läggas till Python-runtime-setupen. I så fall: notera som blocker och eskalera (utanför denna plan's scope — uppdatera python-runtime-build).

*Eftersom faster_whisper bundlas och faster_whisper **kräver** huggingface_hub som transitive dep, är det extremt sannolikt att den redan finns. Verifiera ändå.*

- [ ] **Step 4: Kompilerar**

Run: `cd src-tauri && cargo check -p svoice-stt`
Expected: `Finished` utan fel.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/stt/src/protocol.rs \
        src-tauri/resources/python/stt_sidecar.py
git commit -m "feat(stt): download_model-protokoll i sidecar för lazy HF-fetch"
```

### Task 4.3: PythonStt::download_model + IPC-command

**Files:**
- Modify: `src-tauri/crates/stt/src/engine.rs`
- Modify: `src-tauri/crates/ipc/src/commands.rs`
- Modify: `src-tauri/crates/ipc/src/lib.rs`
- Modify: `src-tauri/src/lib.rs` (invoke_handler)

- [ ] **Step 1: Lägg till PythonStt::download_model**

I `src-tauri/crates/stt/src/engine.rs`, lägg till efter `transcribe`-metoden:

```rust
/// Skicka DownloadModel-request till sidecar och läs sidecar-events tills
/// Downloaded eller Error mottas. `on_event`-callbacken anropas med
/// status-strängar för UI (`"startar"`, `"klar"`) — snapshot_download
/// saknar granular progress, så vi har bara start/done-markers.
///
/// Kräver att sidecar:n är spawnad (ensure_loaded). Om modellen inte är
/// laddad ännu används ensure_loaded, vilket kan ladda nuvarande config's
/// modell i VRAM — OK, download är orthogonal till load:en.
pub async fn download_model<F>(&self, model: &str, mut on_event: F) -> Result<(), SttError>
where
    F: FnMut(&str) + Send + 'static,
{
    self.ensure_loaded().await?;
    let guard = self.sidecar.lock().await;
    let sc = guard.as_ref().ok_or(SttError::NotLoaded)?;
    sc.send_request(&SttRequest::DownloadModel {
        model: model.to_string(),
    })
    .await?;
    // Läs events tills Downloaded eller Error. Vi kan få DownloadStarted
    // först (status-event), sen Downloaded som terminal.
    loop {
        match sc.read_response().await? {
            SttResponse::DownloadStarted { model: m } => {
                tracing::info!("STT download: start {m}");
                on_event("startar");
            }
            SttResponse::Downloaded { model: m, elapsed_ms } => {
                tracing::info!("STT download klar: {m} på {elapsed_ms} ms");
                on_event("klar");
                return Ok(());
            }
            SttResponse::Error { message, .. } => {
                return Err(SttError::Remote(message));
            }
            other => {
                return Err(SttError::Unexpected(format!(
                    "förväntade DownloadStarted/Downloaded, fick {other:?}"
                )));
            }
        }
    }
}
```

- [ ] **Step 2: IPC-command i commands.rs**

Lägg till i slutet av `src-tauri/crates/ipc/src/commands.rs`:

```rust
/// Starta download av HF-modell via Python-sidecar. Emittar
/// `stt_model_download_progress` (per status-event) och ett slutligt
/// `stt_model_download_done`-event + OS-notifikation. Returnerar när
/// download är klar (eller fel).
#[tauri::command]
pub async fn download_stt_model(
    app: AppHandle,
    model: String,
    stt: State<'_, Arc<PythonStt>>,
) -> Result<(), String> {
    let app_for_cb = app.clone();
    let model_for_cb = model.clone();
    stt.download_model(&model, move |status| {
        let _ = app_for_cb.emit(
            "stt_model_download_progress",
            serde_json::json!({ "model": &model_for_cb, "status": status }),
        );
    })
    .await
    .map_err(|e| format!("stt download failed: {e}"))?;
    let _ = app.emit(
        "stt_model_download_done",
        serde_json::json!({ "model": model }),
    );

    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app
        .notification()
        .builder()
        .title("SVoice")
        .body(format!("STT-modell nedladdad: {model}"))
        .show()
    {
        tracing::warn!("kunde inte visa notifikation: {e}");
    }
    Ok(())
}
```

- [ ] **Step 3: Exportera + registrera**

I `src-tauri/crates/ipc/src/lib.rs`, lägg till `download_stt_model` i `pub use commands::{...}`-listan.

I `src-tauri/src/lib.rs` `invoke_handler![...]`-listan, lägg till rad (sorterat alfabetiskt):
```rust
svoice_ipc::download_stt_model,
```

- [ ] **Step 4: Kompilerar**

Run: `cd src-tauri && cargo check --workspace`
Expected: `Finished` utan fel.

- [ ] **Step 5: Manuell verifiering via DevTools**

Run: `pnpm tauri dev`
1. Öppna DevTools för main-fönstret (rightklick → Inspect element).
2. Kör i console:
   ```js
   await window.__TAURI_INTERNALS__.invoke("download_stt_model", { model: "KBLab/kb-whisper-base" })
   ```
3. Förväntat: frontend blockeras 1-3 min medan modellen laddas ned. Terminal visar `STT download: start KBLab/kb-whisper-base` och sedan `STT download klar: ... på NNNN ms`. OS-notis "STT-modell nedladdad" dyker upp.
4. Verifiera cache: `ls "$env:USERPROFILE\.cache\huggingface\hub" | findstr kb-whisper-base` → mapp ska finnas.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/stt/src/engine.rs \
        src-tauri/crates/ipc/src/commands.rs \
        src-tauri/crates/ipc/src/lib.rs \
        src-tauri/src/lib.rs
git commit -m "feat(stt): IPC-command download_stt_model + PythonStt-metod"
```

### Task 4.4: Frontend — "Ladda ner"-knapp + progressbar

**Files:**
- Modify: `src/lib/settings-api.ts`
- Modify: `src/windows/Settings.tsx`

- [ ] **Step 1: TS-bindningar**

I `src/lib/settings-api.ts`, lägg till:
```ts
export interface SttModelDownloadProgress {
  model: string;
  status: string;
}

export async function downloadSttModel(model: string): Promise<void> {
  await invoke<void>("download_stt_model", { model });
}
```

- [ ] **Step 2: State + events i Settings.tsx**

Lägg till state efter `sttCached`:
```ts
const [sttDownload, setSttDownload] = useState<{
  model: string;
  status: string;
  done: boolean;
} | null>(null);
```

Lägg till event-listener i useEffect som redan hanterar Ollama-events (kopiera mönstret):
```ts
const unSttProgress = listen<SttModelDownloadProgress>(
  "stt_model_download_progress",
  (ev) => {
    setSttDownload({
      model: ev.payload.model,
      status: ev.payload.status,
      done: false,
    });
  },
);
const unSttDone = listen<{ model: string }>(
  "stt_model_download_done",
  (ev) => {
    setSttDownload({ model: ev.payload.model, status: "klar", done: true });
    setTimeout(() => setSttDownload(null), 2500);
    // Re-check HF-cache så dropdown-prefix uppdateras.
    MODELS.forEach(async (m) => {
      const cached = await checkHfCached(m.id).catch(() => false);
      setSttCached((prev) => ({ ...prev, [m.id]: cached }));
    });
  },
);
```

Lägg till `unSttProgress` + `unSttDone` i cleanup-returen på useEffect.

Lägg till handler-funktion:
```ts
async function handleDownloadStt(model: string) {
  setSttDownload({ model, status: "startar…", done: false });
  try {
    await downloadSttModel(model);
  } catch (e) {
    setError(`STT-download misslyckades: ${e}`);
    setSttDownload(null);
  }
}
```

- [ ] **Step 3: Modifiera lokal-modell-dropdownen i Audio-tab**

Hitta blocket `{draft.stt_provider === "local" && (...)}` i Settings.tsx. Lägg till "Ladda ner"-knapp + progressbar under dropdownen (mönstret är identiskt med Ollama-pull):

```tsx
{draft.stt_provider === "local" && (
  <>
    <div className="field">
      <label className="field-label" htmlFor="model">
        Lokal modell
      </label>
      <div className="field-with-action">
        <select
          id="model"
          className="select"
          value={draft.stt_model}
          onChange={(e) => setDraft({ ...draft, stt_model: e.target.value })}
        >
          {MODELS.map((m) => {
            const cached = sttCached[m.id];
            const prefix = cached === undefined ? "…" : cached ? "✓" : "↓";
            return (
              <option key={m.id} value={m.id}>
                {prefix} {m.label} — {m.note}
              </option>
            );
          })}
        </select>
        {(() => {
          const cached = sttCached[draft.stt_model];
          const downloading =
            sttDownload &&
            sttDownload.model === draft.stt_model &&
            !sttDownload.done;
          if (cached === undefined) return null;
          if (downloading) return null;
          if (cached) return <span className="field-badge ok">✓ nedladdad</span>;
          return (
            <button
              type="button"
              className="btn btn-primary btn-compact"
              onClick={() => handleDownloadStt(draft.stt_model)}
            >
              Ladda ner
            </button>
          );
        })()}
      </div>

      {sttDownload && sttDownload.model === draft.stt_model && (
        <div className="download-progress">
          <div className="download-progress-label">
            <span>{sttDownload.status}</span>
          </div>
          <div className="download-progress-bar">
            <div
              className="download-progress-fill"
              style={{ width: sttDownload.done ? "100%" : "45%" }}
            />
          </div>
        </div>
      )}

      <div className="field-help">
        💡 Rekommenderat minimum-VRAM: Base 1 GB · Medium 4 GB · Large 6 GB.
        CPU-fallback funkar men är 5-10× långsammare.
        {sttCached[draft.stt_model] === false && !sttDownload && (
          <>
            {" "}
            Modellen är inte nedladdad — klicka "Ladda ner" innan första
            användning (Base ~150 MB · Medium ~1,5 GB · Large ~3 GB).
          </>
        )}
      </div>
    </div>

    {/* ... befintlig Beräkningsläge-field oförändrad ... */}
  </>
)}
```

- [ ] **Step 4: Frontend-typecheck**

Run: `pnpm tsc -b`
Expected: inga fel.

- [ ] **Step 5: Manuell verifiering**

Run: `pnpm tauri dev`
1. Settings → Ljud & STT → välj KB-Whisper Base (om inte nedladdad).
2. "Ladda ner"-knapp ska synas. Klicka.
3. Progressbar med "startar…" → eventuellt text-status → "klar".
4. Prefix i dropdown byter från ↓ till ✓.
5. OS-notis "STT-modell nedladdad" ska visas.

- [ ] **Step 6: Commit**

```bash
git add src/lib/settings-api.ts src/windows/Settings.tsx
git commit -m "feat(stt): Ladda-ner-knapp + progressbar för KB-Whisper-modeller"
```

### Task 4.5: Ta bort modeller från Tauri-bundle

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Kolla om modeller faktiskt bundlas i nuvarande MSI**

Run: `powershell -NoProfile -Command "& 'C:\Program Files\SVoice 3' -Recurse -Include '*.bin','*.safetensors' | Select-Object FullName, Length"`

Om resultat: modeller bundlas i MSI och måste tas bort från resources.

Om tomt resultat: modeller laddas redan lazy via faster-whisper/HF-cache i user-home. MSI:ns storlek kommer från python-runtime. Hoppa Task 4.5 Step 2 och notera att "MSI-size-reduktionen redan är på plats, inget att göra för bundle-config".

**Sannolikt utfall:** modellerna ligger inte i resources/ (faster-whisper snapshot_download:ar till `~/.cache/huggingface/` första körning). Bundle-size handlar om python-runtime + dess embedded deps.

- [ ] **Step 2: (Om modeller bundlas)** Ta bort från tauri.conf.json

Redigera `src-tauri/tauri.conf.json` `bundle.resources`-sektionen:
```json
"resources": {
  "resources/python-runtime": "python-runtime",
  "resources/python/stt_sidecar.py": "python/stt_sidecar.py"
}
```
Ta bort eventuella HF-cache-paths som t.ex. `"resources/models": "models"`.

Verifiera genom att titta på `src-tauri/resources/`-strukturen.

- [ ] **Step 3: Commit (om Step 2 gjordes)**

```bash
git add src-tauri/tauri.conf.json
git commit -m "chore(bundle): ta bort pre-cachade HF-modeller ur MSI-resources"
```

### Task 4.6: MSI-rebuild + verifiering

**Files:**
- Ingen kodändring — verifiering.

- [ ] **Step 1: Rebuild MSI**

Run: `pnpm tauri build`
Expected: `Finished 1 bundle at: ...\SVoice 3_0.1.0_x64_en-US.msi`

- [ ] **Step 2: Kolla MSI-storlek**

Run: `powershell -NoProfile -Command "Get-Item 'src-tauri\target\release\bundle\msi\SVoice 3_0.1.0_x64_en-US.msi' | Select-Object Name, @{N='SizeMB';E={[math]::Round(\$_.Length / 1MB, 1)}}"`

Expected:
- Om Task 4.5 Step 2 utfördes: < 300 MB.
- Om modellerna inte var bundle:ade alls: storlek ungefär samma som före (1,4 GB), och vi noterar att "lazy-download är i UI men MSI-size-reduktionen kräver separat investigation av python-runtime-storlek".

- [ ] **Step 3: Reinstall + full verifiering**

Run: `powershell -NoProfile -Command "Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','C:\Users\marcu\AppData\Local\Temp\svoice-reinstall.ps1'"`

Starta app → kör end-to-end-test:
1. **Click-outside grace** (Fas 1): Insert-PTT långt svar → klicka utanför under streaming → popup stannar.
2. **Update-check** (Fas 2): Settings → Översikt → Version-card visar "Version 0.1.0 (senaste)".
3. **Autostart** (Fas 3): Settings → slå på autostart → Spara → verifiera registry pekar rätt.
4. **Lazy-download** (Fas 4): Settings → Ljud & STT → välj Base → klicka "Ladda ner" → progressbar → notis → STT fungerar.

- [ ] **Step 4: Final commit + push**

```bash
git push
```

Alla faser är då committade separat och pushade till main.

---

## Återanvänd

- **Ollama-pull-mönster**: `pull_ollama_model` + `ollama_pull_progress`-event — Fas 4 mirrar strukturen.
- **Notification-plugin**: redan initierad i `tauri::Builder` — används i Fas 2 och Fas 4.
- **Befintlig `check_hf_cached`**: används i Fas 4 för dropdown-rendering.
- **Befintlig `tauri-plugin-autostart`** — Fas 3 utvidgar `sync_autostart`.

## Gotchas

- **Fas 1**: `focus-lost` fires även när popup får fokus av sig själv via `win.set_focus()` i action_worker_loop. Bekräfta manuellt att popup-visning inte själv-triggar hide under streaming. Om det är ett problem: extra guard `is_visible == true`.
- **Fas 2**: GitHub API rate-limit är 60 req/h för unauthenticated IP. Auto-check 1 gång/dygn är långt under, manuella klick också — ingen risk.
- **Fas 2**: Om `tag_name` inte är valid semver (t.ex. `"pre-alpha"`) → `InvalidVersion`-fel i loggen men inget UI-toast. Acceptabelt för tidig testning.
- **Fas 3**: `tauri-plugin-autostart` version kan skilja mellan Windows-editioner (Home/Pro). Testa på båda om möjligt.
- **Fas 4**: Python-sidecarens exakta IPC-format bestäms av befintlig `transcribe`-implementation — Task 4.1 är kritisk för att få rätt mönster. Om sidecar är helt per-request (inte persistent), kan download-mode behöva egen process-spawn.
- **Fas 4**: `snapshot_download` har inte granular progress — progressbar visar bara "startar" → "klar". För fiksikt: använd `hf_hub_download` per fil + manuell räknare, men scope:a det som uppföljning.
