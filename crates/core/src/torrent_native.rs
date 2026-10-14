use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    ffi::{c_char, c_void, CString},
    path::Path,
    ptr::NonNull,
    sync::{Arc, Mutex, OnceLock},
};

type New = unsafe extern "C" fn(u64, u64, bool) -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Error = unsafe extern "C" fn(*mut c_char, usize) -> usize;
type Describe = unsafe extern "C" fn(*const c_char, usize, *mut c_char, usize) -> usize;
type Add = unsafe extern "C" fn(*mut c_void, *const c_char, usize, bool, *const c_char) -> u64;
type Metadata = unsafe extern "C" fn(*mut c_void, u64, *mut c_char, usize) -> usize;
type Start =
    unsafe extern "C" fn(*mut c_void, u64, *const c_char, *const u32, usize, u32, u32, u32) -> bool;
type StatsFn = unsafe extern "C" fn(*mut c_void, u64, *mut Stats) -> bool;
type Peer = unsafe extern "C" fn(*mut c_void, u64, *const c_char, u16) -> bool;
type Pause = unsafe extern "C" fn(*mut c_void, u64, bool) -> bool;
type Remove = unsafe extern "C" fn(*mut c_void, u64) -> bool;
type Flushed = unsafe extern "C" fn(*mut c_void, u64) -> i32;
type Limits = unsafe extern "C" fn(*mut c_void, u32, u32) -> bool;
type Counts = unsafe extern "C" fn(*mut c_void, *mut u64, *mut u64) -> bool;

#[repr(C)]
#[derive(Default, Debug)]
pub(crate) struct Stats {
    pub completed: u64,
    pub total: u64,
    pub uploaded: u64,
    pub download_rate: u64,
    pub upload_rate: u64,
    pub peers: u64,
    pub metadata: u32,
    pub finished: u32,
    pub paused: u32,
    pub failed: u32,
}

#[derive(Deserialize)]
pub(crate) struct MetadataInfo {
    pub v2: bool,
    pub info_hash: String,
    pub files: Vec<FileInfo>,
}
#[derive(Deserialize)]
pub(crate) struct FileInfo {
    pub index: usize,
    pub path: String,
    pub length: u64,
}

struct Api {
    _library: libloading::Library,
    new: New,
    destroy: Destroy,
    error: Error,
    describe: Describe,
    add: Add,
    metadata: Metadata,
    start: Start,
    stats: StatsFn,
    peer: Peer,
    pause: Pause,
    remove: Remove,
    flush: Remove,
    flushed: Flushed,
    limits: Limits,
    counts: Counts,
}

impl Api {
    fn open() -> Result<Arc<Self>> {
        // Foreign thread-local destructors can outlive a session, so never unload its code.
        static RUNTIME: OnceLock<Arc<Api>> = OnceLock::new();
        if let Some(api) = RUNTIME.get() {
            return Ok(api.clone());
        }
        let api = Self::load()?;
        Ok(RUNTIME.get_or_init(|| api).clone())
    }

