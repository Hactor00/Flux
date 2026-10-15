use crate::store::Store;
use anyhow::{bail, ensure, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    RealDebrid,
    AllDebrid,
    Premiumize,
}

impl Provider {
    pub const ALL: [Self; 3] = [Self::RealDebrid, Self::AllDebrid, Self::Premiumize];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::RealDebrid => "real_debrid",
            Self::AllDebrid => "all_debrid",
            Self::Premiumize => "premiumize",
        }
    }
    fn endpoint(self) -> &'static str {
        match self {
            Self::RealDebrid => "https://api.real-debrid.com/rest/1.0/",
            Self::AllDebrid => "https://api.alldebrid.com/v4/",
            Self::Premiumize => "https://www.premiumize.me/api/",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AccountStatus {
    pub provider: Provider,
    pub enabled: bool,
    pub configured: bool,
}

#[derive(Clone)]
pub(crate) struct DirectLink {
    pub url: String,
}

pub(crate) fn validate_key(key: &str) -> Result<()> {
    ensure!(
        key.len() <= 4096 && key.bytes().all(|byte| byte.is_ascii_graphic()),
        "API key must contain at most 4096 printable ASCII characters without spaces"
    );
    Ok(())
}

pub(crate) fn is_host_url(source: &str) -> bool {
    let Ok(url) = url::Url::parse(source) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    [
        "rapidgator.net",
        "rg.to",
        "1fichier.com",
        "mega.nz",
        "mega.co.nz",
        "turbobit.net",
        "turbobit.cc",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

pub(crate) struct Resolver {
    store: Store,
    client: reqwest::Client,
    cache: Mutex<HashMap<String, (DirectLink, Instant)>>,
    gate: Semaphore,
    pace: Mutex<Instant>,
    generation: AtomicU64,
    endpoints: [String; 3],
}

impl Resolver {
    pub fn new(store: Store) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            store,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(30))
                .user_agent(concat!("Flux/", env!("CARGO_PKG_VERSION")))
                .build()?,
            cache: Mutex::new(HashMap::new()),
            gate: Semaphore::new(2),
            pace: Mutex::new(Instant::now()),
            generation: AtomicU64::new(0),
            endpoints: Provider::ALL.map(|provider| provider.endpoint().into()),
        }))
    }
    pub async fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.cache.lock().await.clear();
    }
    pub async fn resolve(
        &self,
        source: &str,
        refresh: bool,
        cancel: &CancellationToken,
    ) -> Result<Option<DirectLink>> {
        if !is_host_url(source) {
            return Ok(None);
        }
        crate::model::validate_url(source)?;
        let generation = self.generation.load(Ordering::Acquire);
        let cached = self
            .cache
            .lock()
            .await
            .get(source)
            .filter(|(_, expires)| *expires > Instant::now())
            .map(|(link, _)| link.clone());
        if !refresh && cached.is_some() {
            return Ok(cached);
        }
        let _permit = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), permit = self.gate.acquire() => permit? };
        if !refresh {
            if let Some((link, expires)) = self.cache.lock().await.get(source) {
                if *expires > Instant::now() {
                    return Ok(Some(link.clone()));
                }
            }
        }
        let keys = self.store.host_keys().await?;
        if keys.is_empty() {
            return Ok(None);
        }
        let mut failures = Vec::new();
        for (provider, key) in keys {
            match self.unlock(provider, &key, source, cancel).await {
                Ok(link) => {
                    let mut cache = self.cache.lock().await;
                    cache.retain(|_, (_, expires)| *expires > Instant::now());
                    if cache.len() >= 256 {
                        cache.clear();
                    }
                    if generation == self.generation.load(Ordering::Acquire) {
                        cache.insert(
                            source.into(),
                            (link.clone(), Instant::now() + Duration::from_secs(300)),
                        );
                    }
                    return Ok(Some(link));
                }
                Err(error) => {
                    ensure!(!cancel.is_cancelled(), "Cancelled");
                    failures.push(format!("{}: {error}", provider.name()));
                }
            }
        }
        bail!(
            "Linked host accounts could not resolve this file: {}",
            failures.join("; ")
        )
    }
    async fn request(
        &self,
        provider: Provider,
        key: &str,
        path: &str,
        form: &[(&str, String)],
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let mut pace = self.pace.lock().await;
        let wait = pace.saturating_duration_since(Instant::now());
        tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), _ = tokio::time::sleep(wait) => {} }
        *pace = Instant::now() + Duration::from_millis(300);
        drop(pace);
        let request = self
            .client
            .post(format!(
                "{}{path}",
                self.endpoints[match provider {
                    Provider::RealDebrid => 0,
                    Provider::AllDebrid => 1,
                    Provider::Premiumize => 2,
                }]
            ))
            .bearer_auth(key)
            .form(form);
        let response = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), response = request.send() => response.map_err(|error| error.without_url())? };
        ensure!(
            response.status().is_success(),
            "Host API returned HTTP {}",
            response.status().as_u16()
        );
        ensure!(
            response
                .content_length()
                .is_none_or(|length| length <= 512 * 1024),
            "Host API response exceeds the size limit"
        );
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), chunk = stream.next() => chunk }
        {
            let chunk = chunk.map_err(|error| error.without_url())?;
            ensure!(
                body.len() + chunk.len() <= 512 * 1024,
                "Host API response exceeds the size limit"
            );
            body.extend_from_slice(&chunk);
        }
        let value: Value =
            serde_json::from_slice(&body).context("Host API returned invalid JSON")?;
        if value["status"] == "error" || value.get("error_code").is_some() {
            let code = value["error"]["code"]
                .as_str()
                .unwrap_or("REQUEST_REJECTED");
            let safe = match code {
                "AUTH_MISSING_APIKEY"
                | "AUTH_BAD_APIKEY"
                | "AUTH_BLOCKED"
                | "AUTH_USER_BANNED"
                | "LINK_IS_MISSING"
                | "LINK_HOST_NOT_SUPPORTED"
                | "LINK_DOWN"
                | "LINK_HOST_UNAVAILABLE"
                | "LINK_ERROR"
                | "LINK_PASS_PROTECTED"
                | "LINK_TOO_MANY_DOWNLOADS"
                | "LINK_TEMPORARY_UNAVAILABLE" => code,
                _ => "REQUEST_REJECTED",
            };
            bail!("Host API rejected the request ({safe})");
        }
        Ok(value)
    }
    async fn unlock(
        &self,
        provider: Provider,
        key: &str,
        source: &str,
        cancel: &CancellationToken,
    ) -> Result<DirectLink> {
        let (path, parameter) = match provider {
            Provider::RealDebrid => ("unrestrict/link", "link"),
            Provider::AllDebrid => ("link/unlock", "link"),
            Provider::Premiumize => ("transfer/directdl", "src"),
        };
        let mut value = self
            .request(provider, key, path, &[(parameter, source.into())], cancel)
            .await?;
        if provider == Provider::AllDebrid {
            if let Some(id) = value["data"]["delayed"].as_u64() {
                for _ in 0..60 {
                    tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), _ = tokio::time::sleep(Duration::from_secs(5)) => {} }
                    let delayed = self
                        .request(
                            provider,
                            key,
                            "link/delayed",
                            &[("id", id.to_string())],
                            cancel,
                        )
                        .await?;
                    match delayed["data"]["status"].as_u64() {
                        Some(2) => {
                            value = delayed;
                            break;
                        }
                        Some(1) => {}
                        _ => bail!("Host API could not generate the delayed link"),
                    }
                }
            }
        }
        parse_link(provider, &value)
    }
}

