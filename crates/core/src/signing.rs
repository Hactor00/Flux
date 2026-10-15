use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use std::{io::Read, path::Path};

pub fn verify_update(
    path: &Path,
    encoded_key: &str,
    encoded_signature: &str,
    expected_version: Option<&str>,
) -> Result<()> {
    ensure!(
        encoded_key.len() <= 4096 && encoded_signature.len() <= 8192,
        "Update signing metadata exceeds the size limit"
    );
    let key = String::from_utf8(STANDARD.decode(encoded_key.trim())?)?;
    let signature = String::from_utf8(STANDARD.decode(encoded_signature.trim())?)?;
    let public_key = PublicKey::decode(&key).context("Invalid update public key")?;
    let signature = Signature::decode(&signature).context("Invalid update signature")?;
    let mut verifier = public_key
        .verify_stream(&signature)
        .context("Unsupported update signature format")?;
    let mut file = std::fs::File::open(path)?;
    let mut bytes = vec![0u8; 256 * 1024];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        verifier.update(&bytes[..count]);
    }
    verifier
        .finalize()
        .context("Update signature verification failed")?;
    if let Some(expected) = expected_version {
        let version = signature
            .trusted_comment()
            .split('\t')
            .find_map(|field| field.strip_prefix("version:"));
        ensure!(
            version == Some(expected),
            "Update signed version does not match release metadata"
        );
    }
    Ok(())
}
