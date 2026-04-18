//! Smart-functions: återanvändbara LLM-prompts som användaren kan triggerar
//! via command palette eller kontextmeny.
//!
//! Varje function är en JSON-fil i `%APPDATA%/svoice-v3/smart_functions/`.
//! Appen seedar 5 svenska defaults vid första start (om mappen är tom).
//!
//! Format:
//! ```json
//! {
//!   "id": "correct-grammar-sv",
//!   "name": "Rätta grammatik",
//!   "description": "Rättar grammatik + stavning i markerad text.",
//!   "mode": "transform",
//!   "system": "Du är en redaktör...",
//!   "user_template": "{selection}"
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartMode {
    /// Kräver markerad text i target-appen. `user_template` interpolerar
    /// `{selection}` och resultatet ersätter markeringen.
    Transform,
    /// Fri query — `user_template` interpolerar `{command}` (user-input).
    /// Om ingen command ges används function's name som prompt.
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartFunction {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mode: SmartMode,
    pub system: String,
    #[serde(default = "default_template")]
    pub user_template: String,
}

fn default_template() -> String {
    "{selection}".into()
}

#[derive(Debug, thiserror::Error)]
pub enum SfError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-parse av {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
}

/// Default-mapp: `%APPDATA%/svoice-v3/smart_functions/`.
pub fn default_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").expect("APPDATA");
    PathBuf::from(appdata)
        .join("svoice-v3")
        .join("smart_functions")
}

