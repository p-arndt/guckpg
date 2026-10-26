//! The model supplies a query plan, never SQL expressions or function names.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio_postgres::types::{ToSql, Type};

use super::tools::{ToolError, rejected};
use crate::pg::PgConn;
use crate::sql::{qualified, quote_ident};

const MAX_ROWS: usize = 100;
const MAX_COLUMNS: usize = 32;
const MAX_JOINS: usize = 3;
const MAX_FILTERS: usize = 16;
const MAX_OFFSET: usize = 10_000;
const MAX_IDENTIFIER_BYTES: usize = 63;
const MAX_VALUE_BYTES: usize = 1024;
const CELL_CHARS: usize = 1025;

type QueryResult<T> = Result<T, ToolError>;

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
    #[serde(default)]
    distinct: bool,
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
    filter_groups: Vec<Vec<Filter>>,
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
    let identifier = json!({"type":"string", "minLength":1, "maxLength":MAX_IDENTIFIER_BYTES});
    let column = object(json!({
        "table":{"type":"integer","minimum":0,"maximum":MAX_JOINS},
        "column":identifier
    }));
    let nullable_column = json!({"anyOf":[column, {"type":"null"}]});
    let array = |max: usize, items: Value| json!({"type":"array","maxItems":max,"items":items});
    let filter = object(json!({
        "column":column,"op":{"type":"string","enum":["eq","ne","gt","gte","lt","lte","like","is_null","is_not_null"]},
        "value":{"type":["string","null"],"maxLength":MAX_VALUE_BYTES}
    }));
    let filter_group = json!({"type":"array","minItems":1,"maxItems":MAX_FILTERS,"items":filter});
    let parameters = object(json!({
        "schema":identifier, "table":identifier,
        "columns":array(MAX_COLUMNS, column.clone()),
        "joins":array(MAX_JOINS, object(json!({
            "schema":identifier,"table":identifier,"kind":{"type":"string","enum":["inner","left"]},
            "left":column,"rightColumn":identifier
        }))),
        "filters":array(MAX_FILTERS, filter),
        "filterGroups":array(MAX_FILTERS, filter_group),
        "aggregates":array(MAX_COLUMNS, object(json!({
            "function":{"type":"string","enum":["count","sum","avg","min","max"]},"column":nullable_column,"distinct":{"type":"boolean"}
        }))),
        "groupBy":array(MAX_COLUMNS, column),
        "orderBy":array(MAX_COLUMNS, object(json!({
            "field":{"type":"integer","minimum":0,"maximum":MAX_COLUMNS - 1},"direction":{"type":"string","enum":["asc","desc"]}
        }))),
        "limit":{"type":"integer","minimum":1,"maximum":MAX_ROWS},
        "offset":{"type":"integer","minimum":0,"maximum":MAX_OFFSET}
    }));
    json!({"type":"function","function":{
        "name":"query_rows",
        "description":"Run a bounded read-only SELECT built from this plan. Inspect columns first. Table references: 0 = base table, 1..3 = joins in order. Join left must refer to an earlier table. filters combine with AND; filterGroups are OR alternatives, each containing AND filters, and combine with filters using AND. The total number of filters across both is at most 16. Values are strings cast to the column type (or null for is_null/is_not_null). Columns are output first, then aggregates; orderBy.field is their zero-based output position. For grouped queries list every selected column in groupBy. count with column:null counts rows; set distinct:true with a column to aggregate distinct values. Select at least one column or aggregate; for total row count use count with column:null. Use empty arrays for unused features. Supports ordinary tables and built-in scalar columns; views, RLS, custom access methods and expression/partial indexes are blocked. Returns at most 100 rows; use aggregates for totals. No raw SQL, writes, arbitrary expressions or functions.",
        "parameters":parameters
    }})
}

