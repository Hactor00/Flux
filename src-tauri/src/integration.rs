use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

const REMINDER: &str = "browser-integration.seen";
const ACKNOWLEDGED: &[u8] = b"1\n";

#[derive(Clone)]
pub struct BrowserIntegration {
    data: PathBuf,
    registration: Arc<Mutex<Option<Result<String, String>>>>,
}

#[derive(Clone, Serialize)]
pub struct IntegrationStatus {
    pub registered: bool,
    pub connected: bool,
    pub extension_versions: Vec<String>,
    pub extension_id: Option<String>,
    pub error: Option<String>,
    pub reminder_dismissed: bool,
}

impl BrowserIntegration {
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            registration: Arc::default(),
        }
    }

    pub async fn status(&self, repair: bool) -> Result<IntegrationStatus, String> {
        let mut registration = self.registration.lock().await;
        if repair || registration.is_none() {
            *registration = Some(
                tauri::async_runtime::spawn_blocking(crate::install::register)
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result.map_err(|error| error.to_string())),
            );
        }
        let result = registration.as_ref().ok_or("Registration unavailable")?;
        let (extension_id, error) = match result {
            Ok(id) => (Some(id.clone()), None),
            Err(error) => (None, Some(error.clone())),
        };
        drop(registration);
        let reminder_dismissed = match tokio::fs::read(self.data.join(REMINDER)).await {
            Ok(bytes) => bytes == ACKNOWLEDGED,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.to_string()),
        };
        Ok(IntegrationStatus {
            registered: extension_id.is_some(),
            connected: false,
            extension_versions: Vec::new(),
            extension_id,
            error,
            reminder_dismissed,
        })
    }

    pub async fn dismiss(&self) -> Result<(), String> {
        let path = self.data.join(REMINDER);
        tauri::async_runtime::spawn_blocking(move || {
            crate::install::write_changed(&path, ACKNOWLEDGED).map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registration_failure_keeps_the_guide_available_and_dismissal_survives_restart() {
        let data = std::env::temp_dir().join(format!("flux-integration-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir(&data).await.unwrap();
        let integration = BrowserIntegration::new(data.clone());
        *integration.registration.lock().await = Some(Err("Native host is missing".into()));
        let status = integration.status(false).await.unwrap();
        assert!(!status.registered);
        assert_eq!(status.error.as_deref(), Some("Native host is missing"));
        assert!(!status.reminder_dismissed);
        assert!(!data.join(REMINDER).exists());
        integration.dismiss().await.unwrap();
        let first = std::fs::metadata(data.join(REMINDER))
            .unwrap()
            .modified()
            .unwrap();
        integration.dismiss().await.unwrap();
        assert_eq!(
            first,
            std::fs::metadata(data.join(REMINDER))
                .unwrap()
                .modified()
                .unwrap()
        );
        let restarted = BrowserIntegration::new(data.clone());
        *restarted.registration.lock().await = Some(Ok("extension-id".into()));
        let status = restarted.status(false).await.unwrap();
        assert!(status.registered);
        assert!(status.reminder_dismissed);
        assert!(status.error.is_none());
        tokio::fs::remove_dir_all(data).await.unwrap();
    }
}
