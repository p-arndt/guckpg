//! Database inspection, structured reads, and user-approved write proposals.

use serde_json::{Value, json};
use tokio_postgres::types::Type;

use crate::error::{AppError, AppResult};
use crate::{catalog, session::Manager, sql};

const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_ITEMS: usize = 100;
const MAX_CELL_BYTES: usize = 1024;
const SAMPLE_ROWS: usize = 20;
const SAMPLE_COLUMNS: usize = 64;

pub fn definitions(allow_data: bool) -> Vec<Value> {
    let mut tools = vec![
        definition("list_schemas", "List user database schemas.", &[]),
        definition(
            "list_tables",
            "List relations in a schema and their kinds.",
            &["schema"],
        ),
        definition(
            "describe_table",
            "Describe columns and outgoing foreign keys without reading table data.",
            &["schema", "table"],
        ),
    ];
    if allow_data {
        tools.push(definition("sample_rows", "Read at most 20 rows and 64 columns from an ordinary table. Values and output may be truncated. Requires explicit data sharing permission.", &["schema", "table"]));
        tools.push(super::query::definition());
        tools.push(json!({"type":"function","function":{"name":"query_sql","description":"Run one PostgreSQL read query using SQL, including CTEs, views, expressions and aggregates. Results are limited to 100 rows and may be truncated. Use only for reading; writes require propose_write.","parameters":{"type":"object","properties":{"sql":{"type":"string","minLength":1,"maxLength":16384}},"required":["sql"],"additionalProperties":false}}}));
    }
    tools
}

pub fn write_definition() -> Value {
    json!({"type":"function", "function": {"name":"propose_write", "description":"Propose a single SQL statement for explicit user review and approval. Do not include multiple statements. Execution occurs only after approval.", "parameters":{"type":"object", "properties":{"sql":{"type":"string", "minLength":1, "maxLength":16384}}, "required":["sql"], "additionalProperties":false}}})
}

pub fn write_sql(arguments: &Value) -> AppResult<&str> {
    let object = arguments
        .as_object()
        .ok_or_else(|| AppError::msg("Invalid write request"))?;
    let sql = object
        .get("sql")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::msg("Invalid write request"))?;
    if object.len() != 1 || sql.trim().is_empty() || sql.len() > 16_384 || sql.contains('\0') {
        return Err(AppError::msg("Invalid write request"));
    }
    Ok(sql)
}

fn definition(name: &str, description: &str, fields: &[&str]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|field| {
            (
                (*field).to_string(),
                json!({"type": "string", "minLength": 1, "maxLength": 63,
                    "description": "Exact PostgreSQL identifier, at most 63 UTF-8 bytes."}),
            )
        })
        .collect();
    json!({"type": "function", "function": {
        "name": name, "description": description,
        "parameters": {"type": "object", "properties": properties,
            "required": fields, "additionalProperties": false}
    }})
}

fn validate<'a>(name: &str, args: &'a Value, allow_data: bool) -> AppResult<Vec<&'a str>> {
    let fields: &[&str] = match name {
        "list_schemas" => &[],
        "list_tables" => &["schema"],
        "describe_table" => &["schema", "table"],
        "sample_rows" if allow_data => &["schema", "table"],
        "sample_rows" => return Err(AppError::msg("table data sharing is disabled")),
        _ => return Err(AppError::msg("unknown database assistant tool")),
    };
    let object = args
        .as_object()
        .ok_or_else(|| AppError::msg("tool arguments must be an object"))?;
    if object.len() != fields.len() || object.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err(AppError::msg(
            "tool arguments contain missing or unknown fields",
        ));
    }
    fields
        .iter()
        .map(|field| {
            let value = object
                .get(*field)
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::msg("tool identifiers must be strings"))?;
            if value.is_empty() || value.len() > 63 || value.contains('\0') {
                return Err(AppError::msg(
                    "tool identifiers must contain 1 to 63 bytes without NUL",
                ));
            }
            Ok(value)
        })
        .collect()
}

pub async fn execute(
    manager: &Manager,
    connection_id: &str,
    database: &str,
    name: &str,
    args: &Value,
    allow_data: bool,
) -> AppResult<Value> {
    if name == "query_sql" {
        if !allow_data {
            return Err(AppError::msg("AI query: data access is disabled"));
        }
        let sql = write_sql(args).map_err(|_| {
            AppError::msg("AI query: expected one non-empty sql string of at most 16384 bytes")
        })?;
        let conn = manager.ephemeral(connection_id, database).await?;
        return super::query::execute_sql(&conn, sql).await;
    }
    if name == "query_rows" {
        if !allow_data {
            return Err(AppError::msg("AI query: data access is disabled"));
        }
        let query = super::query::Query::parse(args)?;
        let conn = manager.ephemeral(connection_id, database).await?;
        return super::query::execute(&conn, &query).await;
    }
    // Validate permission and every model-controlled argument before connecting.
    let fields = validate(name, args, allow_data)?;
    let conn = manager.ephemeral(connection_id, database).await?;
    inspect(&conn, name, &fields).await
}

