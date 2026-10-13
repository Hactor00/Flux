use crate::Task;
use anyhow::{Context, Result};
use std::path::Path;

pub(crate) struct Secrets {
    #[cfg(not(windows))]
    key: [u8; 32],
}
impl Secrets {
    pub fn open(directory: &Path) -> Result<Self> {
        #[cfg(windows)]
        {
            let _ = directory;
            Ok(Self {})
        }
        #[cfg(not(windows))]
        {
            use std::{
                io::Write,
                os::unix::fs::{OpenOptionsExt, PermissionsExt},
            };
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
            let path = directory.join("credentials.key");
            let key = match std::fs::read(&path) {
                Ok(bytes) => bytes
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid credentials key"))?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let mut key = [0; 32];
                    getrandom::fill(&mut key).map_err(|error| anyhow::anyhow!("{error}"))?;
                    let temporary =
                        directory.join(format!(".credentials-{}.key", uuid::Uuid::new_v4()));
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&temporary)?;
                    file.write_all(&key)?;
                    file.sync_all()?;
                    let published = std::fs::hard_link(&temporary, &path);
                    std::fs::remove_file(&temporary)?;
                    match published {
                        Ok(()) => {
                            std::fs::File::open(directory)?.sync_all()?;
                            key
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                            std::fs::read(&path)?
                                .try_into()
                                .map_err(|_| anyhow::anyhow!("Invalid credentials key"))?
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(error) => return Err(error.into()),
            };
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self { key })
        }
    }
    pub fn protect_credentials(&self, credentials: &[crate::BrowserCredentials]) -> Result<String> {
        let request = crate::DownloadRequest {
            url: "https://example.invalid/".into(),
            filename: "validation.bin".into(),
            credentials: credentials.to_vec(),
            ..crate::DownloadRequest::default()
        };
        crate::model::validate(&request)?;
        Ok(hex::encode(self.seal(&serde_json::to_vec(credentials)?)?))
    }
    pub fn unprotect_credentials(&self, protected: &str) -> Result<Vec<crate::BrowserCredentials>> {
        anyhow::ensure!(
            protected.len() <= 512 * 1024,
            "Protected credentials exceed the size limit"
        );
        Ok(serde_json::from_slice(
            &self.unseal(&hex::decode(protected)?)?,
        )?)
    }
    pub fn encode_task(&self, task: &Task) -> Result<String> {
        let mut value = serde_json::to_value(task)?;
        if !task.request.credentials.is_empty() {
            let bytes = serde_json::to_vec(&task.request.credentials)?;
            value["protected_credentials"] =
                serde_json::Value::String(hex::encode(self.seal(&bytes)?));
        }
        Ok(serde_json::to_string(&value)?)
    }
    pub fn decode_task(&self, body: &str) -> Result<Task> {
        let value: serde_json::Value = serde_json::from_str(body)?;
        let mut task: Task = serde_json::from_value(value.clone())?;
        if let Some(protected) = value["protected_credentials"].as_str() {
            task.request.credentials =
                serde_json::from_slice(&self.unseal(&hex::decode(protected)?)?)
                    .context("Stored authentication cannot be decrypted")?;
        }
        Ok(task)
    }
    #[cfg(windows)]
    fn seal(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        windows_protect(bytes, true)
    }
    #[cfg(windows)]
    fn unseal(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        windows_protect(bytes, false)
    }
    #[cfg(not(windows))]
    fn seal(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce).map_err(|error| anyhow::anyhow!("{error}"))?;
        let cipher = ChaCha20Poly1305::new((&self.key).into());
        let sealed = cipher
            .encrypt(Nonce::from_slice(&nonce), bytes)
            .map_err(|_| anyhow::anyhow!("Cannot encrypt authentication"))?;
        Ok([nonce.as_slice(), sealed.as_slice()].concat())
    }
    #[cfg(not(windows))]
    fn unseal(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
        anyhow::ensure!(bytes.len() >= 28, "Invalid protected authentication");
        ChaCha20Poly1305::new((&self.key).into())
            .decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..])
            .map_err(|_| anyhow::anyhow!("Cannot decrypt authentication"))
    }
}
#[cfg(windows)]
fn windows_protect(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into()?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let entropy = b"Flux credentials v1";
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr().cast_mut(),
    };
    // DPAPI binds the ciphertext to this Windows account. UI prompts are forbidden in workers.
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    anyhow::ensure!(
        success != 0,
        "Windows credential protection failed: {}",
        std::io::Error::last_os_error()
    );
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}
