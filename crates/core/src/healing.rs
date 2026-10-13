//! Requests are short-lived capabilities, replay-safe and bound to one browser page.
use crate::{BrowserCredentials, BrowserSource};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub ticket: String,
    pub task_id: String,
    pub page_url: String,
    pub frame_url: Option<String>,
    pub url: String,
}
#[derive(Clone)]
pub(crate) struct Renewed {
    pub url: String,
    pub credentials: Vec<BrowserCredentials>,
}
struct Pending {
    request: RefreshRequest,
    expires: Instant,
    sender: watch::Sender<Option<Renewed>>,
}
#[derive(Default)]
pub(crate) struct RefreshBroker {
    pending: Mutex<HashMap<String, Pending>>,
}
impl RefreshBroker {
    pub async fn poll(&self) -> Vec<RefreshRequest> {
        let mut pending = self.pending.lock().await;
        pending.retain(|_, item| item.expires > Instant::now() && !item.sender.is_closed());
        let mut size = 2048;
        pending
            .values()
            .filter_map(|item| {
                let bytes = serde_json::to_vec(&item.request).ok()?.len();
                if size + bytes > crate::protocol::MAX_FRAME {
                    return None;
                }
                size += bytes;
                Some(item.request.clone())
            })
            .take(32)
            .collect()
    }
    pub async fn submit(
        &self,
        ticket: &str,
        page_url: &str,
        url: &str,
        credentials: Vec<BrowserCredentials>,
    ) -> Result<()> {
        crate::model::validate_url(url)?;
        let mut pending = self.pending.lock().await;
        let item = pending
            .get(ticket)
            .context("Refresh ticket expired or already consumed")?;
        ensure!(
            item.expires > Instant::now() && page_url == item.request.page_url,
            "Refresh page does not match task"
        );
        // A signature may change, but a different resource may never replace the task.
        ensure!(
            resource_identity(url)? == resource_identity(&item.request.url)?,
            "Refreshed resource identity changed"
        );
        let request = crate::DownloadRequest {
            url: url.into(),
            filename: "refresh.bin".into(),
            credentials: credentials.clone(),
            ..Default::default()
        };
        crate::model::validate(&request)?;
        let sender = pending
            .remove(ticket)
            .context("Refresh ticket expired")?
            .sender;
        let _ = sender.send(Some(Renewed {
            url: url.into(),
            credentials,
        }));
        Ok(())
    }
    pub async fn request(
        self: &Arc<Self>,
        task_id: &str,
        source: &BrowserSource,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<Renewed> {
        let (ticket, mut receiver) = {
            let mut pending = self.pending.lock().await;
            pending.retain(|_, item| item.expires > Instant::now() && !item.sender.is_closed());
            if let Some((ticket, item)) = pending.iter().find(|(_, item)| {
                item.request.task_id == task_id
                    && item.request.page_url == source.page_url
                    && item.request.url == url
            }) {
                (ticket.clone(), item.sender.subscribe())
            } else {
                ensure!(pending.len() < 128, "Too many pending browser refreshes");
                let ticket = uuid::Uuid::new_v4().to_string();
                let (sender, receiver) = watch::channel(None);
                pending.insert(
                    ticket.clone(),
                    Pending {
                        request: RefreshRequest {
                            ticket: ticket.clone(),
                            task_id: task_id.into(),
                            page_url: source.page_url.clone(),
                            frame_url: source.frame_url.clone(),
                            url: url.into(),
                        },
                        expires: Instant::now() + Duration::from_secs(45),
                        sender,
                    },
                );
                (ticket, receiver)
            }
        };
        let result = tokio::select! {
            _ = cancel.cancelled() => Err(anyhow::anyhow!("Cancelled")),
            value = tokio::time::timeout(Duration::from_secs(45), async {
                loop {
                    if let Some(value) = receiver.borrow_and_update().clone() { return Ok(value); }
                    receiver.changed().await.context("Browser source refresh was cancelled")?;
                }
            }) => value.context("Browser source refresh timed out")?,
        };
        drop(receiver);
        self.pending
            .lock()
            .await
            .retain(|key, item| key != &ticket || !item.sender.is_closed());
        result
    }
}
pub(crate) fn resource_identity(value: &str) -> Result<String> {
    let mut url = url::Url::parse(value)?;
    let mut query: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !volatile_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    query.sort();
    url.set_query(None);
    url.set_fragment(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Ok(url.into())
}
fn volatile_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("x-amz-")
        || matches!(
            key.as_str(),
            "signature"
                | "sig"
                | "token"
                | "expires"
                | "expiry"
                | "policy"
                | "key-pair-id"
                | "__token__"
                | "hdnts"
                | "hdnea"
                | "cf_clearance"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rejects_resource_substitution_and_ticket_replay() {
        let broker = Arc::new(RefreshBroker::default());
        let pending = broker.clone();
        let task = tokio::spawn(async move {
            pending
                .request(
                    "task",
                    &BrowserSource {
                        page_url: "https://site.test/watch".into(),
                        frame_url: None,
                    },
                    "https://cdn.test/a.mp4?id=one&X-Amz-Signature=old",
                    &CancellationToken::new(),
                )
                .await
        });
        let request = loop {
            if let Some(value) = broker.poll().await.first() {
                break value.clone();
            }
            tokio::task::yield_now().await;
        };
        assert!(broker
            .submit(
                &request.ticket,
                &request.page_url,
                "https://cdn.test/b.mp4?id=one",
                vec![]
            )
            .await
            .is_err());
        assert!(broker
            .submit(
                &request.ticket,
                &request.page_url,
                "https://cdn.test/a.mp4?id=two",
                vec![]
            )
            .await
            .is_err());
        broker
            .submit(
                &request.ticket,
                &request.page_url,
                "https://cdn.test/a.mp4?id=one&X-Amz-Signature=new",
                vec![],
            )
            .await
            .unwrap();
        assert!(task.await.unwrap().unwrap().url.contains("new"));
        assert!(broker
            .submit(&request.ticket, &request.page_url, &request.url, vec![])
            .await
            .is_err());
    }
}
