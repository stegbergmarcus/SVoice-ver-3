//! Self-contained Ollama-installation från SVoice-appen.
//!
//! Detektion + nedladdning + körning av Ollamas Windows-installer så user
//! slipper växla till webbläsaren. Installern är Inno Setup-baserad och
//! stöder `/SILENT` (visar bara progress) och `/VERYSILENT` (helt tyst);
//! vi kör `/SILENT` så user ser att något händer + UAC-prompten är ändå
//! oundviklig på Windows.
//!
//! På macOS/Linux returnerar [`detect_install`] alltid `Unsupported` och
//! [`install`] panic:ar — installern är tills vidare bara Windows-only.

#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;

/// Var ligger Ollamas installerade binär?
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallStatus {
    /// Vi hittade `ollama.exe` på vanlig plats. `path` är den absoluta
    /// sökvägen så frontend kan visa den om vi vill.
    Installed { path: String },
    /// Inget spår av Ollama på disk — user behöver klicka "Installera".
    NotInstalled,
    /// Plattform vi inte stödjer in-app-install på (macOS/Linux just nu).
    Unsupported { platform: String },
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("nedladdning misslyckades: {0}")]
    Download(String),
    #[error("kunde inte skriva temp-fil: {0}")]
    Io(String),
    #[error("kunde inte starta installer: {0}")]
    Spawn(String),
    #[error("installer avbröts eller misslyckades (exit-kod {0})")]
    InstallerFailed(i32),
    #[error("Ollama startade aldrig efter installation (timeout)")]
    PostInstallTimeout,
    #[error("plattformen stöds inte (just nu bara Windows)")]
    UnsupportedPlatform,
}

/// Progress-event under installation. Skickas via callback från [`install`].
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum InstallProgress {
    /// Vi börjar ladda ned installern.
    DownloadStarted { url: String },
    /// Vi fick en chunk. `total` kan vara `None` om servern inte gav
    /// `Content-Length`.
    DownloadProgress { downloaded: u64, total: Option<u64> },
    /// Nedladdning klar, kör installern (UAC-prompten visas nu).
    Installing,
    /// Installern är klar, vi väntar på att Ollama-servicen ska svara.
    WaitingForService,
    /// Allt klart — Ollama är redo att användas.
    Done { path: Option<String> },
}

