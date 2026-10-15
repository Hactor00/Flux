use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};

pub(crate) fn staging_path(destination: &Path, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id)?;
    Ok(destination
        .parent()
        .context("Missing destination directory")?
        .join(format!(".flux-{id}.staging")))
}

pub(crate) async fn prepare(path: &Path, length: Option<u64>, reset: bool) -> Result<()> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(reset)
            .read(true)
            .write(true)
            .open(path)?;
        if reset {
            sparse(&file)?;
            if let Some(length) = length {
                ensure!(length <= i64::MAX as u64, "File exceeds supported length");
                allocate(&file, length)?;
                file.set_len(length)?;
            }
            file.sync_all()?;
        }
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

#[cfg(windows)]
fn sparse(file: &std::fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::{Ioctl::FSCTL_SET_SPARSE, IO::DeviceIoControl};
    let mut returned = 0;
    let result = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        let error = std::io::Error::last_os_error();
        if !matches!(error.raw_os_error(), Some(1 | 50 | 87)) {
            return Err(error.into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn sparse(_: &std::fs::File) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn allocate(file: &std::fs::File, length: u64) -> Result<()> {
    use std::os::fd::AsRawFd;
    if length == 0 {
        return Ok(());
    }
    let result = unsafe { libc::fallocate(file.as_raw_fd(), 0, 0, length as libc::off_t) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if !matches!(
            error.raw_os_error(),
            Some(libc::EOPNOTSUPP | libc::ENOSYS | libc::EINVAL)
        ) {
            return Err(error.into());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn allocate(file: &std::fs::File, length: u64) -> Result<()> {
    use std::os::fd::AsRawFd;
    if length == 0 {
        return Ok(());
    }
    let mut allocation = libc::fstore_t {
        fst_flags: libc::F_ALLOCATECONTIG,
        fst_posmode: libc::F_PEOFPOSMODE,
        fst_offset: 0,
        fst_length: length as libc::off_t,
        fst_bytesalloc: 0,
    };
    let descriptor = file.as_raw_fd();
    if unsafe { libc::fcntl(descriptor, libc::F_PREALLOCATE, &mut allocation) } != 0 {
        allocation.fst_flags = libc::F_ALLOCATEALL;
        if unsafe { libc::fcntl(descriptor, libc::F_PREALLOCATE, &mut allocation) } != 0 {
            let error = std::io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(libc::ENOTSUP | libc::EINVAL)) {
                return Err(error.into());
            }
        }
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn allocate(_: &std::fs::File, _: u64) -> Result<()> {
    Ok(())
}

pub(crate) async fn range_hasher(path: &Path, offset: u64, length: u64) -> Result<Sha256> {
    let mut file = File::open(path).await?;
    file.seek(SeekFrom::Start(offset)).await?;
    let mut remaining = length;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut hash = Sha256::new();
    while remaining != 0 {
        let count = buffer.len().min(remaining as usize);
        let read = file.read(&mut buffer[..count]).await?;
        ensure!(read != 0, "Verified range is truncated");
        hash.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(hash)
}

pub(crate) async fn writer(
    path: &Path,
    offset: u64,
    direct: bool,
) -> Result<crate::disk_writer::Writer> {
    crate::disk_writer::Writer::open(path, offset, direct).await
}

pub(crate) async fn verify(
    path: &Path,
    parts: &[crate::model::Part],
    length: u64,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<String> {
    ensure!(
        tokio::fs::metadata(path).await?.len() == length,
        "Staging file length mismatch"
    );
    let mut file = File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut cursor = 0;
    for part in parts {
        ensure!(
            part.start == cursor && part.done <= part.len(),
            "Download checkpoint ranges overlap or contain holes"
        );
        let mut remaining = part.done;
        let mut chunk_hash = Sha256::new();
        while remaining != 0 {
            let count = buffer.len().min(remaining as usize);
            let read = tokio::select! {
                _ = cancel.cancelled() => anyhow::bail!("Cancelled"),
                read = file.read(&mut buffer[..count]) => read?,
            };
            ensure!(read != 0, "Verified range is truncated");
            chunk_hash.update(&buffer[..read]);
            hash.update(&buffer[..read]);
            remaining -= read as u64;
        }
        ensure!(
            part.digest.as_deref() == Some(hex::encode(chunk_hash.finalize()).as_str()),
            "Part integrity check failed"
        );
        cursor += part.done;
    }
    ensure!(cursor == length, "Download checkpoint length mismatch");
    Ok(hex::encode(hash.finalize()))
}
