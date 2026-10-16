use anyhow::{ensure, Result};
use std::time::Duration;

pub async fn latest_version() -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut response = client
        .get("https://github.com/Hactor00/Flux/releases/latest/download/release-info.json")
        .header(reqwest::header::USER_AGENT, "Flux update checker")
        .send()
        .await?
        .error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            body.len() + chunk.len() <= 65536,
            "Release metadata exceeds the size limit"
        );
        body.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&body)?;
    let version = value["version"].as_str().unwrap_or_default();
    ensure!(
        crate::browser_connection::valid_version(version),
        "Invalid release version"
    );
    Ok(version.to_owned())
}
