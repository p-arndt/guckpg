//! Editing result rows. A result is editable when all its table-backed
//! columns come from one table and that table's whole primary key is in the
//! result: then every row maps to exactly one table row.

use serde::Serialize;
use tokio_postgres::Client;
use tokio_postgres::types::Type;

use crate::error::{AppError, AppResult};
use crate::sql::{Change, change_sql};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub schema: String,
    pub table: String,
    /// Per result column, the table column it shows; None for expressions.
    pub columns: Vec<Option<String>>,
    /// Result column indexes that hold the primary key, in key order.
    pub key: Vec<usize>,
}

/// Where the rows of `sql` come from, if they can be edited. Uses Parse/
/// Describe only, so the statement is not executed a second time.
pub async fn source_info(client: &Client, sql: &str) -> Option<SourceInfo> {
    let statement = client.prepare(sql).await.ok()?;
    let cols = statement.columns();
    let origins: Vec<Option<(u32, i16)>> = cols
        .iter()
        .map(|c| match (c.table_oid(), c.column_id()) {
            (Some(oid), Some(attnum)) if oid != 0 && attnum > 0 => Some((oid, attnum)),
            _ => None,
        })
        .collect();

    let mut tables = origins.iter().flatten().map(|(oid, _)| *oid);
    let oid = tables.next()?;
    if tables.any(|o| o != oid) {
        return None;
    }

    // One round trip: query_typed skips the separate prepare, and the
    // column names come back aligned with `attnums`.
    let attnums: Vec<i16> = origins.iter().flatten().map(|(_, a)| *a).collect();
    let row = client
        .query_typed_opt(
            "SELECT n.nspname, c.relname,
                    (SELECT array_agg(k::int2 ORDER BY i) FROM pg_index ix,
                            unnest(ix.indkey) WITH ORDINALITY u(k, i)
                     WHERE ix.indrelid = c.oid AND ix.indisprimary),
                    ARRAY(SELECT a.attname::text
                          FROM unnest($2::int2[]) WITH ORDINALITY u(attnum, i)
                          LEFT JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = u.attnum
                          ORDER BY u.i)
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE c.oid = $1 AND c.relkind IN ('r', 'p')",
            &[(&oid, Type::OID), (&attnums, Type::INT2_ARRAY)],
        )
        .await
        .ok()??;
    let pk: Vec<i16> = row.get::<_, Option<Vec<i16>>>(2)?;
    let mut names = row.get::<_, Vec<Option<String>>>(3).into_iter();

    let key = pk
        .iter()
        .map(|attnum| {
            origins
                .iter()
                .position(|o| o.map(|(_, a)| a) == Some(*attnum))
        })
        .collect::<Option<Vec<usize>>>()?;

    Some(SourceInfo {
        schema: row.get(0),
        table: row.get(1),
        columns: origins
            .iter()
            .map(|o| o.and_then(|_| names.next().flatten()))
            .collect(),
        key,
    })
}

/// Apply all changes in one transaction. An UPDATE or DELETE that does not
/// hit exactly one row (someone else changed or deleted it meanwhile) rolls
/// everything back.
pub async fn apply(
    client: &Client,
    schema: &str,
    table: &str,
    changes: &[Change],
) -> AppResult<usize> {
    let statements = changes
        .iter()
        .map(|c| change_sql(schema, table, c).map(|sql| (sql, c.expects_one_row())))
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::msg)?;

    client.batch_execute("BEGIN").await?;
    for (sql, expects_one) in &statements {
        let result = client.execute(sql.as_str(), &[]).await;
        let failure = match result {
            Ok(n) if *expects_one && n != 1 => Some(AppError::msg(format!(
                "Nothing was saved: expected to change 1 row but {n} matched. The row may have been changed or deleted meanwhile; reload and try again.\n{sql}"
            ))),
            Ok(_) => None,
            Err(e) => Some(e.into()),
        };
        if let Some(err) = failure {
            let _ = client.batch_execute("ROLLBACK").await;
            return Err(err);
        }
    }
    client.batch_execute("COMMIT").await?;
    Ok(statements.len())
}
