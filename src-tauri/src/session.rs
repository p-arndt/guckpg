//! Live connections. One SSH tunnel per saved connection is shared by all its
//! Postgres connections. The schema tree uses one "meta" connection per
//! database; every editor tab gets its own connection, so a transaction or a
//! long query in one tab never blocks another tab or the tree.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::config::{ConnectionConfig, ConnectionStore, ResolvedSecrets, Secrets, resolve_secrets};
use crate::error::{AppError, AppResult};
use crate::pg::{PgConn, Transport};
use crate::ssh::{KnownHosts, Tunnel};

struct TabConn {
    connection_id: String,
    database: String,
    conn: Arc<PgConn>,
}

/// Lazily opened values, one slot per key. Concurrent requests for the same
/// key wait for a single open (the tree fires several catalog calls at once),
/// while opens for different keys do not block each other.
struct Slots<K, V>(Mutex<HashMap<K, Slot<V>>>);

type Slot<V> = Arc<Mutex<Option<Arc<V>>>>;

impl<K: Eq + Hash, V> Default for Slots<K, V> {
    fn default() -> Self {
        Self(Mutex::default())
    }
}

impl<K: Eq + Hash, V> Slots<K, V> {
    async fn get_or_open<F, Fut>(
        &self,
        key: K,
        alive: impl Fn(&V) -> bool,
        open: F,
    ) -> AppResult<Arc<V>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<V>>,
    {
        let slot = self.0.lock().await.entry(key).or_default().clone();
        let mut slot = slot.lock().await;
        if let Some(v) = slot.as_ref()
            && alive(v)
        {
            return Ok(v.clone());
        }
        let v = Arc::new(open().await?);
        *slot = Some(v.clone());
        Ok(v)
    }

    async fn retain(&self, keep: impl Fn(&K) -> bool) {
        self.0.lock().await.retain(|k, _| keep(k));
    }
}

pub struct Manager {
    pub store: ConnectionStore,
    pub known_hosts: KnownHosts,
    tunnels: Mutex<HashMap<String, Arc<Tunnel>>>,
    meta: Slots<(String, String), PgConn>,
    tabs: Mutex<HashMap<String, TabConn>>,
}

impl Manager {
    pub fn new(store: ConnectionStore, known_hosts: KnownHosts) -> Self {
        Self {
            store,
            known_hosts,
            tunnels: Mutex::default(),
            meta: Slots::default(),
            tabs: Mutex::default(),
        }
    }

    async fn transport(
        &self,
        cfg: &ConnectionConfig,
        secrets: &ResolvedSecrets,
    ) -> AppResult<Transport> {
        let Some(ssh) = &cfg.ssh else {
            return Ok(Transport::Direct);
        };
        // Held across the SSH handshake so two tabs opening at once do not
        // build two tunnels.
        let mut tunnels = self.tunnels.lock().await;
        if let Some(t) = tunnels.get(&cfg.id)
            && !t.is_closed()
        {
            return Ok(Transport::Tunnel(t.clone()));
        }
        let tunnel = Arc::new(Tunnel::open(ssh, secrets, self.known_hosts.clone()).await?);
        tunnels.insert(cfg.id.clone(), tunnel.clone());
        Ok(Transport::Tunnel(tunnel))
    }

    pub(crate) async fn open(&self, connection_id: &str, database: &str) -> AppResult<PgConn> {
        let cfg = self.store.get(connection_id)?;
        let secrets = resolve_secrets(&cfg.id, &Secrets::default())?;
        let transport = self.transport(&cfg, &secrets).await?;
        PgConn::connect(&cfg, database, secrets.password.as_deref(), transport).await
    }

    /// AI tools must not share transaction state with editors or the schema tree.
    pub async fn ephemeral(&self, connection_id: &str, database: &str) -> AppResult<PgConn> {
        self.open(connection_id, database).await
    }

    /// The tree's connection for `database` (the saved default if empty).
    pub async fn meta(&self, connection_id: &str, database: &str) -> AppResult<Arc<PgConn>> {
        let database = if database.is_empty() {
            self.store.get(connection_id)?.database
        } else {
            database.to_string()
        };
        let key = (connection_id.to_string(), database.clone());
        self.meta
            .get_or_open(
                key,
                |c| !c.is_closed(),
                || self.open(connection_id, &database),
            )
            .await
    }

