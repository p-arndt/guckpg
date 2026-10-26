//! Saved connections. Everything except secrets lives in a JSON file in the
//! app config dir; passwords and key passphrases go to the OS keychain so the
//! file can be copied around or backed up without leaking anything.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    Disable,
    /// TLS if the server offers it, certificate not verified (libpq default).
    #[default]
    Prefer,
    /// TLS required, certificate not verified (libpq semantics).
    Require,
    /// TLS required, certificate and hostname verified against system roots.
    VerifyFull,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum SshAuth {
    Password,
    #[serde(rename_all = "camelCase")]
    Key {
        key_path: String,
    },
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: SshAuth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    /// Seen from the SSH server when a tunnel is configured, so a Docker
    /// container name like `app-postgres-1` works there.
    pub host: String,
    pub port: u16,
    pub user: String,
    pub database: String,
    #[serde(default)]
    pub ssl_mode: SslMode,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    /// Sidebar folder; `None` shows the connection at the top level.
    #[serde(default)]
    pub folder: Option<String>,
}

/// Where one connection sits in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub id: String,
    pub folder: Option<String>,
}

/// Secrets submitted with a save or test. `None` keeps what is stored,
/// `Some("")` deletes it, anything else replaces it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Secrets {
    pub password: Option<String>,
    pub ssh_password: Option<String>,
    pub ssh_passphrase: Option<String>,
}

/// All secrets of one connection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSecrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_passphrase: Option<String>,
}

impl ResolvedSecrets {
    /// Apply submitted values: `None` keeps, `""` clears, anything else sets.
    pub fn merge(&mut self, submitted: &Secrets) {
        let apply = |slot: &mut Option<String>, value: &Option<String>| match value.as_deref() {
            None => {}
            Some("") => *slot = None,
            Some(v) => *slot = Some(v.to_string()),
        };
        apply(&mut self.password, &submitted.password);
        apply(&mut self.ssh_password, &submitted.ssh_password);
        apply(&mut self.ssh_passphrase, &submitted.ssh_passphrase);
    }

    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

const LEGACY_SUFFIXES: [&str; 3] = ["password", "ssh-password", "ssh-passphrase"];

fn read_item(account: &str) -> AppResult<Option<String>> {
    crate::vault::get(account)
}

fn delete_item(account: &str) -> AppResult<()> {
    crate::vault::delete(account)
}

fn write_bundle(id: &str, secrets: &ResolvedSecrets) -> AppResult<()> {
    if secrets.is_empty() {
        delete_item(id)
    } else {
        crate::vault::set(id, &serde_json::to_string(secrets)?)
    }
}

/// Earlier versions kept one item per secret; move them into the bundle.
fn migrate_legacy(id: &str) -> AppResult<ResolvedSecrets> {
    let [password, ssh_password, ssh_passphrase] =
        LEGACY_SUFFIXES.map(|suffix| read_item(&format!("{id}:{suffix}")));
    let secrets = ResolvedSecrets {
        password: password?,
        ssh_password: ssh_password?,
        ssh_passphrase: ssh_passphrase?,
    };
    if !secrets.is_empty() {
        write_bundle(id, &secrets)?;
        for suffix in LEGACY_SUFFIXES {
            delete_item(&format!("{id}:{suffix}"))?;
        }
    }
    Ok(secrets)
}

pub fn load_secrets(id: &str) -> AppResult<ResolvedSecrets> {
    if id.is_empty() {
        return Ok(ResolvedSecrets::default());
    }
    match read_item(id)? {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => migrate_legacy(id),
    }
}

pub fn store_secrets(id: &str, submitted: &Secrets) -> AppResult<()> {
    let mut secrets = load_secrets(id)?;
    secrets.merge(submitted);
    write_bundle(id, &secrets)?;
    Ok(())
}

/// Give a duplicated connection the same secrets as its original.
pub fn copy_secrets(from: &str, to: &str) -> AppResult<()> {
    let secrets = load_secrets(from)?;
    write_bundle(to, &secrets)?;
    Ok(())
}

pub fn delete_secrets(id: &str) -> AppResult<()> {
    delete_item(id)?;
    for suffix in LEGACY_SUFFIXES {
        delete_item(&format!("{id}:{suffix}"))?;
    }
    Ok(())
}

/// Stored secrets with submitted (unsaved) values applied on top.
pub fn resolve_secrets(id: &str, submitted: &Secrets) -> AppResult<ResolvedSecrets> {
    let mut secrets = load_secrets(id)?;
    secrets.merge(submitted);
    Ok(secrets)
}

impl ConnectionConfig {
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::msg("Name is required"));
        }
        if self.host.trim().is_empty() {
            return Err(AppError::msg("Host is required"));
        }
        if self.user.trim().is_empty() {
            return Err(AppError::msg("User is required"));
        }
        if let Some(ssh) = &self.ssh {
            if ssh.host.trim().is_empty() || ssh.user.trim().is_empty() {
                return Err(AppError::msg("SSH host and user are required"));
            }
            if let SshAuth::Key { key_path } = &ssh.auth
                && key_path.trim().is_empty()
            {
                return Err(AppError::msg("SSH key path is required"));
            }
        }
        Ok(())
    }
}

