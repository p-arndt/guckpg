//! The model supplies a query plan, never SQL expressions or function names.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio_postgres::types::{ToSql, Type};

use crate::error::{AppError, AppResult};
use crate::pg::PgConn;
use crate::sql::{qualified, quote_ident};

const MAX_ROWS: usize = 100;
const MAX_COLUMNS: usize = 32;
const MAX_JOINS: usize = 3;
const MAX_FILTERS: usize = 16;
const CELL_CHARS: usize = 1025;

fn rejected(message: &str) -> AppError {
    AppError::msg(format!("AI query: {message}"))
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ColumnRef {
    table: usize,
    column: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Join {
    schema: String,
    table: String,
    kind: JoinKind,
    left: ColumnRef,
    right_column: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum JoinKind {
    Inner,
    Left,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    IsNull,
    IsNotNull,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    column: ColumnRef,
    op: Operator,
    value: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum AggregateKind {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Aggregate {
    function: AggregateKind,
    column: Option<ColumnRef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Direction {
    Asc,
    Desc,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Order {
    field: usize,
    direction: Direction,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Query {
    schema: String,
    table: String,
    #[serde(default)]
    columns: Vec<ColumnRef>,
    #[serde(default)]
    joins: Vec<Join>,
    #[serde(default)]
    filters: Vec<Filter>,
    #[serde(default)]
    aggregates: Vec<Aggregate>,
    #[serde(default)]
    group_by: Vec<ColumnRef>,
    #[serde(default)]
    order_by: Vec<Order>,
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn object(properties: Value) -> Value {
    let required: Vec<_> = properties.as_object().unwrap().keys().cloned().collect();
    json!({"type":"object", "properties":properties, "required":required, "additionalProperties":false})
}

pub(super) fn definition() -> Value {
    let identifier = json!({"type":"string", "minLength":1, "maxLength":63});
    let column =
        object(json!({"table":{"type":"integer","minimum":0,"maximum":3}, "column":identifier}));
    let nullable_column = json!({"anyOf":[column, {"type":"null"}]});
    let parameters = object(json!({
        "schema":identifier, "table":identifier,
        "columns":{"type":"array","maxItems":32,"items":column},
        "joins":{"type":"array","maxItems":3,"items":object(json!({
            "schema":identifier,"table":identifier,"kind":{"type":"string","enum":["inner","left"]},
            "left":column,"rightColumn":identifier
        }))},
        "filters":{"type":"array","maxItems":16,"items":object(json!({
            "column":column,"op":{"type":"string","enum":["eq","ne","gt","gte","lt","lte","like","is_null","is_not_null"]},
            "value":{"type":["string","null"],"maxLength":1024}
        }))},
        "aggregates":{"type":"array","maxItems":32,"items":object(json!({
            "function":{"type":"string","enum":["count","sum","avg","min","max"]},"column":nullable_column
        }))},
        "groupBy":{"type":"array","maxItems":32,"items":column},
        "orderBy":{"type":"array","maxItems":32,"items":object(json!({
            "field":{"type":"integer","minimum":0,"maximum":31},"direction":{"type":"string","enum":["asc","desc"]}
        }))},
        "limit":{"type":"integer","minimum":1,"maximum":100},
        "offset":{"type":"integer","minimum":0,"maximum":10000}
    }));
    json!({"type":"function","function":{
        "name":"query_rows",
        "description":"Run a bounded read-only SELECT built from this plan. Inspect columns first. Table references: 0 = base table, 1..3 = joins in order. Join left must refer to an earlier table. Filters combine with AND; values are strings cast to the column type (or null for is_null/is_not_null). Columns are output first, then aggregates; orderBy.field is their zero-based output position. For grouped queries list every selected column in groupBy. count with column:null counts rows. Use empty arrays for unused features. Supports ordinary tables and built-in scalar columns; views, RLS, custom access methods and expression/partial indexes are blocked. Returns at most 100 rows; use aggregates for totals. No raw SQL, writes, arbitrary expressions or functions.",
        "parameters":parameters
    }})
}

fn identifier(value: &str) -> AppResult<()> {
    if value.is_empty() || value.len() > 63 || value.contains('\0') {
        return Err(rejected(
            "identifiers must contain 1–63 UTF-8 bytes without NUL",
        ));
    }
    Ok(())
}

impl Query {
    pub(super) fn parse(value: &Value) -> AppResult<Self> {
        if value.to_string().len() > 32 * 1024 {
            return Err(rejected("query plan exceeds 32 KiB"));
        }
        let query: Self = serde_json::from_value(value.clone())
            .map_err(|_| rejected("invalid query plan or unknown fields"))?;
        query.validate()?;
        Ok(query)
    }

    fn tables(&self) -> Vec<(&str, &str)> {
        std::iter::once((self.schema.as_str(), self.table.as_str()))
            .chain(
                self.joins
                    .iter()
                    .map(|j| (j.schema.as_str(), j.table.as_str())),
            )
            .collect()
    }

    fn validate(&self) -> AppResult<()> {
        let fields = self.columns.len() + self.aggregates.len();
        if fields == 0
            || fields > MAX_COLUMNS
            || self.joins.len() > MAX_JOINS
            || self.filters.len() > MAX_FILTERS
            || self.group_by.len() > MAX_COLUMNS
            || self.order_by.len() > MAX_COLUMNS
            || self.limit == 0
            || self.limit > MAX_ROWS
            || self.offset > 10_000
        {
            return Err(rejected(
                "query exceeds column, join, filter or pagination limits",
            ));
        }
        for (schema, table) in self.tables() {
            identifier(schema)?;
            identifier(table)?;
            if schema.starts_with("pg_") || schema == "information_schema" {
                return Err(rejected("system schemas cannot be queried"));
            }
        }
        let check = |column: &ColumnRef| -> AppResult<()> {
            identifier(&column.column)?;
            if column.table > self.joins.len() {
                return Err(rejected("column references an unknown table"));
            }
            Ok(())
        };
        for c in self.columns.iter().chain(self.group_by.iter()) {
            check(c)?;
        }
        for (index, join) in self.joins.iter().enumerate() {
            check(&join.left)?;
            identifier(&join.right_column)?;
            if join.left.table > index {
                return Err(rejected("joins must reference an earlier table"));
            }
        }
        for filter in &self.filters {
            check(&filter.column)?;
            let null_check = matches!(filter.op, Operator::IsNull | Operator::IsNotNull);
            if null_check != filter.value.is_none()
                || filter
                    .value
                    .as_ref()
                    .is_some_and(|v| v.len() > 1024 || v.contains('\0'))
            {
                return Err(rejected(
                    "invalid filter value; use is_null/is_not_null for NULL",
                ));
            }
        }
        for aggregate in &self.aggregates {
            if let Some(column) = &aggregate.column {
                check(column)?;
            } else if !matches!(aggregate.function, AggregateKind::Count) {
                return Err(rejected("only count accepts column:null"));
            }
        }
        if !self.aggregates.is_empty() && self.columns.iter().any(|c| !self.group_by.contains(c)) {
            return Err(rejected(
                "selected columns must also occur in groupBy when using aggregates",
            ));
        }
        if self.order_by.iter().any(|o| o.field >= fields) {
            return Err(rejected("sort field is outside the selected output"));
        }
        Ok(())
    }
}

type Columns = HashMap<String, &'static str>;

fn builtin_type(oid: u32) -> Option<&'static str> {
    Some(match oid {
        16 => "bool",
        17 => "bytea",
        20 => "int8",
        21 => "int2",
        23 => "int4",
        25 => "text",
        700 => "float4",
        701 => "float8",
        1042 => "bpchar",
        1043 => "varchar",
        1082 => "date",
        1083 => "time",
        1114 => "timestamp",
        1184 => "timestamptz",
        1186 => "interval",
        1266 => "timetz",
        1700 => "numeric",
        2950 => "uuid",
        114 => "json",
        3802 => "jsonb",
        _ => return None,
    })
}

pub(super) async fn table_columns(conn: &PgConn, schema: &str, table: &str) -> AppResult<Columns> {
    identifier(schema)?;
    identifier(table)?;
    if schema.starts_with("pg_") || schema == "information_schema" {
        return Err(rejected("system schemas cannot be queried"));
    }
    // Keep the inspected relation stable until rollback; ONLY excludes inherited/foreign children.
    conn.client
        .batch_execute(&format!(
            "LOCK TABLE ONLY {} IN ACCESS SHARE MODE",
            qualified(schema, table)
        ))
        .await?;
    let rows = conn.client.query_typed(
        "SELECT c.oid, c.relkind = 'r' AND c.relam = 2 AND NOT c.relrowsecurity
                AND NOT c.relhasrules AND pg_catalog.has_table_privilege(c.oid, 'SELECT')
                AND NOT EXISTS (
                    SELECT 1 FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class ix ON ix.oid = i.indexrelid
                    WHERE i.indrelid = c.oid AND (i.indexprs IS NOT NULL OR i.indpred IS NOT NULL
                        OR ix.relam NOT IN (403,405,783,2742,4000,3580)
                        OR EXISTS (SELECT 1 FROM pg_catalog.pg_opclass op
                            WHERE op.oid = ANY(i.indclass) AND op.opcnamespace <> 'pg_catalog'::pg_catalog.regnamespace)))
         FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND c.relname = $2",
        &[(&schema, Type::NAME), (&table, Type::NAME)],
    ).await?;
    let row = rows
        .first()
        .ok_or_else(|| rejected("table does not exist"))?;
    if !row.get::<_, bool>(1) {
        return Err(rejected(
            "requires an ordinary heap table with SELECT permission, no RLS/rules or expression/partial/custom indexes",
        ));
    }
    let oid: u32 = row.get(0);
    let rows = conn
        .client
        .query_typed(
            "SELECT attname, atttypid FROM pg_catalog.pg_attribute
         WHERE attrelid = $1 AND attnum > 0 AND NOT attisdropped",
            &[(&oid, Type::OID)],
        )
        .await?;
    Ok(rows
        .iter()
        .filter_map(|r| builtin_type(r.get(1)).map(|kind| (r.get(0), kind)))
        .collect())
}

struct Compiled {
    sql: String,
    parameters: Vec<String>,
    columns: Vec<String>,
}

fn compile(query: &Query, tables: &[Columns]) -> AppResult<Compiled> {
    query.validate()?;
    let resolve = |column: &ColumnRef| -> AppResult<(String, &'static str)> {
        let kind = tables
            .get(column.table)
            .and_then(|t| t.get(&column.column))
            .ok_or_else(|| {
                rejected(
                    "unknown column or unsupported type; only built-in scalar types are allowed",
                )
            })?;
        Ok((
            format!("t{}.{}", column.table, quote_ident(&column.column)),
            *kind,
        ))
    };
    let mut expressions = Vec::new();
    let mut columns = Vec::new();
    for column in &query.columns {
        expressions.push(resolve(column)?.0);
        columns.push(format!("t{}_{}", column.table, column.column));
    }
    for aggregate in &query.aggregates {
        let function = match aggregate.function {
            AggregateKind::Count => "count",
            AggregateKind::Sum => "sum",
            AggregateKind::Avg => "avg",
            AggregateKind::Min => "min",
            AggregateKind::Max => "max",
        };
        let argument = match &aggregate.column {
            Some(c) => resolve(c)?.0,
            None => "*".into(),
        };
        expressions.push(format!("pg_catalog.{function}({argument})"));
        columns.push(match &aggregate.column {
            Some(c) => format!("{function}_t{}_{}", c.table, c.column),
            None => "count".into(),
        });
    }
    let projection = expressions
        .iter()
        .enumerate()
        .map(|(i, e)| format!("{e} AS v{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "SELECT {projection} FROM ONLY {} AS t0",
        qualified(&query.schema, &query.table)
    );
    for (index, join) in query.joins.iter().enumerate() {
        let table = index + 1;
        let kind = match join.kind {
            JoinKind::Inner => "INNER",
            JoinKind::Left => "LEFT",
        };
        let right = ColumnRef {
            table,
            column: join.right_column.clone(),
        };
        sql.push_str(&format!(
            " {kind} JOIN ONLY {} AS t{table} ON {} OPERATOR(pg_catalog.=) {}",
            qualified(&join.schema, &join.table),
            resolve(&join.left)?.0,
            resolve(&right)?.0
        ));
    }
    let mut parameters = Vec::new();
    let mut predicates = Vec::new();
    for filter in &query.filters {
        let (column, kind) = resolve(&filter.column)?;
        let operator = match filter.op {
            Operator::IsNull => {
                predicates.push(format!("{column} IS NULL"));
                continue;
            }
            Operator::IsNotNull => {
                predicates.push(format!("{column} IS NOT NULL"));
                continue;
            }
            Operator::Eq => "=",
            Operator::Ne => "<>",
            Operator::Gt => ">",
            Operator::Gte => ">=",
            Operator::Lt => "<",
            Operator::Lte => "<=",
            Operator::Like => "~~",
        };
        if matches!(filter.op, Operator::Like) && !matches!(kind, "text" | "varchar" | "bpchar") {
            return Err(rejected("like requires a text column"));
        }
        parameters.push(filter.value.clone().unwrap());
        predicates.push(format!(
            "{column} OPERATOR(pg_catalog.{operator}) (${}::pg_catalog.text)::pg_catalog.{kind}",
            parameters.len()
        ));
    }
    if !predicates.is_empty() {
        sql.push_str(&format!(" WHERE {}", predicates.join(" AND ")));
    }
    if !query.group_by.is_empty() {
        let groups = query
            .group_by
            .iter()
            .map(|c| resolve(c).map(|v| v.0))
            .collect::<AppResult<Vec<_>>>()?;
        sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
    }
    if !query.order_by.is_empty() {
        let order = query
            .order_by
            .iter()
            .map(|o| {
                format!(
                    "{} {} NULLS LAST",
                    o.field + 1,
                    match o.direction {
                        Direction::Asc => "ASC",
                        Direction::Desc => "DESC",
                    }
                )
            })
            .collect::<Vec<_>>();
        sql.push_str(&format!(" ORDER BY {}", order.join(", ")));
    }
    sql.push_str(&format!(
        " LIMIT {} OFFSET {}",
        query.limit + 1,
        query.offset
    ));
    // Convert only checked built-in outputs, and bound each value before it crosses the wire.
    let output = (0..columns.len())
        .map(|i| format!("pg_catalog.left(q.v{i}::pg_catalog.text, {CELL_CHARS})"))
        .collect::<Vec<_>>()
        .join(", ");
    sql = format!("SELECT {output} FROM ({sql}) AS q");
    if sql.len() > 16 * 1024 {
        return Err(rejected("generated query is too large"));
    }
    Ok(Compiled {
        sql,
        parameters,
        columns,
    })
}

pub(super) async fn execute(conn: &PgConn, query: &Query) -> AppResult<Value> {
    conn.client.batch_execute("BEGIN READ ONLY; SET LOCAL statement_timeout = '5s'; SET LOCAL lock_timeout = '2s'; SET LOCAL idle_in_transaction_session_timeout = '10s'; SET LOCAL search_path = pg_catalog; SET LOCAL work_mem = '4MB'; SET LOCAL max_parallel_workers_per_gather = 0").await?;
    let result = async {
        let mut tables = Vec::new();
        for (schema, table) in query.tables() {
            tables.push(table_columns(conn, schema, table).await?);
        }
        let compiled = compile(query, &tables)?;
        let parameters: Vec<&(dyn ToSql + Sync)> = compiled
            .parameters
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect();
        let rows = conn.client.query(&compiled.sql, &parameters).await?;
        let has_more = rows.len() > query.limit;
        let rows: Vec<Vec<Option<String>>> = rows
            .iter()
            .take(query.limit)
            .map(|row| (0..compiled.columns.len()).map(|i| row.get(i)).collect())
            .collect();
        Ok::<_, AppError>(super::tools::bounded_output(json!({
            "sql": compiled.sql, "parameters":compiled.parameters, "columns":compiled.columns,
            "rows":rows, "hasMore":has_more, "limit":query.limit, "offset":query.offset
        })))
    }
    .await;
    let rollback = conn.client.batch_execute("ROLLBACK").await;
    let value = result?;
    rollback?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires disposable localhost Postgres with GUCKPG_AI_TEST_PORT set"]
    async fn structured_reads_preserve_data_and_reject_indirect_code() {
        use crate::config::{ConnectionConfig, SslMode};
        use crate::pg::Transport;
        let port = std::env::var("GUCKPG_AI_TEST_PORT")
            .expect("set disposable localhost port")
            .parse()
            .unwrap();
        let config = ConnectionConfig {
            id: "ai-read-test".into(),
            name: "AI read test".into(),
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
        let schema = format!("ai_read_{}", uuid::Uuid::new_v4().simple());
        let role = format!("reader_{}", uuid::Uuid::new_v4().simple());
        conn.client.batch_execute(&format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.customers(id int PRIMARY KEY, name text);
             CREATE TABLE {schema}.orders(id int PRIMARY KEY, customer_id int REFERENCES {schema}.customers, amount numeric, note text);
             INSERT INTO {schema}.customers VALUES (1,'Ada'),(2,'Bea');
             INSERT INTO {schema}.orders VALUES (1,1,10,'paid'),(2,1,25,'paid'),(3,2,7,NULL);
             CREATE SEQUENCE {schema}.side_effect;
             CREATE VIEW {schema}.unsafe_view AS SELECT nextval('{schema}.side_effect') AS id;
             CREATE TABLE {schema}.rls_table(id int);
             ALTER TABLE {schema}.rls_table ENABLE ROW LEVEL SECURITY;
             CREATE POLICY unsafe_policy ON {schema}.rls_table USING (nextval('{schema}.side_effect') > 0);
             CREATE TYPE {schema}.custom_type AS ENUM ('a','b');
             CREATE TABLE {schema}.custom_table(id {schema}.custom_type);
             CREATE TABLE {schema}.indexed_table(id text);
             CREATE INDEX expression_index ON {schema}.indexed_table (lower(id));
             CREATE TABLE {schema}.private_table(id int);
             CREATE ROLE {role};
             GRANT USAGE ON SCHEMA {schema} TO {role};
             GRANT SELECT ON {schema}.customers,{schema}.orders,{schema}.unsafe_view,{schema}.rls_table,{schema}.custom_table,{schema}.indexed_table TO {role};
             SET ROLE {role};"
        )).await.unwrap();
        let grouped = json!({"schema":schema,"table":"customers",
            "columns":[{"table":0,"column":"name"}],
            "joins":[{"schema":schema,"table":"orders","kind":"inner","left":{"table":0,"column":"id"},"rightColumn":"customer_id"}],
            "aggregates":[{"function":"sum","column":{"table":1,"column":"amount"}},{"function":"count","column":null}],
            "groupBy":[{"table":0,"column":"name"}],
            "orderBy":[{"field":1,"direction":"desc"}],"limit":10});
        let result = execute(&conn, &Query::parse(&grouped).unwrap())
            .await
            .unwrap();
        assert_eq!(
            result["data"]["rows"],
            json!([["Ada", "35", "2"], ["Bea", "7", "1"]])
        );
        let mut filtered = json!({"schema":schema,"table":"orders","columns":[{"table":0,"column":"id"}],
            "filters":[{"column":{"table":0,"column":"note"},"op":"eq","value":"paid'; DELETE FROM orders; --"}],"limit":1});
        let result = execute(&conn, &Query::parse(&filtered).unwrap())
            .await
            .unwrap();
        assert_eq!(result["data"]["rows"], json!([]));
        filtered["filters"][0]["value"] = json!("paid");
        filtered["orderBy"] = json!([{"field":0,"direction":"asc"}]);
        let result = execute(&conn, &Query::parse(&filtered).unwrap())
            .await
            .unwrap();
        assert_eq!(result["data"]["rows"], json!([["1"]]));
        assert_eq!(result["data"]["hasMore"], true);
        filtered["filters"][0]["op"] = json!("is_null");
        filtered["filters"][0]["value"] = Value::Null;
        let result = execute(&conn, &Query::parse(&filtered).unwrap())
            .await
            .unwrap();
        assert_eq!(result["data"]["rows"], json!([["3"]]));
        for table in [
            "unsafe_view",
            "rls_table",
            "custom_table",
            "indexed_table",
            "private_table",
        ] {
            let query = Query::parse(&json!({"schema":schema,"table":table,
                "columns":[{"table":0,"column":"id"}],"limit":10}))
            .unwrap();
            assert!(execute(&conn, &query).await.is_err(), "{table}");
        }
        // Failed queries leave no transaction behind, and SQL permissions are never elevated.
        assert!(
            conn.client
                .batch_execute(&format!("DELETE FROM {schema}.orders"))
                .await
                .is_err()
        );
        let count: i64 = conn
            .client
            .query_one(&format!("SELECT count(*) FROM {schema}.orders"), &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 3);
        conn.client.batch_execute("RESET ROLE").await.unwrap();
        let called: bool = conn
            .client
            .query_one(&format!("SELECT is_called FROM {schema}.side_effect"), &[])
            .await
            .unwrap()
            .get(0);
        assert!(!called);
    }

    fn plan() -> Value {
        json!({"schema":"public","table":"orders","columns":[{"table":0,"column":"status"}],
            "aggregates":[{"function":"sum","column":{"table":0,"column":"amount"}}],
            "groupBy":[{"table":0,"column":"status"}],"limit":10})
    }

    fn metadata() -> Vec<Columns> {
        vec![([("status".into(), "text"), ("amount".into(), "numeric")]).into()]
    }

    #[test]
    fn builds_aggregates_and_keeps_filter_values_out_of_sql() {
        let mut value = plan();
        let injection = "paid'; DELETE FROM orders; --\\";
        value["filters"] =
            json!([{"column":{"table":0,"column":"status"},"op":"eq","value":injection}]);
        let compiled = compile(&Query::parse(&value).unwrap(), &metadata()).unwrap();
        assert!(compiled.sql.contains("pg_catalog.sum(t0.\"amount\")"));
        assert!(compiled.sql.contains("FROM ONLY \"public\".\"orders\""));
        assert!(compiled.sql.contains("$1::pg_catalog.text"));
        assert!(!compiled.sql.contains(injection));
        assert_eq!(compiled.parameters, [injection]);
        assert!(compiled.sql.contains("LIMIT 11 OFFSET 0"));
    }

    #[test]
    fn rejects_raw_sql_functions_unknown_fields_and_unbounded_plans() {
        for (field, invalid) in [
            ("sql", json!("DROP TABLE orders")),
            ("limit", json!(101)),
            ("offset", json!(10001)),
            ("schema", json!("pg_catalog")),
            ("aggregates", json!([{"function":"pg_sleep","column":null}])),
            (
                "filters",
                json!([{"column":{"table":0,"column":"status"},"op":"eq","value":null}]),
            ),
        ] {
            let mut value = plan();
            value[field] = invalid;
            assert!(Query::parse(&value).is_err(), "{field}");
        }
        let mut value = plan();
        value["groupBy"] = json!([]);
        assert!(Query::parse(&value).is_err());
        assert!(builtin_type(99999).is_none());
        assert!(builtin_type(2205).is_none()); // regclass can resolve arbitrary relations.
    }

    #[test]
    fn validates_joins_column_types_and_quotes_identifiers() {
        let mut value = plan();
        value["joins"] = json!([{"schema":"public","table":"other\"; --","kind":"left",
            "left":{"table":0,"column":"status"},"rightColumn":"name"}]);
        let mut tables = metadata();
        tables.push([("name".into(), "text")].into());
        let compiled = compile(&Query::parse(&value).unwrap(), &tables).unwrap();
        assert!(
            compiled
                .sql
                .contains("LEFT JOIN ONLY \"public\".\"other\"\"; --\"")
        );
        value["joins"][0]["left"]["table"] = json!(1);
        assert!(Query::parse(&value).is_err());
        let mut tables = metadata();
        tables[0].remove("amount");
        assert!(compile(&Query::parse(&plan()).unwrap(), &tables).is_err());
    }
}
