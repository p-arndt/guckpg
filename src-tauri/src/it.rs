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

#[tokio::test]
async fn browses_object_definitions_and_refreshes_materialized_views() {
    let Some(conn) = direct().await else { return };
    conn.run(
        r#"DROP SCHEMA IF EXISTS "it objects" CASCADE;
           CREATE SCHEMA "it objects";
           SET search_path TO "it objects", pg_catalog;
           CREATE TABLE "source table" (id integer);
           INSERT INTO "source table" VALUES (1);
           CREATE TABLE other_table (id integer);
           CREATE VIEW "View Name" WITH (security_barrier = true, check_option = local)
               AS SELECT id FROM "source table";
           CREATE MATERIALIZED VIEW "Materialized View" AS SELECT id FROM "source table";
           CREATE FUNCTION overloaded(value integer) RETURNS integer LANGUAGE SQL AS 'SELECT value';
           CREATE FUNCTION overloaded(value text) RETURNS text LANGUAGE SQL AS 'SELECT value';
           CREATE PROCEDURE do_nothing() LANGUAGE SQL AS 'SELECT 1';
           CREATE FUNCTION on_change() RETURNS trigger LANGUAGE plpgsql AS
               'BEGIN RETURN NEW; END';
           CREATE TRIGGER same_name BEFORE INSERT ON "source table"
               FOR EACH ROW EXECUTE FUNCTION on_change();
           CREATE TRIGGER same_name BEFORE INSERT ON other_table
               FOR EACH ROW EXECUTE FUNCTION on_change();
           CREATE SEQUENCE "Sequence Name" AS integer INCREMENT BY 3 MINVALUE 2
               MAXVALUE 500 START WITH 8 CACHE 4 CYCLE;
           CREATE AGGREGATE summed(integer) (SFUNC = int4pl, STYPE = integer, INITCOND = '0');"#,
        10,
    )
    .await
    .unwrap();

    let objects = catalog::objects(&conn.client, "it objects").await.unwrap();
    let overloaded: Vec<_> = objects.iter().filter(|o| o.name == "overloaded").collect();
    assert_eq!(overloaded.len(), 2);
    assert_ne!(overloaded[0].oid, overloaded[1].oid);
    assert_ne!(overloaded[0].identity, overloaded[1].identity);
    assert!(overloaded.iter().all(|o| o.kind == "function"));
    assert!(!objects.iter().any(|o| o.name == "summed"));
    let triggers: Vec<_> = objects.iter().filter(|o| o.kind == "trigger").collect();
    assert_eq!(triggers.len(), 2);
    assert_ne!(triggers[0].identity, triggers[1].identity);
    assert!(triggers.iter().any(|o| o.identity.contains("source table")));

    for object in &objects {
        let sql = catalog::object_definition(&conn.client, "it objects", object.kind, object.oid)
            .await
            .unwrap();
        assert!(sql.contains("SET search_path TO \"it objects\", \"pg_catalog\";"));
        assert!(sql.ends_with(";\n"));
        match object.kind {
            "function" => assert!(sql.contains("CREATE OR REPLACE FUNCTION")),
            "procedure" => assert!(sql.contains("CREATE OR REPLACE PROCEDURE")),
            "trigger" => {
                assert!(sql.contains("CREATE TRIGGER same_name"));
                assert!(sql.contains("ON \"it objects\"."));
            }
            "sequence" => {
                assert!(
                    sql.contains("CREATE SEQUENCE \"it objects\".\"Sequence Name\" AS integer")
                );
                for option in [
                    "INCREMENT BY 3",
                    "MINVALUE 2",
                    "MAXVALUE 500",
                    "START WITH 8",
                    "CACHE 4",
                    "CYCLE",
                ] {
                    assert!(sql.contains(option), "missing {option}: {sql}");
                }
                assert!(!sql.contains("RESTART"));
                conn.run("SELECT nextval('\"it objects\".\"Sequence Name\"')", 10)
                    .await
                    .unwrap();
                assert_eq!(
                    catalog::object_definition(&conn.client, "it objects", "sequence", object.oid)
                        .await
                        .unwrap(),
                    sql
                );
            }
            kind => panic!("unexpected object kind {kind}"),
        }
    }

    let function_oid = overloaded[0].oid;
    for (schema, kind, oid) in [
        ("public", "function", function_oid),
        ("it objects", "procedure", function_oid),
        ("it objects", "table", function_oid),
        ("it objects", "function", 0),
    ] {
        assert!(
            catalog::object_definition(&conn.client, schema, kind, oid)
                .await
                .is_err()
        );
    }

    let view = catalog::relation_definition(&conn.client, "it objects", "View Name")
        .await
        .unwrap();
    assert!(view.contains("CREATE OR REPLACE VIEW \"it objects\".\"View Name\""));
    assert!(view.contains("security_barrier = 'true'"));
    assert!(view.contains("check_option = 'local'"));
    let path = conn.run("SHOW search_path", 10).await.unwrap();
    assert_eq!(
        path.results[0].rows[0][0].as_deref(),
        Some("\"it objects\", pg_catalog")
    );
    conn.run(&view, 10).await.unwrap();
    let options = conn
        .run(
            "SELECT reloptions::text FROM pg_catalog.pg_class
         WHERE oid = '\"it objects\".\"View Name\"'::regclass",
            10,
        )
        .await
        .unwrap();
    let options = options.results[0].rows[0][0].as_deref().unwrap();
    assert!(options.contains("security_barrier=true"));
    assert!(options.contains("check_option=local"));
    let materialized =
        catalog::relation_definition(&conn.client, "it objects", "Materialized View")
            .await
            .unwrap();
    assert!(materialized.contains("CREATE MATERIALIZED VIEW \"it objects\".\"Materialized View\""));
    assert!(!materialized.contains("DROP"));
    assert!(!materialized.contains("OR REPLACE"));
    assert!(
        catalog::relation_definition(&conn.client, "it objects", "source table")
            .await
            .is_err()
    );
    for name in [
        "View Name",
        "source table",
        "does not exist",
        "Materialized View\"; DROP SCHEMA public; --",
    ] {
        assert!(
            catalog::refresh_materialized_view(&conn.client, "it objects", name)
                .await
                .is_err()
        );
    }
    conn.run("INSERT INTO \"it objects\".\"source table\" VALUES (2)", 10)
        .await
        .unwrap();
    catalog::refresh_materialized_view(&conn.client, "it objects", "Materialized View")
        .await
        .unwrap();
    let rows = conn
        .run(
            "SELECT id FROM \"it objects\".\"Materialized View\" ORDER BY id",
            10,
        )
        .await
        .unwrap();
    assert_eq!(
        rows.results[0].rows,
        vec![vec![Some("1".into())], vec![Some("2".into())]]
    );
    conn.run("BEGIN READ ONLY", 10).await.unwrap();
    let err = catalog::refresh_materialized_view(&conn.client, "it objects", "Materialized View")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Postgres { code: Some(ref code), .. } if code == "25006"));
    conn.run("ROLLBACK", 10).await.unwrap();
    conn.run("DROP SCHEMA \"it objects\" CASCADE", 10)
        .await
        .unwrap();
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

