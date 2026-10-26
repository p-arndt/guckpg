//! Write guards shared by every command that changes data or schema, so a
//! read-only connection is refused the same way everywhere.

use crate::config::ConnectionConfig;
use crate::error::{AppError, AppResult};
use crate::session::Manager;

pub const READ_ONLY_MESSAGE: &str = "This connection is configured as read-only.";

impl Manager {
    /// Fails for connections saved as read-only. Checked before opening a
    /// connection; the server-side `default_transaction_read_only` is the
    /// second line of defense.
    pub fn ensure_writable(&self, connection_id: &str) -> AppResult<()> {
        ensure_writable(&self.store.get(connection_id)?)
    }
}

pub fn ensure_writable(cfg: &ConnectionConfig) -> AppResult<()> {
    if cfg.read_only {
        return Err(AppError::msg(READ_ONLY_MESSAGE));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConnectionStore;
    use crate::ssh::KnownHosts;

    fn config(id: &str, read_only: bool) -> ConnectionConfig {
        ConnectionConfig {
            read_only,
            ..crate::testutil::connection(id)
        }
    }

    #[test]
    fn refuses_read_only_connections_with_one_message() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Manager::new(
            ConnectionStore::new(dir.path()),
            KnownHosts::new(dir.path()),
        );
        manager.store.upsert(config("rw", false)).unwrap();
        manager.store.upsert(config("ro", true)).unwrap();

        assert!(manager.ensure_writable("rw").is_ok());
        let err = manager.ensure_writable("ro").unwrap_err();
        assert_eq!(err.to_string(), READ_ONLY_MESSAGE);
        assert!(manager.ensure_writable("missing").is_err());
    }
}