/// The JSON file holding all saved connections.
pub struct ConnectionStore {
    path: PathBuf,
}

impl ConnectionStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("connections.json"),
        }
    }

    pub fn list(&self) -> AppResult<Vec<ConnectionConfig>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get(&self, id: &str) -> AppResult<ConnectionConfig> {
        self.list()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| AppError::msg(format!("connection {id} not found")))
    }

    pub fn upsert(&self, config: ConnectionConfig) -> AppResult<()> {
        let mut all = self.list()?;
        match all.iter_mut().find(|c| c.id == config.id) {
            Some(existing) => *existing = config,
            None => all.push(config),
        }
        self.write(&all)
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        let mut all = self.list()?;
        all.retain(|c| c.id != id);
        self.write(&all)
    }

    /// Reorder connections and set their folders. Connections missing from
    /// `placements` keep their folder and move to the end, so a stale
    /// frontend list never drops anything.
    pub fn arrange(&self, placements: &[Placement]) -> AppResult<Vec<ConnectionConfig>> {
        let mut rest = self.list()?;
        let mut arranged = Vec::with_capacity(rest.len());
        for p in placements {
            if let Some(i) = rest.iter().position(|c| c.id == p.id) {
                let mut c = rest.remove(i);
                c.folder = p
                    .folder
                    .as_deref()
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(String::from);
                arranged.push(c);
            }
        }
        arranged.extend(rest);
        self.write(&arranged)?;
        Ok(arranged)
    }

    /// Copy a connection right after the original, with a new id and name.
    pub fn duplicate(&self, id: &str, new_id: String) -> AppResult<ConnectionConfig> {
        let mut all = self.list()?;
        let index = all
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| AppError::msg(format!("connection {id} not found")))?;
        let mut copy = all[index].clone();
        copy.id = new_id;
        copy.name = copy_name(&copy.name, &all);
        all.insert(index + 1, copy.clone());
        self.write(&all)?;
        Ok(copy)
    }

    /// Write to a sibling temp file and rename, so a crash never leaves a
    /// half-written file behind.
    fn write(&self, all: &[ConnectionConfig]) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(all)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