fn identifier(value: &str) -> QueryResult<()> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES || value.contains('\0') {
        return Err(rejected(format!(
            "identifiers must contain 1–{MAX_IDENTIFIER_BYTES} UTF-8 bytes without NUL"
        )));
    }
    Ok(())
}

impl Query {
    pub(super) fn parse(value: &Value) -> QueryResult<Self> {
        if value.to_string().len() > 32 * 1024 {
            return Err(rejected("query plan exceeds 32 KiB"));
        }
        let query: Self = serde_json::from_value(value.clone())
            .map_err(|error| rejected(format!("invalid query plan: {error}")))?;
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

    fn validate(&self) -> QueryResult<()> {
        let fields = self.columns.len() + self.aggregates.len();
        if fields == 0 {
            return Err(rejected(
                "select at least one column or aggregate; use count with column:null to count rows",
            ));
        }
        if fields > MAX_COLUMNS
            || self.joins.len() > MAX_JOINS
            || self.filters.len() + self.filter_groups.iter().map(Vec::len).sum::<usize>()
                > MAX_FILTERS
            || self.filter_groups.len() > MAX_FILTERS
            || self.group_by.len() > MAX_COLUMNS
            || self.order_by.len() > MAX_COLUMNS
            || self.limit == 0
            || self.limit > MAX_ROWS
            || self.offset > MAX_OFFSET
        {
            return Err(rejected(format!(
                "limits: at most {MAX_COLUMNS} selected columns and aggregates, {MAX_JOINS} joins, {MAX_FILTERS} filters, {MAX_COLUMNS} groups and sorts; limit 1–{MAX_ROWS}, offset 0–{MAX_OFFSET}"
            )));
        }
        for (schema, table) in self.tables() {
            identifier(schema)?;
            identifier(table)?;
            if schema.starts_with("pg_") || schema == "information_schema" {
                return Err(rejected("system schemas cannot be queried"));
            }
        }
        let check = |column: &ColumnRef| -> QueryResult<()> {
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
        if self.filter_groups.iter().any(Vec::is_empty) {
            return Err(rejected("filter groups must not be empty"));
        }
        for filter in self
            .filters
            .iter()
            .chain(self.filter_groups.iter().flatten())
        {
            check(&filter.column)?;
            let null_check = matches!(filter.op, Operator::IsNull | Operator::IsNotNull);
            if null_check != filter.value.is_none()
                || filter
                    .value
                    .as_ref()
                    .is_some_and(|v| v.len() > MAX_VALUE_BYTES || v.contains('\0'))
            {
                return Err(rejected(
                    "invalid filter value; use is_null/is_not_null for NULL",
                ));
            }
        }
        for aggregate in &self.aggregates {
            if aggregate.distinct && aggregate.column.is_none() {
                return Err(rejected("distinct requires an aggregate column"));
            }
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

pub(super) async fn table_columns(
    conn: &PgConn,
    schema: &str,
    table: &str,
) -> QueryResult<Columns> {
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

/// Resolves column references against the checked columns of each referenced table.
struct Resolver<'a> {
    tables: &'a [Columns],
}

impl Resolver<'_> {
    fn resolve(&self, column: &ColumnRef) -> QueryResult<(String, &'static str)> {
        let kind = self
            .tables
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
    }

    fn expression(&self, column: &ColumnRef) -> QueryResult<String> {
        self.resolve(column).map(|(expression, _)| expression)
    }
}

/// Selected expressions and their output column names, columns before aggregates.
fn projection(query: &Query, resolver: &Resolver) -> QueryResult<(Vec<String>, Vec<String>)> {
    let mut expressions = Vec::new();
    let mut columns = Vec::new();
    for column in &query.columns {
        expressions.push(resolver.expression(column)?);
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
            Some(c) => resolver.expression(c)?,
            None => "*".into(),
        };
        let distinct = if aggregate.distinct { "DISTINCT " } else { "" };
        expressions.push(format!("pg_catalog.{function}({distinct}{argument})"));
        columns.push(match &aggregate.column {
            Some(c) => format!("{function}_t{}_{}", c.table, c.column),
            None => "count".into(),
        });
    }
    Ok((expressions, columns))
}

fn join_clauses(query: &Query, resolver: &Resolver) -> QueryResult<String> {
    let mut sql = String::new();
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
            resolver.expression(&join.left)?,
            resolver.expression(&right)?
        ));
    }
    Ok(sql)
}

