//! Schema browsing via pg_catalog (not information_schema, which hides
//! objects the user cannot write to and is much slower). Queries go through
//! `query_typed`, which skips the separate prepare round trip.

use serde::Serialize;
use tokio_postgres::Client;
use tokio_postgres::types::Type;

use crate::error::AppResult;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub name: String,
    /// table, partitioned, view, materialized, foreign
    pub kind: &'static str,
    pub estimated_rows: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default: Option<String>,
    pub primary_key: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionTable {
    pub schema: String,
    pub name: String,
    pub columns: Vec<String>,
}

pub fn relation_kind(relkind: i8) -> &'static str {
    match relkind as u8 {
        b'r' => "table",
        b'p' => "partitioned",
        b'v' => "view",
        b'm' => "materialized",
        b'f' => "foreign",
        _ => "other",
    }
}

pub async fn server_version(client: &Client) -> AppResult<String> {
    let row = client.query_typed_one("SHOW server_version", &[]).await?;
    Ok(row.get(0))
}

pub async fn databases(client: &Client) -> AppResult<Vec<String>> {
    let rows = client
        .query_typed(
            "SELECT datname FROM pg_database WHERE NOT datistemplate AND datallowconn ORDER BY 1",
            &[],
        )
        .await?;
    Ok(rows.iter().map(|r| r.get(0)).collect())
}

pub async fn schemas(client: &Client, include_system: bool) -> AppResult<Vec<String>> {
    let rows = client
        .query_typed(
            "SELECT nspname FROM pg_namespace
             WHERE nspname NOT LIKE 'pg\\_toast%' AND nspname NOT LIKE 'pg\\_temp\\_%'
               AND ($1 OR (nspname <> 'information_schema' AND nspname NOT LIKE 'pg\\_%'))
             ORDER BY nspname = 'public' DESC, nspname",
            &[(&include_system, Type::BOOL)],
        )
        .await?;
    Ok(rows.iter().map(|r| r.get(0)).collect())
}

pub async fn relations(client: &Client, schema: &str) -> AppResult<Vec<Relation>> {
    let rows = client
        .query_typed(
            "SELECT c.relname, c.relkind,
                    CASE WHEN c.reltuples >= 0 THEN c.reltuples::bigint END
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
               AND NOT c.relispartition
             ORDER BY c.relname",
            &[(&schema, Type::NAME)],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| Relation {
            name: r.get(0),
            kind: relation_kind(r.get(1)),
            estimated_rows: r.get(2),
        })
        .collect())
}

pub async fn columns(client: &Client, schema: &str, table: &str) -> AppResult<Vec<Column>> {
    let rows = client
        .query_typed(
            "SELECT a.attname, format_type(a.atttypid, a.atttypmod), a.attnotnull,
                    pg_get_expr(d.adbin, d.adrelid),
                    EXISTS (SELECT 1 FROM pg_index i
                            WHERE i.indrelid = c.oid AND i.indisprimary
                              AND a.attnum = ANY (i.indkey))
             FROM pg_attribute a
             JOIN pg_class c ON c.oid = a.attrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
             WHERE n.nspname = $1 AND c.relname = $2 AND a.attnum > 0 AND NOT a.attisdropped
             ORDER BY a.attnum",
            &[(&schema, Type::NAME), (&table, Type::NAME)],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| Column {
            name: r.get(0),
            data_type: r.get(1),
            not_null: r.get(2),
            default: r.get(3),
            primary_key: r.get(4),
        })
        .collect())
}

/// Tables and columns of all user schemas, for editor autocompletion.
pub async fn completion(client: &Client) -> AppResult<Vec<CompletionTable>> {
    let rows = client
        .query_typed(
            "SELECT n.nspname, c.relname,
                    array_agg(a.attname::text ORDER BY a.attnum) FILTER (WHERE a.attnum > 0)
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             LEFT JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
             WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND NOT c.relispartition
               AND n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\\_%'
             GROUP BY n.nspname, c.relname
             ORDER BY 1, 2
             LIMIT 5000",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| CompletionTable {
            schema: r.get(0),
            name: r.get(1),
            columns: r.get::<_, Option<Vec<String>>>(2).unwrap_or_default(),
        })
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagramTable {
    pub name: String,
    pub kind: &'static str,
    pub columns: Vec<Column>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub name: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_table: String,
    pub to_columns: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagram {
    pub tables: Vec<DiagramTable>,
    /// Only keys between tables of this schema; there is nothing to draw an
    /// edge to otherwise.
    pub foreign_keys: Vec<ForeignKey>,
}

/// Everything an ER diagram of one schema needs, in two round trips.
pub async fn diagram(client: &Client, schema: &str) -> AppResult<Diagram> {
    let rows = client
        .query_typed(
            "SELECT c.relname, c.relkind, a.attname, format_type(a.atttypid, a.atttypmod),
                    a.attnotnull, pg_get_expr(d.adbin, d.adrelid),
                    EXISTS (SELECT 1 FROM pg_index i
                            WHERE i.indrelid = c.oid AND i.indisprimary
                              AND a.attnum = ANY (i.indkey))
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
             WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
               AND NOT c.relispartition
             ORDER BY c.relname, a.attnum",
            &[(&schema, Type::NAME)],
        )
        .await?;

    let mut tables: Vec<DiagramTable> = Vec::new();
    for r in &rows {
        let name: String = r.get(0);
        if tables.last().map(|t| &t.name) != Some(&name) {
            tables.push(DiagramTable {
                name,
                kind: relation_kind(r.get(1)),
                columns: Vec::new(),
            });
        }
        if let Some(table) = tables.last_mut() {
            table.columns.push(Column {
                name: r.get(2),
                data_type: r.get(3),
                not_null: r.get(4),
                default: r.get(5),
                primary_key: r.get(6),
            });
        }
    }

    // Column lists keep the constraint's own order, which matters for
    // composite keys.
    let rows = client
        .query_typed(
            "SELECT con.conname, src.relname, dst.relname,
                    array(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(attnum, i)
                          JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum
                          ORDER BY k.i),
                    array(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(attnum, i)
                          JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.attnum
                          ORDER BY k.i)
             FROM pg_constraint con
             JOIN pg_class src ON src.oid = con.conrelid
             JOIN pg_class dst ON dst.oid = con.confrelid
             JOIN pg_namespace sn ON sn.oid = src.relnamespace
             JOIN pg_namespace dn ON dn.oid = dst.relnamespace
             WHERE con.contype = 'f' AND sn.nspname = $1 AND dn.nspname = $1
               AND NOT src.relispartition
             ORDER BY src.relname, con.conname",
            &[(&schema, Type::NAME)],
        )
        .await?;
    let foreign_keys = rows
        .iter()
        .map(|r| ForeignKey {
            name: r.get(0),
            from_table: r.get(1),
            to_table: r.get(2),
            from_columns: r.get(3),
            to_columns: r.get(4),
        })
        .collect();

    Ok(Diagram {
        tables,
        foreign_keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relkinds() {
        assert_eq!(relation_kind(b'r' as i8), "table");
        assert_eq!(relation_kind(b'v' as i8), "view");
        assert_eq!(relation_kind(b'm' as i8), "materialized");
        assert_eq!(relation_kind(b'S' as i8), "other");
    }
}
