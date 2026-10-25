//! Integration tests against a real Postgres (and SSH server for the tunnel
//! tests), started by `compose.yaml`. They skip themselves unless the env
//! vars are set; `just test-it` sets them.

use std::sync::Arc;
use std::time::Duration;

use crate::catalog;
use crate::config::{ConnectionConfig, ResolvedSecrets, SshAuth, SshConfig, SslMode};
use crate::editing;
use crate::error::AppError;
use crate::pg::{PgConn, Transport};
use crate::sql::{Change, ColumnValue};
use crate::ssh::{KnownHosts, Tunnel};

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn pg_target() -> Option<(ConnectionConfig, String)> {
    let config = ConnectionConfig {
        id: "it".into(),
        name: "it".into(),
        host: env("POSTGRES_HOST")?,
        port: env("POSTGRES_PORT")?.parse().ok()?,
        user: env("POSTGRES_USER")?,
        database: env("POSTGRES_DB")?,
        ssl_mode: SslMode::Prefer,
        read_only: false,
        color: None,
        ssh: None,
        folder: None,
    };
    Some((config, env("POSTGRES_PASSWORD")?))
}

async fn direct() -> Option<PgConn> {
    let Some((cfg, pw)) = pg_target() else {
        eprintln!("skipped: POSTGRES_* not set");
        return None;
    };
    Some(
        PgConn::connect(&cfg, &cfg.database, Some(&pw), Transport::Direct)
            .await
            .expect("connect"),
    )
}

#[tokio::test]
async fn runs_scripts_with_several_results() {
    let Some(conn) = direct().await else { return };
    let out = conn
        .run(
            "CREATE TEMP TABLE it_t (id int PRIMARY KEY, name text);
             INSERT INTO it_t VALUES (1, 'a'), (2, NULL), (3, 'c');
             SELECT id, name FROM it_t ORDER BY id;",
            2,
        )
        .await
        .unwrap();
    assert_eq!(out.results.len(), 3);
    assert!(out.results[0].columns.is_empty());
    assert_eq!(out.results[1].row_count, 3);

    let select = &out.results[2];
    assert_eq!(select.columns, vec!["id", "name"]);
    assert_eq!(select.rows.len(), 2);
    assert!(select.truncated);
    assert_eq!(select.row_count, 3);
    assert_eq!(select.rows[1], vec![Some("2".to_string()), None]);
}

#[tokio::test]
async fn reports_errors_with_position() {
    let Some(conn) = direct().await else { return };
    let err = conn
        .run("SELECT * FROM no_such_table", 10)
        .await
        .unwrap_err();
    let AppError::Postgres { code, position, .. } = err else {
        panic!("expected postgres error, got {err:?}");
    };
    assert_eq!(code.as_deref(), Some("42P01"));
    assert_eq!(position, Some(15));
}

