use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, State};
use tokio::sync::Mutex;

const DAY: u64 = 86_400;
const ENDPOINT: &str = "https://fluxdownload.org/usage/collect.php";
const NOTICE: &str = "2026-10-26";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    #[default]
    Unasked,
    Enabled,
    Later,
    Declined,
    Never,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    choice: Choice,
    remind_after: u64,
    installation_id: Option<String>,
    pending_deletes: Vec<String>,
    last_sent_day: Option<u64>,
    last_attempt: u64,
    last_success: Option<u64>,
    notice_version: String,
}

#[derive(Clone, Serialize)]
pub struct Status {
    pub choice: Choice,
    pub enabled: bool,
    pub prompt_due: bool,
    pub remind_after: u64,
    pub last_success: Option<u64>,
    pub deletion_pending: bool,
    pub notice_version: &'static str,
}

impl Preferences {
    fn status(&self, now: u64) -> Status {
        Status {
            choice: self.choice,
            enabled: self.choice == Choice::Enabled,
            prompt_due: matches!(
                self.choice,
                Choice::Unasked | Choice::Later | Choice::Declined
            ) && now >= self.remind_after,
            remind_after: self.remind_after,
            last_success: self.last_success,
            deletion_pending: !self.pending_deletes.is_empty(),
            notice_version: NOTICE,
        }
    }

    fn choose(&mut self, choice: Choice, now: u64) -> Result<(), String> {
        if choice == Choice::Unasked {
            return Err("Choose an explicit statistics preference".into());
        }
        if self.choice == Choice::Enabled && choice != Choice::Enabled {
            if let Some(id) = self.installation_id.take() {
                self.pending_deletes.push(id);
            }
            self.last_sent_day = None;
            self.last_success = None;
            self.last_attempt = 0;
        }
        if choice == Choice::Enabled {
            if self.installation_id.is_none() {
                self.installation_id = Some(uuid::Uuid::new_v4().to_string());
            }
            self.notice_version = NOTICE.into();
        }
        self.choice = choice;
        self.remind_after = match choice {
            Choice::Later => now.saturating_add(7 * DAY),
            Choice::Declined => now.saturating_add(45 * DAY),
            _ => 0,
        };
        Ok(())
    }
}

#[derive(Clone)]
pub struct Statistics {
    path: PathBuf,
    preferences: Arc<Mutex<Preferences>>,
    client: reqwest::Client,
    endpoint: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Statistics {
    pub fn open(data: PathBuf) -> Result<Self, String> {
        let path = data.join("usage-preferences.json");
        let preferences = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 16_384 => {
                serde_json::from_slice(&bytes).unwrap_or_default()
            }
            _ => Preferences::default(),
        };
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(5))
            .user_agent(concat!("Flux/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            path,
            preferences: Arc::new(Mutex::new(preferences)),
            client,
            endpoint: ENDPOINT.into(),
        })
    }

    async fn save(&self, value: &Preferences) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        crate::install::write_changed(&self.path, &bytes).map_err(|e| e.to_string())
    }

    pub async fn status(&self) -> Status {
        self.preferences.lock().await.status(now())
    }

    pub async fn choose(&self, choice: Choice) -> Result<Status, String> {
        let mut value = self.preferences.lock().await;
        let mut next = value.clone();
        next.choose(choice, now())?;
        self.save(&next).await?;
        *value = next;
        Ok(value.status(now()))
    }

    pub async fn tick(&self) -> Result<Status, String> {
        let mut value = self.preferences.lock().await;
        let timestamp = now();
        if value.last_attempt > 0 && timestamp < value.last_attempt.saturating_add(6 * 3600) {
            return Ok(value.status(timestamp));
        }
        if value.pending_deletes.is_empty()
            && (value.choice != Choice::Enabled || value.last_sent_day == Some(timestamp / DAY))
        {
            return Ok(value.status(timestamp));
        }
        value.last_attempt = timestamp;
        self.save(&value).await?;
        if let Some(id) = value.pending_deletes.first().cloned() {
            let response = self
                .client
                .post(&self.endpoint)
                .json(&serde_json::json!({"action":"delete","installation_id":id}))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if response.status() != reqwest::StatusCode::NO_CONTENT {
                return Err("Statistics service is not ready to delete the record".into());
            }
            value.pending_deletes.remove(0);
            value.last_attempt = 0;
            self.save(&value).await?;
            if !value.pending_deletes.is_empty() {
                return Ok(value.status(timestamp));
            }
        }
        if value.choice == Choice::Enabled && value.last_sent_day != Some(timestamp / DAY) {
            let id = value
                .installation_id
                .as_deref()
                .ok_or("Missing statistics consent identity")?;
            let response = self
                .client
                .post(&self.endpoint)
                .json(&serde_json::json!({
                    "action":"active", "installation_id":id,
                    "version":env!("CARGO_PKG_VERSION"), "platform":std::env::consts::OS,
                    "notice_version":NOTICE
                }))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if response.status() != reqwest::StatusCode::NO_CONTENT {
                return Err(
                    "Statistics service is not configured or temporarily unavailable".into(),
                );
            }
            value.last_sent_day = Some(timestamp / DAY);
            value.last_success = Some(timestamp);
            self.save(&value).await?;
        }
        value.last_attempt = 0;
        self.save(&value).await?;
        Ok(value.status(timestamp))
    }
}

