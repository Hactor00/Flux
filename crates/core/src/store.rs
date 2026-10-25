use crate::model::{Part, Settings, Status, Task};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub(crate) struct Store(Arc<Mutex<Connection>>, Arc<crate::secrets::Secrets>);

impl Store {
    pub async fn relocate_portable(
        &self,
        old: std::path::PathBuf,
        new: std::path::PathBuf,
    ) -> Result<()> {
        self.run(move |db, _| {
            let transaction = db.transaction()?;
            let tasks: Vec<(String, String)> = {
                let mut statement = transaction.prepare("SELECT id,body FROM tasks")?;
                let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            for (id, body) in tasks {
                let mut value: serde_json::Value = serde_json::from_str(&body)?;
                crate::portable::relocate(&mut value, &old, &new);
                transaction.execute(
                    "UPDATE tasks SET body=?2,destination=?3 WHERE id=?1",
                    params![
                        id,
                        serde_json::to_string(&value)?,
                        value["destination"]
                            .as_str()
                            .context("Missing task destination")?
                    ],
                )?;
            }
            let settings: Option<String> = transaction
                .query_row("SELECT body FROM settings WHERE id=1", [], |row| row.get(0))
                .ok();
            if let Some(body) = settings {
                let mut value = serde_json::from_str(&body)?;
                crate::portable::relocate(&mut value, &old, &new);
                transaction.execute(
                    "UPDATE settings SET body=?1 WHERE id=1",
                    [serde_json::to_string(&value)?],
                )?;
            }
            transaction.commit()?;
            Ok(())
        })
        .await
    }
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let schema: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        anyhow::ensure!(schema <= 2, "Database was created by a newer engine");
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY, destination TEXT UNIQUE NOT NULL, body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS parts(task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE, idx INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY(task_id,idx));
            CREATE TABLE IF NOT EXISTS settings(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS host_accounts(provider TEXT PRIMARY KEY, enabled INTEGER NOT NULL CHECK(enabled IN (0,1)), secret BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS pending_update(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS torrent_watch_imports(fingerprint TEXT PRIMARY KEY, task_id TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS tasks_schedule ON tasks(json_extract(body,'$.status'),json_extract(body,'$.request.priority'));
            PRAGMA user_version=2;")?;
        let secrets = crate::secrets::Secrets::open(
            path.parent().context("Database directory unavailable")?,
        )?;
        Ok(Self(Arc::new(Mutex::new(conn)), Arc::new(secrets)))
    }
    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection, &crate::secrets::Secrets) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let db = self.0.clone();
        let secrets = self.1.clone();
        tokio::task::spawn_blocking(move || {
            let mut connection = db
                .lock()
                .map_err(|_| anyhow::anyhow!("Database lock poisoned"))?;
            f(&mut connection, &secrets)
        })
        .await?
    }
    pub fn unprotect_credentials(&self, protected: &str) -> Result<Vec<crate::BrowserCredentials>> {
        self.1.unprotect_credentials(protected)
    }
    pub async fn host_accounts(&self) -> Result<Vec<crate::debrid::AccountStatus>> {
        self.run(|db, _| {
            crate::debrid::Provider::ALL
                .into_iter()
                .map(|provider| {
                    let enabled = db.query_row(
                        "SELECT enabled FROM host_accounts WHERE provider=?1",
                        [provider.name()],
                        |row| row.get::<_, bool>(0),
                    );
                    match enabled {
                        Ok(enabled) => Ok(crate::debrid::AccountStatus {
                            provider,
                            enabled,
                            configured: true,
                        }),
                        Err(rusqlite::Error::QueryReturnedNoRows) => {
                            Ok(crate::debrid::AccountStatus {
                                provider,
                                enabled: false,
                                configured: false,
                            })
                        }
                        Err(error) => Err(error.into()),
                    }
                })
                .collect()
        })
        .await
    }
    pub async fn save_update_session(&self, queue: bool, ids: Vec<String>) -> Result<()> {
        self.run(move |db, _| {
            db.execute(
                "INSERT INTO pending_update(id,body) VALUES(1,?1)",
                [serde_json::to_string(&(queue, ids))?],
            )?;
            Ok(())
        })
        .await
    }
    pub async fn restore_update_session(&self) -> Result<()> {
        self.run(|db, _| {
            use rusqlite::OptionalExtension;
            let body: Option<String> = db.query_row("SELECT body FROM pending_update WHERE id=1", [], |row| row.get(0)).optional()?;
            if let Some(body) = body {
                let (queue, ids): (bool, Vec<String>) = serde_json::from_str(&body)?;
                let tx = db.transaction()?;
                for id in ids {
                    tx.execute("UPDATE tasks SET body=json_set(body,'$.status','queued') WHERE id=?1 AND json_extract(body,'$.status')='paused'", [&id])?;
                    tx.execute("UPDATE tasks SET body=json_set(body,'$.status','queued','$.torrent.state','seeding') WHERE id=?1 AND json_extract(body,'$.status')='complete' AND json_extract(body,'$.torrent.state')='seed_paused'", [&id])?;
                }
                tx.execute("UPDATE settings SET body=json_set(body,'$.queue_enabled',json(?1)) WHERE id=1", [if queue { "true" } else { "false" }])?;
                tx.execute("DELETE FROM pending_update WHERE id=1", [])?;
                tx.commit()?;
            }
            Ok(())
        }).await
    }
    pub async fn save_host_account(
        &self,
        provider: crate::debrid::Provider,
        enabled: bool,
        key: Option<String>,
    ) -> Result<()> {
        if let Some(key) = &key {
            crate::debrid::validate_key(key)?;
        }
        self.run(move |db, secrets| {
            match key {
                Some(key) if key.is_empty() => { db.execute("DELETE FROM host_accounts WHERE provider=?1", [provider.name()])?; }
                Some(key) => {
                    let encrypted = secrets.seal(key.as_bytes())?;
                    db.execute("INSERT INTO host_accounts(provider,enabled,secret) VALUES(?1,?2,?3) ON CONFLICT(provider) DO UPDATE SET enabled=excluded.enabled,secret=excluded.secret", params![provider.name(), enabled, encrypted])?;
                }
                None => { db.execute("UPDATE host_accounts SET enabled=?2 WHERE provider=?1", params![provider.name(), enabled])?; }
            }
            Ok(())
        }).await
    }
    pub async fn host_keys(&self) -> Result<Vec<(crate::debrid::Provider, String)>> {
        self.run(|db, secrets| {
            let mut statement = db.prepare(
                "SELECT provider,secret FROM host_accounts WHERE enabled=1 ORDER BY provider",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;
            rows.map(|row| {
                let (name, encrypted) = row?;
                let provider = crate::debrid::Provider::ALL
                    .into_iter()
                    .find(|provider| provider.name() == name)
                    .context("Unknown host account")?;
                let key = String::from_utf8(secrets.unseal(&encrypted)?)?;
                crate::debrid::validate_key(&key)?;
                Ok((provider, key))
            })
            .collect()
        })
        .await
    }
    pub async fn watch_imported(&self, fingerprint: String) -> Result<bool> {
        self.run(move |db, _| {
            Ok(db.query_row(
                "SELECT EXISTS(SELECT 1 FROM torrent_watch_imports WHERE fingerprint=?1)",
                [fingerprint],
                |row| row.get(0),
            )?)
        })
        .await
    }
    pub async fn task_exists(&self, id: String) -> Result<bool> {
        self.run(move |db, _| {
            Ok(db.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
                [id],
                |row| row.get(0),
            )?)
        })
        .await
    }
    pub async fn insert_watched(&self, task: Task, fingerprint: Option<String>) -> Result<()> {
        self.run(move |db, _secrets| {
            let tx = db.transaction()?;
            tx.execute(
                "INSERT INTO tasks(id,destination,body) VALUES(?1,?2,?3)",
                params![
                    task.id,
                    task.destination.to_string_lossy(),
                    _secrets.encode_task(&task)?
                ],
            )?;
            if let Some(fingerprint) = fingerprint {
                tx.execute(
                    "INSERT INTO torrent_watch_imports(fingerprint,task_id) VALUES(?1,?2)",
                    params![fingerprint, task.id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }
    pub async fn insert_many(&self, tasks: Vec<Task>) -> Result<()> {
        self.run(move |db, _secrets| {
            let tx = db.transaction()?;
            for task in tasks {
                tx.execute(
                    "INSERT INTO tasks(id,destination,body) VALUES(?1,?2,?3)",
                    params![
                        task.id,
                        task.destination.to_string_lossy(),
                        _secrets.encode_task(&task)?
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }
    pub async fn save(&self, task: Task) -> Result<()> {
        self.run(move |db, _secrets| {
            anyhow::ensure!(
                db.execute(
                    "UPDATE tasks SET body=?2,destination=?3 WHERE id=?1",
                    params![
                        task.id,
                        _secrets.encode_task(&task)?,
                        task.destination.to_string_lossy()
                    ]
                )? == 1,
                "Task not found"
            );
            Ok(())
        })
        .await
    }
    pub async fn get(&self, id: &str) -> Result<Task> {
        let id = id.to_owned();
        self.run(move |db, _secrets| {
            let body: String = db
                .query_row("SELECT body FROM tasks WHERE id=?1", [id], |r| r.get(0))
                .context("Task not found")?;
            _secrets.decode_task(&body)
        })
        .await
    }
    pub async fn source_validated(&self, id: String) -> Result<()> {
        self.run(move |db, _| {
            db.execute("UPDATE tasks SET body=json_set(body,'$.source_validated',json('true')) WHERE id=?1", [id])?;
            Ok(())
        }).await
    }
    pub async fn media_progress(&self, id: String, progress: crate::Progress) -> Result<()> {
        self.run(move |db, _| {
            db.execute("UPDATE tasks SET body=json_set(body,'$.completed',?2,'$.total',?3,'$.total_estimated',json(?4),'$.media_progress',json(?5)) WHERE id=?1 AND json_extract(body,'$.status')='running'",
                params![id, i64::try_from(progress.completed)?, progress.total.map(i64::try_from).transpose()?, if progress.total_estimated { "true" } else { "false" }, serde_json::to_string(&progress.media)?])?;
            Ok(())
        }).await
    }
    pub async fn list_page(&self, offset: usize, limit: usize) -> Result<(usize, Vec<Task>)> {
        self.run(move |db, _| {
            let total: i64 = db.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
            let mut statement =
                db.prepare("SELECT body FROM tasks ORDER BY rowid DESC LIMIT ?1 OFFSET ?2")?;
            let rows = statement.query_map(params![limit as i64, offset as i64], |row| {
                row.get::<_, String>(0)
            })?;
            Ok((
                total.try_into()?,
                rows.map(|row| Ok(serde_json::from_str(&row?)?))
                    .collect::<Result<Vec<_>>>()?,
            ))
        })
        .await
    }
    pub async fn list(&self) -> Result<Vec<Task>> {
        self.run(|db, _secrets| {
            let mut statement = db.prepare("SELECT body FROM tasks ORDER BY rowid DESC")?;
            let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
            rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
        })
        .await
    }
    pub async fn seeds_to_restore(&self, limit: u8) -> Result<Vec<String>> {
        self.run(move |db, _| {
            let mut statement = db.prepare("SELECT id FROM tasks WHERE json_extract(body,'$.status')='queued' AND json_extract(body,'$.torrent.state')='seeding' LIMIT ?1")?;
            let rows = statement.query_map([limit], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        }).await
    }
    pub async fn schedule(&self, now: u64, limit: u8) -> Result<(Vec<String>, Vec<String>)> {
        self.run(move|db,_|{
            let mut stops=db.prepare("SELECT id FROM tasks WHERE json_extract(body,'$.status') IN ('queued','running','verifying') AND json_extract(body,'$.request.stop_at')<=?1")?;
            let expired=stops.query_map([now as i64],|row|row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
            let mut ready=db.prepare("SELECT id FROM tasks WHERE json_extract(body,'$.status')='queued' AND (json_extract(body,'$.request.start_at') IS NULL OR json_extract(body,'$.request.start_at')<=?1) AND (json_extract(body,'$.request.stop_at') IS NULL OR json_extract(body,'$.request.stop_at')>?1) ORDER BY COALESCE(json_extract(body,'$.request.priority'),0) DESC,rowid ASC LIMIT ?2")?;
            let ready=ready.query_map(params![now as i64,limit],|row|row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
            Ok((expired,ready))
        }).await
    }
    pub async fn destinations(&self) -> Result<Vec<std::path::PathBuf>> {
        self.run(|db, _| {
            let mut statement = db.prepare("SELECT destination FROM tasks")?;
            let paths = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .map(std::path::PathBuf::from)
                .collect();
            Ok(paths)
        })
        .await
    }

    pub async fn settings(&self) -> Result<Settings> {
        self.run(|db, _secrets| {
            use rusqlite::OptionalExtension;
            let body: Option<String> = db
                .query_row("SELECT body FROM settings WHERE id=1", [], |r| r.get(0))
                .optional()?;
            match body {
                Some(body) => Ok(serde_json::from_str(&body)?),
                None => Ok(Settings::default()),
            }
        })
        .await
    }
    pub async fn save_settings(&self, settings: Settings) -> Result<()> {
        self.run(move |db, _secrets| { db.execute("INSERT INTO settings(id,body) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET body=excluded.body", [serde_json::to_string(&settings)?])?; Ok(()) }).await
    }
    pub async fn remove(&self, id: String) -> Result<()> {
        self.run(move |db, _secrets| {
            anyhow::ensure!(
                db.execute("DELETE FROM tasks WHERE id=?1", [id])? == 1,
                "Task not found"
            );
            Ok(())
        })
        .await
    }
    pub async fn split(&self, id: String, prefix: Part, tail: Part) -> Result<()> {
        self.run(move |db, _secrets| {
            let tx = db.transaction()?;
            tx.execute(
                "UPDATE parts SET body=?3 WHERE task_id=?1 AND idx=?2",
                params![id, prefix.index, serde_json::to_string(&prefix)?],
            )?;
            tx.execute(
                "INSERT INTO parts(task_id,idx,body) VALUES(?1,?2,?3)",
                params![id, tail.index, serde_json::to_string(&tail)?],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }
    pub async fn parts(&self, id: &str) -> Result<Vec<Part>> {
        let id = id.to_owned();
        self.run(move |db, _secrets| {
            let mut s = db.prepare("SELECT body FROM parts WHERE task_id=?1 ORDER BY idx")?;
            let rows = s.query_map([id], |r| r.get::<_, String>(0))?;
            let mut parts: Vec<Part> = rows
                .map(|r| Ok(serde_json::from_str(&r?)?))
                .collect::<Result<_>>()?;
            parts.sort_by_key(|part| part.start);
            Ok(parts)
        })
        .await
    }
    pub async fn plan(&self, task: Task, parts: Vec<Part>) -> Result<()> {
        self.run(move |db, _secrets| {
            let tx = db.transaction()?;
            tx.execute("DELETE FROM parts WHERE task_id=?1", [&task.id])?;
            for part in parts {
                tx.execute(
                    "INSERT INTO parts(task_id,idx,body) VALUES(?1,?2,?3)",
                    params![task.id, part.index, serde_json::to_string(&part)?],
                )?;
            }
            tx.execute(
                "UPDATE tasks SET body=?2 WHERE id=?1",
                params![task.id, _secrets.encode_task(&task)?],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }
    pub async fn checkpoint(&self, id: String, part: Part) -> Result<()> {
        self.run(move |db, _secrets| {
            db.execute(
                "UPDATE parts SET body=?3 WHERE task_id=?1 AND idx=?2",
                params![id, part.index, serde_json::to_string(&part)?],
            )?;
            Ok(())
        })
        .await
    }
    pub async fn recover(&self) -> Result<()> {
        self.run(|db, _| {
            db.execute("UPDATE tasks SET body=json_set(body,'$.status','queued') WHERE json_extract(body,'$.status')='complete' AND json_extract(body,'$.torrent.state')='seeding'", [])?;
            Ok(())
        }).await?;
        for mut task in self.list().await? {
            if task.status == Status::Complete && task.finished_at.is_none() {
                if let Ok(metadata) = tokio::fs::metadata(&task.destination).await {
                    if let Ok(time) = metadata.modified().and_then(|time| {
                        time.duration_since(std::time::UNIX_EPOCH)
                            .map_err(std::io::Error::other)
                    }) {
                        task = self.get(&task.id).await?;
                        task.finished_at = Some(time.as_secs());
                        task.timestamp_estimated = true;
                        self.save(task.clone()).await?;
                    }
                }
            }
            if matches!(task.status, Status::Running | Status::Verifying) {
                task = self.get(&task.id).await?;
                task.status = Status::Paused;
                task.source_validated = false;
                if !crate::media::is_media(&task.request) {
                    task.completed = self.parts(&task.id).await?.iter().map(|p| p.done).sum();
                }
                self.save(task).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod portable_tests {
    use super::*;
    #[tokio::test]
    async fn relocating_portable_data_preserves_external_paths_and_encrypted_sessions() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(&temporary.path().join("tasks.sqlite")).unwrap();
        let old = temporary.path().join("Original");
        let new = temporary.path().join("Moved");
        let original = serde_json::json!({"destination":old.join("Data/Downloads/movie.mp4"),"protected_credentials":"unchanged-ciphertext","external":temporary.path().join("outside.mp4")});
        let body = serde_json::to_string(&original).unwrap();
        let destination = original["destination"].as_str().unwrap().to_owned();
        store
            .run(move |db, _| {
                db.execute(
                    "INSERT INTO tasks(id,destination,body) VALUES('sample',?1,?2)",
                    params![destination, body],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        store.relocate_portable(old, new.clone()).await.unwrap();
        let body: String = store
            .run(|db, _| {
                Ok(
                    db.query_row("SELECT body FROM tasks WHERE id='sample'", [], |row| {
                        row.get(0)
                    })?,
                )
            })
            .await
            .unwrap();
        let moved: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            moved["destination"],
            new.join("Data/Downloads/movie.mp4")
                .to_string_lossy()
                .as_ref()
        );
        assert_eq!(
            moved["protected_credentials"],
            original["protected_credentials"]
        );
        assert_eq!(moved["external"], original["external"]);
    }
}
