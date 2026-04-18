// Tillfälligt recon-verktyg: listar tillgängliga Gemini-modeller.
// Körs med: cargo run -p svoice-llm --example list_gemini_models

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let key = svoice_secrets::get_gemini_key()?
        .ok_or_else(|| anyhow::anyhow!("Ingen Gemini-nyckel i keyring"))?;
    let client = reqwest::Client::new();
    let resp = client
        .get("https://generativelanguage.googleapis.com/v1beta/models")
        .header("x-goog-api-key", &key)
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        eprintln!("HTTP {}: {}", status, body);
        return Ok(());
    }
    let v: serde_json::Value = serde_json::from_str(&body)?;
    let models = v["models"].as_array().cloned().unwrap_or_default();
    for m in models {
        let name = m["name"].as_str().unwrap_or("");
        if !name.contains("gemini") {
            continue;
        }
        let methods = m["supportedGenerationMethods"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let desc = m["description"].as_str().unwrap_or("");
        println!("{name}  [{methods}]  — {desc}");
    }
    Ok(())
}
