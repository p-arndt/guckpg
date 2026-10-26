//! SSH tunnels. Postgres connections are carried over `direct-tcpip` channels
//! of a single SSH session per saved connection, so no local port is opened.
//!
//! The target host is resolved on the SSH server, like Navicat does. Docker
//! container names usually do not resolve there, so when the server cannot
//! connect to a name we ask Docker on the server for the container's IP.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle, Msg};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::known_hosts::{check_known_hosts_path, learn_known_hosts_path};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelOpenFailure, ChannelStream, Disconnect};

use crate::config::{ResolvedSecrets, SshAuth, SshConfig};
use crate::error::{AppError, AppResult};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Paths of the known_hosts files we consult. The user's own file is read
/// but never written; keys the user accepts in guckpg go to the app's file.
#[derive(Debug, Clone)]
pub struct KnownHosts {
    pub user_file: Option<PathBuf>,
    pub app_file: PathBuf,
}

impl KnownHosts {
    pub fn new(app_dir: &Path) -> Self {
        Self {
            user_file: std::env::home_dir().map(|h| h.join(".ssh").join("known_hosts")),
            app_file: app_dir.join("known_hosts"),
        }
    }

    fn files(&self) -> impl Iterator<Item = &Path> {
        self.user_file
            .iter()
            .map(PathBuf::as_path)
            .chain(std::iter::once(self.app_file.as_path()))
    }

    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> AppResult<()> {
        let fingerprint = || key.fingerprint(HashAlg::Sha256).to_string();
        for file in self.files() {
            match check_known_hosts_path(host, port, key, file) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(russh::keys::Error::KeyChanged { .. }) => {
                    return Err(AppError::HostKeyChanged {
                        host: host.into(),
                        port,
                        fingerprint: fingerprint(),
                    });
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(AppError::UnknownHostKey {
            host: host.into(),
            port,
            fingerprint: fingerprint(),
            key: key
                .to_openssh()
                .map_err(|e| AppError::msg(format!("SSH key: {e}")))?,
        })
    }

    pub fn trust(&self, host: &str, port: u16, openssh_key: &str) -> AppResult<()> {
        let key = PublicKey::from_openssh(openssh_key)
            .map_err(|e| AppError::msg(format!("SSH key: {e}")))?;
        learn_known_hosts_path(host, port, &key, &self.app_file)?;
        Ok(())
    }
}

struct Handler {
    host: String,
    port: u16,
    known_hosts: KnownHosts,
    /// russh only lets us answer yes/no, so the reason for a "no" is parked
    /// here and picked up after `connect` fails.
    rejection: Arc<Mutex<Option<AppError>>>,
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let result = match server_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                self.known_hosts.check(&self.host, self.port, key)
            }
            PublicKeyOrCertificate::Certificate(_) => {
                Err(AppError::msg("SSH host certificates are not supported yet"))
            }
        };
        match result {
            Ok(()) => Ok(true),
            Err(e) => {
                *self.rejection.lock().unwrap() = Some(e);
                Ok(false)
            }
        }
    }
}

pub struct Tunnel {
    handle: Handle<Handler>,
    /// Container IP found via Docker for a target name, reused until it stops
    /// working (containers get a new IP when they are recreated).
    resolved: Mutex<Option<(String, String)>>,
}

