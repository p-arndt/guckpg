//! Schema browsing via pg_catalog (not information_schema, which hides
//! objects the user cannot write to and is much slower). Queries go through
//! `query_typed`, which skips the separate prepare round trip.

use serde::Serialize;
use tokio_postgres::Client;
use tokio_postgres::types::Type;

use crate::error::{AppError, AppResult};
use crate::sql::{qualified, quote_ident};

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
pub struct SchemaObject {
    pub oid: u32,
    pub name: String,
    pub kind: &'static str,
    pub identity: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    name: String,
    can_login: bool,
    superuser: bool,
    create_db: bool,
    create_role: bool,
}

pub async fn roles(client: &Client) -> AppResult<Vec<Role>> {
    let rows = client.query("SELECT rolname::text, rolcanlogin, rolsuper, rolcreatedb, rolcreaterole FROM pg_catalog.pg_roles WHERE rolname !~ '^pg_' ORDER BY rolname", &[]).await?;
    Ok(rows
        .iter()
        .map(|row| Role {
            name: row.get(0),
            can_login: row.get(1),
            superuser: row.get(2),
            create_db: row.get(3),
            create_role: row.get(4),
        })
        .collect())
}

pub async fn objects(client: &Client, schema: &str) -> AppResult<Vec<SchemaObject>> {
    let rows = client
        .query_typed(
            "SELECT p.oid, p.proname::text AS name,
                    CASE p.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END AS kind,
                    pg_catalog.format('%I(%s)', p.proname,
                        pg_catalog.pg_get_function_identity_arguments(p.oid)) AS identity
             FROM pg_catalog.pg_proc p
             JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
             WHERE n.nspname = $1 AND p.prokind IN ('f', 'p')
             UNION ALL
             SELECT t.oid, t.tgname::text, 'trigger',
                    pg_catalog.format('%I ON %I.%I', t.tgname, n.nspname, c.relname)
             FROM pg_catalog.pg_trigger t
             JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND NOT t.tgisinternal
             UNION ALL
             SELECT c.oid, c.relname::text, 'sequence',
                    pg_catalog.format('%I', c.relname)
             FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relkind = 'S'
             ORDER BY kind, name, identity, oid",
            &[(&schema, Type::NAME)],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| SchemaObject {
            oid: row.get(0),
            name: row.get(1),
            kind: match row.get::<_, &str>(2) {
                "procedure" => "procedure",
                "trigger" => "trigger",
                "sequence" => "sequence",
                _ => "function",
            },
            identity: row.get(3),
        })
        .collect())
}

// Deparsers can leave visible functions and types unqualified. Capture the
// resolved path without changing the shared connection while browsing.
fn definition_with_path(definition: &str, path: &[String]) -> String {
    let path = path
        .iter()
        .map(|schema| quote_ident(schema))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "-- Preserve name resolution from the inspected connection.\nSET search_path TO {path};\n\n{};\n",
        definition.trim_end().trim_end_matches(';')
    )
}

fn view_definition(schema: &str, name: &str, kind: &str, query: &str, options: &str) -> String {
    let target = qualified(schema, name);
    let query = query.trim_end().trim_end_matches(';');
    let options = if options.is_empty() {
        String::new()
    } else {
        format!(" WITH ({options})")
    };
    if kind == "materialized" {
        format!(
            "-- Materialized views cannot be replaced directly; review dependencies before recreating.\nCREATE MATERIALIZED VIEW {target}{options} AS\n{query}"
        )
    } else {
        format!("CREATE OR REPLACE VIEW {target}{options} AS\n{query}")
    }
}

