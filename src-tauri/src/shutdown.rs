use fs2::FileExt;
use std::{
    error::Error,
    fs::OpenOptions,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn request() -> Result<(), Box<dyn Error>> {
    let data = flux_core::default_data_directory()?;
    let lock = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(data.join("engine.lock"))
    {
        Ok(lock) => lock,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if lock.try_lock_exclusive().is_ok() {
        return Ok(());
    }
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--shutdown-request")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    if !command.status()?.success() {
        return Err(
            "The data directory is owned by a different engine; close that engine first".into(),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if lock.try_lock_exclusive().is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Application did not finish checkpointing within 30 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
