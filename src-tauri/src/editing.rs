//! Editing result rows. A result is editable when all its table-backed
//! columns come from one table, each table column shows up at most once,
//! the table is referenced only once in the query, and that table's whole
//! primary key is in the result: then every row maps to exactly one table
//! row.

use std::collections::HashSet;

use serde::Serialize;
use tokio_postgres::Client;
use tokio_postgres::types::Type;

use crate::error::{AppError, AppResult};
use crate::sql::{BEGIN_GUARDED, Change, RowOrder, change_sql, in_tx};

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
    // A repeated column would turn one cell edit into `SET c = .., c = ..`,
    // and in a self-join (`a.id, b.id`) the copies belong to different rows.
    if has_duplicates(origins.iter().flatten()) {
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
    let table: String = row.get(1);

    // Describe reports the origin table but not which reference to it a
    // column came from, so `SELECT a.id, b.name FROM emp a JOIN emp b ..`
    // looks like a plain single-table result while its rows mix two table
    // rows. Requiring exactly one mention of the table name in the SQL text
    // rules that out; it also refuses harmless cases like a subquery on the
    // same table, which is the safe side to err on.
    if relation_mentions(sql, &table) != 1 {
        return None;
    }

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
        table,
        columns: origins
            .iter()
            .map(|o| o.and_then(|_| names.next().flatten()))
            .collect(),
        key,
    })
}

fn has_duplicates<'a>(origins: impl Iterator<Item = &'a (u32, i16)>) -> bool {
    let mut seen = HashSet::new();
    origins.into_iter().any(|o| !seen.insert(*o))
}

