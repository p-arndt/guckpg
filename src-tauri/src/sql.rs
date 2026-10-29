//! SQL text built by guckpg itself. Every identifier from the UI goes through
//! `quote_ident`, so table or column names can never inject SQL.

use std::future::Future;

use serde::Deserialize;
use tokio_postgres::Client;

use crate::error::{AppError, AppResult};

pub fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

pub fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(name))
}

/// A string literal of unknown type, which Postgres coerces to the target
/// column's type, so one code path handles ints, uuids, json, arrays...
/// Backslashes switch to an E'' literal, like Postgres' own quote_literal.
pub fn quote_literal(value: &str) -> String {
    let quoted = value.replace('\'', "''");
    if value.contains('\\') {
        format!("E'{}'", quoted.replace('\\', "\\\\"))
    } else {
        format!("'{quoted}'")
    }
}

/// `quote_literal`, or the keyword `NULL` for a missing value.
pub fn literal_or_null(value: Option<&str>) -> String {
    value.map(quote_literal).unwrap_or_else(|| "NULL".into())
}

/// Postgres silently truncates identifiers longer than 63 bytes, which
/// would make guckpg act on a different name than the user typed, and NUL
/// cannot be sent at all.
pub fn checked_ident(name: &str) -> AppResult<&str> {
    if name.is_empty() || name.len() > 63 || name.contains('\0') {
        return Err(AppError::msg(
            "Invalid name: names must be 1 to 63 bytes long and must not contain NUL.",
        ));
    }
    Ok(name)
}

/// Opens a write transaction that fails fast instead of queueing behind a
/// lock held by an editor tab, and that cannot run unbounded.
pub const BEGIN_GUARDED: &str =
    "BEGIN; SET LOCAL lock_timeout = '2s'; SET LOCAL statement_timeout = '60s'";

