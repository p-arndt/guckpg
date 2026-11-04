//! PostgreSQL backups using the locally installed client tools.

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

struct TransferOptions {
    restore: bool,
    sql: bool,
    tables: Vec<String>,
    contents: String,
    clean: bool,
}

fn export_options(cmd: &mut Command, options: &TransferOptions) -> AppResult<()> {
    match options.contents.as_str() {
        "all" => {}
        "schema" => {
            cmd.arg("--schema-only");
        }
        "data" => {
            cmd.arg("--data-only");
        }
        _ => return Err(AppError::msg("Invalid backup contents")),
    }
    if options.clean && options.contents != "data" {
        cmd.args(["--clean", "--if-exists"]);
    }
    for table in options.tables.iter().filter(|t| !t.trim().is_empty()) {
        cmd.arg("--table").arg(table.trim());
    }
    cmd.arg(if options.sql {
        "--format=plain"
    } else {
        "--format=custom"
    });
    Ok(())
}

fn run(
    cfg: ConnectionConfig,
    database: String,
    path: String,
    password: Option<String>,
    options: TransferOptions,
) -> AppResult<()> {
    let TransferOptions { restore, sql, .. } = &options;
    let (restore, sql) = (*restore, *sql);
    validate(&cfg, &database, &path, restore)?;
    let mut cmd = command(
        if restore {
            if sql { "psql" } else { "pg_restore" }
        } else {
            "pg_dump"
        },
        &cfg,
        &database,
        password.as_deref(),
    );
    if restore {
        let file = File::open(&path)?;
        cmd.arg("--dbname").arg(database_option(&database));
        if sql {
            cmd.args([
                "--no-psqlrc",
                "--set=ON_ERROR_STOP=1",
                "--single-transaction",
                "--file=-",
            ]);
        } else {
            cmd.args(["--exit-on-error", "--single-transaction"]);
            if options.clean {
                cmd.args(["--clean", "--if-exists"]);
            }
        }
        cmd.stdin(Stdio::from(file)).stdout(Stdio::null());
    } else {
        export_options(&mut cmd, &options)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        cmd.stdout(Stdio::from(file));
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

#[derive(serde::Deserialize)]
pub struct ExportOptions {
    sql: bool,
    tables: Vec<String>,
    contents: String,
    clean: bool,
}

#[tauri::command]
pub async fn backup_database(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    path: String,
    options: ExportOptions,
) -> AppResult<()> {
    let cfg = manager.store.get(&id)?;
    let password = load_secrets(&id)?.password;
    tauri::async_runtime::spawn_blocking(move || {
        run(
            cfg,
            database,
            path,
            password,
            TransferOptions {
                restore: false,
                sql: options.sql,
                tables: options.tables,
                contents: options.contents,
                clean: options.clean,
            },
        )
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn restore_database(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    path: String,
    sql: bool,
    replace: bool,
) -> AppResult<()> {
    let cfg = manager.store.get(&id)?;
    let password = load_secrets(&id)?.password;
    tauri::async_runtime::spawn_blocking(move || {
        run(
            cfg,
            database,
            path,
            password,
            TransferOptions {
                restore: true,
                sql,
                tables: Vec::new(),
                contents: "all".into(),
                clean: replace,
            },
        )
    })
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
    #[tokio::test]
    async fn selected_table_sql_and_archive_round_trip() {
        let Some((cfg, password)) = crate::testutil::test_db("backup-roundtrip") else {
            return;
        };
        if ["pg_dump", "pg_restore", "psql"]
            .iter()
            .any(|tool| Command::new(tool).arg("--version").output().is_err())
        {
            eprintln!("PostgreSQL client tools unavailable; skipping backup round trip");
            return;
        }
        let conn = crate::pg::PgConn::connect(
            &cfg,
            &cfg.database,
            Some(&password),
            crate::pg::Transport::Direct,
        )
        .await
        .unwrap();
        let schema = format!("backup_{}", uuid::Uuid::new_v4().simple());
        let table = crate::sql::qualified(&schema, "records");
        conn.client.batch_execute(&format!("CREATE SCHEMA \"{schema}\"; CREATE TABLE {table} (id integer PRIMARY KEY, value text); INSERT INTO {table} VALUES (1, 'hello'), (2, '世界')")).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        for sql in [true, false] {
            let path = dir
                .path()
                .join(if sql { "backup.sql" } else { "backup.dump" })
                .to_string_lossy()
                .into_owned();
            let cfg_copy = cfg.clone();
            let db = cfg.database.clone();
            let pw = password.clone();
            let selected = format!("{schema}.records");
            let export_path = path.clone();
            tokio::task::spawn_blocking(move || {
                run(
                    cfg_copy,
                    db,
                    export_path,
                    Some(pw),
                    TransferOptions {
                        restore: false,
                        sql,
                        tables: vec![selected],
                        contents: "all".into(),
                        clean: false,
                    },
                )
            })
            .await
            .unwrap()
            .unwrap();
            conn.client
                .batch_execute(&format!("DROP TABLE {table}"))
                .await
                .unwrap();
            let cfg_copy = cfg.clone();
            let db = cfg.database.clone();
            let pw = password.clone();
            tokio::task::spawn_blocking(move || {
                run(
                    cfg_copy,
                    db,
                    path,
                    Some(pw),
                    TransferOptions {
                        restore: true,
                        sql,
                        tables: vec![],
                        contents: "all".into(),
                        clean: false,
                    },
                )
            })
            .await
            .unwrap()
            .unwrap();
            let rows = conn
                .client
                .query(&format!("SELECT id, value FROM {table} ORDER BY id"), &[])
                .await
                .unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1].get::<_, &str>(1), "世界");
        }
        conn.client
            .batch_execute(&format!("DROP SCHEMA \"{schema}\" CASCADE"))
            .await
            .unwrap();
    }

    #[test]
    fn export_options_select_format_tables_and_contents() {
        let mut cmd = Command::new("pg_dump");
        let mut options = TransferOptions {
            restore: false,
            sql: true,
            tables: vec!["public.orders".into(), " odd table ".into()],
            contents: "schema".into(),
            clean: true,
        };
        export_options(&mut cmd, &options).unwrap();
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--schema-only",
                "--clean",
                "--if-exists",
                "--table",
                "public.orders",
                "--table",
                "odd table",
                "--format=plain"
            ]
        );
        options.contents = "invalid".into();
        assert!(export_options(&mut Command::new("pg_dump"), &options).is_err());
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
                TransferOptions {
                    restore: false,
                    sql: false,
                    tables: Vec::new(),
                    contents: "all".into(),
                    clean: false
                }
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