impl Tunnel {
    pub async fn open(
        ssh: &SshConfig,
        secrets: &ResolvedSecrets,
        known_hosts: KnownHosts,
    ) -> AppResult<Tunnel> {
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(10)),
            keepalive_max: 3,
            nodelay: true,
            ..Default::default()
        });
        let rejection = Arc::new(Mutex::new(None));
        let handler = Handler {
            host: ssh.host.clone(),
            port: ssh.port,
            known_hosts,
            rejection: rejection.clone(),
        };

        let connect = client::connect(config, (ssh.host.as_str(), ssh.port), handler);
        let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
            Err(_) => {
                return Err(AppError::msg(format!(
                    "SSH: timed out connecting to {}:{}",
                    ssh.host, ssh.port
                )));
            }
            Ok(Err(e)) => return Err(rejection.lock().unwrap().take().unwrap_or(e.into())),
            Ok(Ok(h)) => h,
        };

        let authenticated = match &ssh.auth {
            SshAuth::Password => {
                let password = secrets
                    .ssh_password
                    .as_deref()
                    .ok_or_else(|| AppError::msg("SSH password is missing"))?;
                handle
                    .authenticate_password(&ssh.user, password)
                    .await?
                    .success()
            }
            SshAuth::Key { key_path } => {
                let path = expand_home(key_path);
                let key = russh::keys::load_secret_key(&path, secrets.ssh_passphrase.as_deref())
                    .map_err(|e| match e {
                        russh::keys::Error::KeyIsEncrypted => {
                            AppError::msg("SSH key is encrypted: enter its passphrase")
                        }
                        e => AppError::msg(format!("SSH key {}: {e}", path.display())),
                    })?;
                let hash = handle.best_supported_rsa_hash().await?.flatten();
                handle
                    .authenticate_publickey(
                        &ssh.user,
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                    )
                    .await?
                    .success()
            }
            SshAuth::Agent => {
                authenticate_auto(&mut handle, ssh, secrets.ssh_passphrase.as_deref()).await?
            }
        };
        if !authenticated {
            return Err(AppError::msg(format!(
                "SSH: authentication failed for {}@{}",
                ssh.user, ssh.host
            )));
        }

        Ok(Tunnel {
            handle,
            resolved: Mutex::new(None),
        })
    }

    /// Open a byte stream to `host:port` as seen from the SSH server.
    pub async fn open_stream(&self, host: &str, port: u16) -> AppResult<ChannelStream<Msg>> {
        let cached = self
            .resolved
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(name, _)| name == host)
            .map(|(_, ip)| ip.clone());

        let first_try = cached.as_deref().unwrap_or(host);
        match self.direct(first_try, port).await {
            Ok(stream) => Ok(stream),
            Err(e) if is_connect_failure(&e) && is_container_name(host) => {
                let ip = self.docker_ip(host).await.map_err(|docker_err| {
                    AppError::msg(format!(
                        "SSH server cannot reach {host}:{port}, and Docker lookup failed: {docker_err}"
                    ))
                })?;
                let stream = self.direct(&ip, port).await.map_err(AppError::from)?;
                *self.resolved.lock().unwrap() = Some((host.to_string(), ip));
                Ok(stream)
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn direct(&self, host: &str, port: u16) -> Result<ChannelStream<Msg>, russh::Error> {
        let channel = self
            .handle
            .channel_open_direct_tcpip(host, port.into(), "127.0.0.1", 0)
            .await?;
        Ok(channel.into_stream())
    }

    /// Run a command on the SSH server and return stdout; fails on a non-zero
    /// exit status with stderr as the message.
    pub async fn exec(&self, command: &str) -> AppResult<String> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        let (mut stdout, mut stderr, mut status) = (Vec::new(), Vec::new(), None);
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
        }
        match status {
            Some(0) => Ok(String::from_utf8_lossy(&stdout).into_owned()),
            _ => {
                let err = String::from_utf8_lossy(&stderr).trim().to_string();
                Err(AppError::msg(if err.is_empty() {
                    format!("`{command}` failed")
                } else {
                    err
                }))
            }
        }
    }

    async fn docker_ip(&self, container: &str) -> AppResult<String> {
        // `container` passed is_container_name, so it is safe in a shell line.
        let out = self
            .exec(&format!(
                "docker inspect -f '{{{{range .NetworkSettings.Networks}}}}{{{{.IPAddress}}}} {{{{end}}}}' {container}"
            ))
            .await?;
        parse_inspect_ips(&out)
            .ok_or_else(|| AppError::msg(format!("container {container} has no network IP")))
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    pub async fn close(&self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
    }
}

/// What plain `ssh` does without an IdentityFile: keys from the agent first,
/// then the default key files in ~/.ssh. On macOS the agent is often empty
/// because ssh reads the key files directly, so the files matter.
async fn authenticate_auto(
    handle: &mut Handle<Handler>,
    ssh: &SshConfig,
    passphrase: Option<&str>,
) -> AppResult<bool> {
    let user = ssh.user.as_str();
    let rsa_hash = handle.best_supported_rsa_hash().await?.flatten();
    let mut tried = Vec::new();

    match connect_agent().await {
        Ok(mut agent) => match agent.request_identities().await {
            Ok(identities) if identities.is_empty() => tried.push("agent (no keys loaded)".into()),
            Ok(identities) => {
                for identity in identities {
                    let AgentIdentity::PublicKey { key, .. } = identity else {
                        continue;
                    };
                    let hash = if key.algorithm().is_rsa() {
                        rsa_hash
                    } else {
                        None
                    };
                    let result = handle
                        .authenticate_publickey_with(user, key, hash, &mut agent)
                        .await
                        .map_err(|e| AppError::msg(format!("SSH agent: {e}")))?;
                    if result.success() {
                        return Ok(true);
                    }
                }
                tried.push("agent keys (rejected)".into());
            }
            Err(e) => tried.push(format!("agent ({e})")),
        },
        Err(_) => tried.push("agent (not running)".into()),
    }

    let Some(home) = std::env::home_dir() else {
        return Err(no_key_accepted(ssh, &tried));
    };
    for path in default_key_files(&home) {
        if !path.exists() {
            continue;
        }
        let shown = format!("~/.ssh/{}", path.file_name().unwrap_or_default().display());
        let key = match russh::keys::load_secret_key(&path, passphrase) {
            Ok(key) => key,
            Err(russh::keys::Error::KeyIsEncrypted) => {
                tried.push(format!("{shown} (needs its passphrase)"));
                continue;
            }
            Err(e) => {
                tried.push(format!("{shown} ({e})"));
                continue;
            }
        };
        let hash = if key.algorithm().is_rsa() {
            rsa_hash
        } else {
            None
        };
        let result = handle
            .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
            .await?;
        if result.success() {
            return Ok(true);
        }
        tried.push(format!("{shown} (rejected)"));
    }
    Err(no_key_accepted(ssh, &tried))
}

