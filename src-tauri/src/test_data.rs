use tauri::State;
use tokio_postgres::Client;

use crate::error::{AppError, AppResult};
use crate::session::Manager;
use crate::sql::{qualified, quote_ident};

struct Column {
    name: String,
    kind: String,
    length: i32,
}

fn build_sql(schema: &str, table: &str, count: u32, columns: &[Column]) -> AppResult<String> {
    if !(1..=1000).contains(&count) {
        return Err(AppError::msg("Choose between 1 and 1000 rows."));
    }
    if columns.is_empty() {
        return Err(AppError::msg(
            "This table has no required columns to generate.",
        ));
    }
    let mut names = Vec::new();
    let mut values = Vec::new();
    for column in columns {
        names.push(quote_ident(&column.name));
        let value = match column.kind.as_str() {
            "text" | "varchar" | "bpchar" => {
                let value = "'test_' || n::text";
                if column.length > 4 {
                    format!("left({value}, {})", column.length - 4)
                } else {
                    value.to_string()
                }
            }
            "int2" | "int4" | "int8" | "numeric" | "float4" | "float8" => "n".into(),
            "bool" => "n % 2 = 0".into(),
            "date" => "CURRENT_DATE".into(),
            "timestamp" | "timestamptz" => "CURRENT_TIMESTAMP".into(),
            "uuid" => "md5(random()::text || clock_timestamp()::text || n::text)::uuid".into(),
            _ => {
                return Err(AppError::msg(format!(
                    "Unsupported required column: {} ({})",
                    column.name, column.kind
                )));
            }
        };
        values.push(value);
    }
    Ok(format!(
        "INSERT INTO {} ({}) SELECT {} FROM generate_series(1, {count}) AS n",
        qualified(schema, table),
        names.join(", "),
        values.join(", ")
    ))
}

async fn preview(client: &Client, schema: &str, table: &str, count: u32) -> AppResult<String> {
    let relation = client.query_opt(
        "SELECT c.oid FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace s ON s.oid = c.relnamespace WHERE s.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p')",
        &[&schema, &table],
    ).await?.ok_or_else(|| AppError::msg("Table not found."))?;
    let oid: u32 = relation.get(0);
    let rows = client.query(
        "SELECT a.attname::text, t.typname::text, a.atttypmod FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_type t ON t.oid = a.atttypid WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped AND a.attnotnull AND NOT a.atthasdef AND a.attidentity = '' AND a.attgenerated = '' ORDER BY a.attnum",
        &[&oid],
    ).await?;
    let columns = rows
        .iter()
        .map(|row| Column {
            name: row.get(0),
            kind: row.get(1),
            length: row.get(2),
        })
        .collect::<Vec<_>>();
    build_sql(schema, table, count, &columns)
}

#[tauri::command]
pub async fn preview_test_data(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
    count: u32,
) -> AppResult<String> {
    let conn = manager.open(&id, &database).await?;
    preview(&conn.client, &schema, &table, count).await
}

#[tauri::command]
pub async fn insert_test_data(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
    count: u32,
    expected_sql: String,
) -> AppResult<u64> {
    manager.ensure_writable(&id)?;
    let conn = manager.open(&id, &database).await?;
    let sql = preview(&conn.client, &schema, &table, count).await?;
    if sql != expected_sql {
        return Err(AppError::msg(
            "The table changed since the preview. Preview again.",
        ));
    }
    Ok(conn.client.execute(&sql, &[]).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_and_quotes_generated_sql() {
        let columns = [Column {
            name: "x\"y".into(),
            kind: "varchar".into(),
            length: 9,
        }];
        let sql = build_sql("a\"b", "t", 2, &columns).unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"a\"\"b\".\"t\" (\"x\"\"y\") SELECT left('test_' || n::text, 5) FROM generate_series(1, 2) AS n"
        );
        assert!(build_sql("a", "t", 0, &columns).is_err());
        assert!(build_sql("a", "t", 1001, &columns).is_err());
        assert!(build_sql("a", "t", 1, &[]).is_err());
        assert!(
            build_sql(
                "a",
                "t",
                1,
                &[Column {
                    name: "x".into(),
                    kind: "custom".into(),
                    length: -1
                }]
            )
            .is_err()
        );
    }
}