async fn inspect(conn: &crate::pg::PgConn, name: &str, fields: &[&str]) -> AppResult<Value> {
    conn.client.batch_execute(
        "BEGIN READ ONLY; SET LOCAL statement_timeout = '5s'; SET LOCAL lock_timeout = '2s'; SET LOCAL search_path = pg_catalog"
    ).await?;
    let result: AppResult<Value> = async {
        match name {
            "list_schemas" => Ok(json!({"schemas": catalog::schemas(&conn.client, false).await?})),
            "list_tables" => {
                Ok(json!({"tables": catalog::relations(&conn.client, fields[0]).await?}))
            }
            "describe_table" => {
                let columns = catalog::columns(&conn.client, fields[0], fields[1]).await?;
                let keys = conn
                    .client
                    .query_typed(
                        "SELECT con.conname, dn.nspname, dst.relname, pg_get_constraintdef(con.oid)
                     FROM pg_constraint con
                     JOIN pg_class src ON src.oid = con.conrelid
                     JOIN pg_namespace sn ON sn.oid = src.relnamespace
                     JOIN pg_class dst ON dst.oid = con.confrelid
                     JOIN pg_namespace dn ON dn.oid = dst.relnamespace
                     WHERE con.contype = 'f' AND sn.nspname = $1 AND src.relname = $2
                     ORDER BY con.conname LIMIT 101",
                        &[(&fields[0], Type::NAME), (&fields[1], Type::NAME)],
                    )
                    .await?;
                let keys: Vec<Value> = keys.iter().map(|row| json!({
                    "name": row.get::<_, String>(0), "targetSchema": row.get::<_, String>(1),
                    "targetTable": row.get::<_, String>(2), "definition": row.get::<_, String>(3)
                })).collect();
                Ok(json!({"columns": columns, "foreignKeys": keys}))
            }
            "sample_rows" => {
                let checked = super::query::table_columns(conn, fields[0], fields[1]).await?;
                let columns = catalog::columns(&conn.client, fields[0], fields[1]).await?;
                // One enum or array column should not hide the rest of the table; only
                // built-in scalar types are rendered, the others are named instead.
                let (columns, omitted): (Vec<_>, Vec<_>) = columns
                    .iter()
                    .partition(|column| checked.contains_key(&column.name));
                let omitted: Vec<&str> = omitted.iter().map(|c| c.name.as_str()).collect();

                if columns.is_empty() {
                    return Ok(json!({"columns": [], "rows": [], "limit": SAMPLE_ROWS,
                        "omittedColumns": omitted}));
                }
                let names: Vec<&str> = columns
                    .iter()
                    .take(SAMPLE_COLUMNS)
                    .map(|column| column.name.as_str())
                    .collect();
                let output = conn
                    .run(&sample_sql(fields[0], fields[1], &names), SAMPLE_ROWS)
                    .await?;
                let set = output
                    .results
                    .into_iter()
                    .next()
                    .ok_or_else(|| AppError::msg("sample returned no result"))?;
                Ok(
                    json!({"columns": set.columns, "rows": set.rows, "limit": SAMPLE_ROWS,
                    "columnsTruncated": columns.len() > SAMPLE_COLUMNS,
                    "omittedColumns": omitted,
                    "valuesMayBeTruncated": true, "order": "unspecified"}),
                )
            }
            _ => unreachable!("validated tool name"),
        }
    }
    .await;
    // This connection is never cached. Dropping it also rolls back on cancellation.
    let rollback = conn.client.batch_execute("ROLLBACK").await;
    let value = result?;
    rollback?;
    Ok(bounded_output(value))
}