/// Detektera om Ollama redan är installerat genom att kolla vanliga sökvägar.
/// Snabb (< 1 ms) — vi kollar bara filsystemets existens.
pub fn detect_install() -> InstallStatus {
    #[cfg(target_os = "windows")]
    {
        for candidate in windows_candidate_paths() {
            if candidate.exists() {
                return InstallStatus::Installed {
                    path: candidate.to_string_lossy().into_owned(),
                };
            }
        }
        InstallStatus::NotInstalled
    }
    #[cfg(not(target_os = "windows"))]
    {
        InstallStatus::Unsupported {
            platform: std::env::consts::OS.into(),
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_candidate_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    // Default-platsen för Ollamas Windows-installer (per-user install,
    // ingen admin krävs vid själva nedladdningen):
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        out.push(
            PathBuf::from(&local)
                .join("Programs")
                .join("Ollama")
                .join("ollama.exe"),
        );
    }
    // Fallback: machine-wide install (om någon kört som admin via MSI eller liknande).
    if let Ok(pf) = std::env::var("ProgramFiles") {
        out.push(PathBuf::from(&pf).join("Ollama").join("ollama.exe"));
    }
    out
}

/// Försök starta Ollama-tjänsten i bakgrunden. Föredrar tray-appen
/// (`ollama app.exe`) eftersom den både startar `ollama serve` och
/// sätter sig i system-tray:n; faller tillbaka till `ollama.exe serve`
/// om tray-binären saknas.
///
/// Detached process — ingen child wait, inget terminal-fönster blinkar
/// upp. Returnerar `Ok(true)` om en process spawnades, `Ok(false)` om
/// ingen binär hittades. Säger inget om huruvida tjänsten faktiskt kom
/// upp — caller får polla `/api/tags` för det.
#[cfg(target_os = "windows")]
pub fn try_autostart() -> std::io::Result<bool> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // CREATE_NO_WINDOW (0x08000000) + DETACHED_PROCESS (0x00000008) så
    // att en console-window inte flashar upp och så att processen
    // överlever även om SVoice avslutas.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let flags = CREATE_NO_WINDOW | DETACHED_PROCESS;

    let local = std::env::var("LOCALAPPDATA").ok();
    let pf = std::env::var("ProgramFiles").ok();

    // 1. Tray-appen — den föredragna entry-pointen (visas i system-tray).
    let tray_candidates: Vec<PathBuf> = [
        local.as_ref().map(|p| {
            PathBuf::from(p)
                .join("Programs")
                .join("Ollama")
                .join("ollama app.exe")
        }),
        pf.as_ref()
            .map(|p| PathBuf::from(p).join("Ollama").join("ollama app.exe")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in &tray_candidates {
        if path.exists() {
            Command::new(path)
                .creation_flags(flags)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            return Ok(true);
        }
    }

    // 2. Fallback: ollama.exe serve (headless, ingen tray-ikon men
    //    tjänsten kör).
    for path in windows_candidate_paths() {
        if path.exists() {
            Command::new(&path)
                .arg("serve")
                .creation_flags(flags)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(not(target_os = "windows"))]
pub fn try_autostart() -> std::io::Result<bool> {
    Ok(false)
}

/// URL för Ollamas Windows-installer. Använder den officiella latest-URL:en
/// från ollama.com (302-redirectar till GitHub Releases bakom kulisserna).
#[cfg(target_os = "windows")]
const WINDOWS_INSTALLER_URL: &str = "https://ollama.com/download/OllamaSetup.exe";

/// Ladda ned + kör Ollamas installer. På Windows: ladda ned `OllamaSetup.exe`,
/// spara i `%TEMP%`, kör med `/SILENT`. UAC-prompten är oundviklig (Windows-
/// krav för per-user install av en oerverifierad signatur).
///
/// `progress` anropas vid varje fas + ungefär en gång per 256 kB under
/// nedladdningen. Returnerar Ok när Ollama-servicen svarar på `/api/tags`
/// (= installation klar och redo).
#[cfg(target_os = "windows")]
pub async fn install<F>(mut progress: F) -> Result<InstallStatus, InstallError>
where
    F: FnMut(InstallProgress) + Send + 'static,
{
    progress(InstallProgress::DownloadStarted {
        url: WINDOWS_INSTALLER_URL.into(),
    });

    let temp_path = std::env::temp_dir().join("svoice-OllamaSetup.exe");
    download_to_file(WINDOWS_INSTALLER_URL, &temp_path, &mut progress).await?;

    progress(InstallProgress::Installing);
    run_installer(&temp_path)?;

    progress(InstallProgress::WaitingForService);
    wait_for_service("http://127.0.0.1:11434").await?;

    // Bästa-effort cleanup. Failar tyst — Windows städar %TEMP% ändå.
    let _ = std::fs::remove_file(&temp_path);

    let status = detect_install();
    let path = match &status {
        InstallStatus::Installed { path } => Some(path.clone()),
        _ => None,
    };
    progress(InstallProgress::Done { path });
    Ok(status)
}

#[cfg(not(target_os = "windows"))]
pub async fn install<F>(_progress: F) -> Result<InstallStatus, InstallError>
where
    F: FnMut(InstallProgress) + Send + 'static,
{
    Err(InstallError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
async fn download_to_file<F>(
    url: &str,
    dest: &std::path::Path,
    progress: &mut F,
) -> Result<(), InstallError>
where
    F: FnMut(InstallProgress),
{
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| InstallError::Download(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| InstallError::Download(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(InstallError::Download(format!(
            "HTTP {}: {}",
            resp.status(),
            resp.status().canonical_reason().unwrap_or("")
        )));
    }
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| InstallError::Io(e.to_string()))?;
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| InstallError::Download(e.to_string()))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| InstallError::Io(e.to_string()))?;
        downloaded += bytes.len() as u64;
        if downloaded - last_emit >= 256 * 1024 {
            progress(InstallProgress::DownloadProgress { downloaded, total });
            last_emit = downloaded;
        }
    }
    file.flush()
        .await
        .map_err(|e| InstallError::Io(e.to_string()))?;
    progress(InstallProgress::DownloadProgress { downloaded, total });
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_installer(path: &std::path::Path) -> Result<(), InstallError> {
    use std::process::Command;
    // `/SILENT` visar Inno Setup-progress utan att kräva nästa-knappar; user
    // ser ändå UAC-prompten. `/SUPPRESSMSGBOXES` gör att eventuella
    // bekräftelse-dialoger auto-accepteras.
    let status = Command::new(path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NOCANCEL"])
        .status()
        .map_err(|e| InstallError::Spawn(e.to_string()))?;
    if !status.success() {
        return Err(InstallError::InstallerFailed(status.code().unwrap_or(-1)));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
async fn wait_for_service(base_url: &str) -> Result<(), InstallError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| InstallError::Download(e.to_string()))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if let Ok(resp) = client.get(format!("{base_url}/api/tags")).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(InstallError::PostInstallTimeout)
}
