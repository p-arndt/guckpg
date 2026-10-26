//! Database inspection, structured reads, and user-approved write proposals.

use std::str::FromStr;

use serde_json::{Value, json};
use tokio_postgres::types::Type;

use crate::error::{AppError, AppResult};
use crate::{catalog, session::Manager, sql};

const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_ITEMS: usize = 100;
const MAX_CELL_BYTES: usize = 1024;
const SAMPLE_ROWS: usize = 20;
const SAMPLE_COLUMNS: usize = 64;
const MAX_SQL_BYTES: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    ListSchemas,
    ListTables,
    DescribeTable,
    SampleRows,
    QueryRows,
    QuerySql,
    ProposeWrite,
}

impl Tool {
    const ALL: [Tool; 7] = [
        Tool::ListSchemas,
        Tool::ListTables,
        Tool::DescribeTable,
        Tool::SampleRows,
        Tool::QueryRows,
        Tool::QuerySql,
        Tool::ProposeWrite,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::ListSchemas => "list_schemas",
            Tool::ListTables => "list_tables",
            Tool::DescribeTable => "describe_table",
            Tool::SampleRows => "sample_rows",
            Tool::QueryRows => "query_rows",
            Tool::QuerySql => "query_sql",
            Tool::ProposeWrite => "propose_write",
        }
    }

    /// Model-written SQL is reviewed by the user even when reads run unattended: a
    /// read-only transaction does not stop every function with side effects.
    pub fn requires_approval(self) -> bool {
        matches!(self, Tool::QuerySql | Tool::ProposeWrite)
    }
}

impl FromStr for Tool {
    type Err = AppError;

    fn from_str(name: &str) -> AppResult<Self> {
        Tool::ALL
            .into_iter()
            .find(|tool| tool.name() == name)
            .ok_or_else(|| AppError::msg("unknown database assistant tool"))
    }
}

/// Errors from model-controlled tool calls. `Rejected` explains a refused request to the
/// model so it can correct itself; `Failed` is withheld from the model because database
/// errors may quote row values or connection details.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("AI query: {0}")]
    Rejected(String),
    #[error(transparent)]
    Failed(#[from] AppError),
}

impl From<tokio_postgres::Error> for ToolError {
    fn from(error: tokio_postgres::Error) -> Self {
        ToolError::Failed(error.into())
    }
}

pub(super) fn rejected(message: impl Into<String>) -> ToolError {
    ToolError::Rejected(message.into())
}

pub fn definitions(allow_data: bool) -> Vec<Value> {
    let mut tools = vec![
        definition(Tool::ListSchemas, "List user database schemas."),
        definition(
            Tool::ListTables,
            "List relations in a schema and their kinds.",
        ),
        definition(
            Tool::DescribeTable,
            "Describe columns and outgoing foreign keys without reading table data.",
        ),
    ];
    if allow_data {
        tools.push(definition(Tool::SampleRows, "Read at most 20 rows and 64 columns from an ordinary table. Values and output may be truncated. Requires explicit data sharing permission."));
        tools.push(super::query::definition());
        tools.push(sql_definition(Tool::QuerySql, "Run one PostgreSQL read query using SQL, including CTEs, views, expressions and aggregates. The user must approve every call, so prefer query_rows when it suffices. Functions with side effects, file or server access, sleeps, locks and setting changes are blocked. Results are limited to 100 rows and may be truncated. Use only for reading; writes require propose_write."));
    }
    tools
}

pub fn write_definition() -> Value {
    sql_definition(
        Tool::ProposeWrite,
        "Propose a single SQL statement for explicit user review and approval. Do not include multiple statements. Execution occurs only after approval.",
    )
}

fn sql_definition(tool: Tool, description: &str) -> Value {
    json!({"type":"function", "function": {"name": tool.name(), "description": description, "parameters":{"type":"object", "properties":{"sql":{"type":"string", "minLength":1, "maxLength":MAX_SQL_BYTES}}, "required":["sql"], "additionalProperties":false}}})
}

/// The checked SQL of a `query_sql` call. It is also checked before asking for approval,
/// so the user never reviews a request that would be refused anyway.
pub fn read_sql(arguments: &Value, allow_data: bool) -> Result<&str, ToolError> {
    if !allow_data {
        return Err(rejected("data access is disabled"));
    }
    let sql = write_sql(arguments).map_err(|_| {
        rejected(format!(
            "expected one non-empty sql string of at most {MAX_SQL_BYTES} bytes"
        ))
    })?;
    if let Some(name) = super::query::blocked_function(sql) {
        return Err(rejected(format!(
            "{name} is not allowed in query_sql; use plain reads without side effects"
        )));
    }
    Ok(sql)
}

