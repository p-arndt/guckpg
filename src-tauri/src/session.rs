//! Live connections. One SSH tunnel per saved connection is shared by all its
//! Postgres connections. The schema tree uses one "meta" connection per
//! database; every editor tab gets its own connection, so a transaction or a
//! long query in one tab never blocks another tab or the tree.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::config::{ConnectionConfig, ConnectionStore, ResolvedSecrets, Secrets};
use crate::error::{AppError, AppResult};
use crate::pg::{PgConn, Transport};
use crate::ssh::{KnownHosts, Tunnel};

/// Shown once when a tab's connection died under it: a silent reconnect
/// would let a later COMMIT "succeed" on a fresh session.
pub const CONNECTION_LOST: &str = "The connection was lost and has been reopened. \
    Any open transaction was rolled back; run the statement again.";

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
        self.get_or_reopen(key, |_| true, alive, open)
            .await
            .map(|(v, _)| v)
    }

    /// Like `get_or_open`, but a stored value that no longer fits (`same` is
    /// false) is replaced, and the flag tells whether the replaced value did
    /// fit but had died. Only the one caller that reopens sees `true`.
    async fn get_or_reopen<F, Fut>(
        &self,
        key: K,
        same: impl Fn(&V) -> bool,
        alive: impl Fn(&V) -> bool,
        open: F,
    ) -> AppResult<(Arc<V>, bool)>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<V>>,
    {
        let slot = self.0.lock().await.entry(key).or_default().clone();
        let mut slot = slot.lock().await;
        let lost = match slot.as_ref() {
            Some(v) if same(v) && alive(v) => return Ok((v.clone(), false)),
            Some(v) => same(v),
            None => false,
        };
        let v = Arc::new(open().await?);
        *slot = Some(v.clone());
        Ok((v, lost))
    }

    /// The current value, without waiting for an open in progress.
    async fn peek<Q>(&self, key: &Q) -> Option<Arc<V>>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let slot = self.0.lock().await.get(key)?.clone();
        slot.try_lock().ok()?.clone()
    }

    /// Drop the slots `remove` selects and return their values. `remove` sees
    /// `None` for a slot whose open is still running: waiting for it would
    /// make disconnecting as slow as the slowest server.
    async fn remove_where(&self, remove: impl Fn(&K, Option<&V>) -> bool) -> Vec<Arc<V>> {
        let mut taken = Vec::new();
        self.0.lock().await.retain(|k, slot| {
            let guard = slot.try_lock().ok();
            let value = guard.as_ref().and_then(|g| g.as_ref());
            if !remove(k, value.map(|v| &**v)) {
                return true;
            }
            taken.extend(value.cloned());
            false
        });
        taken
    }
}

/// Keychain and file locks block; keep them off the async workers.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::msg(format!("background task failed: {e}")))?
}

pub struct Manager {
    pub store: ConnectionStore,
    pub known_hosts: KnownHosts,
    tunnels: Slots<String, Tunnel>,
    meta: Slots<(String, String), PgConn>,
    tabs: Slots<String, TabConn>,
}