#[tokio::test]
async fn inspects_and_edits_roles() {
    let Some(conn) = direct().await else { return };
    let c = &conn.client;
    c.batch_execute(
        "DROP TABLE IF EXISTS it_role_t; DROP ROLE IF EXISTS it_role_app, it_role_web, it_role_readers;
         CREATE ROLE it_role_readers NOLOGIN;
         CREATE ROLE it_role_app LOGIN CONNECTION LIMIT 3;
         CREATE TABLE it_role_t (id int);
         GRANT SELECT ON it_role_t TO it_role_readers;
         GRANT it_role_readers TO it_role_app;",
    )
    .await
    .unwrap();

    let details =
        serde_json::to_value(crate::roles::details(c, "it_role_app").await.unwrap()).unwrap();
    assert_eq!(details["canLogin"], true);
    assert_eq!(details["connectionLimit"], 3);
    assert_eq!(details["validUntil"], serde_json::Value::Null);
    assert_eq!(details["memberOf"][0]["role"], "it_role_readers");
    let db = env("POSTGRES_DB").unwrap();
    let own_db = details["databases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == db.as_str())
        .unwrap();
    // CONNECT comes from PUBLIC, not from a direct grant.
    assert!(
        own_db["effective"]
            .as_array()
            .unwrap()
            .contains(&"CONNECT".into())
    );
    assert!(own_db["granted"].as_array().unwrap().is_empty());

    let privileges =
        serde_json::to_value(crate::roles::privileges(c, "it_role_app").await.unwrap()).unwrap();
    let table = privileges["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "it_role_t")
        .unwrap()
        .clone();
    assert_eq!(table["kind"], "table");
    assert_eq!(table["granted"], serde_json::json!([]));
    assert_eq!(table["effective"], serde_json::json!(["SELECT"]));

    let changes = serde_json::from_value(serde_json::json!({
        "name": "it_role_app",
        "rename": "it_role_web",
        "attributes": { "createDb": true, "validUntil": "2099-01-01" },
        "password": "secret",
        "comment": "web app",
        "revokeRoles": ["it_role_readers"],
        "privileges": [
            { "target": { "kind": "table", "schema": "public", "name": "it_role_t" }, "privilege": "INSERT", "grant": true },
        ],
    }))
    .unwrap();
    let statements = crate::roles::plan(&changes, |p| {
        postgres_protocol::password::scram_sha_256(p.as_bytes())
    })
    .unwrap();
    crate::roles::apply(c, &statements).await.unwrap();

    let details =
        serde_json::to_value(crate::roles::details(c, "it_role_web").await.unwrap()).unwrap();
    assert_eq!(details["createDb"], true);
    assert_eq!(details["comment"], "web app");
    assert!(
        details["validUntil"]
            .as_str()
            .unwrap()
            .starts_with("2099-01-01")
    );
    assert_eq!(details["memberOf"], serde_json::json!([]));
    let stored: String = c
        .query_one(
            "SELECT rolpassword FROM pg_authid WHERE rolname = 'it_role_web'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(stored.starts_with("SCRAM-SHA-256$"));

    // A failing statement rolls back the earlier ones.
    let failing = serde_json::from_value(serde_json::json!({
        "name": "it_role_web",
        "attributes": { "createDb": false },
        "grantRoles": ["it_role_missing"],
    }))
    .unwrap();
    let statements = crate::roles::plan(&failing, |p| p.into()).unwrap();
    assert!(crate::roles::apply(c, &statements).await.is_err());
    let details =
        serde_json::to_value(crate::roles::details(c, "it_role_web").await.unwrap()).unwrap();
    assert_eq!(details["createDb"], true);

    c.batch_execute("DROP TABLE it_role_t; DROP ROLE it_role_web, it_role_readers;")
        .await
        .unwrap();
}
