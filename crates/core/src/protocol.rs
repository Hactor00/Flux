//! Versioned, bounded framing shared by the loopback broker and Native Messaging host.
use crate::{DownloadRequest, Engine};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const VERSION: u8 = 1;
pub const MAX_FRAME: usize = 256 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u8,
    pub request_id: String,
    #[serde(flatten)]
    pub command: Command,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum Command {
    Add(Box<DownloadRequest>),
    Propose(Box<DownloadRequest>),
    Resolve {
        url: String,
        #[serde(default)]
        protected_credentials: Option<String>,
    },
    ResolveCollection {
        url: String,
        offset: usize,
        limit: usize,
        #[serde(default)]
        protected_credentials: Option<String>,
    },
    List,
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Inspect {
        url: String,
        referer: Option<String>,
        #[serde(default)]
        protected_credentials: Option<String>,
    },
    ListPage {
        offset: usize,
        limit: usize,
    },
    Protect {
        credentials: Vec<crate::BrowserCredentials>,
    },
    Task {
        id: String,
    },
    TorrentFiles {
        id: String,
        offset: usize,
        limit: usize,
    },
    RefreshPoll,
    RefreshSubmit {
        ticket: String,
        page_url: String,
        url: String,
        protected_credentials: Option<String>,
    },
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub version: u8,
    pub request_id: String,
    pub ok: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedRequest {
    pub token: String,
    pub request: Request,
}
#[derive(Serialize, Deserialize)]
pub struct Discovery {
    pub port: u16,
    pub token: String,
}

pub async fn dispatch(engine: &Engine, request: Request) -> Response {
    let request_id = request.request_id.clone();
    let result = async {
        ensure!(request.version == VERSION, "Unsupported protocol version");
        uuid::Uuid::parse_str(&request_id)?;
        let value = match request.command {
            Command::Resolve {
                url,
                protected_credentials,
            } => serde_json::to_value(
                engine
                    .resolve_media_authenticated(&url, protected_credentials)
                    .await?,
            )?,
            Command::ResolveCollection {
                url,
                offset,
                limit,
                protected_credentials,
            } => serde_json::to_value(
                engine
                    .resolve_collection_authenticated(&url, offset, limit, protected_credentials)
                    .await?,
            )?,
            Command::Propose(input) => {
                serde_json::to_value(engine.propose_id(request_id.clone(), *input).await?.summary())?
            }
            Command::Add(input) => {
                let task = engine.enqueue_id(request_id.clone(), *input).await?;
                serde_json::to_value(engine.task(&task.id).await?.summary())?
            }
            Command::List => task_page(engine, 0, 50).await?["tasks"].clone(),
            Command::Task { id } => serde_json::to_value(engine.task(&id).await?.summary())?,
            Command::TorrentFiles { id, offset, limit } => {
                ensure!((1..=50).contains(&limit), "File page size must be between 1 and 50");
                let task = engine.task(&id).await?;
                let torrent = task.torrent.context("Task has no torrent metadata")?;
                let total = torrent.files.len();
                let mut files = Vec::new();
                let mut bytes = 2048;
                for file in torrent.files.into_iter().skip(offset).take(limit) {
                    let size = serde_json::to_vec(&file)?.len();
                    if bytes + size > MAX_FRAME - 2048 { break; }
                    bytes += size;
                    files.push(file);
                }
                ensure!(!files.is_empty() || offset >= total, "Torrent filename exceeds the frame limit");
                let next = offset.saturating_add(files.len());
                serde_json::json!({"files":files,"total":total,"next_offset":next,"has_more":next<total})
            },
            Command::RefreshPoll => serde_json::to_value(engine.refresh_poll().await)?,
            Command::RefreshSubmit {
                ticket,
                page_url,
                url,
                protected_credentials,
            } => {
                engine
                    .refresh_submit(ticket, page_url, url, protected_credentials)
                    .await?;
                serde_json::Value::Null
            }
            Command::Protect { .. } => {
                anyhow::bail!("Credential protection is handled by the native host")
            }
            Command::Pause { id } => serde_json::to_value(engine.pause(&id).await?.summary())?,
            Command::Resume { id } => serde_json::to_value(engine.queue(&id).await?.summary())?,
            Command::Inspect {
                url,
                referer,
                protected_credentials,
            } => serde_json::to_value(
                engine
                    .inspect_media(url, referer, protected_credentials)
                    .await?,
            )?,
            Command::ListPage { offset, limit } => {
                ensure!(
                    (1..=50).contains(&limit),
                    "Page size must be between 1 and 50"
                );
                task_page(engine, offset, limit).await?
            }
        };
        ensure!(serde_json::to_vec(&value)?.len() <= MAX_FRAME - 1024, "Response metadata exceeds the native frame limit");
        anyhow::Ok(value)
    }
    .await;
    match result {
        Ok(value) => Response {
            version: VERSION,
            request_id,
            ok: true,
            result: Some(value),
            error: None,
        },
        Err(error) => Response {
            version: VERSION,
            request_id,
            ok: false,
            result: None,
            error: Some(format!("{error:#}")),
        },
    }
}

async fn task_page(engine: &Engine, offset: usize, limit: usize) -> Result<serde_json::Value> {
    let (total, tasks) = engine.list_page(offset, limit).await?;
    let mut bounded = Vec::new();
    let mut size = 2048usize;
    for task in tasks {
        let task = task.summary();
        let bytes = serde_json::to_vec(&task)?;
        if size + bytes.len() + 1 > MAX_FRAME {
            break;
        }
        size += bytes.len() + 1;
        bounded.push(task);
    }
    ensure!(
        !bounded.is_empty() || offset >= total,
        "Task exceeds the protocol size limit"
    );
    let next_offset = offset.saturating_add(bounded.len());
    Ok(
        serde_json::json!({"total":total,"tasks":bounded,"next_offset":next_offset,"has_more":next_offset<total}),
    )
}
pub async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> Result<Option<Vec<u8>>> {
    let mut prefix = [0u8; 4];
    match reader.read(&mut prefix[..1]).await? {
        0 => return Ok(None),
        _ => reader.read_exact(&mut prefix[1..]).await?,
    };
    let size = u32::from_ne_bytes(prefix) as usize;
    ensure!(
        size > 0 && size <= MAX_FRAME,
        "Frame size is outside protocol limits"
    );
    let mut body = vec![0u8; size];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}
pub async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    ensure!(body.len() <= MAX_FRAME, "Response exceeds protocol limit");
    writer.write_all(&(body.len() as u32).to_ne_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

/// Browser pages cannot use this TCP framing. Only the registered host gets the secret.
pub async fn serve(
    engine: Engine,
    data: &std::path::Path,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let discovery = Discovery {
        port: listener.local_addr()?.port(),
        token: token.clone(),
    };
    let path = data.join("broker.json");
    let bytes = serde_json::to_vec(&discovery)?;
    let protected_path = path.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let temporary_path = protected_path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&temporary_path)?;
        // Protect the file before writing the authentication secret.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let who = std::process::Command::new("whoami")
                .args(["/user", "/fo", "csv", "/nh"])
                .creation_flags(0x08000000)
                .output()?;
            ensure!(who.status.success(), "Cannot determine account identity");
            // Account names use the Windows OEM code page; the numeric SID is always ASCII.
            let output = String::from_utf8_lossy(&who.stdout);
            let account = output
                .rsplit(',')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_matches('"');
            ensure!(
                account.starts_with("S-1-")
                    && account
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'S' || byte == b'-'),
                "Invalid account SID"
            );
            let acl = std::process::Command::new("icacls")
                .arg(&temporary_path)
                .args(["/inheritance:r", "/grant:r", &format!("*{account}:F")])
                .creation_flags(0x08000000)
                .output()?;
            ensure!(acl.status.success(), "Cannot protect broker credentials");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        use std::io::Write;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        // Native hosts must see either the complete previous discovery record or
        // the complete new one, including its already-restricted ACL.
        std::fs::rename(temporary_path, protected_path)?;
        Ok(())
    })
    .await??;
    let capacity = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
    let mut peers = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            Some(_) = peers.join_next(), if !peers.is_empty() => {},
            accepted = listener.accept() => {
                let (mut stream, address) = accepted?;
                if !address.ip().is_loopback() { continue; }
                let Ok(permit) = capacity.clone().try_acquire_owned() else { continue; };
                let engine = engine.clone(); let token = token.clone();
                peers.spawn(async move {
                    let _permit = permit;
                    let result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
                        let frame = read_frame(&mut stream).await?.ok_or_else(|| anyhow::anyhow!("Empty request"))?;
                        let input: AuthenticatedRequest = serde_json::from_slice(&frame)?;
                        ensure!(input.token.len() == token.len() && input.token.bytes().zip(token.bytes()).fold(0u8, |a, (b, c)| a | (b ^ c)) == 0, "Authentication failed");
                        write_frame(&mut stream, &dispatch(&engine, input.request).await).await
                    }).await;
                    if let Ok(Err(error)) = result { eprintln!("Broker request rejected: {error}"); }
                });
            }
        }
    }
    peers.shutdown().await;
    let _ = tokio::fs::remove_file(path).await;
    Ok(())
}
