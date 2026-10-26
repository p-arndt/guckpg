//! PostgreSQL custom-format archives using the locally installed client tools.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::process::{Command, Stdio};

use tauri::State;

use crate::config::{ConnectionConfig, SslMode, load_secrets};
use crate::error::{AppError, AppResult};
use crate::session::Manager;

fn command(
    program: &str,
    cfg: &ConnectionConfig,
    database: &str,
    password: Option<&str>,
) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear();
    for name in ["PATH", "HOME", "USERPROFILE", "SYSTEMROOT", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    cmd.env("PGHOST", &cfg.host)
        .env("PGPORT", cfg.port.to_string())
        .env("PGUSER", &cfg.user)
        .env("PGDATABASE", database)
        .env(
            "PGSSLMODE",
            match cfg.ssl_mode {
                SslMode::Disable => "disable",
                SslMode::Prefer => "prefer",
                SslMode::Require => "require",
                SslMode::VerifyCa => "verify-ca",
                SslMode::VerifyFull => "verify-full",
            },
        )
        .env("PGCONNECT_TIMEOUT", "15")
        .env("PGPASSWORD", password.unwrap_or(""))
        .stdin(Stdio::null());
    if let Some(root_cert) = cfg
        .ssl_root_cert
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        cmd.env("PGSSLROOTCERT", crate::ssh::expand_home(root_cert.trim()));
    }
    cmd
}

fn validate(cfg: &ConnectionConfig, database: &str, path: &str, restore: bool) -> AppResult<()> {
    if cfg.ssh.is_some() {
        return Err(AppError::msg(
            "Backup and restore do not support SSH connections",
        ));
    }
    if restore {
        crate::guard::ensure_writable(cfg)?;
    }
    if database.trim().is_empty() {
        return Err(AppError::msg("Select a database"));
    }
    if !Path::new(path).is_absolute() {
        return Err(AppError::msg("Enter an absolute file path"));
    }
    Ok(())
}

fn database_option(database: &str) -> String {
    format!(
        "dbname='{}'",
        database.replace('\\', "\\\\").replace('\'', "\\'")
    )
}

fn run(
    cfg: ConnectionConfig,
    database: String,
    path: String,
    password: Option<String>,
    restore: bool,
) -> AppResult<()> {
    validate(&cfg, &database, &path, restore)?;
    let mut cmd = command(
        if restore { "pg_restore" } else { "pg_dump" },
        &cfg,
        &database,
        password.as_deref(),
    );
    if restore {
        let file = File::open(&path)?;
        cmd.arg("--dbname").arg(database_option(&database));
        cmd.args([
            "--exit-on-error",
            "--single-transaction",
            "--clean",
            "--if-exists",
        ])
        .stdin(Stdio::from(file))
        .stdout(Stdio::null());
    } else {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        cmd.arg("--format=custom").stdout(Stdio::from(file));
    }
    let output = cmd.output();
    if !restore && !output.as_ref().is_ok_and(|o| o.status.success()) {
        let _ = fs::remove_file(&path);
    }
    let output = output
        .map_err(|e| AppError::msg(format!("Could not start PostgreSQL client tool: {e}")))?;
    if !output.status.success() {
        return Err(AppError::msg(format!(
            "PostgreSQL client failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[tauri::command]
pub async fn backup_database(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    path: String,
) -> AppResult<()> {
    let cfg = manager.store.get(&id)?;
    let password = load_secrets(&id)?.password;
    tauri::async_runtime::spawn_blocking(move || run(cfg, database, path, password, false))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn restore_database(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    path: String,
) -> AppResult<()> {
    let cfg = manager.store.get(&id)?;
    let password = load_secrets(&id)?.password;
    tauri::async_runtime::spawn_blocking(move || run(cfg, database, path, password, true))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SshAuth, SshConfig};

    fn config() -> ConnectionConfig {
        ConnectionConfig {
            host: "host".into(),
            user: "user".into(),
            database: "db".into(),
            ..crate::testutil::connection("id")
        }
    }
    #[test]
    fn rejects_unsafe_or_unsupported_targets() {
        let mut cfg = config();
        let path = std::env::temp_dir().join("guckpg-backup-test");
        let path = path.to_str().unwrap();
        assert!(validate(&cfg, "", path, false).is_err());
        assert!(validate(&cfg, "db", "relative", false).is_err());
        cfg.read_only = true;
        assert_eq!(
            validate(&cfg, "db", path, true).unwrap_err().to_string(),
            crate::guard::READ_ONLY_MESSAGE
        );
        assert!(validate(&cfg, "db", path, false).is_ok());
        cfg.ssh = Some(SshConfig {
            host: "host".into(),
            port: 22,
            user: "user".into(),
            auth: SshAuth::Agent,
        });
        assert!(validate(&cfg, "db", path, false).is_err());
    }
    #[test]
    fn never_overwrites_an_existing_archive() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(
            run(
                config(),
                "db".into(),
                file.path().to_string_lossy().into(),
                None,
                false
            )
            .is_err()
        );
        assert!(file.path().exists());
    }
    #[test]
    fn restore_database_name_is_a_single_escaped_conninfo_value() {
        assert_eq!(
            database_option("db' host=evil\\x"),
            "dbname='db\\' host=evil\\\\x'"
        );
    }
    #[test]
    fn connection_values_are_environment_not_arguments() {
        let cmd = command("pg_dump", &config(), "--dbname=other", Some("secret"));
        assert_eq!(cmd.get_args().count(), 0);
        assert!(
            cmd.get_envs()
                .any(|(k, v)| k == "PGDATABASE" && v.unwrap() == "--dbname=other")
        );
    }
}
