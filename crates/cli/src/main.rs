#[cfg(feature = "test-harness")]
mod harness;
use anyhow::Result;
use clap::{Parser, Subcommand};
use flux_core::{DownloadRequest, Engine, Settings, Status};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Flux persistent download engine")]
struct Args {
    #[arg(long, default_value = ".flux")]
    data: PathBuf,
    #[arg(long, default_value = "downloads")]
    output: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    VerifyUpdate {
        artifact: PathBuf,
        signature: PathBuf,
        public_key: String,
        #[arg(long)]
        expected_version: Option<String>,
    },
    Download {
        url: String,
        filename: String,
        #[arg(long, default_value_t = 0)]
        connections: u8,
        #[arg(long, default_value_t = 0)]
        speed_limit: u64,
        #[arg(long)]
        sha256: Option<String>,
    },
    Resume {
        id: String,
    },
    List,
    Settings {
        #[arg(long)]
        input: Option<PathBuf>,
    },
    Serve {
        #[arg(long)]
        shutdown_file: Option<PathBuf>,
    },
    #[cfg(feature = "test-harness")]
    Harness {
        #[arg(long, default_value_t = 1421)]
        port: u16,
        #[arg(long)]
        frontend_origin: String,
        #[arg(long)]
        shutdown_file: PathBuf,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if let Command::VerifyUpdate {
        artifact,
        signature,
        public_key,
        expected_version,
    } = &args.command
    {
        let signature = std::fs::read_to_string(signature)?;
        flux_core::signing::verify_update(
            artifact,
            public_key,
            &signature,
            expected_version.as_deref(),
        )?;
        println!("Update signature verified");
        return Ok(());
    }
    let engine = Engine::open(&args.data, &args.output).await?;
    let id = match args.command {
        Command::VerifyUpdate { .. } => {
            unreachable!("Verification completes before opening the engine")
        }
        Command::Download {
            url,
            filename,
            connections,
            speed_limit,
            sha256,
        } => Some(
            engine
                .enqueue(DownloadRequest {
                    url,
                    filename,
                    connections,
                    speed_limit,
                    expected_sha256: sha256,
                    ..DownloadRequest::default()
                })
                .await?
                .id,
        ),
        Command::Resume { id } => Some(id),
        Command::List => {
            println!("{}", serde_json::to_string_pretty(&engine.list().await?)?);
            None
        }
        Command::Settings { input } => {
            let settings = if let Some(path) = input {
                let settings: Settings = serde_json::from_slice(&tokio::fs::read(path).await?)?;
                engine.save_settings(settings).await?
            } else {
                engine.settings().await?
            };
            println!("{}", serde_json::to_string_pretty(&settings)?);
            None
        }
        #[cfg(feature = "test-harness")]
        Command::Harness {
            port,
            frontend_origin,
            shutdown_file,
        } => {
            harness::run(engine.clone(), port, frontend_origin, shutdown_file).await?;
            None
        }
        Command::Serve { shutdown_file } => {
            let cancel = tokio_util::sync::CancellationToken::new();
            let token = cancel.clone();
            if let Some(path) = shutdown_file {
                let stop = cancel.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::select! {_=stop.cancelled()=>break,_=tokio::time::sleep(std::time::Duration::from_millis(100))=>{if tokio::fs::try_exists(&path).await.unwrap_or(false){stop.cancel();break;}}}
                    }
                });
            }
            let scheduler_engine = engine.clone();
            let scheduler_cancel = cancel.clone();
            let scheduler =
                tokio::spawn(async move { scheduler_engine.run_scheduler(scheduler_cancel).await });
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                token.cancel();
            });
            let result = flux_core::protocol::serve(engine.clone(), &args.data, cancel).await;
            engine.shutdown().await?;
            scheduler.await??;
            result?;
            None
        }
    };
    if let Some(id) = id {
        engine.start(&id).await?;
        eprintln!("Task: {id}");
        let task = tokio::select! {
            task = engine.wait(&id) => task?,
            _ = tokio::signal::ctrl_c() => engine.pause(&id).await?
        };
        println!("{}", serde_json::to_string_pretty(&task)?);
        anyhow::ensure!(
            matches!(task.status, Status::Complete | Status::Paused),
            "{}",
            task.error.unwrap_or_else(|| "Download failed".into())
        );
    }
    Ok(())
}