    fn load() -> Result<Arc<Self>> {
        let executable = std::env::current_exe()?;
        let directory = executable
            .parent()
            .context("Executable directory is unavailable")?;
        #[cfg(windows)]
        let name = "FluxTorrentBridge.dll";
        #[cfg(target_os = "linux")]
        let name = "libflux_torrent.so";
        #[cfg(target_os = "macos")]
        let name = "libflux_torrent.dylib";
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let name = "libflux_torrent.so";
        let mut path = directory.join(name);
        if cfg!(debug_assertions)
            && !path.exists()
            && directory.file_name().is_some_and(|name| name == "deps")
        {
            path = directory
                .parent()
                .context("Build directory is unavailable")?
                .join(name);
        }
        ensure!(
            path.is_file(),
            "Native BEP 52 torrent runtime is not installed"
        );
        #[cfg(windows)]
        let library: libloading::Library = unsafe {
            libloading::os::windows::Library::load_with_flags(&path, 0x00000100 | 0x00001000)
        }
        .context("Cannot load the native torrent runtime")?
        .into();
        #[cfg(not(windows))]
        let library = unsafe { libloading::Library::new(&path) }
            .context("Cannot load the native torrent runtime")?;
        unsafe {
            let version = library.get::<unsafe extern "C" fn() -> u32>(b"flux_torrent_abi\0")?;
            ensure!(version() == 1, "Unsupported native torrent ABI");
            Ok(Arc::new(Self {
                new: *library.get(b"flux_torrent_session\0")?,
                destroy: *library.get(b"flux_torrent_destroy\0")?,
                error: *library.get(b"flux_torrent_error\0")?,
                describe: *library.get(b"flux_torrent_describe\0")?,
                add: *library.get(b"flux_torrent_add\0")?,
                metadata: *library.get(b"flux_torrent_metadata\0")?,
                start: *library.get(b"flux_torrent_start\0")?,
                stats: *library.get(b"flux_torrent_stats\0")?,
                peer: *library.get(b"flux_torrent_peer\0")?,
                pause: *library.get(b"flux_torrent_pause\0")?,
                remove: *library.get(b"flux_torrent_remove\0")?,
                flush: *library.get(b"flux_torrent_flush\0")?,
                flushed: *library.get(b"flux_torrent_flushed\0")?,
                limits: *library.get(b"flux_torrent_limits\0")?,
                counts: *library.get(b"flux_torrent_counts\0")?,
                _library: library,
            }))
        }
    }
    fn failure(&self) -> anyhow::Error {
        let mut bytes = vec![0u8; 8192];
        let size = unsafe { (self.error)(bytes.as_mut_ptr().cast(), bytes.len()) };
        if size == 0 || size > bytes.len() {
            return anyhow::anyhow!("Native torrent operation failed");
        }
        anyhow::anyhow!("{}", String::from_utf8_lossy(&bytes[..size]))
    }
    fn json<T: serde::de::DeserializeOwned>(
        &self,
        operation: impl Fn(*mut c_char, usize) -> usize,
    ) -> Result<T> {
        let size = operation(std::ptr::null_mut(), 0);
        ensure!(size > 0 && size <= 4 * 1024 * 1024, "{}", self.failure());
        let mut bytes = vec![0u8; size];
        ensure!(
            operation(bytes.as_mut_ptr().cast(), size) == size,
            "Native torrent metadata changed during inspection"
        );
        Ok(serde_json::from_slice(&bytes)?)
    }
}

struct State {
    pointer: NonNull<c_void>,
    api: Arc<Api>,
}
// Every foreign call is serialized by the session mutex. libtorrent owns its I/O threads.
unsafe impl Send for State {}
impl Drop for State {
    fn drop(&mut self) {
        unsafe { (self.api.destroy)(self.pointer.as_ptr()) };
    }
}