#[tokio::test]
async fn cancels_running_query() {
    let Some(conn) = direct().await else { return };
    let conn = Arc::new(conn);
    let running = {
        let conn = conn.clone();
        tokio::spawn(async move { conn.run("SELECT pg_sleep(30)", 10).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    conn.cancel().await.unwrap();
    let err = tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .expect("query did not stop")
        .unwrap()
        .unwrap_err();
    assert!(matches!(err, AppError::Postgres { code: Some(ref c), .. } if c == "57014"));
}

#[tokio::test]
async fn read_only_blocks_writes() {
    let Some((mut cfg, pw)) = pg_target() else {
        return;
    };
    cfg.read_only = true;
    let conn = PgConn::connect(&cfg, &cfg.database, Some(&pw), Transport::Direct)
        .await
        .unwrap();
    let err = conn
        .run("CREATE TABLE it_should_not_exist (id int)", 10)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Postgres { code: Some(ref c), .. } if c == "25006"));
}

#[tokio::test]
async fn browses_catalog() {
    let Some(conn) = direct().await else { return };
    conn.run(
        "DROP TABLE IF EXISTS it_catalog;
         CREATE TABLE it_catalog (id bigint PRIMARY KEY, label text NOT NULL DEFAULT 'x');",
        10,
    )
    .await
    .unwrap();

    let schemas = catalog::schemas(&conn.client, false).await.unwrap();
    assert_eq!(schemas.first().map(String::as_str), Some("public"));
    assert!(!schemas.iter().any(|s| s == "pg_catalog"));

    let rels = catalog::relations(&conn.client, "public").await.unwrap();
    assert!(
        rels.iter()
            .any(|r| r.name == "it_catalog" && r.kind == "table")
    );

    let cols = catalog::columns(&conn.client, "public", "it_catalog")
        .await
        .unwrap();
    assert_eq!(cols.len(), 2);
    assert!(cols[0].primary_key);
    assert_eq!(cols[0].data_type, "bigint");
    assert!(cols[1].not_null);
    assert_eq!(cols[1].default.as_deref(), Some("'x'::text"));

    let completion = catalog::completion(&conn.client).await.unwrap();
    let t = completion.iter().find(|t| t.name == "it_catalog").unwrap();
    assert_eq!(t.columns, vec!["id", "label"]);

    assert!(!catalog::databases(&conn.client).await.unwrap().is_empty());
}

/// SSH server plus the Postgres host name as the SSH server sees it.
fn ssh_target() -> Option<(SshConfig, String, String)> {
    Some((
        SshConfig {
            host: env("SSH_HOST")?,
            port: env("SSH_PORT")?.parse().ok()?,
            user: env("SSH_USER")?,
            auth: SshAuth::Password,
        },
        env("SSH_PASSWORD")?,
        env("SSH_PG_HOST")?,
    ))
}

#[tokio::test]
async fn tunnels_through_ssh() {
    let (Some((ssh, ssh_pw, pg_host)), Some((mut cfg, pg_pw))) = (ssh_target(), pg_target()) else {
        eprintln!("skipped: SSH_* or POSTGRES_* not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let known_hosts = KnownHosts {
        user_file: None,
        app_file: dir.path().join("known_hosts"),
    };
    let secrets = ResolvedSecrets {
        ssh_password: Some(ssh_pw),
        ..Default::default()
    };

    // First contact: the key is unknown and must be confirmed.
    let err = Tunnel::open(&ssh, &secrets, known_hosts.clone())
        .await
        .err()
        .expect("unknown host key must be rejected");
    let AppError::UnknownHostKey {
        host, port, key, ..
    } = err
    else {
        panic!("expected unknown host key, got {err:?}");
    };
    known_hosts.trust(&host, port, &key).unwrap();

    let tunnel = Arc::new(Tunnel::open(&ssh, &secrets, known_hosts).await.unwrap());

    // Postgres listens on 5432 inside the Docker network, addressed by name.
    cfg.host = pg_host;
    cfg.port = 5432;
    let conn = PgConn::connect(
        &cfg,
        &cfg.database,
        Some(&pg_pw),
        Transport::Tunnel(tunnel.clone()),
    )
    .await
    .unwrap();
    let out = conn.run("SELECT 40 + 2 AS answer", 10).await.unwrap();
    assert_eq!(out.results[0].rows[0][0].as_deref(), Some("42"));

    // Cancel goes through the tunnel too.
    let conn = Arc::new(conn);
    let running = {
        let conn = conn.clone();
        tokio::spawn(async move { conn.run("SELECT pg_sleep(30)", 10).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    conn.cancel().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .expect("query did not stop")
        .unwrap();
    assert!(result.is_err());

    // A host the SSH server cannot reach fails with a clear message.
    let mut bad = cfg.clone();
    bad.host = "no-such-container".into();
    let err = PgConn::connect(
        &bad,
        &bad.database,
        Some(&pg_pw),
        Transport::Tunnel(tunnel.clone()),
    )
    .await
    .err()
    .unwrap();
    assert!(err.to_string().contains("no-such-container"), "{err}");

    tunnel.close().await;
}

#[tokio::test]
async fn builds_diagram_with_composite_foreign_keys() {
    let Some(conn) = direct().await else { return };
    conn.run(
        "DROP SCHEMA IF EXISTS it_er CASCADE;
         CREATE SCHEMA it_er;
         CREATE TABLE it_er.customers (id int PRIMARY KEY, name text);
         CREATE TABLE it_er.orders (
             id int PRIMARY KEY,
             customer_id int NOT NULL REFERENCES it_er.customers (id)
         );
         CREATE TABLE it_er.order_lines (
             order_id int REFERENCES it_er.orders (id),
             line int,
             PRIMARY KEY (order_id, line)
         );
         CREATE TABLE it_er.shipments (
             id int PRIMARY KEY,
             ref_line int, ref_order int,
             CONSTRAINT shipments_line_fk FOREIGN KEY (ref_order, ref_line)
                 REFERENCES it_er.order_lines (order_id, line)
         );
         CREATE VIEW it_er.big_orders AS SELECT * FROM it_er.orders;",
        10,
    )
    .await
    .unwrap();

    let d = catalog::diagram(&conn.client, "it_er").await.unwrap();
    let names: Vec<_> = d.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "big_orders",
            "customers",
            "order_lines",
            "orders",
            "shipments"
        ]
    );
    let lines = d.tables.iter().find(|t| t.name == "order_lines").unwrap();
    assert!(lines.columns.iter().all(|c| c.primary_key));
    assert_eq!(d.tables[0].kind, "view");

    assert_eq!(d.foreign_keys.len(), 3);
    let composite = d
        .foreign_keys
        .iter()
        .find(|f| f.name == "shipments_line_fk")
        .unwrap();
    assert_eq!(composite.from_columns, ["ref_order", "ref_line"]);
    assert_eq!(composite.to_table, "order_lines");
    assert_eq!(composite.to_columns, ["order_id", "line"]);

    conn.run("DROP SCHEMA it_er CASCADE", 10).await.unwrap();
}

fn cv(column: &str, value: Option<&str>) -> ColumnValue {
    ColumnValue {
        column: column.into(),
        value: value.map(str::to_string),
    }
}

#[tokio::test]
async fn detects_editable_results() {
    let Some(conn) = direct().await else { return };
    conn.run(
        "DROP TABLE IF EXISTS it_edit_src, it_edit_other, it_edit_nopk;
         CREATE TABLE it_edit_src (tenant text, id int, name text, PRIMARY KEY (tenant, id));
         CREATE TABLE it_edit_other (id int PRIMARY KEY);
         CREATE TABLE it_edit_nopk (a int);",
        10,
    )
    .await
    .unwrap();

    // Aliases and expressions are fine; the key comes back in key order.
    let info = editing::source_info(
        &conn.client,
        "SELECT name AS label, id, upper(name), tenant FROM it_edit_src;",
    )
    .await
    .expect("editable");
    assert_eq!(info.table, "it_edit_src");
    assert_eq!(info.schema, "public");
    assert_eq!(
        info.columns,
        vec![
            Some("name".into()),
            Some("id".into()),
            None,
            Some("tenant".into())
        ]
    );
    assert_eq!(info.key, vec![3, 1]);

    // Names stay aligned with the result even when a column repeats.
    let info = editing::source_info(&conn.client, "SELECT id, 1, tenant, id FROM it_edit_src")
        .await
        .expect("editable");
    assert_eq!(
        info.columns,
        vec![
            Some("id".into()),
            None,
            Some("tenant".into()),
            Some("id".into())
        ]
    );
    assert_eq!(info.key, vec![2, 0]);

    for sql in [
        "SELECT name FROM it_edit_src",
        "SELECT s.id, s.tenant, o.id FROM it_edit_src s, it_edit_other o",
        "SELECT * FROM it_edit_nopk",
        "SELECT 1",
        "this is not sql",
    ] {
        assert!(
            editing::source_info(&conn.client, sql).await.is_none(),
            "{sql} should not be editable"
        );
    }
}

#[tokio::test]
async fn applies_changes_in_one_transaction() {
    let Some(conn) = direct().await else { return };
    conn.run(
        "DROP TABLE IF EXISTS it_apply;
         CREATE TABLE it_apply (id int PRIMARY KEY, name text, note text DEFAULT 'd', data jsonb);
         INSERT INTO it_apply VALUES (1, 'a', 'x', NULL), (2, 'b', 'y', NULL), (3, 'c', 'z', NULL);",
        10,
    )
    .await
    .unwrap();

    let changes = vec![
        Change::Update {
            key: vec![cv("id", Some("1"))],
            values: vec![
                cv("name", Some(r"O'Brien \ back")),
                cv("note", None),
                cv("data", Some(r#"{"k": [1, 2]}"#)),
            ],
        },
        Change::Delete {
            key: vec![cv("id", Some("2"))],
        },
        Change::Insert {
            values: vec![cv("id", Some("4")), cv("name", Some("new"))],
        },
    ];
    assert_eq!(
        editing::apply(&conn.client, "public", "it_apply", &changes)
            .await
            .unwrap(),
        3
    );
    let out = conn
        .run(
            "SELECT id, name, note, data->'k'->>1 FROM it_apply ORDER BY id",
            10,
        )
        .await
        .unwrap();
    let rows = &out.results[0].rows;
    let s = |v: &str| Some(v.to_string());
    assert_eq!(rows[0], vec![s("1"), s(r"O'Brien \ back"), None, s("2")]);
    assert_eq!(rows[1][0], s("3"));
    assert_eq!(rows[2], vec![s("4"), s("new"), s("d"), None]);

    // A key that no longer exists rolls back the whole batch.
    let err = editing::apply(
        &conn.client,
        "public",
        "it_apply",
        &[
            Change::Update {
                key: vec![cv("id", Some("3"))],
                values: vec![cv("name", Some("changed"))],
            },
            Change::Delete {
                key: vec![cv("id", Some("999"))],
            },
        ],
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("0 matched"), "{err}");
    let out = conn
        .run("SELECT name FROM it_apply WHERE id = 3", 10)
        .await
        .unwrap();
    assert_eq!(out.results[0].rows[0][0].as_deref(), Some("c"));

    // A type error also rolls back, and the connection stays usable.
    assert!(
        editing::apply(
            &conn.client,
            "public",
            "it_apply",
            &[Change::Update {
                key: vec![cv("id", Some("3"))],
                values: vec![cv("data", Some("not json"))],
            }],
        )
        .await
        .is_err()
    );
    conn.run("SELECT 1", 10).await.unwrap();
}
