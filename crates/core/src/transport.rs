//! GET-only transport. QUIC is learned from a same-authority Alt-Svc advertisement.
use reqwest::{header, IntoUrl, Response, Version};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub(crate) struct Client {
    tcp: reqwest::Client,
    quic: reqwest::Client,
    origins: Arc<Mutex<HashMap<String, Route>>>,
    healing: Option<Healing>,
    policy: crate::TransportPreference,
    renewed: Arc<Mutex<HashMap<String, (crate::healing::Renewed, Instant)>>>,
}
#[derive(Clone)]
struct Healing {
    broker: Arc<crate::healing::RefreshBroker>,
    source: crate::BrowserSource,
    task_id: String,
    cancel: tokio_util::sync::CancellationToken,
}
struct Route {
    expires: Instant,
    blocked_until: Option<Instant>,
}
pub(crate) struct RequestBuilder {
    inner: reqwest::RequestBuilder,
    client: Client,
}

fn builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        // Redirects are handled identically for TCP and QUIC below.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("Flux/", env!("CARGO_PKG_VERSION")))
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
}
impl Client {
    pub fn new() -> reqwest::Result<Self> {
        Self::from_builders(builder(), builder())
    }
    fn from_builders(
        tcp: reqwest::ClientBuilder,
        quic: reqwest::ClientBuilder,
    ) -> reqwest::Result<Self> {
        Ok(Self {
            tcp: tcp.build()?,
            quic: quic
                .http3_prior_knowledge()
                .http3_max_idle_timeout(Duration::from_secs(30))
                .http3_max_field_section_size(64 * 1024)
                .build()?,
            origins: Arc::new(Mutex::new(HashMap::new())),
            healing: None,
            policy: crate::TransportPreference::Auto,
            renewed: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    pub fn with_healing(
        mut self,
        broker: Arc<crate::healing::RefreshBroker>,
        source: Option<&crate::BrowserSource>,
        task_id: &str,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Self {
        self.healing = source.map(|source| Healing {
            broker,
            source: source.clone(),
            task_id: task_id.into(),
            cancel: cancel.clone(),
        });
        self
    }
    pub fn with_policy(mut self, policy: crate::TransportPreference) -> Self {
        self.policy = policy;
        self
    }
    pub fn get(&self, url: impl IntoUrl) -> RequestBuilder {
        RequestBuilder {
            inner: self.tcp.get(url),
            client: self.clone(),
        }
    }
    fn execute(
        &self,
        mut request: reqwest::Request,
    ) -> futures_util::future::BoxFuture<'_, reqwest::Result<Response>> {
        // Do not embed the entire TCP/QUIC state machine into every IPC command future.
        Box::pin(async move {
            let mut healed = false;
            let original = format!(
                "{}:{}",
                self.healing
                    .as_ref()
                    .map(|context| context.task_id.as_str())
                    .unwrap_or(""),
                request.url()
            );
            let replacement = self
                .renewed
                .lock()
                .await
                .get(&original)
                .filter(|(_, until)| *until > Instant::now())
                .map(|(fresh, _)| fresh.clone());
            if let Some(fresh) = replacement {
                apply_refresh(&mut request, &fresh);
            }
            for redirect in 0..=9 {
                let origin = request.url().origin().ascii_serialization();
                let now = Instant::now();
                let route = self
                    .origins
                    .lock()
                    .await
                    .get(&origin)
                    .map(|route| (route.expires, route.blocked_until));
                let use_quic = request.url().scheme() == "https"
                    && self.policy != crate::TransportPreference::Tcp
                    && route.is_none_or(|(_, blocked)| blocked.is_none_or(|until| until <= now))
                    && (self.policy == crate::TransportPreference::Http3
                        || route.is_some_and(|(expires, _)| expires > now));
                let mut response = None;
                if use_quic {
                    let mut candidate = request
                        .try_clone()
                        .expect("GET requests have no streaming body");
                    *candidate.version_mut() = Version::HTTP_3;
                    if let Ok(Ok(value)) =
                        tokio::time::timeout(Duration::from_secs(3), self.quic.execute(candidate))
                            .await
                    {
                        response = Some(value);
                    } else {
                        let mut origins = self.origins.lock().await;
                        if origins.len() < 256 || origins.contains_key(&origin) {
                            origins
                                .entry(origin.clone())
                                .or_insert(Route {
                                    expires: now,
                                    blocked_until: None,
                                })
                                .blocked_until = Some(now + Duration::from_secs(60));
                        }
                    }
                }
                let response = match response {
                    Some(value) => value,
                    None => {
                        self.tcp
                            .execute(
                                request
                                    .try_clone()
                                    .expect("GET requests have no streaming body"),
                            )
                            .await?
                    }
                };
                self.learn(&response).await;
                if !healed && matches!(response.status().as_u16(), 401 | 403 | 410) {
                    if let Some(context) = &self.healing {
                        healed = true;
                        if let Ok(fresh) = context
                            .broker
                            .request(
                                &context.task_id,
                                &context.source,
                                request.url().as_str(),
                                &context.cancel,
                            )
                            .await
                        {
                            let mut renewed = self.renewed.lock().await;
                            renewed.retain(|_, (_, until)| *until > Instant::now());
                            if renewed.len() < 256 {
                                renewed.insert(
                                    original.clone(),
                                    (fresh.clone(), Instant::now() + Duration::from_secs(300)),
                                );
                            }
                            apply_refresh(&mut request, &fresh);
                            if redirect < 9 {
                                continue;
                            }
                        }
                    }
                }
                if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                    return Ok(response);
                }
                let Some(location) = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                else {
                    return Ok(response);
                };
                let Ok(next) = response.url().join(location) else {
                    return Ok(response);
                };
                // Return the redirect as a status error instead of following an unsafe target.
                if redirect >= 8
                    || crate::model::validate_url(next.as_str()).is_err()
                    || request.url().scheme() == "https" && next.scheme() != "https"
                {
                    return Ok(response);
                }
                if next.origin() != request.url().origin() {
                    for name in [
                        header::COOKIE,
                        header::AUTHORIZATION,
                        header::PROXY_AUTHORIZATION,
                        header::USER_AGENT,
                    ] {
                        request.headers_mut().remove(name);
                    }
                }
                *request.url_mut() = next;
            }
            unreachable!("redirect loop is bounded")
        })
    }
    async fn learn(&self, response: &Response) {
        if response.url().scheme() != "https" {
            return;
        }
        let Some(value) = response
            .headers()
            .get(header::ALT_SVC)
            .and_then(|v| v.to_str().ok())
        else {
            return;
        };
        let mut routes = self.origins.lock().await;
        let origin = response.url().origin().ascii_serialization();
        if value.trim() == "clear" {
            routes.remove(&origin);
            return;
        }
        let Some(age) = advertisement(value, response.url().port_or_known_default().unwrap_or(443))
        else {
            return;
        };
        if routes.len() >= 256 && !routes.contains_key(&origin) {
            routes.retain(|_, route| route.expires > Instant::now());
            if routes.len() >= 256 {
                return;
            }
        }
        let blocked_until = routes.get(&origin).and_then(|route| route.blocked_until);
        routes.insert(
            origin,
            Route {
                expires: Instant::now() + Duration::from_secs(age),
                blocked_until,
            },
        );
    }
}
fn apply_refresh(request: &mut reqwest::Request, fresh: &crate::healing::Renewed) {
    *request.url_mut() = url::Url::parse(&fresh.url).expect("Broker validated URL");
    for name in [header::COOKIE, header::AUTHORIZATION, header::USER_AGENT] {
        request.headers_mut().remove(name);
    }
    if let Some(credentials) = fresh
        .credentials
        .iter()
        .find(|c| c.origin == request.url().origin().ascii_serialization())
    {
        for (name, value) in [
            (header::COOKIE, &credentials.cookie),
            (header::AUTHORIZATION, &credentials.authorization),
            (header::USER_AGENT, &credentials.user_agent),
        ] {
            if let Some(value) = value {
                request.headers_mut().insert(
                    name,
                    header::HeaderValue::from_str(value).expect("Broker validated header"),
                );
            }
        }
    }
}
fn advertisement(value: &str, port: u16) -> Option<u64> {
    for entry in value.split(',').take(16) {
        let mut fields = entry.trim().split(';');
        if fields.next()?.trim() != format!("h3=\":{port}\"") {
            continue;
        }
        let age = fields
            .filter_map(|field| field.trim().strip_prefix("ma="))
            .next()
            .map(|age| age.trim_matches('"').parse::<u64>().ok())
            .unwrap_or(Some(86400))?;
        return Some(age.min(86400));
    }
    None
}
impl RequestBuilder {
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        header::HeaderName: TryFrom<K>,
        <header::HeaderName as TryFrom<K>>::Error: Into<axum::http::Error>,
        header::HeaderValue: TryFrom<V>,
        <header::HeaderValue as TryFrom<V>>::Error: Into<axum::http::Error>,
    {
        self.inner = self.inner.header(key, value);
        self
    }
    pub async fn send(self) -> reqwest::Result<Response> {
        self.client.execute(self.inner.build()?).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advertisements_are_bounded_and_same_authority_only() {
        assert_eq!(advertisement("h3=\":443\"; ma=999999", 443), Some(86400));
        assert_eq!(
            advertisement("h2=\":443\", h3=\":443\"; ma=30", 443),
            Some(30)
        );
        assert_eq!(advertisement("h3=\"attacker.example:443\"", 443), None);
        assert_eq!(advertisement("h3=\":444\"", 443), None);
        assert_eq!(advertisement("h3=\":443\"; ma=wrong", 443), None);
    }
    #[tokio::test]
    async fn real_quic_ranges_preserve_bytes_and_blocked_udp_falls_back() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der());
        let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.cert.der().clone()], key.into())
        .unwrap();
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = tcp.local_addr().unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls.clone()));
        let bytes: Arc<Vec<u8>> = Arc::new((0..65536).map(|index| (index % 251) as u8).collect());
        let payload = bytes.clone();
        let tcp_server = tokio::spawn(async move {
            loop {
                let (socket, _) = tcp.accept().await.unwrap();
                let acceptor = acceptor.clone();
                let bytes = payload.clone();
                tokio::spawn(async move {
                    let mut stream = acceptor.accept(socket).await.unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 2048];
                    while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                        let size = stream.read(&mut buffer).await.unwrap();
                        if size == 0 {
                            return;
                        }
                        request.extend_from_slice(&buffer[..size]);
                        if request.len() > 16384 {
                            return;
                        }
                    }
                    let headers = String::from_utf8_lossy(&request).to_ascii_lowercase();
                    let (start, end) = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("range: bytes="))
                        .and_then(|range| range.split_once('-'))
                        .map(|(a, b)| {
                            (
                                a.parse::<usize>().unwrap(),
                                b.trim().parse::<usize>().unwrap(),
                            )
                        })
                        .unwrap_or((0, bytes.len() - 1));
                    let head = format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nETag: \"quic-fixture\"\r\nAlt-Svc: h3=\":{}\"; ma=60\r\nConnection: close\r\n\r\n", end-start+1, bytes.len(), address.port());
                    stream.write_all(head.as_bytes()).await.unwrap();
                    stream.write_all(&bytes[start..=end]).await.unwrap();
                    let _ = stream.shutdown().await;
                });
            }
        });
        tls.alpn_protocols = vec![b"h3".to_vec()];
        let quic = quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap();
        let endpoint =
            quinn::Endpoint::server(quinn::ServerConfig::with_crypto(Arc::new(quic)), address)
                .unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let hits = counter.clone();
        let payload = bytes.clone();
        let listener = endpoint.clone();
        let server = tokio::spawn(async move {
            while let Some(incoming) = listener.accept().await {
                let hits = hits.clone();
                let payload = payload.clone();
                tokio::spawn(async move {
                    let connection = incoming.await.unwrap();
                    let mut http =
                        h3::server::Connection::new(h3_quinn::Connection::new(connection))
                            .await
                            .unwrap();
                    while let Ok(Some(resolver)) = http.accept().await {
                        let (request, mut stream) = resolver.resolve_request().await.unwrap();
                        assert_eq!(request.version(), Version::HTTP_3);
                        let range = request.headers()[header::RANGE]
                            .to_str()
                            .unwrap()
                            .strip_prefix("bytes=")
                            .unwrap();
                        let (a, b) = range.split_once('-').unwrap();
                        let start: usize = a.parse().unwrap();
                        let end: usize = b.parse().unwrap();
                        hits.fetch_add(1, Ordering::SeqCst);
                        stream
                            .send_response(
                                axum::http::Response::builder()
                                    .status(206)
                                    .header(
                                        "content-range",
                                        format!("bytes {start}-{end}/{}", payload.len()),
                                    )
                                    .header("content-length", end - start + 1)
                                    .header("etag", "\"quic-fixture\"")
                                    .body(())
                                    .unwrap(),
                            )
                            .await
                            .unwrap();
                        stream
                            .send_data(bytes::Bytes::copy_from_slice(&payload[start..=end]))
                            .await
                            .unwrap();
                        stream.finish().await.unwrap();
                    }
                });
            }
        });
        let root = reqwest::Certificate::from_der(certificate.cert.der()).unwrap();
        let client = Client::from_builders(
            builder()
                .add_root_certificate(root.clone())
                .resolve("localhost", address),
            builder()
                .add_root_certificate(root)
                .resolve("localhost", address),
        )
        .unwrap();
        let url = format!("https://localhost:{}/file", address.port());
        let probe = crate::http::probe(&client, &url, None, &[]).await.unwrap();
        assert_eq!(probe.total, Some(bytes.len() as u64));
        let mut workers = tokio::task::JoinSet::new();
        for index in 0..16 {
            let client = client.clone();
            let url = url.clone();
            let bytes = bytes.clone();
            workers.spawn(async move {
                let start = index * 4096;
                let end = start + 4095;
                let response = client
                    .get(url)
                    .header(header::RANGE, format!("bytes={start}-{end}"))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.version(), Version::HTTP_3);
                assert_eq!(
                    response.bytes().await.unwrap().as_ref(),
                    &bytes[start..=end]
                );
            });
        }
        while let Some(result) = workers.join_next().await {
            result.unwrap();
        }
        assert_eq!(counter.load(Ordering::SeqCst), 16);
        endpoint.close(0u32.into(), b"test UDP unavailable");
        server.abort();
        let response = client
            .get(&url)
            .header(header::RANGE, "bytes=0-3")
            .send()
            .await
            .unwrap();
        assert_eq!(response.version(), Version::HTTP_11);
        assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[..4]);
        let start = Instant::now();
        let response = client
            .get(url)
            .header(header::RANGE, "bytes=4-7")
            .send()
            .await
            .unwrap();
        assert_eq!(response.version(), Version::HTTP_11);
        assert!(start.elapsed() < Duration::from_secs(2));
        tcp_server.abort();
    }
}