/// One predicate; filter values become bound text parameters cast to the column type.
fn filter_predicate(
    filter: &Filter,
    resolver: &Resolver,
    parameters: &mut Vec<String>,
) -> QueryResult<String> {
    let (column, kind) = resolver.resolve(&filter.column)?;
    let operator = match filter.op {
        Operator::IsNull => return Ok(format!("{column} IS NULL")),
        Operator::IsNotNull => return Ok(format!("{column} IS NOT NULL")),
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
    let value = filter
        .value
        .clone()
        .ok_or_else(|| rejected("invalid filter value; use is_null/is_not_null for NULL"))?;
    parameters.push(value);
    Ok(format!(
        "{column} OPERATOR(pg_catalog.{operator}) (${}::pg_catalog.text)::pg_catalog.{kind}",
        parameters.len()
    ))
}

fn where_clause(
    query: &Query,
    resolver: &Resolver,
    parameters: &mut Vec<String>,
) -> QueryResult<String> {
    let mut predicates = Vec::new();
    for filter in &query.filters {
        predicates.push(filter_predicate(filter, resolver, parameters)?);
    }
    if !query.filter_groups.is_empty() {
        let mut groups = Vec::new();
        for group in &query.filter_groups {
            let parts = group
                .iter()
                .map(|filter| filter_predicate(filter, resolver, parameters))
                .collect::<QueryResult<Vec<_>>>()?;
            groups.push(format!("({})", parts.join(" AND ")));
        }
        predicates.push(format!("({})", groups.join(" OR ")));
    }
    Ok(if predicates.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", predicates.join(" AND "))
    })
}

fn group_and_order_clauses(query: &Query, resolver: &Resolver) -> QueryResult<String> {
    let mut sql = String::new();
    if !query.group_by.is_empty() {
        let groups = query
            .group_by
            .iter()
            .map(|c| resolver.expression(c))
            .collect::<QueryResult<Vec<_>>>()?;
        sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
    }
    if !query.order_by.is_empty() {
        let order = query
            .order_by
            .iter()
            .map(|o| {
                let direction = match o.direction {
                    Direction::Asc => "ASC",
                    Direction::Desc => "DESC",
                };
                format!("{} {direction} NULLS LAST", o.field + 1)
            })
            .collect::<Vec<_>>();
        sql.push_str(&format!(" ORDER BY {}", order.join(", ")));
    }
    Ok(sql)
}