pub async fn object_definition(
    client: &Client,
    schema: &str,
    kind: &str,
    oid: u32,
) -> AppResult<String> {
    let query = match kind {
        "view" | "materialized" => {
            "SELECT c.relname::text, pg_catalog.pg_get_viewdef(c.oid, false),
                    pg_catalog.current_schemas(true)::text[],
                    COALESCE((SELECT pg_catalog.string_agg(
                        pg_catalog.format('%I = %L', option_name, option_value), ', ')
                        FROM pg_catalog.pg_options_to_table(c.reloptions)), '')
             FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE c.oid = $1 AND n.nspname = $2
               AND c.relkind = CASE $3 WHEN 'view' THEN 'v'::\"char\" ELSE 'm'::\"char\" END"
        }
        "function" | "procedure" => {
            "SELECT p.proname::text, pg_catalog.pg_get_functiondef(p.oid),
                    pg_catalog.current_schemas(true)::text[]
             FROM pg_catalog.pg_proc p
             JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
             WHERE p.oid = $1 AND n.nspname = $2
               AND p.prokind = CASE $3 WHEN 'function' THEN 'f'::\"char\" ELSE 'p'::\"char\" END"
        }
        "trigger" => {
            "SELECT t.tgname::text,
                    '-- For an existing trigger, use CREATE OR REPLACE TRIGGER where supported or an explicit change.\n'
                        || pg_catalog.pg_get_triggerdef(t.oid, false),
                    pg_catalog.current_schemas(true)::text[]
             FROM pg_catalog.pg_trigger t
             JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE t.oid = $1 AND n.nspname = $2 AND $3 = 'trigger' AND NOT t.tgisinternal"
        }
        "sequence" => {
            "SELECT c.relname::text,
                    pg_catalog.format(
                        '-- For an existing sequence, use ALTER SEQUENCE; current value is not included.\nCREATE SEQUENCE %I.%I AS %s\n    INCREMENT BY %s\n    MINVALUE %s\n    MAXVALUE %s\n    START WITH %s\n    CACHE %s\n    %s',
                        n.nspname, c.relname, pg_catalog.format_type(s.seqtypid, NULL),
                        s.seqincrement, s.seqmin, s.seqmax, s.seqstart, s.seqcache,
                        CASE WHEN s.seqcycle THEN 'CYCLE' ELSE 'NO CYCLE' END),
                    pg_catalog.current_schemas(true)::text[]
             FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             JOIN pg_catalog.pg_sequence s ON s.seqrelid = c.oid
             WHERE c.oid = $1 AND n.nspname = $2 AND c.relkind = 'S' AND $3 = 'sequence'"
        }
        _ => return Err(AppError::msg("unsupported object kind")),
    };
    let row = client
        .query_typed_opt(
            query,
            &[
                (&oid, Type::OID),
                (&schema, Type::NAME),
                (&kind, Type::TEXT),
            ],
        )
        .await?
        .ok_or_else(|| {
            AppError::msg("object does not exist in this schema or has a different kind")
        })?;
    let definition: String = row.get(1);
    let definition = if matches!(kind, "view" | "materialized") {
        view_definition(schema, row.get(0), kind, &definition, row.get(3))
    } else {
        definition
    };
    Ok(definition_with_path(
        &definition,
        &row.get::<_, Vec<String>>(2),
    ))
}

pub async fn relation_definition(client: &Client, schema: &str, name: &str) -> AppResult<String> {
    let row = client
        .query_typed_opt(
            "SELECT c.oid, c.relkind FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('v', 'm')",
            &[(&schema, Type::NAME), (&name, Type::NAME)],
        )
        .await?
        .ok_or_else(|| AppError::msg("view does not exist in this schema"))?;
    object_definition(client, schema, relation_kind(row.get(1)), row.get(0)).await
}

pub async fn refresh_materialized_view(client: &Client, schema: &str, name: &str) -> AppResult<()> {
    let exists: bool = client
        .query_typed_one(
            "SELECT EXISTS (
                SELECT 1 FROM pg_catalog.pg_class c
                JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind = 'm')",
            &[(&schema, Type::NAME), (&name, Type::NAME)],
        )
        .await?
        .get(0);
    if !exists {
        return Err(AppError::msg(
            "materialized view does not exist in this schema",
        ));
    }
    client
        .batch_execute(&format!(
            "REFRESH MATERIALIZED VIEW {}",
            qualified(schema, name)
        ))
        .await?;
    Ok(())
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

    #[test]
    fn definitions_preserve_quoted_search_path_and_statement_terminator() {
        let sql = definition_with_path(
            "CREATE VIEW v AS SELECT 1;\n",
            &["pg_catalog".into(), "a\"; DROP SCHEMA public; --".into()],
        );
        assert!(
            sql.contains("SET search_path TO \"pg_catalog\", \"a\"\"; DROP SCHEMA public; --\";")
        );
        assert!(sql.ends_with("CREATE VIEW v AS SELECT 1;\n"));
        assert!(!sql.ends_with(";;\n"));
    }

    #[test]
    fn view_definitions_quote_names_and_never_drop_materialized_views() {
        assert_eq!(
            view_definition("a\"b", "View Name", "view", " SELECT 1;\n", ""),
            "CREATE OR REPLACE VIEW \"a\"\"b\".\"View Name\" AS\n SELECT 1"
        );
        let sql = view_definition("s", "m", "materialized", "SELECT 1;", "");
        assert!(sql.contains("CREATE MATERIALIZED VIEW \"s\".\"m\" AS\nSELECT 1"));
        assert!(!sql.contains("DROP"));
        assert!(!sql.contains("OR REPLACE"));
        let sql = view_definition(
            "s",
            "v",
            "view",
            "SELECT 1",
            "security_barrier = 'true', check_option = 'local'",
        );
        assert!(sql.contains("WITH (security_barrier = 'true', check_option = 'local') AS"));
    }

    #[test]
    fn schema_object_serializes_identity_and_oid() {
        let json = serde_json::to_value(SchemaObject {
            oid: 42,
            name: "f".into(),
            kind: "function",
            identity: "f(integer)".into(),
        })
        .unwrap();
        assert_eq!(json["oid"], 42);
        assert_eq!(json["kind"], "function");
        assert_eq!(json["identity"], "f(integer)");
    }
}