pub fn write_sql(arguments: &Value) -> AppResult<&str> {
    let object = arguments
        .as_object()
        .ok_or_else(|| AppError::msg("Invalid write request"))?;
    let sql = object
        .get("sql")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::msg("Invalid write request"))?;
    if object.len() != 1 || sql.trim().is_empty() || sql.len() > MAX_SQL_BYTES || sql.contains('\0')
    {
        return Err(AppError::msg("Invalid write request"));
    }
    Ok(sql)
}

fn definition(tool: Tool, description: &str) -> Value {
    let fields = inspection_fields(tool).unwrap_or_default();
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
        "name": tool.name(), "description": description,
        "parameters": {"type": "object", "properties": properties,
            "required": fields, "additionalProperties": false}
    }})
}

/// The identifier arguments of the catalog inspection tools.
fn inspection_fields(tool: Tool) -> Option<&'static [&'static str]> {
    match tool {
        Tool::ListSchemas => Some(&[]),
        Tool::ListTables => Some(&["schema"]),
        Tool::DescribeTable | Tool::SampleRows => Some(&["schema", "table"]),
        Tool::QueryRows | Tool::QuerySql | Tool::ProposeWrite => None,
    }
}

fn validate(tool: Tool, args: &Value, allow_data: bool) -> AppResult<Vec<&str>> {
    if tool == Tool::SampleRows && !allow_data {
        return Err(AppError::msg("table data sharing is disabled"));
    }
    let fields =
        inspection_fields(tool).ok_or_else(|| AppError::msg("unknown database assistant tool"))?;
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
            sql::checked_ident(value).map_err(|_| {
                AppError::msg("tool identifiers must contain 1 to 63 bytes without NUL")
            })
        })
        .collect()
}

pub async fn execute(
    manager: &Manager,
    connection_id: &str,
    database: &str,
    tool: Tool,
    args: &Value,
    allow_data: bool,
) -> Result<Value, ToolError> {
    // Validate permission and every model-controlled argument before connecting.
    match tool {
        Tool::QuerySql => {
            let sql = read_sql(args, allow_data)?;
            let conn = manager.ephemeral(connection_id, database).await?;
            super::query::execute_sql(&conn, sql).await
        }
        Tool::QueryRows => {
            if !allow_data {
                return Err(rejected("data access is disabled"));
            }
            let query = super::query::Query::parse(args)?;
            let conn = manager.ephemeral(connection_id, database).await?;
            super::query::execute(&conn, &query).await
        }
        _ => {
            let fields = validate(tool, args, allow_data)?;
            let conn = manager.ephemeral(connection_id, database).await?;
            inspect(&conn, tool, &fields).await
        }
    }
}