fn sample_sql(schema: &str, table: &str, columns: &[&str]) -> String {
    let projection = columns
        .iter()
        .take(SAMPLE_COLUMNS)
        .map(|name| {
            let ident = sql::quote_ident(name);
            format!("pg_catalog.left({ident}::pg_catalog.text, {MAX_CELL_BYTES}) AS {ident}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT {projection} FROM ONLY {} LIMIT {SAMPLE_ROWS}",
        sql::qualified(schema, table)
    )
}

fn clamp(value: &mut Value) -> bool {
    match value {
        Value::String(text) if text.len() > MAX_CELL_BYTES => {
            let mut end = MAX_CELL_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            true
        }
        Value::Array(items) => {
            let mut truncated = items.len() > MAX_ITEMS;
            items.truncate(MAX_ITEMS);
            for item in items {
                truncated |= clamp(item);
            }
            truncated
        }
        Value::Object(object) => {
            let mut truncated = false;
            for (key, item) in object.iter_mut() {
                // Generated SQL is independently bounded and must remain reviewable in the tool trace.
                if key != "sql" {
                    truncated |= clamp(item);
                }
            }
            truncated
        }
        _ => false,
    }
}

fn remove_largest_array_item(value: &mut Value) -> bool {
    let Value::Object(object) = value else {
        return false;
    };
    let key = object
        .iter()
        .filter(|(_, item)| item.as_array().is_some_and(|items| !items.is_empty()))
        .max_by_key(|(_, item)| item.to_string().len())
        .map(|(key, _)| key.clone());
    if let Some(key) = key {
        object.get_mut(&key).unwrap().as_array_mut().unwrap().pop();
        true
    } else {
        false
    }
}

pub(super) fn bounded_output(mut value: Value) -> Value {
    let mut truncated = clamp(&mut value);
    // Reserve room for the truncation marker and envelope before fitting arrays.
    while value.to_string().len() > MAX_OUTPUT_BYTES - 64 {
        if !remove_largest_array_item(&mut value) {
            value = json!({});
            truncated = true;
            break;
        }
        truncated = true;
    }
    json!({"data": value, "truncated": truncated})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn query_permission_and_plan_are_checked_before_connecting() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Manager::new(
            crate::config::ConnectionStore::new(dir.path()),
            crate::ssh::KnownHosts::new(dir.path()),
        );
        let denied = execute(
            &manager,
            "missing",
            "missing",
            "query_rows",
            &json!({}),
            false,
        )
        .await
        .unwrap_err();
        assert!(denied.to_string().contains("data access is disabled"));
        let denied_sql = execute(
            &manager,
            "missing",
            "missing",
            "query_sql",
            &json!({"sql":"SELECT 1"}),
            false,
        )
        .await
        .unwrap_err();
        assert!(denied_sql.to_string().contains("data access is disabled"));
        let invalid_sql = execute(
            &manager,
            "missing",
            "missing",
            "query_sql",
            &json!({"sql":"SELECT 1", "extra":true}),
            true,
        )
        .await
        .unwrap_err();
        assert!(
            invalid_sql
                .to_string()
                .contains("expected one non-empty sql")
        );
        let invalid = execute(
            &manager,
            "missing",
            "missing",
            "query_rows",
            &json!({"sql":"DELETE FROM users"}),
            true,
        )
        .await
        .unwrap_err();
        assert!(invalid.to_string().contains("invalid query plan"));
    }

    #[tokio::test]
    #[ignore = "requires disposable localhost Postgres with GUCKPG_AI_TEST_PORT set"]
    async fn inspects_real_postgres_without_modifying_data() {
        use crate::config::{ConnectionConfig, SslMode};
        use crate::pg::{PgConn, Transport};

        let port = std::env::var("GUCKPG_AI_TEST_PORT")
            .expect("set the disposable local Postgres port")
            .parse()
            .unwrap();
        let config = ConnectionConfig {
            id: "ai-test".into(),
            name: "AI test".into(),
            host: "127.0.0.1".into(),
            port,
            user: "postgres".into(),
            database: "guckpg_ai_test".into(),
            ssl_mode: SslMode::Disable,
            read_only: false,
            color: None,
            ssh: None,
            folder: None,
        };
        let conn = PgConn::connect(&config, &config.database, None, Transport::Direct)
            .await
            .unwrap();
        conn.client.batch_execute(
            "CREATE SCHEMA ai_test;
             CREATE TABLE ai_test.parents (id int PRIMARY KEY);
             CREATE TABLE ai_test.children (id int PRIMARY KEY, parent_id int REFERENCES ai_test.parents, note text);
             INSERT INTO ai_test.parents VALUES (1);
             INSERT INTO ai_test.children SELECT n, 1, repeat('🦀', 2000) FROM generate_series(1, 30) n;
             CREATE VIEW ai_test.child_view AS SELECT * FROM ai_test.children;"
        ).await.unwrap();
        let schemas = inspect(&conn, "list_schemas", &[]).await.unwrap();
        assert!(
            schemas["data"]["schemas"]
                .as_array()
                .unwrap()
                .contains(&json!("ai_test"))
        );
        let tables = inspect(&conn, "list_tables", &["ai_test"]).await.unwrap();
        assert_eq!(tables["data"]["tables"].as_array().unwrap().len(), 3);
        let description = inspect(&conn, "describe_table", &["ai_test", "children"])
            .await
            .unwrap();
        assert_eq!(description["data"]["columns"].as_array().unwrap().len(), 3);
        assert_eq!(
            description["data"]["foreignKeys"][0]["targetTable"],
            "parents"
        );
        let sample = inspect(&conn, "sample_rows", &["ai_test", "children"])
            .await
            .unwrap();
        assert_eq!(sample["data"]["rows"].as_array().unwrap().len(), 20);
        assert_eq!(sample["truncated"], true);
        assert!(sample["data"]["rows"][0][2].as_str().unwrap().len() <= MAX_CELL_BYTES);
        assert!(
            inspect(&conn, "sample_rows", &["ai_test", "child_view"])
                .await
                .is_err()
        );
        assert!(
            inspect(
                &conn,
                "sample_rows",
                &["ai_test", "children\"; DELETE FROM ai_test.children; --"]
            )
            .await
            .is_err()
        );
        let count: i64 = conn
            .client
            .query_one("SELECT count(*) FROM ai_test.children", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 30);
        let read_only: String = conn
            .client
            .query_one("SHOW transaction_read_only", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(read_only, "off");
    }

    #[test]
    fn write_proposals_require_one_bounded_sql_field() {
        assert_eq!(
            write_sql(&json!({"sql":"UPDATE t SET x = 1"})).unwrap(),
            "UPDATE t SET x = 1"
        );
        for value in [
            json!({}),
            json!({"sql":" "}),
            json!({"sql":"a\0b"}),
            json!({"sql":"UPDATE t SET x=1", "extra":true}),
            json!({"sql":"x".repeat(16_385)}),
        ] {
            assert!(write_sql(&value).is_err());
        }
    }

    #[test]
    fn descriptors_and_validation_agree() {
        for allow_data in [false, true] {
            let tools = definitions(allow_data);
            assert_eq!(tools.len(), if allow_data { 6 } else { 3 });
            for tool in tools {
                assert_eq!(tool["type"], "function");
                let function = &tool["function"];
                assert_eq!(function["parameters"]["additionalProperties"], false);
                if function["name"] == "query_sql" {
                    assert!(write_sql(&json!({"sql":"SELECT 1"})).is_ok());
                    continue;
                }
                if function["name"] == "query_rows" {
                    assert_eq!(
                        function["parameters"]["properties"]["limit"]["maximum"],
                        100
                    );
                    continue;
                }
                let args: serde_json::Map<String, Value> = function["parameters"]["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|field| (field.as_str().unwrap().to_string(), json!("example")))
                    .collect();
                assert!(
                    validate(
                        function["name"].as_str().unwrap(),
                        &Value::Object(args),
                        allow_data
                    )
                    .is_ok()
                );
            }
        }
    }

    #[test]
    fn rejects_untrusted_arguments_and_disabled_data() {
        assert!(validate("execute_sql", &json!({"sql": "SELECT 1"}), true).is_err());
        assert!(
            validate(
                "sample_rows",
                &json!({"schema": "public", "table": "users"}),
                false
            )
            .is_err()
        );
        assert!(validate("list_schemas", &json!({"sql": "DROP TABLE users"}), true).is_err());
        assert!(validate("list_tables", &json!({}), true).is_err());
        for invalid in [
            json!(null),
            json!({"schema": 1}),
            json!({"schema": ""}),
            json!({"schema": "a\0b"}),
            json!({"schema": "é".repeat(32)}),
        ] {
            assert!(validate("list_tables", &invalid, true).is_err());
        }
    }

    #[test]
    fn sample_identifiers_are_quoted_and_limits_are_fixed() {
        let query = sample_sql("s\"; --", "t\"; DROP TABLE x; --", &["c\"; --"]);
        assert_eq!(
            query,
            "SELECT pg_catalog.left(\"c\"\"; --\"::pg_catalog.text, 1024) AS \"c\"\"; --\" FROM ONLY \"s\"\"; --\".\"t\"\"; DROP TABLE x; --\" LIMIT 20"
        );
        let names = vec!["x"; 100];
        assert_eq!(
            sample_sql("s", "t", &names).matches(" AS ").count(),
            SAMPLE_COLUMNS
        );
    }

    #[test]
    fn bounded_output_preserves_utf8_and_serialized_byte_limit() {
        let value = json!({"rows": vec![vec!["🦀\"\\".repeat(2000); 64]; 20]});
        let output = bounded_output(value);
        assert!(output.to_string().len() <= MAX_OUTPUT_BYTES);
        assert_eq!(output["truncated"], true);
        for row in output["data"]["rows"].as_array().unwrap() {
            for cell in row.as_array().unwrap() {
                assert!(cell.as_str().unwrap().len() <= MAX_CELL_BYTES);
            }
        }
        assert_eq!(
            bounded_output(json!({"schemas": ["public"]}))["truncated"],
            false
        );
        assert_eq!(
            bounded_output(json!({"tables": vec!["x"; 101]}))["data"]["tables"]
                .as_array()
                .unwrap()
                .len(),
            100
        );
    }
}
