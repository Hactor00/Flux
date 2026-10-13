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
async fn forward(data: &Path, request: Request, startup: &Mutex<()>) -> Result<Response> {
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
    let (mut stream, discovery) = match connect(data).await {
        Ok(connection) => connection,
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
    Ok(serde_json::from_slice(&bytes)?)
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
    let reader = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        while let Some(frame) = protocol::read_frame(&mut stdin).await? {
            let request: Request = serde_json::from_slice(&frame)?;
            let request_id = request.request_id.clone();
            let permit = limit.clone().acquire_owned().await?;
            let data: PathBuf = data.clone();
            let startup = startup.clone();
            let sender = sender.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let result = tokio::time::timeout(
                    Duration::from_secs(120),
                    forward(&data, request, &startup),
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
    Ok(())
}