/// The files OpenSSH tries when no IdentityFile is configured, in its order.
pub fn default_key_files(home: &Path) -> Vec<PathBuf> {
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|name| home.join(".ssh").join(name))
        .collect()
}

fn no_key_accepted(ssh: &SshConfig, tried: &[String]) -> AppError {
    let tried = if tried.is_empty() {
        "no agent and no key files in ~/.ssh".to_string()
    } else {
        tried.join(", ")
    };
    AppError::msg(format!(
        "SSH: no key accepted for {}@{} (tried {tried})",
        ssh.user, ssh.host
    ))
}

type DynAgent = AgentClient<Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>>;

#[cfg(unix)]
async fn connect_agent() -> AppResult<DynAgent> {
    AgentClient::connect_env()
        .await
        .map(|a| a.dynamic())
        .map_err(|e| AppError::msg(format!("SSH agent not reachable (SSH_AUTH_SOCK): {e}")))
}

#[cfg(windows)]
async fn connect_agent() -> AppResult<DynAgent> {
    if let Ok(agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        return Ok(agent.dynamic());
    }
    AgentClient::connect_pageant()
        .await
        .map(|a| a.dynamic())
        .map_err(|e| {
            AppError::msg(format!(
                "SSH agent not reachable (OpenSSH agent or Pageant): {e}"
            ))
        })
}

fn is_connect_failure(err: &russh::Error) -> bool {
    matches!(
        err,
        russh::Error::ChannelOpenFailure(
            ChannelOpenFailure::ConnectFailed | ChannelOpenFailure::Other { .. }
        )
    )
}

/// Docker container names are `[a-zA-Z0-9][a-zA-Z0-9_.-]*`. Anything that is
/// an IP or `localhost` is never looked up. This check also keeps the name
/// safe to put into the `docker inspect` command line.
pub fn is_container_name(host: &str) -> bool {
    if host.parse::<IpAddr>().is_ok() || host.eq_ignore_ascii_case("localhost") {
        return false;
    }
    let mut chars = host.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// First IP in `docker inspect` output (space-separated, one per network).
pub fn parse_inspect_ips(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|s| s.parse::<IpAddr>().is_ok())
        .map(str::to_string)
}

pub fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => std::env::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| PathBuf::from(path)),
        None => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_names() {
        assert!(is_container_name("bobiverse-postgres-1"));
        assert!(is_container_name("db"));
        assert!(is_container_name("my_app.db"));
        assert!(!is_container_name("10.0.0.5"));
        assert!(!is_container_name("::1"));
        assert!(!is_container_name("localhost"));
        assert!(!is_container_name(""));
        assert!(!is_container_name("-rf"));
        assert!(!is_container_name("db; rm -rf /"));
        assert!(!is_container_name("db$(id)"));
        assert!(!is_container_name("a b"));
    }

    #[test]
    fn inspect_output() {
        assert_eq!(
            parse_inspect_ips("172.18.0.4 \n").as_deref(),
            Some("172.18.0.4")
        );
        assert_eq!(
            parse_inspect_ips(" 172.20.0.2 172.18.0.4 ").as_deref(),
            Some("172.20.0.2")
        );
        assert_eq!(parse_inspect_ips("  \n"), None);
    }

    #[test]
    fn default_keys_follow_openssh_order() {
        let files = default_key_files(Path::new("/home/u"));
        assert_eq!(
            files,
            vec![
                PathBuf::from("/home/u/.ssh/id_ed25519"),
                PathBuf::from("/home/u/.ssh/id_ecdsa"),
                PathBuf::from("/home/u/.ssh/id_rsa"),
            ]
        );
    }

    #[test]
    fn home_expansion() {
        assert_eq!(expand_home("/etc/key"), PathBuf::from("/etc/key"));
        if let Some(home) = std::env::home_dir() {
            assert_eq!(expand_home("~/.ssh/id"), home.join(".ssh/id"));
        }
    }

    #[test]
    fn known_hosts_unknown_then_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let kh = KnownHosts {
            user_file: None,
            app_file: dir.path().join("known_hosts"),
        };
        let line =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
        let key = PublicKey::from_openssh(line).unwrap();

        let err = kh.check("vps.example", 22, &key).unwrap_err();
        let AppError::UnknownHostKey { key: offered, .. } = err else {
            panic!("expected unknown host key, got {err:?}");
        };
        kh.trust("vps.example", 22, &offered).unwrap();
        kh.check("vps.example", 22, &key).unwrap();
        // The port is part of the identity.
        assert!(kh.check("vps.example", 2222, &key).is_err());
    }
}