impl Manager {
    pub fn new(store: ConnectionStore, known_hosts: KnownHosts) -> Self {
        Self {
            store,
            known_hosts,
            tunnels: Slots::default(),
            meta: Slots::default(),
            tabs: Slots::default(),
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
        let tunnel = self
            .tunnels
            .get_or_open(
                cfg.id.clone(),
                |t| !t.is_closed(),
                || Tunnel::open(ssh, secrets, self.known_hosts.clone()),
            )
            .await?;
        Ok(Transport::Tunnel(tunnel))
    }

    pub(crate) async fn open(&self, connection_id: &str, database: &str) -> AppResult<PgConn> {
        let cfg = self.store.get(connection_id)?;
        let id = cfg.id.clone();
        let secrets = blocking(move || crate::config::load_secrets(&id)).await?;
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

    /// The tab's own connection, reopened when the tab switches database.
    /// When it died instead, it is reopened too, but the call fails once
    /// with `CONNECTION_LOST` so the user learns their session state is gone.
    pub async fn tab(
        &self,
        tab_id: &str,
        connection_id: &str,
        database: &str,
    ) -> AppResult<Arc<PgConn>> {
        let (tab, lost) = self
            .tabs
            .get_or_reopen(
                tab_id.to_string(),
                |t| t.connection_id == connection_id && t.database == database,
                |t| !t.conn.is_closed(),
                || async {
                    Ok(TabConn {
                        connection_id: connection_id.to_string(),
                        database: database.to_string(),
                        conn: Arc::new(self.open(connection_id, database).await?),
                    })
                },
            )
            .await?;
        if lost {
            return Err(AppError::msg(CONNECTION_LOST));
        }
        Ok(tab.conn.clone())
    }

    pub async fn cancel(&self, tab_id: &str) -> AppResult<()> {
        let tab = self
            .tabs
            .peek(tab_id)
            .await
            .ok_or_else(|| AppError::msg("nothing is running in this tab"))?;
        tab.conn.cancel().await
    }

    pub async fn close_tab(&self, tab_id: &str) {
        self.tabs.remove_where(|k, _| k == tab_id).await;
    }

    /// Drop every connection belonging to `connection_id` and its tunnel.
    pub async fn disconnect(&self, connection_id: &str) {
        self.meta
            .remove_where(|(id, _), _| id == connection_id)
            .await;
        self.tabs
            .remove_where(|_, t| t.is_some_and(|t| t.connection_id == connection_id))
            .await;
        for tunnel in self.tunnels.remove_where(|id, _| id == connection_id).await {
            tunnel.close().await;
        }
    }

    /// Close everything on app exit, so SSH servers see a clean disconnect.
    /// Bounded: a dead network must not keep the app from quitting.
    pub async fn shutdown(&self) {
        self.meta.remove_where(|_, _| true).await;
        self.tabs.remove_where(|_, _| true).await;
        let tunnels = self.tunnels.remove_where(|_, _| true).await;
        let close_all = futures_util::future::join_all(tunnels.iter().map(|t| t.close()));
        let _ = tokio::time::timeout(Duration::from_secs(2), close_all).await;
    }

    /// Connect with an unsaved config and report the server version. Uses a
    /// throwaway tunnel so a failed test never poisons the cache. Stored
    /// secrets are only used when the target is the saved one.
    pub async fn test(&self, cfg: &ConnectionConfig, secrets: &Secrets) -> AppResult<String> {
        cfg.validate()?;
        let stored = self.store.get(&cfg.id).ok();
        let (edited, submitted) = (cfg.clone(), secrets.clone());
        let secrets = blocking(move || {
            crate::config::resolve_secrets_for(&edited, stored.as_ref(), &submitted)
        })
        .await?;
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

        slots.remove_where(|_, _| true).await;
        let after_remove = slots
            .get_or_open("db", |_| true, || async { Ok(4) })
            .await
            .unwrap();
        assert_eq!(*after_remove, 4);
    }

    /// Stand-in for a tab connection: which database it is on and whether
    /// the server is still there.
    struct Fake {
        database: &'static str,
        alive: AtomicBool,
    }

    fn fake(database: &'static str) -> AppResult<Fake> {
        Ok(Fake {
            database,
            alive: AtomicBool::new(true),
        })
    }

    #[tokio::test]
    async fn a_dead_connection_is_reported_once_but_a_switch_is_not() {
        let slots: Slots<&str, Fake> = Slots::default();
        let alive = |f: &Fake| f.alive.load(Ordering::SeqCst);
        let on = |db: &'static str| move |f: &Fake| f.database == db;

        let (first, lost) = slots
            .get_or_reopen("tab", on("a"), alive, || async { fake("a") })
            .await
            .unwrap();
        assert!(!lost);
        let (same, lost) = slots
            .get_or_reopen("tab", on("a"), alive, || async { fake("a") })
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&first, &same) && !lost);

        first.alive.store(false, Ordering::SeqCst);
        let failed = slots
            .get_or_reopen("tab", on("a"), alive, || async {
                Err(AppError::msg("still down"))
            })
            .await;
        assert!(failed.is_err());
        let (reopened, lost) = slots
            .get_or_reopen("tab", on("a"), alive, || async { fake("a") })
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &reopened));
        assert!(lost, "the loss survives a failed reopen until reported");
        let (_, lost) = slots
            .get_or_reopen("tab", on("a"), alive, || async { fake("a") })
            .await
            .unwrap();
        assert!(!lost);

        // Switching database replaces the connection deliberately.
        let (switched, lost) = slots
            .get_or_reopen("tab", on("b"), alive, || async { fake("b") })
            .await
            .unwrap();
        assert_eq!(switched.database, "b");
        assert!(!lost);
    }

    #[tokio::test]
    async fn concurrent_reopen_after_loss_opens_once_and_reports_once() {
        let slots: Slots<&str, Fake> = Slots::default();
        let alive = |f: &Fake| f.alive.load(Ordering::SeqCst);
        let same = |_: &Fake| true;
        let (first, _) = slots
            .get_or_reopen("tab", same, alive, || async { fake("a") })
            .await
            .unwrap();
        first.alive.store(false, Ordering::SeqCst);

        let opens = AtomicUsize::new(0);
        let open = || async {
            opens.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            fake("a")
        };
        let (a, b) = tokio::join!(
            slots.get_or_reopen("tab", same, alive, open),
            slots.get_or_reopen("tab", same, alive, open),
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert!(Arc::ptr_eq(&a.0, &b.0));
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert_eq!([a.1, b.1].iter().filter(|l| **l).count(), 1);
    }

    #[tokio::test]
    async fn a_hanging_open_blocks_neither_other_keys_nor_removal() {
        let slots: Arc<Slots<String, usize>> = Arc::default();
        let hung = tokio::spawn({
            let slots = slots.clone();
            async move {
                slots
                    .get_or_open(
                        "stuck".into(),
                        |_| true,
                        std::future::pending::<AppResult<usize>>,
                    )
                    .await
            }
        });
        // Let the spawned open take its slot lock.
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        let limit = Duration::from_secs(1);
        let other = tokio::time::timeout(
            limit,
            slots.get_or_open("fine".into(), |_| true, || async { Ok(7) }),
        )
        .await
        .expect("other keys must not wait for the stuck open");
        assert_eq!(*other.unwrap(), 7);

        assert!(slots.peek("stuck").await.is_none());
        assert_eq!(*slots.peek("fine").await.unwrap(), 7);

        let taken = tokio::time::timeout(limit, slots.remove_where(|_, _| true))
            .await
            .expect("removal must not wait for the stuck open");
        assert_eq!(taken.len(), 1, "only the finished value is handed back");
        assert!(slots.peek("fine").await.is_none());
        hung.abort();
    }
}