pub(crate) struct Session(Mutex<State>);
impl Session {
    pub fn new(download: u64, upload: u64, discovery: bool) -> Result<Arc<Self>> {
        ensure!(
            download <= i32::MAX as u64 && upload <= i32::MAX as u64,
            "Native torrent bandwidth limit exceeds two GiB per second"
        );
        let api = Api::open()?;
        let pointer = NonNull::new(unsafe { (api.new)(download, upload, discovery) })
            .ok_or_else(|| api.failure())?;
        Ok(Arc::new(Self(Mutex::new(State { pointer, api }))))
    }
    pub fn describe(bytes: &[u8]) -> Result<MetadataInfo> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= 8 * 1024 * 1024,
            "Invalid torrent metadata size"
        );
        let api = Api::open()?;
        api.json(|out, capacity| unsafe {
            (api.describe)(bytes.as_ptr().cast(), bytes.len(), out, capacity)
        })
    }
    fn call<T>(&self, operation: impl FnOnce(&State) -> Result<T>) -> Result<T> {
        let state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Native torrent session lock poisoned"))?;
        operation(&state)
    }
    pub fn add(&self, source: &[u8], magnet: bool, destination: &Path) -> Result<u64> {
        let path = CString::new(
            destination
                .to_str()
                .context("Torrent destination must use Unicode")?,
        )?;
        self.call(|state| {
            let id = unsafe {
                (state.api.add)(
                    state.pointer.as_ptr(),
                    source.as_ptr().cast(),
                    source.len(),
                    magnet,
                    path.as_ptr(),
                )
            };
            ensure!(id != 0, "{}", state.api.failure());
            Ok(id)
        })
    }
    pub fn metadata(&self, id: u64) -> Result<MetadataInfo> {
        self.call(|state| {
            state.api.json(|out, capacity| unsafe {
                (state.api.metadata)(state.pointer.as_ptr(), id, out, capacity)
            })
        })
    }
    pub fn start(
        &self,
        id: u64,
        destination: &Path,
        options: &crate::TorrentOptions,
        download: u64,
    ) -> Result<()> {
        let path = CString::new(
            destination
                .to_str()
                .context("Torrent destination must use Unicode")?,
        )?;
        let files = options
            .only_files
            .iter()
            .copied()
            .map(u32::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            download <= i32::MAX as u64 && options.upload_limit <= i32::MAX as u64,
            "Native torrent bandwidth limit exceeds two GiB per second"
        );
        self.call(|state| {
            ensure!(
                unsafe {
                    (state.api.start)(
                        state.pointer.as_ptr(),
                        id,
                        path.as_ptr(),
                        files.as_ptr(),
                        files.len(),
                        download as u32,
                        options.upload_limit as u32,
                        options.peer_limit as u32,
                    )
                },
                "{}",
                state.api.failure()
            );
            Ok(())
        })
    }
    pub fn peer(&self, id: u64, peer: std::net::SocketAddr) -> Result<()> {
        let host = CString::new(peer.ip().to_string())?;
        self.call(|state| {
            ensure!(
                unsafe { (state.api.peer)(state.pointer.as_ptr(), id, host.as_ptr(), peer.port()) },
                "{}",
                state.api.failure()
            );
            Ok(())
        })
    }
    pub fn stats(&self, id: u64) -> Result<Stats> {
        self.call(|state| {
            let mut stats = Stats::default();
            ensure!(
                unsafe { (state.api.stats)(state.pointer.as_ptr(), id, &mut stats) },
                "{}",
                state.api.failure()
            );
            ensure!(stats.failed == 0, "{}", state.api.failure());
            Ok(stats)
        })
    }
    pub fn pause(&self, id: u64, paused: bool) -> Result<()> {
        self.call(|state| {
            ensure!(
                unsafe { (state.api.pause)(state.pointer.as_ptr(), id, paused) },
                "{}",
                state.api.failure()
            );
            Ok(())
        })
    }
    pub fn remove(&self, id: u64) -> Result<()> {
        self.call(|state| {
            ensure!(
                unsafe { (state.api.remove)(state.pointer.as_ptr(), id) },
                "{}",
                state.api.failure()
            );
            Ok(())
        })
    }
    pub async fn pause_and_flush(&self, id: u64) -> Result<()> {
        self.pause(id, true)?;
        self.call(|state| {
            ensure!(
                unsafe { (state.api.flush)(state.pointer.as_ptr(), id) },
                "{}",
                state.api.failure()
            );
            Ok(())
        })?;
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let ready = self.call(|state| {
                    let ready = unsafe { (state.api.flushed)(state.pointer.as_ptr(), id) };
                    ensure!(ready >= 0, "{}", state.api.failure());
                    Ok(ready != 0)
                })?;
                if ready {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .context("Torrent disk flush timed out")?
    }
    pub fn limits(&self, download: u64, upload: u64) -> Result<()> {
        self.call(|state| {
            ensure!(
                unsafe {
                    (state.api.limits)(
                        state.pointer.as_ptr(),
                        download.min(i32::MAX as u64) as u32,
                        upload.min(i32::MAX as u64) as u32,
                    )
                },
                "{}",
                state.api.failure()
            );
            Ok(())
        })
    }
    pub fn counts(&self) -> Result<(u64, u64)> {
        self.call(|state| {
            let (mut downloading, mut active) = (0, 0);
            ensure!(
                unsafe {
                    (state.api.counts)(state.pointer.as_ptr(), &mut downloading, &mut active)
                },
                "{}",
                state.api.failure()
            );
            Ok((downloading, active))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DownloadRequest, Engine, Status, TorrentOptions};
    use sha2::{Digest, Sha256};
    use std::time::Duration;

    fn create(path: &Path, mode: u32) -> Vec<u8> {
        let api = Api::open().unwrap();
        let path = CString::new(path.to_str().unwrap()).unwrap();
        let function = unsafe {
            *api._library
                .get::<unsafe extern "C" fn(*const c_char, u32, *mut c_char, usize) -> usize>(
                    b"flux_torrent_create\0",
                )
                .unwrap()
        };
        let size = unsafe { function(path.as_ptr(), mode, std::ptr::null_mut(), 0) };
        assert!(size > 0, "{}", api.failure());
        let mut bytes = vec![0u8; size];
        assert_eq!(
            unsafe { function(path.as_ptr(), mode, bytes.as_mut_ptr().cast(), size) },
            size
        );
        bytes
    }
    fn port(session: &Session) -> u16 {
        session
            .call(|state| {
                let function = unsafe {
                    *state
                        .api
                        ._library
                        .get::<unsafe extern "C" fn(*mut c_void) -> u16>(b"flux_torrent_port\0")?
                };
                Ok(unsafe { function(state.pointer.as_ptr()) })
            })
            .unwrap()
    }
    fn diagnostic(session: &Session) -> String {
        session
            .call(|state| {
                let function = unsafe {
                    *state.api._library.get::<unsafe extern "C" fn(
                        *mut c_void,
                        *mut c_char,
                        usize,
                    ) -> usize>(
                        b"flux_torrent_diagnostic\0"
                    )?
                };
                let mut buffer = vec![0u8; 2048];
                let size = unsafe {
                    function(
                        state.pointer.as_ptr(),
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                    )
                };
                Ok(String::from_utf8_lossy(&buffer[..size.min(buffer.len())]).into_owned())
            })
            .unwrap()
    }
    #[test]
    fn metadata_runtime_survives_worker_thread_exit() {
        const CHILD: &str = "FLUX_TORRENT_LIFETIME_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "torrent_native::tests::metadata_runtime_survives_worker_thread_exit",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Native runtime worker exit failed: {:?}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        for _ in 0..8 {
            std::thread::spawn(|| {
                for _ in 0..4 {
                    assert!(Session::describe(b"invalid torrent metadata").is_err());
                }
            })
            .join()
            .unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn v2_and_hybrid_transfer_verify_sha256_and_seed_lifecycle() {
        for (mode, magnet_source) in [(2, false), (0, false), (2, true)] {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source");
            tokio::fs::create_dir(&source).await.unwrap();
            let payload: Vec<u8> = (0..512 * 1024)
                .map(|offset| (offset * 31 % 251) as u8)
                .collect();
            let file = source.join("fixture.bin");
            tokio::fs::write(&file, &payload).await.unwrap();
            let metainfo = create(&file, mode);
            let info = Session::describe(&metainfo).unwrap();
            assert!(info.v2);
            assert_eq!(info.info_hash.len(), 64);
            let seeder = Session::new(0, 256 * 1024, false).unwrap();
            let handle = seeder.add(&metainfo, false, &source).unwrap();
            seeder
                .start(handle, &source, &TorrentOptions::default(), 0)
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while seeder.stats(handle).unwrap().finished == 0 || port(&seeder) == 0 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .unwrap();
            let torrent = root.path().join("fixture.torrent");
            tokio::fs::write(&torrent, metainfo).await.unwrap();
            let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
                .await
                .unwrap();
            let imported = engine
                .import_torrent_file(
                    torrent,
                    Some("Fixture".into()),
                    None,
                    true,
                    Some(TorrentOptions {
                        initial_peers: vec![format!("127.0.0.1:{}", port(&seeder))
                            .parse()
                            .unwrap()],
                        seeding: Some(crate::seeding::Rule::Infinite),
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
            let task = if magnet_source {
                engine
                    .enqueue(DownloadRequest {
                        url: format!("magnet:?xt=urn:btmh:1220{}", info.info_hash),
                        filename: "MagnetFixture".into(),
                        torrent: imported.request.torrent.clone(),
                        ..Default::default()
                    })
                    .await
                    .unwrap()
            } else {
                imported
            };
            engine.start(&task.id).await.unwrap();
            let completed =
                tokio::time::timeout(Duration::from_secs(15), engine.wait(&task.id)).await;
            let task = match completed {
                Ok(task) => task.unwrap(),
                Err(error) => panic!(
                    "{error}; task={:?}; seeder={:?}; diagnostic={}",
                    engine.task(&task.id).await,
                    seeder.stats(handle),
                    diagnostic(&seeder)
                ),
            };
            assert_eq!(task.status, Status::Complete, "{:?}", task.error);
            assert_eq!(task.torrent.as_ref().unwrap().state, "seeding");
            assert_eq!(
                task.sha256.as_deref(),
                Some(hex::encode(Sha256::digest(&payload)).as_str())
            );
            assert_eq!(
                tokio::fs::read(task.destination.join("fixture.bin"))
                    .await
                    .unwrap(),
                payload
            );
            engine.pause(&task.id).await.unwrap();
            assert_eq!(
                engine.task(&task.id).await.unwrap().torrent.unwrap().state,
                "seed_paused"
            );
            if mode == 2 && !magnet_source {
                seeder.pause(handle, true).unwrap();
                engine.queue(&task.id).await.unwrap();
                engine.start(&task.id).await.unwrap();
                let resumed = tokio::time::timeout(Duration::from_secs(10), engine.wait(&task.id))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(resumed.finished_at, task.finished_at);
                assert_eq!(resumed.torrent.as_ref().unwrap().state, "seeding");
                engine.pause(&task.id).await.unwrap();
                let output = task.destination.join("fixture.bin");
                let mut damaged = tokio::fs::read(&output).await.unwrap();
                damaged[4099] ^= 0xaa;
                tokio::fs::write(&output, damaged).await.unwrap();
                seeder.pause(handle, false).unwrap();
                engine.queue(&task.id).await.unwrap();
                engine.start(&task.id).await.unwrap();
                let repaired = tokio::time::timeout(Duration::from_secs(15), engine.wait(&task.id))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(repaired.status, Status::Complete, "{:?}", repaired.error);
                assert_eq!(tokio::fs::read(&output).await.unwrap(), payload);
                seeder.pause(handle, true).unwrap();
                let mut preferences = engine.settings().await.unwrap();
                preferences.queue_enabled = false;
                engine.save_settings(preferences).await.unwrap();
                engine.shutdown().await.unwrap();
                drop(engine);
                let restored =
                    Engine::open(root.path().join("data"), root.path().join("downloads"))
                        .await
                        .unwrap();
                let cancel = tokio_util::sync::CancellationToken::new();
                let scheduler = {
                    let restored = restored.clone();
                    let cancel = cancel.clone();
                    tokio::spawn(async move { restored.run_scheduler(cancel).await })
                };
                tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        let current = restored.task(&task.id).await.unwrap();
                        if current.status == Status::Complete
                            && current.torrent.as_ref().unwrap().state == "seeding"
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                .unwrap();
                restored.pause(&task.id).await.unwrap();
                cancel.cancel();
                scheduler.await.unwrap().unwrap();
                restored.shutdown().await.unwrap();
            } else {
                engine.shutdown().await.unwrap();
            }
            seeder.remove(handle).unwrap();
        }
    }
}
