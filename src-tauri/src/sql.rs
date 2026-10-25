//! SQL text built by guckpg itself. Every identifier from the UI goes through
//! `quote_ident`, so table or column names can never inject SQL.

use serde::Deserialize;

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

fn literal_or_null(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(quote_literal)
        .unwrap_or_else(|| "NULL".into())
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
                .map(|v| format!("{} = {}", quote_ident(&v.column), literal_or_null(&v.value)))
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
                .map(|v| literal_or_null(&v.value))
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseRequest {
    pub schema: String,
    pub table: String,
    #[serde(default)]
    pub sort: Option<SortBy>,
    pub limit: u32,
    pub offset: u64,
}

pub fn browse_sql(req: &BrowseRequest) -> String {
    let mut sql = format!("SELECT * FROM {}", qualified(&req.schema, &req.table));
    if let Some(sort) = &req.sort {
        let dir = match sort.dir {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        };
        sql.push_str(&format!(" ORDER BY {} {dir}", quote_ident(&sort.column)));
    }
    sql.push_str(&format!(" LIMIT {} OFFSET {}", req.limit, req.offset));
    sql
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
            limit: 100,
            offset: 200,
        };
        assert_eq!(
            browse_sql(&req),
            r#"SELECT * FROM "public"."users" LIMIT 100 OFFSET 200"#
        );
    }

    #[test]
    fn browse_with_sort() {
        let req: BrowseRequest = serde_json::from_str(
            r#"{"schema":"app","table":"Order","sort":{"column":"created_at","dir":"desc"},"limit":50,"offset":0}"#,
        )
        .unwrap();
        assert_eq!(
            browse_sql(&req),
            r#"SELECT * FROM "app"."Order" ORDER BY "created_at" DESC LIMIT 50 OFFSET 0"#
        );
    }
}
