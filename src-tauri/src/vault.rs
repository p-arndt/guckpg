use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

use crate::error::{AppError, AppResult};

const SERVICE: &str = "guckpg";
const VAULT: &str = "guckpg:credentials:v1";

trait Backend {
    fn read(&self, account: &str) -> AppResult<Option<String>>;
    fn write(&self, account: &str, value: &str) -> AppResult<()>;
    fn delete(&self, account: &str) -> AppResult<()>;
}

struct Keychain;

impl Backend for Keychain {
    fn read(&self, account: &str) -> AppResult<Option<String>> {
        match keyring::Entry::new(SERVICE, account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, account: &str, value: &str) -> AppResult<()> {
        keyring::Entry::new(SERVICE, account)?.set_password(value)?;
        Ok(())
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        match keyring::Entry::new(SERVICE, account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Default)]
struct Vault {
    values: Option<HashMap<String, Option<String>>>,
}

impl Vault {
    fn refresh(&mut self) {
        self.values = None;
    }

    fn values(
        &mut self,
        backend: &impl Backend,
    ) -> AppResult<&mut HashMap<String, Option<String>>> {
        if self.values.is_none() {
            self.values = Some(match backend.read(VAULT)? {
                Some(value) => serde_json::from_str(&value)?,
                None => HashMap::new(),
            });
        }
        Ok(self.values.as_mut().unwrap())
    }

    fn set(
        &mut self,
        backend: &impl Backend,
        account: &str,
        value: Option<String>,
    ) -> AppResult<()> {
        if account == VAULT {
            return Err(AppError::msg("Invalid credential account."));
        }
        let mut updated = self.values(backend)?.clone();
        updated.insert(account.to_owned(), value);
        backend.write(VAULT, &serde_json::to_string(&updated)?)?;
        self.values = Some(updated);
        Ok(())
    }

    fn get(&mut self, backend: &impl Backend, account: &str) -> AppResult<Option<String>> {
        if account == VAULT {
            return Err(AppError::msg("Invalid credential account."));
        }
        if let Some(value) = self.values(backend)?.get(account) {
            return Ok(value.clone());
        }
        let value = backend.read(account)?;
        self.set(backend, account, value.clone())?;
        if value.is_some() {
            backend.delete(account)?;
        }
        Ok(value)
    }

    fn remove(&mut self, backend: &impl Backend, account: &str) -> AppResult<()> {
        self.set(backend, account, None)?;
        backend.delete(account)
    }
}

static CACHE: LazyLock<Mutex<Vault>> = LazyLock::new(Default::default);
static LOCK_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init(dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    LOCK_PATH
        .set(dir.join("credentials.lock"))
        .map_err(|_| AppError::msg("Credential store already initialized."))
}

fn update<T>(vault: &mut Vault, f: impl FnOnce(&mut Vault) -> AppResult<T>) -> AppResult<T> {
    let path = LOCK_PATH
        .get()
        .ok_or_else(|| AppError::msg("Credential store unavailable."))?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    lock.lock()?;
    // Refresh under the process-wide file lock so another app instance cannot lose changes.
    vault.refresh();
    f(vault)
}

pub fn get(account: &str) -> AppResult<Option<String>> {
    let mut vault = CACHE
        .lock()
        .map_err(|_| AppError::msg("Credential store unavailable."))?;
    if account == VAULT {
        return Err(AppError::msg("Invalid credential account."));
    }
    if let Some(Some(value)) = vault.values.as_ref().and_then(|values| values.get(account)) {
        return Ok(Some(value.clone()));
    }
    if vault
        .values
        .as_ref()
        .is_some_and(|values| values.contains_key(account))
    {
        return Ok(None);
    }
    update(&mut vault, |vault| vault.get(&Keychain, account))
}

pub fn set(account: &str, value: &str) -> AppResult<()> {
    let mut vault = CACHE
        .lock()
        .map_err(|_| AppError::msg("Credential store unavailable."))?;
    update(&mut vault, |vault| {
        vault.set(&Keychain, account, Some(value.to_owned()))?;
        Keychain.delete(account)
    })
}

pub fn delete(account: &str) -> AppResult<()> {
    let mut vault = CACHE
        .lock()
        .map_err(|_| AppError::msg("Credential store unavailable."))?;
    update(&mut vault, |vault| vault.remove(&Keychain, account))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Memory {
        entries: RefCell<HashMap<String, String>>,
        reads: RefCell<Vec<String>>,
        fail_write: Cell<bool>,
    }

    impl Backend for Memory {
        fn read(&self, account: &str) -> AppResult<Option<String>> {
            self.reads.borrow_mut().push(account.to_owned());
            Ok(self.entries.borrow().get(account).cloned())
        }
        fn write(&self, account: &str, value: &str) -> AppResult<()> {
            if self.fail_write.get() {
                return Err(AppError::msg("write failed"));
            }
            self.entries
                .borrow_mut()
                .insert(account.into(), value.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> AppResult<()> {
            self.entries.borrow_mut().remove(account);
            Ok(())
        }
    }

    #[test]
    fn migrates_and_shares_one_keychain_item_across_restarts() {
        let backend = Memory::default();
        backend
            .entries
            .borrow_mut()
            .insert("db".into(), "db-secret".into());
        backend
            .entries
            .borrow_mut()
            .insert("ai".into(), "ai-secret".into());
        let mut vault = Vault::default();
        assert_eq!(
            vault.get(&backend, "db").unwrap().as_deref(),
            Some("db-secret")
        );
        assert_eq!(
            vault.get(&backend, "ai").unwrap().as_deref(),
            Some("ai-secret")
        );
        assert_eq!(backend.entries.borrow().len(), 1);
        backend.reads.borrow_mut().clear();
        let mut reopened = Vault::default();
        assert_eq!(
            reopened.get(&backend, "db").unwrap().as_deref(),
            Some("db-secret")
        );
        assert_eq!(
            reopened.get(&backend, "ai").unwrap().as_deref(),
            Some("ai-secret")
        );
        assert_eq!(&*backend.reads.borrow(), &[VAULT]);
        reopened.remove(&backend, "db").unwrap();
        backend
            .entries
            .borrow_mut()
            .insert("db".into(), "stale".into());
        assert_eq!(Vault::default().get(&backend, "db").unwrap(), None);
    }

    #[test]
    fn refreshed_writes_preserve_another_instances_changes() {
        let backend = Memory::default();
        let mut first = Vault::default();
        let mut second = Vault::default();
        first.set(&backend, "db", Some("first".into())).unwrap();
        second.get(&backend, "db").unwrap();
        first.set(&backend, "ai", Some("second".into())).unwrap();
        second.refresh();
        second.set(&backend, "db", Some("updated".into())).unwrap();
        let mut reopened = Vault::default();
        assert_eq!(
            reopened.get(&backend, "db").unwrap().as_deref(),
            Some("updated")
        );
        assert_eq!(
            reopened.get(&backend, "ai").unwrap().as_deref(),
            Some("second")
        );
    }

    #[test]
    fn failed_migration_keeps_original_and_can_be_retried() {
        let backend = Memory::default();
        backend
            .entries
            .borrow_mut()
            .insert("db".into(), "db-secret".into());
        backend.fail_write.set(true);
        let mut vault = Vault::default();
        assert!(vault.get(&backend, "db").is_err());
        assert!(backend.entries.borrow().contains_key("db"));
        backend.fail_write.set(false);
        assert_eq!(
            vault.get(&backend, "db").unwrap().as_deref(),
            Some("db-secret")
        );
    }
}