/// Lista alla functions i mappen. Ogiltiga JSON-filer loggas men ignoreras.
pub fn list_from(dir: &Path) -> Result<Vec<SmartFunction>, SfError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("kan inte läsa {}: {e}", path.display());
                continue;
            }
        };
        match serde_json::from_str::<SmartFunction>(&raw) {
            Ok(sf) => out.push(sf),
            Err(e) => {
                tracing::warn!("skipar ogiltig smart-function {}: {e}", path.display());
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn list() -> Result<Vec<SmartFunction>, SfError> {
    list_from(&default_dir())
}

/// Seedar default-functions i `dir` om mappen är tom eller saknas.
/// Idempotent — kör vid varje app-start, skriver bara om filerna saknas.
pub fn seed_defaults(dir: &Path) -> Result<(), SfError> {
    std::fs::create_dir_all(dir)?;
    for sf in bundled_defaults() {
        let path = dir.join(format!("{}.json", sf.id));
        if path.exists() {
            continue;
        }
        let json = serde_json::to_string_pretty(&sf).expect("serialize default");
        std::fs::write(&path, json)?;
        tracing::info!("seedad smart-function: {}", sf.id);
    }
    Ok(())
}

/// De 5 defaults som följer med appen. User kan radera dem manuellt
/// (de seedas bara om filen saknas, inte om den är raderad + tom mapp).
pub fn bundled_defaults() -> Vec<SmartFunction> {
    vec![
        SmartFunction {
            id: "correct-grammar-sv".into(),
            name: "Rätta grammatik".into(),
            description: "Korrigerar grammatik, stavning och interpunktion i markerad svensk text utan att ändra stil eller tone.".into(),
            mode: SmartMode::Transform,
            system: "Du är en professionell svensk redaktör. Din uppgift är att rätta grammatik, stavning och interpunktion i den givna texten. Bibehåll originalets ton, stil och ordval. Returnera ENDAST den rättade texten — ingen förklaring, ingen markdown.".into(),
            user_template: "{selection}".into(),
        },
        SmartFunction {
            id: "summarize-sv".into(),
            name: "Sammanfatta".into(),
            description: "Sammanfattar markerad text till 2–3 meningar på svenska.".into(),
            mode: SmartMode::Transform,
            system: "Du är en koncis redaktör. Sammanfatta den givna texten till 2–3 meningar på svenska. Fokusera på huvudbudskapet. Returnera bara sammanfattningen, ingen inledning.".into(),
            user_template: "{selection}".into(),
        },
        SmartFunction {
            id: "translate-sv-en".into(),
            name: "Översätt svenska → engelska".into(),
            description: "Översätter markerad svensk text till naturlig engelska.".into(),
            mode: SmartMode::Transform,
            system: "Du är en svensk-engelsk översättare. Översätt den givna svenska texten till naturlig, idiomatisk engelska. Bibehåll tonen (formell vs informell). Returnera bara den engelska översättningen.".into(),
            user_template: "{selection}".into(),
        },
        SmartFunction {
            id: "rewrite-formal-sv".into(),
            name: "Gör mer formell".into(),
            description: "Omformulerar markerad text till mer formell svenska (t.ex. för e-post eller dokument).".into(),
            mode: SmartMode::Transform,
            system: "Du är en språkstilist. Skriv om den givna texten till en mer formell svenska — lämplig för professionell e-post eller dokument. Bibehåll innehållet exakt. Returnera bara den omformulerade texten.".into(),
            user_template: "{selection}".into(),
        },
        SmartFunction {
            id: "rewrite-casual-sv".into(),
            name: "Gör mer avslappnad".into(),
            description: "Omformulerar markerad text till mer avslappnad svenska (t.ex. för chat eller Slack).".into(),
            mode: SmartMode::Transform,
            system: "Du är en språkstilist. Skriv om den givna texten till en mer avslappnad, vardaglig svenska — lämplig för chat, SMS eller Slack. Bibehåll innehållet exakt men gör tonen lättsammare. Returnera bara den omformulerade texten.".into(),
            user_template: "{selection}".into(),
        },
    ]
}

/// Interpolerar user_template med selection + command. Tomma ersättningar
/// om källan saknas.
pub fn build_user_prompt(template: &str, selection: Option<&str>, command: Option<&str>) -> String {
    template
        .replace("{selection}", selection.unwrap_or(""))
        .replace("{command}", command.unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn defaults_seed_into_empty_dir() {
        let dir = tempdir().unwrap();
        seed_defaults(dir.path()).unwrap();
        let list = list_from(dir.path()).unwrap();
        assert_eq!(list.len(), 5);
        let ids: Vec<_> = list.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"correct-grammar-sv"));
        assert!(ids.contains(&"translate-sv-en"));
    }

    #[test]
    fn seed_is_idempotent() {
        let dir = tempdir().unwrap();
        seed_defaults(dir.path()).unwrap();
        // Modifiera en fil — seed ska inte skriva över.
        let path = dir.path().join("correct-grammar-sv.json");
        std::fs::write(&path, "{\"id\":\"x\",\"name\":\"CUSTOM\",\"description\":\"\",\"mode\":\"query\",\"system\":\"\",\"user_template\":\"\"}").unwrap();
        seed_defaults(dir.path()).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("CUSTOM"), "seed skrev över befintlig fil!");
    }

    #[test]
    fn list_skips_non_json_and_corrupt() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("not-a-function.txt"), "plain text").unwrap();
        std::fs::write(dir.path().join("corrupt.json"), "{ not json }").unwrap();
        std::fs::write(
            dir.path().join("valid.json"),
            r#"{"id":"a","name":"A","description":"","mode":"query","system":"","user_template":"{command}"}"#,
        )
        .unwrap();
        let list = list_from(dir.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "a");
    }

    #[test]
    fn build_user_prompt_interpolates() {
        assert_eq!(
            build_user_prompt("hej {selection}", Some("världen"), None),
            "hej världen"
        );
        assert_eq!(
            build_user_prompt("{command}: {selection}", Some("text"), Some("korta")),
            "korta: text"
        );
        assert_eq!(build_user_prompt("fast text", None, None), "fast text");
    }

    #[test]
    fn missing_dir_returns_empty_list() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        let list = list_from(&missing).unwrap();
        assert!(list.is_empty());
    }
}
