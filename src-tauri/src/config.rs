//! Saved connections. Everything except secrets lives in a JSON file in the
//! app config dir; passwords and key passphrases go to the OS keychain so the
//! file can be copied around or backed up without leaking anything.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
    /// TLS required, certificate chain verified, hostname not checked.
    VerifyCa,
    /// TLS required, certificate and hostname verified.
    VerifyFull,
}

impl SslMode {
    /// How well this mode keeps the password away from a man in the middle.
    fn strength(self) -> u8 {
        match self {
            SslMode::Disable => 0,
            SslMode::Prefer => 1,
            SslMode::Require => 2,
            SslMode::VerifyCa => 3,
            SslMode::VerifyFull => 4,
        }
    }

    pub fn verifies(self) -> bool {
        matches!(self, SslMode::VerifyCa | SslMode::VerifyFull)
    }
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
    /// PEM file with the CA certificates `verify-ca` and `verify-full` trust
    /// instead of the system roots (libpq's `sslrootcert`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_root_cert: Option<String>,
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

/// Which stored secrets may be sent for an edited config. Secrets are stored
/// per connection id, but the user trusted a specific server with them: a
/// form that points the id at another host must not ship the old password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretReuse {
    /// The Postgres password, sent to the Postgres server.
    pub password: bool,
    /// The SSH password and key passphrase, used for the SSH server.
    pub ssh: bool,
}

pub fn secret_reuse(stored: Option<&ConnectionConfig>, edited: &ConnectionConfig) -> SecretReuse {
    let Some(stored) = stored else {
        return SecretReuse {
            password: false,
            ssh: false,
        };
    };
    let ssh_hop = |c: &ConnectionConfig| {
        c.ssh
            .as_ref()
            .map(|s| (s.host.clone(), s.port, s.user.clone()))
    };
    let ssh = ssh_hop(stored) == ssh_hop(edited);
    // A weaker TLS mode on the same host lets a man in the middle collect a
    // password the user only ever sent over a verified connection.
    let tls_kept = edited.ssl_mode.strength() >= stored.ssl_mode.strength()
        && (!stored.ssl_mode.verifies() || stored.ssl_root_cert == edited.ssl_root_cert);
    let password = ssh
        && stored.host == edited.host
        && stored.port == edited.port
        && stored.user == edited.user
        && tls_kept;
    SecretReuse { password, ssh }
}

/// Apply `reuse` to the stored secrets, then the submitted values on top. A
/// stored secret that may not be reused and was not entered again is an
/// error, not a silent omission, so the user knows what to do.
pub fn bind_secrets(
    mut stored: ResolvedSecrets,
    reuse: SecretReuse,
    submitted: &Secrets,
) -> AppResult<ResolvedSecrets> {
    let mut missing = Vec::new();
    let mut check =
        |slot: &mut Option<String>, reusable: bool, entered: bool, name: &'static str| {
            if !reusable && slot.take().is_some() && !entered {
                missing.push(name);
            }
        };
    check(
        &mut stored.password,
        reuse.password,
        submitted.password.is_some(),
        "password",
    );
    check(
        &mut stored.ssh_password,
        reuse.ssh,
        submitted.ssh_password.is_some(),
        "SSH password",
    );
    check(
        &mut stored.ssh_passphrase,
        reuse.ssh,
        submitted.ssh_passphrase.is_some(),
        "SSH key passphrase",
    );
    if !missing.is_empty() {
        return Err(AppError::msg(format!(
            "Host, port, user or TLS verification changed, so the saved {} is not sent to the new target. Enter it again.",
            missing.join(" and ")
        )));
    }
    stored.merge(submitted);
    Ok(stored)
}

/// Secrets for an edited, possibly unsaved config: stored secrets are only
/// reused when `stored` (the saved config with the same id) has the same
/// target.
pub fn resolve_secrets_for(
    edited: &ConnectionConfig,
    stored: Option<&ConnectionConfig>,
    submitted: &Secrets,
) -> AppResult<ResolvedSecrets> {
    let saved = match stored {
        Some(_) => load_secrets(&edited.id)?,
        None => ResolvedSecrets::default(),
    };
    bind_secrets(saved, secret_reuse(stored, edited), submitted)
}