#[tauri::command]
pub async fn statistics_status(state: State<'_, Statistics>) -> Result<Status, String> {
    Ok(state.status().await)
}

#[tauri::command]
pub async fn statistics_choice(
    app: tauri::AppHandle,
    choice: Choice,
    state: State<'_, Statistics>,
) -> Result<Status, String> {
    let status = state.choose(choice).await?;
    let _ = app.emit("usage-preferences-changed", &status);
    Ok(status)
}

#[tauri::command]
pub async fn statistics_tick(
    window: tauri::WebviewWindow,
    state: State<'_, Statistics>,
) -> Result<Status, String> {
    if window.label() != "main"
        || !window.is_visible().unwrap_or(false)
        || !window.is_focused().unwrap_or(false)
    {
        return Ok(state.status().await);
    }
    state.tick().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_identity_or_transmission_permission_before_explicit_acceptance() {
        let value = Preferences::default();
        assert!(!value.status(1).enabled);
        assert!(value.installation_id.is_none());
        assert!(value.status(1).prompt_due);
        let public = serde_json::to_string(&value.status(1)).unwrap();
        assert!(!public.contains("installation_id"));
    }

    #[test]
    fn reminders_are_seven_and_forty_five_days_and_never_is_persistent() {
        let mut value = Preferences::default();
        value.choose(Choice::Later, 100).unwrap();
        assert!(!value.status(100 + 7 * DAY - 1).prompt_due);
        assert!(value.status(100 + 7 * DAY).prompt_due);
        value.choose(Choice::Declined, 100).unwrap();
        assert!(!value.status(100 + 45 * DAY - 1).prompt_due);
        assert!(value.status(100 + 45 * DAY).prompt_due);
        value.choose(Choice::Never, 100).unwrap();
        let restored: Preferences =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(!restored.status(u64::MAX).prompt_due);
        assert!(restored.installation_id.is_none());
    }

    #[test]
    fn withdrawing_consent_stops_counting_and_schedules_deletion() {
        let mut value = Preferences::default();
        value.choose(Choice::Enabled, 100).unwrap();
        let original = value.installation_id.clone();
        value.choose(Choice::Never, 200).unwrap();
        assert!(!value.status(200).enabled);
        assert!(value.installation_id.is_none());
        assert_eq!(value.pending_deletes.first(), original.as_ref());
        value.choose(Choice::Enabled, 300).unwrap();
        assert_ne!(value.installation_id, original);
        value.choose(Choice::Never, 400).unwrap();
        assert_eq!(value.pending_deletes.len(), 2);
    }

    #[tokio::test]
    async fn refusing_does_not_contact_the_network_and_survives_restart() {
        let path = std::env::temp_dir().join(format!("flux-usage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        let mut statistics = Statistics::open(path.clone()).unwrap();
        statistics.endpoint = "http://127.0.0.1:1/not-contacted".into();
        statistics.choose(Choice::Never).await.unwrap();
        assert!(!statistics.tick().await.unwrap().enabled);
        assert_eq!(
            Statistics::open(path.clone())
                .unwrap()
                .status()
                .await
                .choice,
            Choice::Never
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}
