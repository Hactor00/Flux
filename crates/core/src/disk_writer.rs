#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
#[cfg(not(windows))]
use std::io::SeekFrom;
use std::path::Path;

pub(crate) const ALIGNMENT: u64 = 64 * 1024;

pub(crate) struct Writer {
    #[cfg(windows)]
    state: Option<WindowsWriter>,
    #[cfg(not(windows))]
    file: tokio::fs::File,
}

impl Writer {
    pub async fn open(path: &Path, offset: u64, direct: bool) -> Result<Self> {
        #[cfg(windows)]
        {
            let path = path.to_owned();
            let state =
                tokio::task::spawn_blocking(move || WindowsWriter::open(&path, offset, direct))
                    .await??;
            Ok(Self { state: Some(state) })
        }
        #[cfg(not(windows))]
        {
            use tokio::io::AsyncSeekExt;
            let _ = direct;
            let mut file = tokio::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .await?;
            file.seek(SeekFrom::Start(offset)).await?;
            Ok(Self { file })
        }
    }
    #[cfg(windows)]
    async fn run<T: Send + 'static>(
        &mut self,
        operation: impl FnOnce(&mut WindowsWriter) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let mut state = self.state.take().context("Disk writer is unavailable")?;
        let (state, result) = tokio::task::spawn_blocking(move || {
            let result = operation(&mut state);
            (state, result)
        })
        .await?;
        self.state = Some(state);
        result
    }
    pub async fn write_bytes(&mut self, bytes: bytes::Bytes) -> Result<()> {
        #[cfg(windows)]
        {
            self.run(move |state| state.write(&bytes)).await
        }
        #[cfg(not(windows))]
        {
            use tokio::io::AsyncWriteExt;
            self.file.write_all(&bytes).await?;
            Ok(())
        }
    }
    pub async fn sync_all(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            self.run(|state| {
                state.flush()?;
                state.file.sync_all()?;
                Ok(())
            })
            .await
        }
        #[cfg(not(windows))]
        {
            self.file.sync_all().await?;
            Ok(())
        }
    }
    pub async fn sync_data(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            self.run(|state| {
                state.flush()?;
                state.file.sync_data()?;
                Ok(())
            })
            .await
        }
        #[cfg(not(windows))]
        {
            self.file.sync_data().await?;
            Ok(())
        }
    }
}

#[cfg(windows)]
struct Block {
    pointer: std::ptr::NonNull<u8>,
    layout: std::alloc::Layout,
}
#[cfg(windows)]
impl Block {
    fn new() -> Result<Self> {
        let layout = std::alloc::Layout::from_size_align(256 * 1024, ALIGNMENT as usize)?;
        let pointer = std::ptr::NonNull::new(unsafe { std::alloc::alloc_zeroed(layout) })
            .context("Cannot allocate the aligned disk buffer")?;
        Ok(Self { pointer, layout })
    }
    fn bytes(&mut self) -> &mut [u8] {
        // The allocation is exclusively owned, initialized, and lives until Block::drop.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.layout.size()) }
    }
}
#[cfg(windows)]
// Moving the exclusively owned allocation between blocking workers preserves its alignment.
unsafe impl Send for Block {}
#[cfg(windows)]
impl Drop for Block {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.pointer.as_ptr(), self.layout) };
    }
}

#[cfg(windows)]
struct WindowsWriter {
    file: std::fs::File,
    block: Block,
    used: usize,
    cursor: u64,
    direct: bool,
}
#[cfg(windows)]
impl WindowsWriter {
    fn open(path: &Path, offset: u64, direct: bool) -> Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH,
        };
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true);
        if direct {
            options.custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH);
        }
        let (file, direct) = match options.open(path) {
            Ok(file) => (file, direct),
            Err(error) if direct && matches!(error.raw_os_error(), Some(1 | 50 | 87)) => (
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)?,
                false,
            ),
            Err(error) => return Err(error.into()),
        };
        let mut writer = Self {
            file,
            block: Block::new()?,
            used: 0,
            cursor: 0,
            direct,
        };
        writer.position(offset)?;
        Ok(writer)
    }
    fn position(&mut self, offset: u64) -> Result<()> {
        use std::os::windows::fs::FileExt;
        self.used = if self.direct {
            (offset % ALIGNMENT) as usize
        } else {
            0
        };
        self.cursor = offset - self.used as u64;
        if self.used != 0 {
            let count = self
                .file
                .seek_read(&mut self.block.bytes()[..ALIGNMENT as usize], self.cursor)?;
            anyhow::ensure!(count >= self.used, "Disk checkpoint prefix is truncated");
        }
        Ok(())
    }
    fn write(&mut self, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            let count = bytes.len().min(self.block.layout.size() - self.used);
            self.block.bytes()[self.used..self.used + count].copy_from_slice(&bytes[..count]);
            self.used += count;
            bytes = &bytes[count..];
            if self.used == self.block.layout.size() {
                self.flush()?;
            }
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        use std::os::windows::fs::FileExt;
        if self.used == 0 {
            return Ok(());
        }
        let length = if self.direct {
            self.used.next_multiple_of(ALIGNMENT as usize)
        } else {
            self.used
        };
        self.block.bytes()[self.used..length].fill(0);
        let written = self
            .file
            .seek_write(&self.block.bytes()[..length], self.cursor)?;
        anyhow::ensure!(written == length, "Short direct disk write");
        let full = if self.direct {
            self.used / ALIGNMENT as usize * ALIGNMENT as usize
        } else {
            self.used
        };
        let remaining = self.used - full;
        self.block.bytes().copy_within(full..self.used, 0);
        self.cursor = self
            .cursor
            .checked_add(full as u64)
            .context("Disk offset overflow")?;
        self.used = remaining;
        Ok(())
    }
}