/// Save counterpart of `resolve_secrets_for`: secrets bound to the old
/// target are not carried over to the new one.
pub fn store_secrets_for(
    edited: &ConnectionConfig,
    stored: Option<&ConnectionConfig>,
    submitted: &Secrets,
) -> AppResult<()> {
    let secrets = resolve_secrets_for(edited, stored, submitted)?;
    write_bundle(&edited.id, &secrets)
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

/// The JSON file holding all saved connections. Every query and tree request
/// looks up its connection, so the parsed file is kept in memory; the lock
/// also serializes read-modify-write cycles.
pub struct ConnectionStore {
    path: PathBuf,
    cache: Mutex<Option<Vec<ConnectionConfig>>>,
}

impl ConnectionStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("connections.json"),
            cache: Mutex::default(),
        }
    }

    fn read_file(&self) -> AppResult<Vec<ConnectionConfig>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Run `f` on the current list under the lock. When it returns `Some`,
    /// that list is written to disk and becomes the cached state.
    fn with<T>(
        &self,
        f: impl FnOnce(&[ConnectionConfig]) -> AppResult<(Option<Vec<ConnectionConfig>>, T)>,
    ) -> AppResult<T> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| AppError::msg("connection store unavailable"))?;
        if cache.is_none() {
            *cache = Some(self.read_file()?);
        }
        let (updated, out) = f(cache.as_deref().unwrap_or_default())?;
        if let Some(all) = updated {
            self.write(&all)?;
            *cache = Some(all);
        }
        Ok(out)
    }

    pub fn list(&self) -> AppResult<Vec<ConnectionConfig>> {
        self.with(|all| Ok((None, all.to_vec())))
    }

    pub fn get(&self, id: &str) -> AppResult<ConnectionConfig> {
        self.with(|all| {
            let found = all
                .iter()
                .find(|c| c.id == id)
                .cloned()
                .ok_or_else(|| AppError::msg(format!("connection {id} not found")))?;
            Ok((None, found))
        })
    }

    pub fn upsert(&self, config: ConnectionConfig) -> AppResult<()> {
        self.with(|all| {
            let mut all = all.to_vec();
            match all.iter_mut().find(|c| c.id == config.id) {
                Some(existing) => *existing = config,
                None => all.push(config),
            }
            Ok((Some(all), ()))
        })
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        self.with(|all| {
            let mut all = all.to_vec();
            all.retain(|c| c.id != id);
            Ok((Some(all), ()))
        })
    }

    /// Reorder connections and set their folders. Connections missing from
    /// `placements` keep their folder and move to the end, so a stale
    /// frontend list never drops anything.
    pub fn arrange(&self, placements: &[Placement]) -> AppResult<Vec<ConnectionConfig>> {
        self.with(|all| {
            let arranged = arrange(all.to_vec(), placements);
            Ok((Some(arranged.clone()), arranged))
        })
    }

    /// Copy a connection right after the original, with a new id and name.
    pub fn duplicate(&self, id: &str, new_id: String) -> AppResult<ConnectionConfig> {
        self.with(|all| {
            let mut all = all.to_vec();
            let index = all
                .iter()
                .position(|c| c.id == id)
                .ok_or_else(|| AppError::msg(format!("connection {id} not found")))?;
            let mut copy = all[index].clone();
            copy.id = new_id;
            copy.name = copy_name(&copy.name, &all);
            all.insert(index + 1, copy.clone());
            Ok((Some(all), copy))
        })
    }

    /// Write to a sibling temp file and rename, so a crash never leaves a
    /// half-written file behind.
    fn write(&self, all: &[ConnectionConfig]) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        write_private(&tmp, &serde_json::to_vec_pretty(all)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

/// Write a file only the current user can read: it names hosts, users and
/// key paths.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    // `mode` only applies when the file is created; tighten leftovers too.
    restrict_permissions(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(unix)]
pub fn restrict_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
pub fn restrict_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn arrange(mut rest: Vec<ConnectionConfig>, placements: &[Placement]) -> Vec<ConnectionConfig> {
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
    arranged
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
    use std::sync::Arc;

    use super::*;

    fn sample(id: &str) -> ConnectionConfig {
        ConnectionConfig {
            name: "local".into(),
            ..crate::testutil::connection(id)
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

    fn with_ssh(mut c: ConnectionConfig, host: &str) -> ConnectionConfig {
        c.ssh = Some(SshConfig {
            host: host.into(),
            port: 22,
            user: "deploy".into(),
            auth: SshAuth::Agent,
        });
        c
    }

    #[test]
    fn secrets_are_reused_only_for_the_same_target() {
        let stored = with_ssh(sample("a"), "vps");
        let both = SecretReuse {
            password: true,
            ssh: true,
        };
        assert_eq!(
            secret_reuse(None, &stored),
            SecretReuse {
                password: false,
                ssh: false
            }
        );

        let mut renamed = stored.clone();
        renamed.name = "other name".into();
        renamed.database = "other".into();
        renamed.read_only = true;
        assert_eq!(secret_reuse(Some(&stored), &renamed), both);

        for edit in [
            |c: &mut ConnectionConfig| c.host = "evil".into(),
            |c: &mut ConnectionConfig| c.port = 6543,
            |c: &mut ConnectionConfig| c.user = "admin".into(),
        ] {
            let mut changed = stored.clone();
            edit(&mut changed);
            assert_eq!(
                secret_reuse(Some(&stored), &changed),
                SecretReuse {
                    password: false,
                    ssh: true
                }
            );
        }

        let moved = with_ssh(sample("a"), "elsewhere");
        assert_eq!(
            secret_reuse(Some(&stored), &moved),
            SecretReuse {
                password: false,
                ssh: false
            }
        );
        let mut no_tunnel = stored.clone();
        no_tunnel.ssh = None;
        assert!(!secret_reuse(Some(&stored), &no_tunnel).password);
        let mut other_ssh_user = stored.clone();
        other_ssh_user.ssh.as_mut().unwrap().user = "root".into();
        assert!(!secret_reuse(Some(&stored), &other_ssh_user).ssh);
    }

    #[test]
    fn weaker_tls_does_not_reuse_the_password() {
        let mut stored = sample("a");
        stored.ssl_mode = SslMode::VerifyFull;
        let mut weaker = stored.clone();
        weaker.ssl_mode = SslMode::Require;
        assert!(!secret_reuse(Some(&stored), &weaker).password);

        let mut other_ca = stored.clone();
        other_ca.ssl_root_cert = Some("/tmp/evil.pem".into());
        assert!(!secret_reuse(Some(&stored), &other_ca).password);

        let mut plain = sample("a");
        plain.ssl_mode = SslMode::Prefer;
        let mut stronger = plain.clone();
        stronger.ssl_mode = SslMode::VerifyCa;
        stronger.ssl_root_cert = Some("/etc/ca.pem".into());
        assert!(secret_reuse(Some(&plain), &stronger).password);
    }

    #[test]
    fn bind_requires_reentering_secrets_for_a_new_target() {
        let stored = ResolvedSecrets {
            password: Some("pg".into()),
            ssh_password: Some("ssh".into()),
            ssh_passphrase: None,
        };
        let keep = SecretReuse {
            password: true,
            ssh: true,
        };
        let only_ssh = SecretReuse {
            password: false,
            ssh: true,
        };

        let same = bind_secrets(stored.clone(), keep, &Secrets::default()).unwrap();
        assert_eq!(same, stored);

        let err = bind_secrets(stored.clone(), only_ssh, &Secrets::default()).unwrap_err();
        assert!(err.to_string().contains("saved password"), "{err}");

        let entered = bind_secrets(
            stored.clone(),
            only_ssh,
            &Secrets {
                password: Some("new".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(entered.password.as_deref(), Some("new"));
        assert_eq!(entered.ssh_password.as_deref(), Some("ssh"));

        // Clearing explicitly is also an answer.
        let cleared = bind_secrets(
            stored.clone(),
            only_ssh,
            &Secrets {
                password: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(cleared.password, None);

        let none = SecretReuse {
            password: false,
            ssh: false,
        };
        let err = bind_secrets(stored, none, &Secrets::default()).unwrap_err();
        assert!(
            err.to_string().contains("password and SSH password"),
            "{err}"
        );

        // Nothing stored means nothing to leak.
        assert_eq!(
            bind_secrets(ResolvedSecrets::default(), none, &Secrets::default()).unwrap(),
            ResolvedSecrets::default()
        );
    }

    #[test]
    fn tls_fields_are_backward_compatible() {
        let json = r#"{"id":"x","name":"n","host":"h","port":5432,"user":"u",
            "database":"d","sslMode":"verify-ca","sslRootCert":"/etc/ca.pem"}"#;
        let c: ConnectionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.ssl_mode, SslMode::VerifyCa);
        assert_eq!(c.ssl_root_cert.as_deref(), Some("/etc/ca.pem"));

        let old: ConnectionConfig = serde_json::from_str(
            r#"{"id":"x","name":"n","host":"h","port":5432,"user":"u","database":"d"}"#,
        )
        .unwrap();
        assert_eq!(old.ssl_root_cert, None);
        let out = serde_json::to_value(&old).unwrap();
        assert!(out.get("sslRootCert").is_none());
    }

    #[test]
    fn store_serves_from_cache_and_serializes_writes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ConnectionStore::new(dir.path()));
        let threads: Vec<_> = (0..16)
            .map(|i| {
                let store = store.clone();
                std::thread::spawn(move || store.upsert(sample(&format!("c{i}"))).unwrap())
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(store.list().unwrap().len(), 16);
        assert_eq!(ConnectionStore::new(dir.path()).list().unwrap().len(), 16);

        // Reads come from memory, not from the file.
        std::fs::write(dir.path().join("connections.json"), "not json").unwrap();
        assert_eq!(store.get("c3").unwrap().id, "c3");
    }

    #[cfg(unix)]
    #[test]
    fn store_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(dir.path());
        store.upsert(sample("a")).unwrap();
        let mode = std::fs::metadata(dir.path().join("connections.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