fn compile(query: &Query, tables: &[Columns]) -> QueryResult<Compiled> {
    query.validate()?;
    let resolver = Resolver { tables };
    let (expressions, columns) = projection(query, &resolver)?;
    let projection = expressions
        .iter()
        .enumerate()
        .map(|(i, e)| format!("{e} AS v{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut parameters = Vec::new();
    let inner = format!(
        "SELECT {projection} FROM ONLY {} AS t0{}{}{} LIMIT {} OFFSET {}",
        qualified(&query.schema, &query.table),
        join_clauses(query, &resolver)?,
        where_clause(query, &resolver, &mut parameters)?,
        group_and_order_clauses(query, &resolver)?,
        query.limit + 1,
        query.offset
    );
    // Convert only checked built-in outputs, and bound each value before it crosses the wire.
    let output = (0..columns.len())
        .map(|i| format!("pg_catalog.left(q.v{i}::pg_catalog.text, {CELL_CHARS})"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("SELECT {output} FROM ({inner}) AS q");
    if sql.len() > 16 * 1024 {
        return Err(rejected("generated query is too large"));
    }
    Ok(Compiled {
        sql,
        parameters,
        columns,
    })
}

pub(super) async fn execute(conn: &PgConn, query: &Query) -> QueryResult<Value> {
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
        Ok::<_, ToolError>(super::tools::bounded_output(json!({
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

/// Functions `query_sql` refuses by name. The read-only transaction and the user's
/// approval are the actual boundary; this list is only a best-effort net against calls
/// that act outside the transaction (files, other sessions, server configuration,
/// sleeps, locks, remote connections) or run SQL hidden in string literals. It is easy
/// to miss a call made indirectly, for example through a view or a user-defined wrapper.
const BLOCKED_FUNCTIONS: &[&str] = &[
    "pg_read_file",
    "pg_read_binary_file",
    "pg_stat_file",
    "pg_terminate_backend",
    "pg_cancel_backend",
    "pg_reload_conf",
    "pg_rotate_logfile",
    "pg_promote",
    "pg_switch_wal",
    "pg_create_restore_point",
    "pg_log_backend_memory_contexts",
    "pg_notify",
    "pg_file_write",
    "pg_file_rename",
    "pg_file_unlink",
    "pg_logdir_ls",
    "lo_import",
    "lo_export",
    "set_config",
    "query_to_xml",
    "query_to_xmlschema",
    "query_to_xml_and_xmlschema",
];
const BLOCKED_PREFIXES: &[&str] = &[
    "dblink",
    "pg_sleep",
    "pg_advisory",
    "pg_try_advisory",
    "pg_ls_",
];
/// Marks a Unicode-escaped quoted identifier, which can spell any function name.
const UNICODE_IDENTIFIER: &str = "u&\"";

/// The first blocked function `sql` refers to, matched case-insensitively outside
/// string literals and comments.
pub(super) fn blocked_function(sql: &str) -> Option<String> {
    // Whether backslashes escape quotes depends on server settings; checking both
    // readings keeps text that either one treats as code from hiding in a literal.
    for backslash_escapes in [false, true] {
        let words = sql_words(sql, backslash_escapes);
        if let Some(word) = words.iter().find(|word| {
            BLOCKED_FUNCTIONS.contains(&word.as_str())
                || BLOCKED_PREFIXES
                    .iter()
                    .any(|prefix| word.starts_with(prefix))
        }) {
            return Some(word.clone());
        }
        if words.iter().any(|word| word == UNICODE_IDENTIFIER) {
            return Some("U& quoted identifier".into());
        }
        if words.iter().any(|word| word == "copy") && words.iter().any(|word| word == "program") {
            return Some("COPY … PROGRAM".into());
        }
    }
    None
}

/// Lowercased words outside string literals and comments. Quoted identifiers count as
/// words because `"pg_sleep"(1)` still calls the function.
fn sql_words(sql: &str, backslash_escapes: bool) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let at = |index: usize| chars.get(index).copied();
    let mut words = Vec::new();
    let mut i = 0;
    while let Some(c) = at(i) {
        let next = at(i + 1);
        if c == '-' && next == Some('-') {
            while at(i).is_some_and(|c| c != '\n') {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0usize;
            while let Some(c) = at(i) {
                if c == '/' && at(i + 1) == Some('*') {
                    depth += 1;
                    i += 2;
                } else if c == '*' && at(i + 1) == Some('/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c == '\'' {
            i = skip_string(&chars, i, backslash_escapes);
        } else if c == '"' {
            let mut word = String::new();
            i += 1;
            while let Some(c) = at(i) {
                if c == '"' {
                    if at(i + 1) == Some('"') {
                        word.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                word.push(c);
                i += 1;
            }
            words.push(word.to_lowercase());
        } else if c == '$'
            && let Some(tag) = dollar_tag(&chars, i)
        {
            let body = i + tag.len();
            i = (body..chars.len())
                .find(|&end| chars[end..].starts_with(&tag))
                .map_or(chars.len(), |end| end + tag.len());
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while at(i).is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
            if word == "u" && at(i) == Some('&') && at(i + 1) == Some('"') {
                words.push(UNICODE_IDENTIFIER.into());
            }
            // E'…' always treats backslashes as escapes.
            if word == "e" && at(i) == Some('\'') {
                i = skip_string(&chars, i, true);
            }
            words.push(word);
        } else {
            i += 1;
        }
    }
    words
}

/// The index after the string literal that opens at `start`.
fn skip_string(chars: &[char], start: usize, backslash_escapes: bool) -> usize {
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if backslash_escapes => i += 2,
            '\'' if chars.get(i + 1) == Some(&'\'') => i += 2,
            '\'' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// The `$tag$` delimiter opening a dollar-quoted string at `start`, if any.
fn dollar_tag(chars: &[char], start: usize) -> Option<Vec<char>> {
    let mut end = start + 1;
    while let Some(&c) = chars.get(end) {
        if c == '$' {
            return Some(chars[start..=end].to_vec());
        }
        let valid = if end == start + 1 {
            c.is_alphabetic() || c == '_'
        } else {
            c.is_alphanumeric() || c == '_'
        };
        if !valid {
            return None;
        }
        end += 1;
    }
    None
}

fn wrap_sql(sql: &str) -> String {
    let sql = sql.trim().trim_end_matches(';').trim_end();
    format!(
        "SELECT pg_catalog.left(pg_catalog.row_to_json(q)::pg_catalog.text, 16385) FROM (\n{sql}\n) AS q LIMIT 101"
    )
}

pub(super) async fn execute_sql(conn: &PgConn, sql: &str) -> QueryResult<Value> {
    conn.client.batch_execute("BEGIN READ ONLY; SET LOCAL statement_timeout = '5s'; SET LOCAL lock_timeout = '2s'; SET LOCAL idle_in_transaction_session_timeout = '10s'; SET LOCAL work_mem = '4MB'; SET LOCAL max_parallel_workers_per_gather = 0").await?;
    let result = async {
        let statement = conn.client.prepare(&wrap_sql(sql)).await?;
        let rows = conn.client.query(&statement, &[]).await?;
        let has_more = rows.len() > MAX_ROWS;
        let mut truncated = false;
        let rows: Vec<Value> = rows.iter().take(MAX_ROWS).map(|row| {
            let value: String = row.get(0);
            match serde_json::from_str(&value) {
                Ok(value) => value,
                Err(_) => { truncated = true; Value::String(value) }
            }
        }).collect();
        Ok::<_, ToolError>(super::tools::bounded_output(json!({"sql":sql,"parameters":[],"rows":rows,"hasMore":has_more,"rowTruncated":truncated})))
    }.await;
    let rollback = conn.client.batch_execute("ROLLBACK").await;
    let value = result?;
    rollback?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn structured_reads_preserve_data_and_reject_indirect_code() {
        use crate::pg::Transport;
        let Some((config, password)) = crate::testutil::test_db("ai-read-test") else {
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
        let sql = format!(
            "WITH totals AS (SELECT customer_id, count(DISTINCT note) AS n FROM {schema}.orders GROUP BY customer_id) SELECT * FROM totals ORDER BY customer_id"
        );
        let result = execute_sql(&conn, &sql).await.unwrap();
        assert_eq!(
            result["data"]["rows"],
            json!([{"customer_id":1,"n":1},{"customer_id":2,"n":0}])
        );
        assert!(
            execute_sql(&conn, &format!("DELETE FROM {schema}.orders RETURNING id"))
                .await
                .is_err()
        );
        assert!(
            execute_sql(&conn, &format!("SELECT 1; DELETE FROM {schema}.orders"))
                .await
                .is_err()
        );
        let distinct = json!({"schema":schema,"table":"orders",
            "columns":[{"table":0,"column":"customer_id"}],
            "aggregates":[{"function":"count","column":{"table":0,"column":"note"},"distinct":true}],
            "groupBy":[{"table":0,"column":"customer_id"}],"orderBy":[{"field":0,"direction":"asc"}],"limit":10});
        let result = execute(&conn, &Query::parse(&distinct).unwrap())
            .await
            .unwrap();
        assert_eq!(result["data"]["rows"], json!([["1", "1"], ["2", "0"]]));
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
        conn.client
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE; DROP ROLE {role}"))
            .await
            .unwrap();
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
    fn supports_distinct_aggregates_and_explains_invalid_plans() {
        let mut value = plan();
        value["aggregates"][0]["distinct"] = json!(true);
        let compiled = compile(&Query::parse(&value).unwrap(), &metadata()).unwrap();
        assert!(
            compiled
                .sql
                .contains("pg_catalog.sum(DISTINCT t0.\"amount\")")
        );
        value["aggregates"][0] = json!({"function":"count","column":null,"distinct":true});
        assert!(
            Query::parse(&value)
                .err()
                .unwrap()
                .to_string()
                .contains("distinct requires")
        );
        value["aggregates"][0] = json!({"function":"count","column":null,"unexpected":true});
        assert!(
            Query::parse(&value)
                .err()
                .unwrap()
                .to_string()
                .contains("unknown field `unexpected`")
        );
    }

    #[test]
    fn blocks_dangerous_functions_outside_literals_and_comments() {
        for sql in [
            "SELECT pg_read_file('/etc/passwd')",
            "select PG_READ_BINARY_FILE('x')",
            "SELECT * FROM pg_catalog.pg_ls_dir('.')",
            "SELECT pg_ls_waldir()",
            "SELECT pg_stat_file('postgresql.conf')",
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity",
            "SELECT pg_cancel_backend(1)",
            "SELECT pg_reload_conf()",
            "SELECT pg_rotate_logfile()",
            "SELECT * FROM dblink('host=x', 'select 1') AS t(a int)",
            "SELECT dblink_exec('host=x', 'DROP TABLE t')",
            "SELECT lo_import('/etc/passwd')",
            "SELECT lo_export(1, '/tmp/x')",
            "SELECT pg_sleep(10)",
            "SELECT pg_sleep_for('1 hour')",
            "SELECT set_config('statement_timeout', '0', false)",
            "SELECT pg_advisory_lock(1)",
            "SELECT pg_try_advisory_xact_lock(1)",
            "SELECT \"pg_sleep\"(1)",
            "SELECT \"PG_SLEEP\"(1)",
            "SELECT U&\"pg\\005fsleep\"(1)",
            "SELECT query_to_xml('select pg_sleep(1)', true, true, '')",
            "COPY (SELECT 1) TO PROGRAM 'rm -rf /'",
            "SELECT 1 /* outer /* nested */ still comment */, pg_sleep(1)",
            "SELECT 'it''s', pg_sleep(1)",
            "SELECT $$text$$, pg_sleep(1)",
            "SELECT $tag$ $$ $tag$, pg_sleep(1)",
            "SELECT E'\\'', pg_sleep(1) --'",
            // Read as code when backslashes escape quotes (standard_conforming_strings off).
            "SELECT 'a\\'', pg_sleep(1) --'",
        ] {
            assert!(blocked_function(sql).is_some(), "{sql}");
        }
        assert_eq!(
            blocked_function("select Pg_Sleep(1)").as_deref(),
            Some("pg_sleep")
        );
    }

    #[test]
    fn allows_ordinary_reads_and_names_inside_literals_or_comments() {
        for sql in [
            "SELECT count(*) FROM orders WHERE note = 'pg_sleep(1)'",
            "SELECT 1 -- pg_read_file('x')\n",
            "SELECT 1 /* pg_terminate_backend(1) */",
            "SELECT $$pg_sleep(1)$$",
            "SELECT $body$ dblink('x') $body$",
            "SELECT \"sleep_minutes\", copy_count FROM programs",
            "WITH totals AS (SELECT customer_id, sum(amount) FROM orders GROUP BY 1) SELECT * FROM totals",
            "SELECT e.name FROM employees e WHERE e.title = 'it''s'",
            "SELECT $1",
        ] {
            assert_eq!(blocked_function(sql), None, "{sql}");
        }
    }

    #[test]
    fn tool_schema_limits_follow_the_validation_constants() {
        let definition = definition();
        let properties = &definition["function"]["parameters"]["properties"];
        assert_eq!(properties["limit"]["maximum"], MAX_ROWS);
        assert_eq!(properties["offset"]["maximum"], MAX_OFFSET);
        assert_eq!(properties["columns"]["maxItems"], MAX_COLUMNS);
        assert_eq!(properties["joins"]["maxItems"], MAX_JOINS);
        assert_eq!(properties["filters"]["maxItems"], MAX_FILTERS);
        assert_eq!(properties["filterGroups"]["items"]["maxItems"], MAX_FILTERS);
        assert_eq!(
            properties["columns"]["items"]["properties"]["table"]["maximum"],
            MAX_JOINS
        );
        assert_eq!(
            properties["orderBy"]["items"]["properties"]["field"]["maximum"],
            MAX_COLUMNS - 1
        );
        assert_eq!(
            properties["filters"]["items"]["properties"]["value"]["maxLength"],
            MAX_VALUE_BYTES
        );
        assert_eq!(properties["schema"]["maxLength"], MAX_IDENTIFIER_BYTES);
        assert_eq!(
            properties["aggregates"]["items"]["properties"]["column"]["anyOf"][1]["type"],
            "null"
        );
    }

    #[test]
    fn plan_rejections_are_typed() {
        let mut value = plan();
        value["limit"] = json!(0);
        assert!(matches!(Query::parse(&value), Err(ToolError::Rejected(_))));
    }

    #[test]
    fn wraps_sql_and_limits_results() {
        assert_eq!(
            wrap_sql(" SELECT 1; "),
            "SELECT pg_catalog.left(pg_catalog.row_to_json(q)::pg_catalog.text, 16385) FROM (\nSELECT 1\n) AS q LIMIT 101"
        );
    }

    #[test]
    fn combines_filter_groups_with_bound_parameters() {
        let mut value = plan();
        value["filters"] = json!([{"column":{"table":0,"column":"amount"},"op":"gt","value":"10"}]);
        value["filterGroups"] = json!([
            [{"column":{"table":0,"column":"status"},"op":"eq","value":"paid"}],
            [{"column":{"table":0,"column":"status"},"op":"eq","value":"pending"}]
        ]);
        let compiled = compile(&Query::parse(&value).unwrap(), &metadata()).unwrap();
        assert!(
            compiled
                .sql
                .contains(" AND ((t0.\"status\" OPERATOR(pg_catalog.=)")
        );
        assert!(
            compiled
                .sql
                .contains(" OR (t0.\"status\" OPERATOR(pg_catalog.=)")
        );
        assert_eq!(compiled.parameters, ["10", "paid", "pending"]);
        value["filterGroups"] = json!([[]]);
        assert!(Query::parse(&value).is_err());
    }

    #[test]
    fn explains_empty_projection_and_limit_errors() {
        let mut value = plan();
        value["columns"] = json!([]);
        value["aggregates"] = json!([]);
        assert!(
            Query::parse(&value)
                .err()
                .unwrap()
                .to_string()
                .contains("select at least one column or aggregate")
        );
        value["limit"] = json!(101);
        value["columns"] = json!([{"table":0,"column":"status"}]);
        assert!(
            Query::parse(&value)
                .err()
                .unwrap()
                .to_string()
                .contains("limit 1–100")
        );
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