    /// The tab's own connection, reopened when the tab switches database or
    /// the old connection died.
    pub async fn tab(
        &self,
        tab_id: &str,
        connection_id: &str,
        database: &str,
    ) -> AppResult<Arc<PgConn>> {
        if let Some(t) = self.tabs.lock().await.get(tab_id)
            && t.connection_id == connection_id
            && t.database == database
            && !t.conn.is_closed()
        {
            return Ok(t.conn.clone());
        }
        let conn = Arc::new(self.open(connection_id, database).await?);
        self.tabs.lock().await.insert(
            tab_id.to_string(),
            TabConn {
                connection_id: connection_id.to_string(),
                database: database.to_string(),
                conn: conn.clone(),
            },
        );
        Ok(conn)
    }

    pub async fn cancel(&self, tab_id: &str) -> AppResult<()> {
        let conn = self
            .tabs
            .lock()
            .await
            .get(tab_id)
            .map(|t| t.conn.clone())
            .ok_or_else(|| AppError::msg("nothing is running in this tab"))?;
        conn.cancel().await
    }

    pub async fn close_tab(&self, tab_id: &str) {
        self.tabs.lock().await.remove(tab_id);
    }

    /// Drop every connection belonging to `connection_id` and its tunnel.
    pub async fn disconnect(&self, connection_id: &str) {
        self.meta.retain(|(id, _)| id != connection_id).await;
        self.tabs
            .lock()
            .await
            .retain(|_, t| t.connection_id != connection_id);
        if let Some(tunnel) = self.tunnels.lock().await.remove(connection_id) {
            tunnel.close().await;
        }
    }

    /// Connect with an unsaved config and report the server version. Uses a
    /// throwaway tunnel so a failed test never poisons the cache.
    pub async fn test(&self, cfg: &ConnectionConfig, secrets: &Secrets) -> AppResult<String> {
        cfg.validate()?;
        let secrets = resolve_secrets(&cfg.id, secrets)?;
        let tunnel = match &cfg.ssh {
            Some(ssh) => Some(Arc::new(
                Tunnel::open(ssh, &secrets, self.known_hosts.clone()).await?,
            )),
            None => None,
        };
        let transport = match &tunnel {
            Some(t) => Transport::Tunnel(t.clone()),
            None => Transport::Direct,
        };
        let result = async {
            let conn =
                PgConn::connect(cfg, &cfg.database, secrets.password.as_deref(), transport).await?;
            crate::catalog::server_version(&conn.client).await
        }
        .await;
        if let Some(t) = tunnel {
            t.close().await;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn concurrent_requests_share_one_open() {
        let slots: Slots<&str, usize> = Slots::default();
        let opens = AtomicUsize::new(0);
        let open = || async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(opens.fetch_add(1, Ordering::SeqCst))
        };
        let (a, b, c) = tokio::join!(
            slots.get_or_open("db", |_| true, open),
            slots.get_or_open("db", |_| true, open),
            slots.get_or_open("other", |_| true, open),
        );
        assert!(Arc::ptr_eq(&a.unwrap(), &b.unwrap()));
        assert!(c.is_ok());
        assert_eq!(opens.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn dead_or_failed_values_are_reopened() {
        let slots: Slots<&str, usize> = Slots::default();
        let failed = slots
            .get_or_open("db", |_| true, || async { Err(AppError::msg("down")) })
            .await;
        assert!(failed.is_err());

        let first = slots
            .get_or_open("db", |_| true, || async { Ok(1) })
            .await
            .unwrap();
        assert_eq!(*first, 1);
        let reused = slots
            .get_or_open("db", |_| true, || async { Ok(2) })
            .await
            .unwrap();
        assert_eq!(*reused, 1);
        let reopened = slots
            .get_or_open("db", |_| false, || async { Ok(3) })
            .await
            .unwrap();
        assert_eq!(*reopened, 3);

        slots.retain(|_| false).await;
        let after_retain = slots
            .get_or_open("db", |_| true, || async { Ok(4) })
            .await
            .unwrap();
        assert_eq!(*after_retain, 4);
    }
}
