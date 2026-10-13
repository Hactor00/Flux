use crate::{http, model};
use anyhow::{ensure, Context, Result};
use futures_util::StreamExt;
use scraper::{Html, Selector};
use std::collections::HashSet;

/// Expands newline-separated URLs, one numeric range per line, or an HTTP directory wildcard.
pub async fn expand_batch(input: &str) -> Result<Vec<String>> {
    ensure!(input.len() <= 256 * 1024, "Batch input is too large");
    let numeric_range = regex::Regex::new(r"\[([0-9]+)-([0-9]+)\]")?;
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for line in input.lines().map(str::trim).filter(|s| !s.is_empty()) {
        let urls = if let Some(range) = numeric_range.captures(line) {
            let matched = range.get(0).context("Missing numeric range")?;
            let (left, right) = (matched.start(), matched.end());
            let (first, last) = (&range[1], &range[2]);
            let start: u32 = first.parse()?;
            let end: u32 = last.parse()?;
            ensure!(
                end >= start && end - start < 500,
                "Numeric range must contain at most 500 URLs"
            );
            let width = if first.starts_with('0') {
                first.len()
            } else {
                0
            };
            (start..=end)
                .map(|number| format!("{}{:0width$}{}", &line[..left], number, &line[right..]))
                .collect()
        } else if line.contains('*') || line.split('?').next().unwrap_or(line).contains('*') {
            directory_links(line).await?
        } else {
            vec![line.to_owned()]
        };
        for value in urls {
            model::validate_url(&value)?;
            if seen.insert(value.clone()) {
                output.push(value);
            }
            ensure!(output.len() <= 500, "Batch contains more than 500 URLs");
        }
    }
    ensure!(!output.is_empty(), "Batch contains no URLs");
    Ok(output)
}
async fn directory_links(pattern: &str) -> Result<Vec<String>> {
    let pattern_url = url::Url::parse(pattern)?;
    model::validate_url(pattern)?;
    let directory = pattern_url.join(".")?;
    let glob =
        globset::Glob::new(pattern_url.path().rsplit('/').next().unwrap_or("*"))?.compile_matcher();
    let response = http::client()?
        .get(directory.clone())
        .send()
        .await?
        .error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            bytes.len() + chunk.len() <= 1024 * 1024,
            "Directory listing exceeds one MiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    let page = Html::parse_document(&String::from_utf8_lossy(&bytes));
    let selector = Selector::parse("a[href]").map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut links = Vec::new();
    for anchor in page.select(&selector) {
        if let Some(href) = anchor.value().attr("href") {
            if let Ok(candidate) = directory.join(href) {
                if candidate.origin() == directory.origin()
                    && candidate.path().starts_with(directory.path())
                    && glob.is_match(candidate.path().rsplit('/').next().unwrap_or(""))
                {
                    links.push(candidate.to_string());
                    ensure!(
                        links.len() <= 500,
                        "Directory listing contains too many matching links"
                    );
                }
            }
        }
    }
    Ok(links)
}