/// Run `body` inside a transaction opened with `begin_sql` (usually
/// `BEGIN_GUARDED`): COMMIT when it returns Ok, ROLLBACK when it returns Err.
///
/// Use a connection nobody else shares (`Manager::open`): Postgres has one
/// transaction per connection, so concurrent callers on a shared client
/// would interleave their statements and roll back each other's work. If the
/// returned future is dropped midway, the transaction stays open until the
/// connection closes, which is another reason for a dedicated connection.
pub async fn in_tx<T, F, Fut>(client: &Client, begin_sql: &str, body: F) -> AppResult<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = AppResult<T>>,
{
    client.batch_execute(begin_sql).await?;
    match body().await {
        Ok(value) => {
            client.batch_execute("COMMIT").await?;
            Ok(value)
        }
        Err(err) => {
            let _ = client.batch_execute("ROLLBACK").await;
            Err(err)
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ColumnValue {
    pub column: String,
    pub value: Option<String>,
}

/// One row edit from the grid. `key` holds the primary key values as they
/// were loaded, so the WHERE clause hits exactly the row the user saw.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Change {
    Update {
        key: Vec<ColumnValue>,
        values: Vec<ColumnValue>,
    },
    Delete {
        key: Vec<ColumnValue>,
    },
    Insert {
        values: Vec<ColumnValue>,
    },
}

impl Change {
    /// UPDATE and DELETE must hit exactly one row; INSERT always adds one.
    pub fn expects_one_row(&self) -> bool {
        !matches!(self, Change::Insert { .. })
    }
}

fn where_key(key: &[ColumnValue]) -> String {
    key.iter()
        .map(|k| match &k.value {
            Some(v) => format!("{} = {}", quote_ident(&k.column), quote_literal(v)),
            None => format!("{} IS NULL", quote_ident(&k.column)),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

pub fn change_sql(schema: &str, table: &str, change: &Change) -> Result<String, String> {
    let target = qualified(schema, table);
    match change {
        Change::Update { key, values } => {
            if key.is_empty() || values.is_empty() {
                return Err("update needs a key and at least one value".into());
            }
            let set = values
                .iter()
                .map(|v| {
                    format!(
                        "{} = {}",
                        quote_ident(&v.column),
                        literal_or_null(v.value.as_deref())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok(format!(
                "UPDATE {target} SET {set} WHERE {}",
                where_key(key)
            ))
        }
        Change::Delete { key } => {
            if key.is_empty() {
                return Err("delete needs a key".into());
            }
            Ok(format!("DELETE FROM {target} WHERE {}", where_key(key)))
        }
        Change::Insert { values } if values.is_empty() => {
            Ok(format!("INSERT INTO {target} DEFAULT VALUES"))
        }
        Change::Insert { values } => {
            let cols = values
                .iter()
                .map(|v| quote_ident(&v.column))
                .collect::<Vec<_>>()
                .join(", ");
            let vals = values
                .iter()
                .map(|v| literal_or_null(v.value.as_deref()))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(format!("INSERT INTO {target} ({cols}) VALUES ({vals})"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortBy {
    pub column: String,
    pub dir: SortDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOp {
    /// Case-insensitive substring.
    Contains,
    StartsWith,
    EndsWith,
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum Connector {
    #[default]
    #[serde(rename = "AND")]
    And,
    #[serde(rename = "OR")]
    Or,
}

/// One condition of the table filter. `connector` joins it to the conditions
/// before it and is ignored on the first one.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseFilter {
    pub column: String,
    pub op: FilterOp,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub connector: Connector,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseRequest {
    pub schema: String,
    pub table: String,
    #[serde(default)]
    pub sort: Option<SortBy>,
    #[serde(default)]
    pub filters: Vec<BrowseFilter>,
    pub limit: u32,
    pub offset: u64,
}

/// What makes the browse order total, so LIMIT/OFFSET pages neither repeat
/// nor skip rows when the requested sort has ties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowOrder {
    /// Primary key column names, in key order.
    Key(Vec<String>),
    /// No primary key, but physical rows (tables, matviews): `tableoid, ctid`
    /// is unique even across partitions and inheritance children.
    Physical,
    /// Views and foreign tables have nothing unique to order by.
    Unknown,
}

/// Escape LIKE wildcards and the escape character itself.
fn like_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// One predicate. Comparisons use `col op 'literal'`, so Postgres coerces the
/// unknown-typed literal to the column type (numbers compare as numbers).
fn filter_predicate(filter: &BrowseFilter) -> AppResult<String> {
    let column = quote_ident(&filter.column);
    let value = || {
        filter.value.as_deref().ok_or_else(|| {
            AppError::msg(format!(
                "The filter on \"{}\" needs a value.",
                filter.column
            ))
        })
    };
    let pattern = |prefix: &str, suffix: &str| -> AppResult<String> {
        Ok(format!(
            "{column}::text ILIKE {}",
            quote_literal(&format!("{prefix}{}{suffix}", like_escape(value()?)))
        ))
    };
    let compare = |op: &str| -> AppResult<String> {
        Ok(format!("{column} {op} {}", quote_literal(value()?)))
    };
    match filter.op {
        FilterOp::Contains => pattern("%", "%"),
        FilterOp::StartsWith => pattern("", "%"),
        FilterOp::EndsWith => pattern("%", ""),
        FilterOp::Eq => compare("="),
        FilterOp::Ne => compare("<>"),
        FilterOp::Lt => compare("<"),
        FilterOp::Lte => compare("<="),
        FilterOp::Gt => compare(">"),
        FilterOp::Gte => compare(">="),
        FilterOp::IsNull => Ok(format!("{column} IS NULL")),
        FilterOp::IsNotNull => Ok(format!("{column} IS NOT NULL")),
    }
}

/// The WHERE condition. Conditions fold strictly left to right, so the
/// parentheses make `a OR b AND c` mean `((a OR b) AND c)`, as in Navicat.
fn where_condition(filters: &[BrowseFilter]) -> AppResult<Option<String>> {
    let mut condition: Option<String> = None;
    for filter in filters {
        let predicate = filter_predicate(filter)?;
        condition = Some(match condition {
            None => predicate,
            Some(left) => {
                let connector = match filter.connector {
                    Connector::And => "AND",
                    Connector::Or => "OR",
                };
                format!("({left} {connector} {predicate})")
            }
        });
    }
    Ok(condition)
}

pub fn browse_sql(req: &BrowseRequest, order: &RowOrder) -> AppResult<String> {
    let mut sql = format!("SELECT * FROM {}", qualified(&req.schema, &req.table));
    if let Some(condition) = where_condition(&req.filters)? {
        sql.push_str(&format!(" WHERE {condition}"));
    }
    let mut terms = Vec::new();
    if let Some(sort) = &req.sort {
        let dir = match sort.dir {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        };
        terms.push(format!("{} {dir}", quote_ident(&sort.column)));
    }
    let sort_column = req.sort.as_ref().map(|s| s.column.as_str());
    match order {
        RowOrder::Key(columns) => terms.extend(
            columns
                .iter()
                .filter(|c| Some(c.as_str()) != sort_column)
                .map(|c| quote_ident(c)),
        ),
        // Ordering an unsorted heap by ctid would read the whole table for
        // every page; only pay for it when the user's sort needs a sort anyway.
        RowOrder::Physical if sort_column.is_some() => {
            terms.extend(["tableoid".to_string(), "ctid".to_string()]);
        }
        RowOrder::Physical | RowOrder::Unknown => {}
    }
    if !terms.is_empty() {
        sql.push_str(&format!(" ORDER BY {}", terms.join(", ")));
    }
    sql.push_str(&format!(" LIMIT {} OFFSET {}", req.limit, req.offset));
    Ok(sql)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers() {
        assert_eq!(quote_ident("users"), r#""users""#);
        assert_eq!(quote_ident("User Table"), r#""User Table""#);
        assert_eq!(quote_ident(r#"a"b"#), r#""a""b""#);
        assert_eq!(
            quote_ident(r#"x"; DROP TABLE t; --"#),
            r#""x""; DROP TABLE t; --""#
        );
    }

    #[test]
    fn quotes_literals() {
        assert_eq!(quote_literal("abc"), "'abc'");
        assert_eq!(quote_literal("it's"), "'it''s'");
        assert_eq!(
            quote_literal("'; DROP TABLE t; --"),
            "'''; DROP TABLE t; --'"
        );
        assert_eq!(quote_literal(r"a\b'c"), r"E'a\\b''c'");
        assert_eq!(quote_literal(""), "''");
    }

    fn cv(column: &str, value: Option<&str>) -> ColumnValue {
        ColumnValue {
            column: column.into(),
            value: value.map(str::to_string),
        }
    }

    #[test]
    fn builds_update() {
        let change = Change::Update {
            key: vec![cv("id", Some("7")), cv("tenant", Some("a"))],
            values: vec![cv("name", Some("O'Brien")), cv("note", None)],
        };
        assert_eq!(
            change_sql("public", "users", &change).unwrap(),
            r#"UPDATE "public"."users" SET "name" = 'O''Brien', "note" = NULL WHERE "id" = '7' AND "tenant" = 'a'"#
        );
        assert!(change.expects_one_row());
    }

    #[test]
    fn builds_delete_and_insert() {
        let delete = Change::Delete {
            key: vec![cv("id", Some("7"))],
        };
        assert_eq!(
            change_sql("s", "t", &delete).unwrap(),
            r#"DELETE FROM "s"."t" WHERE "id" = '7'"#
        );

        let insert = Change::Insert {
            values: vec![cv("name", Some("x")), cv("note", None)],
        };
        assert_eq!(
            change_sql("s", "t", &insert).unwrap(),
            r#"INSERT INTO "s"."t" ("name", "note") VALUES ('x', NULL)"#
        );
        assert!(!insert.expects_one_row());

        let empty = Change::Insert { values: vec![] };
        assert_eq!(
            change_sql("s", "t", &empty).unwrap(),
            r#"INSERT INTO "s"."t" DEFAULT VALUES"#
        );
    }

    #[test]
    fn rejects_keyless_changes() {
        assert!(change_sql("s", "t", &Change::Delete { key: vec![] }).is_err());
        assert!(
            change_sql(
                "s",
                "t",
                &Change::Update {
                    key: vec![],
                    values: vec![cv("a", Some("b"))]
                }
            )
            .is_err()
        );
    }

    #[test]
    fn parses_frontend_changes() {
        let json = r#"[
            {"op":"update","key":[{"column":"id","value":"1"}],"values":[{"column":"name","value":null}]},
            {"op":"delete","key":[{"column":"id","value":"2"}]},
            {"op":"insert","values":[]}
        ]"#;
        let changes: Vec<Change> = serde_json::from_str(json).unwrap();
        assert_eq!(changes.len(), 3);
        assert!(matches!(changes[1], Change::Delete { .. }));
    }

    #[test]
    fn browse_without_sort() {
        let req = BrowseRequest {
            schema: "public".into(),
            table: "users".into(),
            sort: None,
            filters: Vec::new(),
            limit: 100,
            offset: 200,
        };
        assert_eq!(
            browse_sql(&req, &RowOrder::Unknown).unwrap(),
            r#"SELECT * FROM "public"."users" LIMIT 100 OFFSET 200"#
        );
        // Without a sort, an unkeyed heap is not sorted: that would scan the
        // whole table for every page.
        assert_eq!(
            browse_sql(&req, &RowOrder::Physical).unwrap(),
            r#"SELECT * FROM "public"."users" LIMIT 100 OFFSET 200"#
        );
        assert_eq!(
            browse_sql(&req, &RowOrder::Key(vec!["tenant".into(), "i\"d".into()])).unwrap(),
            r#"SELECT * FROM "public"."users" ORDER BY "tenant", "i""d" LIMIT 100 OFFSET 200"#
        );
    }

    #[test]
    fn browse_with_sort() {
        let req: BrowseRequest = serde_json::from_str(
            r#"{"schema":"app","table":"Order","sort":{"column":"created_at","dir":"desc"},"limit":50,"offset":0}"#,
        )
        .unwrap();
        assert_eq!(
            browse_sql(&req, &RowOrder::Unknown).unwrap(),
            r#"SELECT * FROM "app"."Order" ORDER BY "created_at" DESC LIMIT 50 OFFSET 0"#
        );
    }

    #[test]
    fn browse_sort_gets_a_unique_tie_breaker() {
        let req: BrowseRequest = serde_json::from_str(
            r#"{"schema":"app","table":"t","sort":{"column":"name","dir":"asc"},"limit":50,"offset":50}"#,
        )
        .unwrap();
        assert_eq!(
            browse_sql(&req, &RowOrder::Key(vec!["id".into()])).unwrap(),
            r#"SELECT * FROM "app"."t" ORDER BY "name" ASC, "id" LIMIT 50 OFFSET 50"#
        );
        assert_eq!(
            browse_sql(&req, &RowOrder::Physical).unwrap(),
            r#"SELECT * FROM "app"."t" ORDER BY "name" ASC, tableoid, ctid LIMIT 50 OFFSET 50"#
        );
        // Sorting by a key column does not repeat it.
        assert_eq!(
            browse_sql(&req, &RowOrder::Key(vec!["name".into(), "id".into()])).unwrap(),
            r#"SELECT * FROM "app"."t" ORDER BY "name" ASC, "id" LIMIT 50 OFFSET 50"#
        );
    }

    #[test]
    fn browse_filter_quotes_identifiers_and_escapes_like_wildcards() {
        let req: BrowseRequest = serde_json::from_str(
            r#"{"schema":"public","table":"users","filters":[{"column":"name\"x","op":"contains","value":"a'%_\\b"}],"limit":10,"offset":0}"#,
        ).unwrap();
        assert_eq!(
            browse_sql(&req, &RowOrder::Unknown).unwrap(),
            r#"SELECT * FROM "public"."users" WHERE "name""x"::text ILIKE E'%a''\\%\\_\\\\b%' LIMIT 10 OFFSET 0"#
        );
    }

    #[test]
    fn browse_filter_operators() {
        let sql = |filter: &str| {
            let req: BrowseRequest = serde_json::from_str(&format!(
                r#"{{"schema":"public","table":"t","filters":[{filter}],"limit":5,"offset":0}}"#
            ))
            .unwrap();
            browse_sql(&req, &RowOrder::Unknown).unwrap()
        };
        let select =
            |where_: &str| format!(r#"SELECT * FROM "public"."t" WHERE {where_} LIMIT 5 OFFSET 0"#);
        let compare = |op: &str, value: &str| {
            sql(&format!(
                r#"{{"column":"c","op":"{op}","value":"{value}"}}"#
            ))
        };
        assert_eq!(
            compare("startsWith", "ab"),
            select(r#""c"::text ILIKE 'ab%'"#)
        );
        assert_eq!(
            compare("endsWith", "ab"),
            select(r#""c"::text ILIKE '%ab'"#)
        );
        assert_eq!(compare("eq", "7"), select(r#""c" = '7'"#));
        assert_eq!(compare("ne", "7"), select(r#""c" <> '7'"#));
        assert_eq!(compare("lt", "7"), select(r#""c" < '7'"#));
        assert_eq!(compare("lte", "7"), select(r#""c" <= '7'"#));
        assert_eq!(compare("gt", "7"), select(r#""c" > '7'"#));
        assert_eq!(compare("gte", "7"), select(r#""c" >= '7'"#));
        assert_eq!(
            sql(r#"{"column":"c","op":"isNull"}"#),
            select(r#""c" IS NULL"#)
        );
        assert_eq!(
            sql(r#"{"column":"c","op":"isNotNull"}"#),
            select(r#""c" IS NOT NULL"#)
        );
    }

    #[test]
    fn browse_filters_fold_left_to_right_with_connectors() {
        let req: BrowseRequest = serde_json::from_str(
            r#"{"schema":"public","table":"t","filters":[
                {"column":"a","op":"eq","value":"1","connector":"AND"},
                {"column":"b","op":"isNull","connector":"OR"},
                {"column":"c","op":"contains","value":"x","connector":"AND"}
            ],"limit":5,"offset":0}"#,
        )
        .unwrap();
        assert_eq!(
            browse_sql(&req, &RowOrder::Unknown).unwrap(),
            r#"SELECT * FROM "public"."t" WHERE (("a" = '1' OR "b" IS NULL) AND "c"::text ILIKE '%x%') LIMIT 5 OFFSET 0"#
        );
    }

    #[test]
    fn browse_filter_without_value_is_an_error() {
        let req: BrowseRequest = serde_json::from_str(
            r#"{"schema":"public","table":"t","filters":[{"column":"c","op":"eq"}],"limit":5,"offset":0}"#,
        )
        .unwrap();
        assert!(browse_sql(&req, &RowOrder::Unknown).is_err());
    }

    #[test]
    fn literal_or_null_handles_missing_values() {
        assert_eq!(literal_or_null(None), "NULL");
        assert_eq!(literal_or_null(Some("NULL")), "'NULL'");
        assert_eq!(literal_or_null(Some("it's")), "'it''s'");
    }

    #[test]
    fn checks_identifier_length_and_nul() {
        assert_eq!(checked_ident("users").unwrap(), "users");
        assert!(checked_ident(&"a".repeat(63)).is_ok());
        assert!(checked_ident(&"a".repeat(64)).is_err());
        // 63 bytes, not 63 characters: 32 two-byte chars are 64 bytes.
        assert!(checked_ident(&"ä".repeat(32)).is_err());
        assert!(checked_ident("").is_err());
        assert!(checked_ident("a\0b").is_err());
    }
}