async fn inspect(
    conn: &crate::pg::PgConn,
    tool: Tool,
    fields: &[&str],
) -> Result<Value, ToolError> {
    conn.client.batch_execute(
        "BEGIN READ ONLY; SET LOCAL statement_timeout = '5s'; SET LOCAL lock_timeout = '2s'; SET LOCAL search_path = pg_catalog"
    ).await?;
    let result: Result<Value, ToolError> = async {
        match tool {
            Tool::ListSchemas => {
                Ok(json!({"schemas": catalog::schemas(&conn.client, false).await?}))
            }
            Tool::ListTables => {
                Ok(json!({"tables": catalog::relations(&conn.client, fields[0]).await?}))
            }
            Tool::DescribeTable => {
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
            Tool::SampleRows => {
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
            Tool::QueryRows | Tool::QuerySql | Tool::ProposeWrite => {
                Err(AppError::msg("unknown database assistant tool").into())
            }
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
            Tool::QueryRows,
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
            Tool::QuerySql,
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
            Tool::QuerySql,
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
            Tool::QueryRows,
            &json!({"sql":"DELETE FROM users"}),
            true,
        )
        .await
        .unwrap_err();
        assert!(invalid.to_string().contains("invalid query plan"));
        let blocked = execute(
            &manager,
            "missing",
            "missing",
            Tool::QuerySql,
            &json!({"sql":"SELECT pg_sleep(60)"}),
            true,
        )
        .await
        .unwrap_err();
        assert!(matches!(blocked, ToolError::Rejected(_)));
    }

    #[tokio::test]
    async fn inspects_real_postgres_without_modifying_data() {
        use crate::pg::{PgConn, Transport};

        let Some((config, password)) = crate::testutil::test_db("ai-test") else {
            return;
        };
        let conn = PgConn::connect(
            &config,
            &config.database,
            Some(&password),
            Transport::Direct,
        )
        .await
        .unwrap();
        let schema = format!("ai_test_{}", uuid::Uuid::new_v4().simple());
        let s = schema.as_str();
        conn.client.batch_execute(&format!(
            "CREATE SCHEMA {s};
             CREATE TABLE {s}.parents (id int PRIMARY KEY);
             CREATE TABLE {s}.children (id int PRIMARY KEY, parent_id int REFERENCES {s}.parents, note text);
             INSERT INTO {s}.parents VALUES (1);
             INSERT INTO {s}.children SELECT n, 1, repeat('🦀', 2000) FROM generate_series(1, 30) n;
             CREATE VIEW {s}.child_view AS SELECT * FROM {s}.children;"
        )).await.unwrap();
        let schemas = inspect(&conn, Tool::ListSchemas, &[]).await.unwrap();
        assert!(
            schemas["data"]["schemas"]
                .as_array()
                .unwrap()
                .contains(&json!(s))
        );
        let tables = inspect(&conn, Tool::ListTables, &[s]).await.unwrap();
        assert_eq!(tables["data"]["tables"].as_array().unwrap().len(), 3);
        let description = inspect(&conn, Tool::DescribeTable, &[s, "children"])
            .await
            .unwrap();
        assert_eq!(description["data"]["columns"].as_array().unwrap().len(), 3);
        assert_eq!(
            description["data"]["foreignKeys"][0]["targetTable"],
            "parents"
        );
        let sample = inspect(&conn, Tool::SampleRows, &[s, "children"])
            .await
            .unwrap();
        assert_eq!(sample["data"]["rows"].as_array().unwrap().len(), 20);
        assert_eq!(sample["truncated"], true);
        assert!(sample["data"]["rows"][0][2].as_str().unwrap().len() <= MAX_CELL_BYTES);
        assert!(
            inspect(&conn, Tool::SampleRows, &[s, "child_view"])
                .await
                .is_err()
        );
        assert!(
            inspect(
                &conn,
                Tool::SampleRows,
                &[s, &format!("children\"; DELETE FROM {s}.children; --")]
            )
            .await
            .is_err()
        );
        let count: i64 = conn
            .client
            .query_one(&format!("SELECT count(*) FROM {s}.children"), &[])
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
        conn.client
            .batch_execute(&format!("DROP SCHEMA {s} CASCADE"))
            .await
            .unwrap();
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
                let tool: Tool = function["name"].as_str().unwrap().parse().unwrap();
                if tool == Tool::QuerySql {
                    assert_eq!(
                        function["parameters"]["properties"]["sql"]["maxLength"],
                        MAX_SQL_BYTES
                    );
                    assert!(read_sql(&json!({"sql":"SELECT 1"}), allow_data).is_ok());
                    continue;
                }
                if tool == Tool::QueryRows {
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
                assert!(validate(tool, &Value::Object(args), allow_data).is_ok());
            }
        }
        assert_eq!(write_definition()["function"]["name"], "propose_write");
    }

    #[test]
    fn tool_names_round_trip_and_unknown_names_are_rejected() {
        for tool in Tool::ALL {
            assert_eq!(tool.name().parse::<Tool>().unwrap(), tool);
        }
        assert!("execute_sql".parse::<Tool>().is_err());
        assert!("QUERY_SQL".parse::<Tool>().is_err());
        assert!(Tool::QuerySql.requires_approval());
        assert!(Tool::ProposeWrite.requires_approval());
        assert!(!Tool::QueryRows.requires_approval());
        assert!(!Tool::SampleRows.requires_approval());
    }

    #[test]
    fn read_sql_rejects_disabled_data_malformed_and_dangerous_requests() {
        assert_eq!(
            read_sql(&json!({"sql": "SELECT count(*) FROM orders"}), true).unwrap(),
            "SELECT count(*) FROM orders"
        );
        for (args, allow_data, reason) in [
            (json!({"sql": "SELECT 1"}), false, "data access is disabled"),
            (json!({"sql": " "}), true, "expected one non-empty sql"),
            (
                json!({"sql": "SELECT pg_read_file('/etc/passwd')"}),
                true,
                "pg_read_file is not allowed",
            ),
            (
                json!({"sql": "SELECT pg_terminate_backend(pid) FROM pg_stat_activity"}),
                true,
                "pg_terminate_backend is not allowed",
            ),
        ] {
            let error = read_sql(&args, allow_data).unwrap_err();
            assert!(matches!(error, ToolError::Rejected(_)));
            assert!(error.to_string().contains(reason), "{error}");
        }
    }

    #[test]
    fn rejects_untrusted_arguments_and_disabled_data() {
        assert!(validate(Tool::QuerySql, &json!({"sql": "SELECT 1"}), true).is_err());
        assert!(validate(Tool::ProposeWrite, &json!({"sql": "SELECT 1"}), true).is_err());
        assert!(
            validate(
                Tool::SampleRows,
                &json!({"schema": "public", "table": "users"}),
                false
            )
            .is_err()
        );
        assert!(validate(Tool::ListSchemas, &json!({"sql": "DROP TABLE users"}), true).is_err());
        assert!(validate(Tool::ListTables, &json!({}), true).is_err());
        for invalid in [
            json!(null),
            json!({"schema": 1}),
            json!({"schema": ""}),
            json!({"schema": "a\0b"}),
            json!({"schema": "é".repeat(32)}),
        ] {
            assert!(validate(Tool::ListTables, &invalid, true).is_err());
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
