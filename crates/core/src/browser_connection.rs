use anyhow::{ensure, Result};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
pub struct BrowserConnections(Arc<Mutex<BTreeMap<uuid::Uuid, String>>>);

pub struct BrowserConnection {
    connections: BrowserConnections,
    id: uuid::Uuid,
}

impl BrowserConnections {
    pub fn connect(&self, version: &str) -> Result<BrowserConnection> {
        ensure!(
            version == "unknown" || valid_version(version),
            "Invalid extension version"
        );
        let mut connections = self.0.lock().unwrap_or_else(|error| error.into_inner());
        ensure!(connections.len() < 16, "Too many browser connections");
        let id = uuid::Uuid::new_v4();
        connections.insert(id, version.into());
        Ok(BrowserConnection {
            connections: self.clone(),
            id,
        })
    }

    pub fn versions(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .cloned()
            .collect()
    }
}

impl Drop for BrowserConnection {
    fn drop(&mut self) {
        self.connections
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id);
    }
}

pub fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && version.split('.').count() <= 4
        && version.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u16>().is_ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presence_exists_only_while_a_browser_socket_is_alive() {
        let connections = BrowserConnections::default();
        assert!(connections.versions().is_empty());
        let chrome = connections.connect("2.4.2").unwrap();
        let firefox = connections.connect("2.4.1").unwrap();
        assert_eq!(connections.versions().len(), 2);
        drop(chrome);
        assert_eq!(connections.versions(), ["2.4.1"]);
        drop(firefox);
        assert!(connections.versions().is_empty());
        for invalid in ["", "2.4.2evil", "999999.0", "1..0", "1.2.3.4.5"] {
            assert!(connections.connect(invalid).is_err());
        }
        let peers: Vec<_> = (0..16)
            .map(|_| connections.connect("unknown").unwrap())
            .collect();
        assert!(connections.connect("2.4.2").is_err());
        drop(peers);
        assert!(connections.versions().is_empty());
    }
}
