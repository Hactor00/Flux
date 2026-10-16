mod caller;

use anyhow::{ensure, Context, Result};
use flux_core::protocol::{self, AuthenticatedRequest, Discovery, Request, Response, VERSION};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, Mutex, Semaphore};
async fn connect(data: &Path) -> Result<(tokio::net::TcpStream, Discovery)> {
    let discovery: Discovery =
        serde_json::from_slice(&tokio::fs::read(data.join("broker.json")).await?)?;
    let stream =
        tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, discovery.port)).await?;
    Ok((stream, discovery))
}
async fn forward(
    data: &Path,
    request: Request,
    startup: &Mutex<()>,
    presence: &Mutex<Option<tokio::net::TcpStream>>,
) -> Result<Response> {
    ensure!(request.version == VERSION, "Unsupported protocol version");
    uuid::Uuid::parse_str(&request.request_id)?;
    if let protocol::Command::Protect { credentials } = &request.command {
        let data = data.to_owned();
        let credentials = credentials.clone();
        let protected = tokio::task::spawn_blocking(move || {
            flux_core::protect_credentials(&data, &credentials)
        })
        .await??;
        return Ok(Response {
            version: VERSION,
            request_id: request.request_id,
            ok: true,
            result: Some(serde_json::Value::String(protected)),
            error: None,
        });
    }
    let browser_connect = matches!(request.command, protocol::Command::BrowserConnect { .. });
    let passive = browser_connect
        || matches!(
            request.command,
            protocol::Command::RefreshPoll | protocol::Command::ListPage { probe: true, .. }
        );
    let (mut stream, discovery) = match connect(data).await {
        Ok(connection) => connection,
        Err(error) if passive => return Err(error.context("Flux is not running")),
        Err(_) => {
            let _guard = startup.lock().await;
            match connect(data).await {
                Ok(connection) => connection,
                Err(_) => {
                    let executable = std::env::current_exe()?;
                    let desktop = executable
                        .parent()
                        .context("Application directory unavailable")?
                        .join(if cfg!(windows) {
                            "flux-desktop.exe"
                        } else {
                            "flux-desktop"
                        });
                    ensure!(desktop.is_file(), "Desktop executable is not installed");
                    let mut command = tokio::process::Command::new(desktop);
                    command
                        .arg("--background")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null());
                    #[cfg(windows)]
                    command.creation_flags(0x08000000);
                    let mut child = command.spawn()?;
                    tokio::spawn(async move {
                        let _ = child.wait().await;
                    });
                    tokio::time::timeout(Duration::from_secs(15), async {
                        loop {
                            if let Ok(connection) = connect(data).await {
                                return connection;
                            }
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    })
                    .await
                    .context("Desktop startup timed out")?
                }
            }
        }
    };
    #[cfg(windows)]
    if matches!(request.command, protocol::Command::Propose(_)) {
        if let Some(process_id) = discovery.process_id {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(process_id);
            }
        }
    }
    protocol::write_frame(
        &mut stream,
        &AuthenticatedRequest {
            token: discovery.token,
            request,
        },
    )
    .await?;
    let bytes = protocol::read_frame(&mut stream)
        .await?
        .context("Broker closed without a response")?;
    let response: Response = serde_json::from_slice(&bytes)?;
    if response.ok {
        let mut active = presence.lock().await;
        if browser_connect {
            *active = Some(stream);
        } else if active.is_none() {
            // Older extensions still establish a real connection, with an unknown version.
            if let Ok((mut socket, discovery)) = connect(data).await {
                let hello = AuthenticatedRequest {
                    token: discovery.token,
                    request: Request {
                        version: VERSION,
                        request_id: uuid::Uuid::new_v4().to_string(),
                        command: protocol::Command::BrowserConnect {
                            extension_version: "unknown".into(),
                        },
                    },
                };
                if protocol::write_frame(&mut socket, &hello).await.is_ok() {
                    if let Ok(Some(bytes)) = tokio::time::timeout(
                        Duration::from_secs(3),
                        protocol::read_frame(&mut socket),
                    )
                    .await
                    .unwrap_or(Ok(None))
                    {
                        if serde_json::from_slice::<Response>(&bytes).is_ok_and(|reply| reply.ok) {
                            *active = Some(socket);
                        }
                    }
                }
            }
        }
    }
    Ok(response)
}
#[tokio::main]
async fn main() -> Result<()> {
    let data = flux_core::default_data_directory()?;
    caller::authorize(
        &data,
        &std::env::current_exe()?,
        &std::env::args().skip(1).collect::<Vec<_>>(),
    )?;
    let (sender, mut replies) = mpsc::channel::<Response>(16);
    let startup = Arc::new(Mutex::new(()));
    let limit = Arc::new(Semaphore::new(8));
    let presence = Arc::new(Mutex::new(None));
    let live_presence = presence.clone();
    let reader = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        while let Some(frame) = protocol::read_frame(&mut stdin).await? {
            let request: Request = serde_json::from_slice(&frame)?;
            let request_id = request.request_id.clone();
            let permit = limit.clone().acquire_owned().await?;
            let data: PathBuf = data.clone();
            let startup = startup.clone();
            let sender = sender.clone();
            let presence = live_presence.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let result = tokio::time::timeout(
                    Duration::from_secs(120),
                    forward(&data, request, &startup, &presence),
                )
                .await;
                let response = match result {
                    Ok(Ok(response)) => response,
                    other => Response {
                        version: VERSION,
                        request_id,
                        ok: false,
                        result: None,
                        error: Some(format!("Desktop broker unavailable: {other:?}")),
                    },
                };
                let _ = sender.send(response).await;
            });
        }
        anyhow::Ok(())
    });
    let mut stdout = tokio::io::stdout();
    while let Some(response) = replies.recv().await {
        protocol::write_frame(&mut stdout, &response).await?;
    }
    reader.await??;
    drop(presence);
    Ok(())
}