/// How often `relname` appears in `sql` as a relation name: an identifier
/// that is not followed by `.`, so qualifiers like `emp.id` or `emp.*` do
/// not count but `public.emp` does. Strings, comments and dollar quotes are
/// skipped. Unquoted names are folded and truncated like Postgres does.
/// Over-counting (a column or alias spelled like the table) only makes a
/// result read-only; U&"..." escapes are not decoded and are never counted.
fn relation_mentions(sql: &str, relname: &str) -> usize {
    let chars: Vec<char> = sql.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut count = 0;
    let is_ident_start = |c: char| c.is_alphabetic() || c == '_' || !c.is_ascii();
    let is_ident_char = |c: char| is_ident_start(c) || c.is_ascii_digit() || c == '$';

    while i < len {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let name = if c == '-' && next == Some('-') {
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            None
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < len {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            None
        } else if c == '\'' {
            i = skip_string(&chars, i, false);
            None
        } else if c == '"' {
            let mut ident = String::new();
            i += 1;
            while i < len {
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        ident.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                ident.push(chars[i]);
                i += 1;
            }
            Some(ident)
        } else if c == '$' {
            i = skip_dollar(&chars, i, is_ident_char);
            None
        } else if c.is_ascii_digit() {
            while i < len && is_ident_char(chars[i]) {
                i += 1;
            }
            None
        } else if is_ident_start(c) {
            let start = i;
            while i < len && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if chars.get(i) == Some(&'\'') {
                // A literal prefix like E'..' or B'..', not a name.
                i = skip_string(&chars, i, word.eq_ignore_ascii_case("e"));
                None
            } else {
                Some(fold_ident(&word))
            }
        } else {
            i += 1;
            None
        };

        if let Some(name) = name {
            let mut j = i;
            while j < len && chars[j].is_whitespace() {
                j += 1;
            }
            if chars.get(j) != Some(&'.') && name == relname {
                count += 1;
            }
        }
    }
    count
}

/// Index after the string literal starting at the quote at `i`.
fn skip_string(chars: &[char], mut i: usize, backslash_escapes: bool) -> usize {
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if backslash_escapes => i += 2,
            '\'' if chars.get(i + 1) == Some(&'\'') => i += 2,
            '\'' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

/// Index after a `$tag$ .. $tag$` body, or after a lone `$` / `$1`
/// parameter that does not start one.
fn skip_dollar(chars: &[char], i: usize, is_ident_char: impl Fn(char) -> bool) -> usize {
    let mut j = i + 1;
    if chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
        return j;
    }
    while j < chars.len() && chars[j] != '$' && is_ident_char(chars[j]) {
        j += 1;
    }
    if chars.get(j) != Some(&'$') {
        return i + 1;
    }
    let tag = &chars[i..=j];
    let mut k = j + 1;
    while k < chars.len() {
        if chars[k..].starts_with(tag) {
            return k + tag.len();
        }
        k += 1;
    }
    k
}

/// Postgres folds unquoted identifiers to lower case (ASCII only for
/// multibyte encodings) and truncates them to 63 bytes.
fn fold_ident(word: &str) -> String {
    let mut folded = word.to_ascii_lowercase();
    if folded.len() > 63 {
        let mut end = 63;
        while !folded.is_char_boundary(end) {
            end -= 1;
        }
        folded.truncate(end);
    }
    folded
}

/// What `browse_sql` can order by to make its pages deterministic.
pub async fn row_order(client: &Client, schema: &str, table: &str) -> AppResult<RowOrder> {
    let row = client
        .query_typed_opt(
            "SELECT c.relkind::text,
                    ARRAY(SELECT a.attname::text
                          FROM pg_index ix,
                               unnest(ix.indkey) WITH ORDINALITY u(k, i)
                          JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = u.k
                          WHERE ix.indrelid = c.oid AND ix.indisprimary
                          ORDER BY u.i)
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relname = $2",
            &[(&schema, Type::NAME), (&table, Type::NAME)],
        )
        .await?;
    let Some(row) = row else {
        return Ok(RowOrder::Unknown);
    };
    let kind: String = row.get(0);
    let key: Vec<String> = row.get(1);
    Ok(if !key.is_empty() {
        RowOrder::Key(key)
    } else if matches!(kind.as_str(), "r" | "p" | "m") {
        RowOrder::Physical
    } else {
        RowOrder::Unknown
    })
}

/// Apply all changes in one transaction. An UPDATE or DELETE that does not
/// hit exactly one row (someone else changed or deleted it meanwhile) rolls
/// everything back. `client` must not be shared, see `sql::in_tx`.
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

    in_tx(client, BEGIN_GUARDED, || async {
        for (sql, expects_one) in &statements {
            let n = client.execute(sql.as_str(), &[]).await?;
            if *expects_one && n != 1 {
                return Err(AppError::msg(format!(
                    "Nothing was saved: expected to change 1 row but {n} matched. The row may have been changed or deleted meanwhile; reload and try again.\n{sql}"
                )));
            }
        }
        Ok(statements.len())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_repeated_origins() {
        assert!(!has_duplicates([(1, 1), (1, 2), (2, 1)].iter()));
        assert!(has_duplicates([(1, 1), (1, 2), (1, 1)].iter()));
        assert!(!has_duplicates([].iter()));
    }

    #[test]
    fn counts_relation_references_not_qualifiers() {
        assert_eq!(relation_mentions("SELECT id, name FROM emp", "emp"), 1);
        assert_eq!(
            relation_mentions("SELECT emp.id, emp.* FROM public.emp", "emp"),
            1
        );
        assert_eq!(
            relation_mentions("select e.id from public . emp e", "emp"),
            1
        );
        assert_eq!(relation_mentions("SELECT id FROM EMP", "emp"), 1);
        assert_eq!(relation_mentions("SELECT emp_id FROM emps", "emp"), 0);
    }

    #[test]
    fn counts_self_joins_and_subqueries() {
        for sql in [
            "SELECT a.id, b.name FROM emp a JOIN emp b ON b.id = a.manager_id",
            "SELECT a.id, b.name FROM emp a, (SELECT * FROM emp) b",
            "SELECT id FROM emp WHERE id IN (SELECT manager_id FROM emp)",
            r#"SELECT a.id FROM emp a JOIN "emp" b USING (id)"#,
        ] {
            assert_eq!(relation_mentions(sql, "emp"), 2, "{sql}");
        }
    }

    #[test]
    fn quoted_names_are_exact() {
        assert_eq!(relation_mentions(r#"SELECT * FROM "Emp""#, "Emp"), 1);
        assert_eq!(relation_mentions("SELECT * FROM Emp", "Emp"), 0);
        assert_eq!(relation_mentions(r#"SELECT * FROM "Emp""#, "emp"), 0);
        assert_eq!(
            relation_mentions(r#"SELECT * FROM "we""ird" w, x"#, r#"we"ird"#),
            1
        );
        assert_eq!(relation_mentions(r#"SELECT "emp".id FROM emp"#, "emp"), 1);
    }

    #[test]
    fn ignores_strings_comments_and_dollar_quotes() {
        let sql = "SELECT 'emp', E'\\' emp', $$ emp $$, $t$ emp $t$, $1, b'1' -- emp
                   /* emp /* nested emp */ emp */ FROM emp";
        assert_eq!(relation_mentions(sql, "emp"), 1);
        assert_eq!(relation_mentions("SELECT 'it''s emp' FROM emp", "emp"), 1);
    }

    #[test]
    fn folds_long_unquoted_names_like_postgres() {
        let long = "a".repeat(70);
        assert_eq!(
            relation_mentions(&format!("SELECT * FROM {long}"), &"a".repeat(63)),
            1
        );
    }
}