fn parse_link(provider: Provider, value: &Value) -> Result<DirectLink> {
    let link = match provider {
        Provider::RealDebrid => value["download"].as_str(),
        Provider::AllDebrid => value["data"]["link"].as_str(),
        Provider::Premiumize => {
            let files = value["content"]
                .as_array()
                .context("Host API returned no files")?;
            ensure!(
                files.len() == 1,
                "This host link contains multiple files; select a file-specific link"
            );
            files[0]["link"].as_str()
        }
    }
    .context("Host API returned no direct link")?;
    crate::model::validate_url(link)?;
    ensure!(
        url::Url::parse(link)?.scheme() == "https",
        "Host API direct links must use HTTPS"
    );
    Ok(DirectLink { url: link.into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn provider_http_contracts_cache_refresh_and_secret_boundaries() {
        use axum::{extract::State, http::HeaderMap, routing::post, Form, Json, Router};
        use std::sync::atomic::AtomicUsize;
        #[derive(Clone)]
        struct Calls {
            count: Arc<AtomicUsize>,
            provider: Provider,
        }
        async fn unlock(
            State(calls): State<Calls>,
            headers: HeaderMap,
            Form(fields): Form<HashMap<String, String>>,
        ) -> Json<Value> {
            assert_eq!(
                headers.get("authorization").unwrap(),
                "Bearer private-contract-key"
            );
            assert!(!headers.contains_key("cookie"));
            let parameter = if calls.provider == Provider::Premiumize {
                "src"
            } else {
                "link"
            };
            assert_eq!(fields[parameter], "https://rapidgator.net/file/123");
            let sequence = calls.count.fetch_add(1, Ordering::SeqCst);
            let link = format!("https://cdn.example/file?sequence={sequence}");
            Json(match calls.provider {
                Provider::RealDebrid => serde_json::json!({"download":link}),
                Provider::AllDebrid => serde_json::json!({"status":"success","data":{"link":link}}),
                Provider::Premiumize => {
                    serde_json::json!({"status":"success","content":[{"link":link}]})
                }
            })
        }
        for (index, provider) in Provider::ALL.into_iter().enumerate() {
            let root = tempfile::tempdir().unwrap();
            let store = Store::open(&root.path().join("tasks.sqlite")).unwrap();
            store
                .save_host_account(provider, true, Some("private-contract-key".into()))
                .await
                .unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let calls = Calls {
                count: Arc::new(AtomicUsize::new(0)),
                provider,
            };
            let path = match provider {
                Provider::RealDebrid => "/unrestrict/link",
                Provider::AllDebrid => "/link/unlock",
                Provider::Premiumize => "/transfer/directdl",
            };
            let application = Router::new()
                .route(path, post(unlock))
                .with_state(calls.clone());
            let server = tokio::spawn(async move {
                axum::serve(listener, application).await.unwrap();
            });
            let mut resolver = Resolver::new(store).unwrap();
            Arc::get_mut(&mut resolver).unwrap().endpoints[index] = format!("http://{address}/");
            let cancel = CancellationToken::new();
            let first = resolver
                .resolve("https://rapidgator.net/file/123", false, &cancel)
                .await
                .unwrap()
                .unwrap();
            let cached = resolver
                .resolve("https://rapidgator.net/file/123", false, &cancel)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(first.url, cached.url);
            assert_eq!(calls.count.load(Ordering::SeqCst), 1);
            let renewed = resolver
                .resolve("https://rapidgator.net/file/123", true, &cancel)
                .await
                .unwrap()
                .unwrap();
            assert_ne!(first.url, renewed.url);
            assert_eq!(calls.count.load(Ordering::SeqCst), 2);
            server.abort();
        }
    }
    #[test]
    fn provider_contracts_and_unsafe_targets() {
        for (provider, value) in [
            (
                Provider::RealDebrid,
                serde_json::json!({"download":"https://cdn.example/file"}),
            ),
            (
                Provider::AllDebrid,
                serde_json::json!({"status":"success","data":{"link":"https://cdn.example/file"}}),
            ),
            (
                Provider::Premiumize,
                serde_json::json!({"status":"success","content":[{"link":"https://cdn.example/file"}]}),
            ),
        ] {
            assert_eq!(
                parse_link(provider, &value).unwrap().url,
                "https://cdn.example/file"
            );
        }
        assert!(parse_link(
            Provider::RealDebrid,
            &serde_json::json!({"download":"file:///secret"})
        )
        .is_err());
        assert!(parse_link(
            Provider::RealDebrid,
            &serde_json::json!({"download":"https://key@cdn.example/file"})
        )
        .is_err());
        assert!(is_host_url("https://rapidgator.net/file/123"));
        assert!(!is_host_url("https://rapidgator.net.evil.test/file/123"));
        assert!(!is_host_url("https://example.com/file"));
        assert!(validate_key("secret\r\nInjected: value").is_err());
    }
}
