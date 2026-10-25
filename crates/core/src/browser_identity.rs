use anyhow::{ensure, Result};

/// The GitHub and Mozilla distributions share the same local protocol.
pub fn firefox_ids() -> Result<Vec<String>> {
    let ids: Vec<String> = serde_json::from_str(include_str!(
        "../../../native-host/firefox-extension-ids.json"
    ))?;
    ensure!(
        !ids.is_empty()
            && ids.len() <= 8
            && ids.iter().all(|id| !id.is_empty()
                && id.len() <= 255
                && !id.chars().any(char::is_whitespace)),
        "Invalid packaged Firefox extension IDs"
    );
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    ensure!(unique.len() == ids.len(), "Duplicate Firefox extension ID");
    Ok(ids)
}
