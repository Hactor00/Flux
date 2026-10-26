use crate::transport::{Client, RequestBuilder};
use anyhow::{ensure, Context, Result};
use reqwest::{header, Response, StatusCode};

pub(crate) const MAX_RETRIES: u32 = 7;

#[derive(Debug)]
pub(crate) struct Retryable {
    pub message: String,
    pub delay: u64,
}
impl std::fmt::Display for Retryable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Retryable {}
pub(crate) fn transient(response: &Response) -> Option<Retryable> {
    if response.status() != StatusCode::TOO_MANY_REQUESTS && !response.status().is_server_error() {
        return None;
    }
    let delay = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|h| h.to_str().ok())
        .map(|value| {
            value
                .parse()
                .ok()
                .or_else(|| {
                    httpdate::parse_http_date(value)
                        .ok()
                        .and_then(|date| date.duration_since(std::time::SystemTime::now()).ok())
                        .map(|delay| delay.as_secs().saturating_add(1))
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    Some(Retryable {
        message: format!("Server returned {}", response.status()),
        delay,
    })
}

#[derive(Debug)]
pub(crate) struct Metadata {
    pub content_type: String,
    pub total: Option<u64>,
    pub etag: Option<String>,
    pub ranged: bool,
    pub range_supported: bool,
}

pub(crate) fn client() -> Result<Client> {
    Ok(Client::new()?)
}
pub(crate) fn etag(response: &Response) -> Option<String> {
    response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .filter(|s| {
            s.len() <= 1024 && s.starts_with('"') && s.ends_with('"') && !s.starts_with("W/")
        })
        .map(str::to_owned)
}
pub(crate) fn identity(response: &Response) -> Result<()> {
    if let Some(v) = response.headers().get(header::CONTENT_ENCODING) {
        ensure!(v == "identity", "Encoded responses cannot be merged safely");
    }
    Ok(())
}
pub(crate) fn content_range(value: &str) -> Result<(u64, u64, u64)> {
    let value = value
        .strip_prefix("bytes ")
        .context("Invalid Content-Range unit")?;
    let (range, total) = value.split_once('/').context("Invalid Content-Range")?;
    let (start, end) = range.split_once('-').context("Invalid Content-Range")?;
    let (start, end, total) = (start.parse()?, end.parse()?, total.parse()?);
    ensure!(
        start <= end && end < total && total <= i64::MAX as u64,
        "Invalid Content-Range bounds"
    );
    Ok((start, end, total))
}
pub(crate) async fn probe(
    client: &Client,
    url: &str,
    referer: Option<&str>,
    credentials: &[crate::BrowserCredentials],
) -> Result<Metadata> {
    let mut request = authenticate(
        client
            .get(url)
            .header(header::RANGE, "bytes=0-0")
            .header(header::ACCEPT_ENCODING, "identity"),
        url,
        credentials,
    )?;
    if let Some(referer) = referer {
        request = request.header(header::REFERER, referer);
    }
    let mut response = request.send().await?;
    // Some signed-download servers reject Range even for valid browser sessions.
    // A successful plain GET is still safe; it forces a single-stream transfer.
    if matches!(
        response.status(),
        StatusCode::BAD_REQUEST
            | StatusCode::FORBIDDEN
            | StatusCode::METHOD_NOT_ALLOWED
            | StatusCode::NOT_IMPLEMENTED
    ) || (response.status() == StatusCode::SERVICE_UNAVAILABLE
        && !response.headers().contains_key(header::RETRY_AFTER))
    {
        let mut plain = authenticate(
            client.get(url).header(header::ACCEPT_ENCODING, "identity"),
            url,
            credentials,
        )?;
        if let Some(referer) = referer {
            plain = plain.header(header::REFERER, referer);
        }
        let fallback = plain.send().await?;
        if fallback.status().is_success() {
            response = fallback;
        }
    }
    if let Some(error) = transient(&response) {
        return Err(error.into());
    }
    identity(&response)?;
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE
        && response
            .headers()
            .get(header::CONTENT_RANGE)
            .is_some_and(|h| h == "bytes */0")
    {
        return Ok(Metadata {
            content_type,
            total: Some(0),
            etag: etag(&response),
            ranged: false,
            range_supported: false,
        });
    }
    response.error_for_status_ref()?;
    let tag = etag(&response);
    if response.status() == StatusCode::PARTIAL_CONTENT {
        let (start, end, total) = content_range(
            response
                .headers()
                .get(header::CONTENT_RANGE)
                .context("Missing Content-Range")?
                .to_str()?,
        )?;
        ensure!(
            start == 0 && end == 0,
            "Server returned an incorrect probe range"
        );
        ensure!(
            response.bytes().await?.len() == 1,
            "Invalid range probe body"
        );
        return Ok(Metadata {
            content_type,
            total: Some(total),
            ranged: true,
            range_supported: true,
            etag: tag,
        });
    }
    ensure!(
        response.status() == StatusCode::OK,
        "Unexpected probe status"
    );
    let total = response.content_length();
    ensure!(
        total.is_none_or(|size| size <= i64::MAX as u64),
        "File exceeds supported size"
    );
    Ok(Metadata {
        content_type,
        total,
        etag: tag,
        ranged: false,
        range_supported: false,
    })
}
pub(crate) async fn verify_checkpoint(
    client: &Client,
    input: &crate::DownloadRequest,
    range: (u64, u64, u64),
    expected: &str,
    cancel: &tokio_util::sync::CancellationToken,
    local_limit: &crate::limiter::Limiter,
    global_limit: &crate::limiter::Limiter,
) -> Result<()> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    let (start, length, total) = range;
    let end = start.checked_add(length).context("Checkpoint overflow")? - 1;
    let mut request = authenticate(
        client
            .get(&input.url)
            .header(header::ACCEPT_ENCODING, "identity")
            .header(header::RANGE, format!("bytes={start}-{end}")),
        &input.url,
        &input.credentials,
    )?;
    if let Some(referer) = &input.referer {
        request = request.header(header::REFERER, referer);
    }
    let response = tokio::select! {
        _ = cancel.cancelled() => anyhow::bail!("Cancelled"),
        response = request.send() => response.map_err(|error| Retryable { message: error.to_string(), delay: 0 })?,
    };
    if let Some(error) = transient(&response) {
        return Err(error.into());
    }
    response.error_for_status_ref()?;
    identity(&response)?;
    ensure!(
        response.status() == StatusCode::PARTIAL_CONTENT,
        "Server no longer supports checkpoint verification"
    );
    ensure!(
        content_range(
            response
                .headers()
                .get(header::CONTENT_RANGE)
                .context("Missing checkpoint Content-Range")?
                .to_str()?
        )? == (start, end, total),
        "Incorrect checkpoint Content-Range"
    );
    let mut stream = response.bytes_stream();
    let mut hash = Sha256::new();
    let mut received = 0u64;
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("Cancelled"),
            next = tokio::time::timeout(crate::watchdog::IDLE_TIMEOUT, stream.next()) => next.map_err(|_| Retryable { message: "Checkpoint verification stalled".into(), delay: 0 })?,
        };
        let Some(bytes) = next else {
            break;
        };
        let bytes = bytes.map_err(|error| Retryable {
            message: error.to_string(),
            delay: 0,
        })?;
        received = received
            .checked_add(bytes.len() as u64)
            .context("Checkpoint overflow")?;
        ensure!(received <= length, "Server sent too many checkpoint bytes");
        for slice in bytes.chunks(65536) {
            local_limit.acquire(slice.len(), cancel).await?;
            global_limit.acquire(slice.len(), cancel).await?;
            hash.update(slice);
        }
    }
    if received != length {
        return Err(Retryable {
            message: "Checkpoint response ended early".into(),
            delay: 0,
        }
        .into());
    }
    ensure!(
        hex::encode(hash.finalize()) == expected,
        "Remote file changed; saved download bytes have been preserved"
    );
    Ok(())
}
pub(crate) fn authenticate(
    mut request: RequestBuilder,
    url: &str,
    credentials: &[crate::BrowserCredentials],
) -> Result<RequestBuilder> {
    let origin = url::Url::parse(url)?.origin().ascii_serialization();
    if let Some(credentials) = credentials
        .iter()
        .find(|credentials| credentials.origin == origin)
    {
        if let Some(cookie) = &credentials.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        if let Some(authorization) = &credentials.authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        if let Some(user_agent) = &credentials.user_agent {
            request = request.header(header::USER_AGENT, user_agent);
        }
    }
    Ok(request)
}
