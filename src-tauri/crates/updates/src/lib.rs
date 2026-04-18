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
        .timeout(std::time::Duration::from_secs(15))
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
        // Trunca långa release-notes till ~2000 bytes men respektera
        // UTF-8 char-boundaries så svenska å/ä/ö i notes inte panik:ar
        // string-slicing.
        if b.len() > 2000 {
            let mut end = 2000;
            while !b.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            format!("{}…", &b[..end])
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

/// Returnerar cached om recent, annars hämtar färskt och returnerar det.
/// Används av auto-check-pathen för att undvika att blanda cache-logik i
/// caller-koden.
pub async fn check_latest_cached_fallback() -> Result<UpdateStatus, UpdateError> {
    if let Some(cached) = cached_recent() {
        return Ok(cached);
    }
    check_latest().await
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

    #[test]
    fn release_notes_truncation_respects_char_boundaries() {
        // Skapa release-notes > 2000 bytes där byte 2000 råkar landa mitt
        // i ett flerbyte-tecken (ö = 2 bytes i UTF-8). Utan char-boundary-
        // hantering panik:ar string-slicing.
        let mut body = String::new();
        for _ in 0..500 {
            body.push_str("aöaö"); // 4 chars, 6 bytes
        }
        assert!(body.len() > 2000);
        // Simulera mapping-logiken i check_latest
        let truncated = if body.len() > 2000 {
            let mut end = 2000;
            while !body.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            format!("{}…", &body[..end])
        } else {
            body
        };
        // Ska inte panika — bara bekräfta att resultatet är valid UTF-8
        // och slutar med ellipsen.
        assert!(truncated.ends_with("…"));
    }
}
