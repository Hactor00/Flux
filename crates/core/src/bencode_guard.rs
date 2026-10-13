use anyhow::{ensure, Context, Result};

pub(crate) fn validate(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.first() == Some(&b'd'),
        "Torrent metadata must be a dictionary"
    );
    let mut offset = 0usize;
    let mut stack = Vec::new();
    let mut tokens = 0usize;
    let mut closed = false;
    while offset < bytes.len() {
        ensure!(!closed, "Trailing data in torrent metadata");
        let byte = bytes[offset];
        tokens += 1;
        ensure!(
            tokens <= 1_000_000,
            "Torrent metadata contains too many entries"
        );
        match byte {
            b'd' | b'l' => {
                stack.push(byte);
                ensure!(
                    stack.len() <= 64,
                    "Torrent metadata exceeds the nesting limit"
                );
                offset += 1;
            }
            b'e' => {
                stack.pop().context("Invalid torrent metadata nesting")?;
                offset += 1;
                closed = stack.is_empty();
            }
            b'i' => {
                let end = bytes[offset + 1..]
                    .iter()
                    .take(22)
                    .position(|byte| *byte == b'e')
                    .context("Invalid torrent integer")?
                    + offset
                    + 1;
                let number = std::str::from_utf8(&bytes[offset + 1..end])?;
                ensure!(
                    !number.is_empty()
                        && number != "-0"
                        && !number.starts_with('+')
                        && !(number.len() > 1 && number.starts_with('0')),
                    "Invalid torrent integer encoding"
                );
                number.parse::<i64>()?;
                offset = end + 1;
            }
            b'0'..=b'9' => {
                let colon = bytes[offset..]
                    .iter()
                    .take(21)
                    .position(|byte| *byte == b':')
                    .context("Invalid torrent string length")?
                    + offset;
                let number = std::str::from_utf8(&bytes[offset..colon])?;
                ensure!(
                    !(number.len() > 1 && number.starts_with('0')),
                    "Invalid torrent string length encoding"
                );
                let length: usize = number.parse()?;
                offset = colon
                    .checked_add(1)
                    .and_then(|start| start.checked_add(length))
                    .context("Torrent string length overflow")?;
                ensure!(offset <= bytes.len(), "Truncated torrent metadata string");
            }
            _ => anyhow::bail!("Invalid torrent metadata token"),
        }
    }
    ensure!(closed && stack.is_empty(), "Truncated torrent metadata");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_strings_are_skipped_and_nesting_is_bounded() {
        assert!(validate(b"d4:info5:dieele").is_ok());
        assert!(validate(b"d4:info100:short").is_err());
        assert!(validate(b"d4:infoi-0ee").is_err());
        assert!(validate(b"d4:infoleeextra").is_err());
        let nested = format!("d4:info{}{}e", "l".repeat(100), "e".repeat(100));
        assert!(validate(nested.as_bytes()).is_err());
    }
}