/// "prod" -> "prod copy", then "prod copy 2", ... skipping names in use.
fn copy_name(name: &str, all: &[ConnectionConfig]) -> String {
    let taken = |n: &str| all.iter().any(|c| c.name == n);
    let first = format!("{name} copy");
    if !taken(&first) {
        return first;
    }
    (2..)
        .map(|i| format!("{name} copy {i}"))
        .find(|n| !taken(n))
        .expect("unbounded range")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: "local".into(),
            host: "localhost".into(),
            port: 5432,
            user: "postgres".into(),
            database: "postgres".into(),
            ssl_mode: SslMode::Prefer,
            read_only: false,
            color: None,
            ssh: None,
            folder: None,
        }
    }

    fn ids(all: &[ConnectionConfig]) -> Vec<&str> {
        all.iter().map(|c| c.id.as_str()).collect()
    }

    fn place(id: &str, folder: Option<&str>) -> Placement {
        Placement {
            id: id.into(),
            folder: folder.map(String::from),
        }
    }

    #[test]
    fn arrange_reorders_and_sets_folders() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(dir.path());
        for id in ["a", "b", "c"] {
            store.upsert(sample(id)).unwrap();
        }
        let all = store
            .arrange(&[
                place("c", Some(" prod ")),
                place("a", Some("")),
                place("x", None),
            ])
            .unwrap();
        assert_eq!(ids(&all), ["c", "a", "b"]);
        assert_eq!(all[0].folder.as_deref(), Some("prod"));
        assert_eq!(all[1].folder, None);
        assert_eq!(store.list().unwrap(), all);
    }

    #[test]
    fn duplicate_inserts_copy_after_original() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(dir.path());
        let mut a = sample("a");
        a.folder = Some("prod".into());
        store.upsert(a).unwrap();
        store.upsert(sample("b")).unwrap();

        let copy = store.duplicate("a", "a2".into()).unwrap();
        assert_eq!(copy.name, "local copy");
        assert_eq!(copy.folder.as_deref(), Some("prod"));
        assert_eq!(ids(&store.list().unwrap()), ["a", "a2", "b"]);

        assert_eq!(
            store.duplicate("a", "a3".into()).unwrap().name,
            "local copy 2"
        );
        assert!(store.duplicate("missing", "z".into()).is_err());
    }

    #[test]
    fn store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(dir.path());
        assert!(store.list().unwrap().is_empty());

        store.upsert(sample("a")).unwrap();
        store.upsert(sample("b")).unwrap();
        let mut renamed = sample("a");
        renamed.name = "renamed".into();
        store.upsert(renamed).unwrap();

        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(store.get("a").unwrap().name, "renamed");

        store.remove("a").unwrap();
        assert!(store.get("a").is_err());
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn parses_frontend_shape() {
        let json = r#"{
            "id": "x", "name": "prod", "host": "app-postgres-1", "port": 5432,
            "user": "app", "database": "app", "sslMode": "disable",
            "ssh": { "host": "vps", "port": 22, "user": "deploy",
                     "auth": { "method": "key", "keyPath": "~/.ssh/id_ed25519" } }
        }"#;
        let c: ConnectionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.ssl_mode, SslMode::Disable);
        assert!(!c.read_only);
        let ssh = c.ssh.unwrap();
        assert_eq!(
            ssh.auth,
            SshAuth::Key {
                key_path: "~/.ssh/id_ed25519".into()
            }
        );
    }

    #[test]
    fn validate_rejects_missing_fields() {
        let mut c = sample("a");
        assert!(c.validate().is_ok());
        c.host = " ".into();
        assert!(c.validate().is_err());

        let mut c = sample("a");
        c.ssh = Some(SshConfig {
            host: "vps".into(),
            port: 22,
            user: "u".into(),
            auth: SshAuth::Key {
                key_path: "".into(),
            },
        });
        assert!(c.validate().is_err());
    }

    #[test]
    fn merge_keeps_clears_and_sets() {
        let mut secrets = ResolvedSecrets {
            password: Some("old".into()),
            ssh_password: Some("ssh".into()),
            ssh_passphrase: None,
        };
        secrets.merge(&Secrets {
            password: Some("new".into()),
            ssh_password: Some(String::new()),
            ssh_passphrase: None,
        });
        assert_eq!(secrets.password.as_deref(), Some("new"));
        assert_eq!(secrets.ssh_password, None);
        assert_eq!(secrets.ssh_passphrase, None);
    }

    #[test]
    fn bundle_json_omits_missing_secrets() {
        let secrets = ResolvedSecrets {
            password: Some("pw".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&secrets).unwrap();
        assert_eq!(json, r#"{"password":"pw"}"#);
        assert_eq!(
            serde_json::from_str::<ResolvedSecrets>(&json).unwrap(),
            secrets
        );
        assert!(ResolvedSecrets::default().is_empty());
    }
}
