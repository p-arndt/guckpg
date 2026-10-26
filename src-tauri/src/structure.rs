use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_postgres::Client;

use crate::error::{AppError, AppResult};
use crate::session::Manager;
use crate::sql::{qualified, quote_ident, quote_literal};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableStructure {
    pub schema: String,
    pub name: String,
    pub comment: Option<String>,
    pub kind: String,
    pub columns: Vec<StructureColumn>,
    pub constraints: Vec<StructureConstraint>,
    pub indexes: Vec<StructureIndex>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructureColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub identity: String,
    pub comment: Option<String>,
    pub generated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructureConstraint {
    pub name: String,
    pub kind: String,
    pub columns: Vec<String>,
    pub ref_schema: Option<String>,
    pub ref_table: Option<String>,
    pub ref_columns: Vec<String>,
    pub on_update: Option<String>,
    pub on_delete: Option<String>,
    pub expression: Option<String>,
    pub definition: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructureIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub method: String,
    pub definition: String,
    pub managed: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInput {
    name: String,
    data_type: String,
    nullable: bool,
    default: Option<String>,
    identity: String,
    comment: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintInput {
    name: String,
    kind: String,
    columns: Vec<String>,
    ref_schema: Option<String>,
    ref_table: Option<String>,
    ref_columns: Vec<String>,
    on_update: Option<String>,
    on_delete: Option<String>,
    expression: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInput {
    name: String,
    columns: Vec<String>,
    unique: bool,
    method: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureRequest {
    schema: String,
    table: Option<String>,
    expected: Option<TableStructure>,
    changes: Vec<StructureChange>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StructureChange {
    CreateTable {
        name: String,
        columns: Vec<ColumnInput>,
        comment: Option<String>,
    },
    RenameTable {
        name: String,
    },
    SetTableComment {
        comment: Option<String>,
    },
    AddColumn {
        column: ColumnInput,
    },
    AlterColumn {
        name: String,
        column: ColumnInput,
    },
    DropColumn {
        name: String,
    },
    AddConstraint {
        constraint: ConstraintInput,
    },
    DropConstraint {
        name: String,
    },
    AddIndex {
        index: IndexInput,
    },
    DropIndex {
        name: String,
    },
}

fn ident(s: &str) -> AppResult<String> {
    if s.trim().is_empty() || s.contains('\0') || s.len() > 63 {
        Err(AppError::msg("Invalid identifier"))
    } else {
        Ok(quote_ident(s))
    }
}
fn literal(s: &Option<String>) -> String {
    s.as_deref()
        .map(quote_literal)
        .unwrap_or_else(|| "NULL".into())
}
// Fragments must remain inside the surrounding expression; comments and dollar quotes are rejected.
fn fragment(s: &str) -> AppResult<()> {
    if s.trim().is_empty() || s.contains('\0') || s.contains('\\') {
        return Err(AppError::msg("Invalid SQL expression"));
    }
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut depth = 0i32;
    let mut quote = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == q {
                if chars.get(i + 1) == Some(&q) {
                    i += 1;
                } else {
                    quote = None;
                }
            } else if c == '\\' && q == '\'' {
                i += 1;
            }
        } else {
            match c {
                '\'' | '"' => quote = Some(c),
                '(' | '[' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(AppError::msg("Unbalanced SQL expression"));
                    }
                }
                ';' | '$' | '\\' => return Err(AppError::msg("Invalid SQL expression")),
                '-' if chars.get(i + 1) == Some(&'-') => {
                    return Err(AppError::msg("SQL comments are not allowed"));
                }
                '/' if chars.get(i + 1) == Some(&'*') => {
                    return Err(AppError::msg("SQL comments are not allowed"));
                }
                _ => {}
            }
        }
        i += 1;
    }
    if quote.is_some() || depth != 0 {
        return Err(AppError::msg("Unbalanced SQL expression"));
    }
    Ok(())
}
fn data_type(s: &str) -> AppResult<()> {
    fragment(s)?;
    if !s
        .chars()
        .all(|c| c.is_alphanumeric() || c.is_whitespace() || "_.,\"()[]".contains(c))
    {
        return Err(AppError::msg("Invalid data type"));
    }
    let mut depth = 0;
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => return Err(AppError::msg("Invalid data type")),
            _ => {}
        }
    }
    let lower = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if [
        " default ",
        " constraint ",
        " primary ",
        " references ",
        " unique ",
        " check ",
        " not ",
        " null ",
        " generated ",
        " collate ",
        " using ",
        " include ",
        " exclude ",
        " deferrable ",
        " initially ",
    ]
    .iter()
    .any(|x| format!(" {} ", lower).contains(x))
    {
        return Err(AppError::msg("Invalid data type"));
    }
    Ok(())
}
fn column(c: &ColumnInput) -> AppResult<String> {
    let name = ident(&c.name)?;
    data_type(&c.data_type)?;
    if let Some(d) = &c.default {
        fragment(d)?;
    }
    let identity = match c.identity.as_str() {
        "" => "",
        "always" => " GENERATED ALWAYS AS IDENTITY",
        "byDefault" => " GENERATED BY DEFAULT AS IDENTITY",
        _ => return Err(AppError::msg("Invalid identity")),
    };
    if !identity.is_empty() && c.nullable {
        return Err(AppError::msg("Identity columns must be not null"));
    }
    if !identity.is_empty() && c.default.is_some() {
        return Err(AppError::msg("Identity cannot have a default"));
    }
    Ok(format!(
        "{name} {}{}{}{}",
        c.data_type,
        identity,
        c.default
            .as_ref()
            .map(|d| format!(" DEFAULT ({d})"))
            .unwrap_or_default(),
        if c.nullable { "" } else { " NOT NULL" }
    ))
}
fn names(v: &[String]) -> AppResult<String> {
    if v.is_empty() {
        return Err(AppError::msg("At least one column is required"));
    }
    v.iter()
        .map(|s| ident(s))
        .collect::<AppResult<Vec<_>>>()
        .map(|x| x.join(", "))
}
fn action(a: &Option<String>) -> AppResult<String> {
    match a
        .as_deref()
        .unwrap_or("NO ACTION")
        .to_ascii_uppercase()
        .as_str()
    {
        "NO ACTION" => Ok("NO ACTION".into()),
        "RESTRICT" => Ok("RESTRICT".into()),
        "CASCADE" => Ok("CASCADE".into()),
        "SET NULL" => Ok("SET NULL".into()),
        "SET DEFAULT" => Ok("SET DEFAULT".into()),
        _ => Err(AppError::msg("Invalid foreign key action")),
    }
}
fn constraint(c: &ConstraintInput) -> AppResult<String> {
    let n = ident(&c.name)?;
    let body = match c.kind.as_str() {
        "primaryKey" => format!("PRIMARY KEY ({})", names(&c.columns)?),
        "unique" => format!("UNIQUE ({})", names(&c.columns)?),
        "foreignKey" => {
            let schema = c
                .ref_schema
                .as_deref()
                .ok_or_else(|| AppError::msg("Missing referenced schema"))?;
            let table = c
                .ref_table
                .as_deref()
                .ok_or_else(|| AppError::msg("Missing referenced table"))?;
            ident(schema)?;
            ident(table)?;
            if c.columns.len() != c.ref_columns.len() {
                return Err(AppError::msg("Foreign key column counts differ"));
            }
            format!(
                "FOREIGN KEY ({}) REFERENCES {} ({}) ON UPDATE {} ON DELETE {}",
                names(&c.columns)?,
                qualified(schema, table),
                names(&c.ref_columns)?,
                action(&c.on_update)?,
                action(&c.on_delete)?
            )
        }
        "check" => {
            let e = c
                .expression
                .as_deref()
                .ok_or_else(|| AppError::msg("Missing check expression"))?;
            fragment(e)?;
            format!("CHECK ({e})")
        }
        _ => return Err(AppError::msg("Unsupported constraint")),
    };
    Ok(format!("CONSTRAINT {n} {body}"))
}

async fn load(client: &Client, schema: &str, table: &str) -> AppResult<TableStructure> {
    let row = client
        .query_opt(r#"
            SELECT c.oid::int8, CASE WHEN c.relkind = 'r' AND EXISTS (SELECT 1
            FROM pg_inherits i
            WHERE i.inhparent=c.oid OR i.inhrelid=c.oid) THEN 'inherited' ELSE c.relkind::text END, obj_description(c.oid, 'pg_class')
            FROM pg_class c
            JOIN pg_namespace n ON n.oid=c.relnamespace
            WHERE n.nspname=$1 AND c.relname=$2 AND c.relkind IN ('r','p','v','m','f')
        "#, &[&schema, &table]).await?.ok_or_else(|| AppError::msg("Table no longer exists"))?;
    let oid: i64 = row.get(0);
    let kind: String = row.get(1);
    let column_rows = client
        .query(r#"
            SELECT a.attname, format_type(a.atttypid,a.atttypmod), NOT a.attnotnull, pg_get_expr(d.adbin,d.adrelid), a.attidentity::text, col_description(a.attrelid,a.attnum), a.attgenerated <> ''
            FROM pg_attribute a
            LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
            WHERE a.attrelid=$1::int8::oid AND a.attnum>0 AND NOT a.attisdropped
            ORDER BY a.attnum
        "#, &[&oid])
        .await?;
    let columns = column_rows
        .into_iter()
        .map(|r| {
            let identity: String = r.get(4);
            StructureColumn {
                name: r.get(0),
                data_type: r.get(1),
                nullable: r.get(2),
                default: r.get(3),
                identity: match identity.as_str() {
                    "a" => "always",
                    "d" => "byDefault",
                    _ => "",
                }
                .into(),
                comment: r.get(5),
                generated: r.get(6),
            }
        })
        .collect();
    let constraint_rows = client
        .query(r#"
            SELECT con.conname, con.contype::text, ARRAY(SELECT a.attname
            FROM unnest(con.conkey) WITH ORDINALITY k(n,i)
            JOIN pg_attribute a ON a.attrelid=con.conrelid AND a.attnum=k.n
            ORDER BY k.i), rn.nspname, rc.relname, ARRAY(SELECT a.attname
            FROM unnest(con.confkey) WITH ORDINALITY k(n,i)
            JOIN pg_attribute a ON a.attrelid=con.confrelid AND a.attnum=k.n
            ORDER BY k.i), con.confupdtype::text, con.confdeltype::text, CASE WHEN con.contype='c' THEN pg_get_expr(con.conbin,con.conrelid) END, pg_get_constraintdef(con.oid), (con.condeferrable OR NOT con.convalidated OR NOT con.conislocal OR (con.contype = 'f' AND con.confmatchtype <> 's') OR pg_get_constraintdef(con.oid) LIKE '%NULLS NOT DISTINCT%')
            FROM pg_constraint con
            LEFT JOIN pg_class rc ON rc.oid=con.confrelid
            LEFT JOIN pg_namespace rn ON rn.oid=rc.relnamespace
            WHERE con.conrelid=$1::int8::oid
            ORDER BY con.conname
        "#, &[&oid])
        .await?;
    let constraints = constraint_rows
        .into_iter()
        .map(|r| {
            let k: String = r.get(1);
            let update: String = r.get(6);
            let delete: String = r.get(7);
            let code = |s: &str| {
                match s {
                    "a" => "NO ACTION",
                    "r" => "RESTRICT",
                    "c" => "CASCADE",
                    "n" => "SET NULL",
                    "d" => "SET DEFAULT",
                    _ => "NO ACTION",
                }
                .to_string()
            };
            StructureConstraint {
                name: r.get(0),
                kind: if r.get::<_, bool>(10) {
                    "other"
                } else {
                    match k.as_str() {
                        "p" => "primaryKey",
                        "u" => "unique",
                        "f" => "foreignKey",
                        "c" => "check",
                        _ => "other",
                    }
                }
                .into(),
                columns: r.get(2),
                ref_schema: r.get(3),
                ref_table: r.get(4),
                ref_columns: r.get(5),
                on_update: if k == "f" { Some(code(&update)) } else { None },
                on_delete: if k == "f" { Some(code(&delete)) } else { None },
                expression: r.get(8),
                definition: r.get(9),
            }
        })
        .collect();
    let index_rows = client
        .query(r#"
            SELECT ic.relname, ARRAY(SELECT a.attname
            FROM unnest(i.indkey::smallint[]) WITH ORDINALITY k(n,ord)
            JOIN pg_attribute a ON a.attrelid=i.indrelid AND a.attnum=k.n
            WHERE k.ord <= i.indnkeyatts
            ORDER BY k.ord), i.indisunique, am.amname, pg_get_indexdef(i.indexrelid), (EXISTS (SELECT 1
            FROM pg_constraint c
            WHERE c.conindid=i.indexrelid) OR i.indexprs IS NOT NULL OR i.indpred IS NOT NULL OR i.indnatts <> i.indnkeyatts OR ic.reloptions IS NOT NULL OR ic.reltablespace <> 0 OR pg_get_indexdef(i.indexrelid) LIKE '%NULLS NOT DISTINCT%' OR EXISTS (SELECT 1
            FROM unnest(i.indcollation) WITH ORDINALITY co(oid,ord)
            JOIN pg_attribute a ON a.attrelid=i.indrelid AND a.attnum=i.indkey[(co.ord-1)::int]
            WHERE co.oid <> a.attcollation) OR EXISTS (SELECT 1
            FROM unnest(i.indoption) o
            WHERE o <> 0) OR EXISTS (SELECT 1
            FROM unnest(i.indclass) WITH ORDINALITY cl(oid,ord)
            JOIN pg_opclass op ON op.oid=cl.oid
            WHERE NOT op.opcdefault))
            FROM pg_index i
            JOIN pg_class ic ON ic.oid=i.indexrelid
            JOIN pg_am am ON am.oid=ic.relam
            WHERE i.indrelid=$1::int8::oid
            ORDER BY ic.relname
        "#, &[&oid]).await?;
    let indexes = index_rows
        .into_iter()
        .map(|r| StructureIndex {
            name: r.get(0),
            columns: r.get(1),
            unique: r.get(2),
            method: r.get(3),
            definition: r.get(4),
            managed: r.get(5),
        })
        .collect();
    Ok(TableStructure {
        schema: schema.into(),
        name: table.into(),
        comment: row.get(2),
        kind,
        columns,
        constraints,
        indexes,
    })
}

fn plan(req: &StructureRequest) -> AppResult<(Vec<String>, String)> {
    ident(&req.schema)?;
    let mut table = req.table.clone().unwrap_or_default();
    if req.table.is_some() {
        ident(&table)?;
    }
    let mut state = match (&req.table, &req.expected) {
        (Some(name), Some(s)) if s.schema == req.schema && s.name == *name => {
            if s.kind != "r" {
                return Err(AppError::msg("Only ordinary tables can be edited"));
            }
            Some(s.clone())
        }
        (None, None) => None,
        _ => return Err(AppError::msg("A matching table snapshot is required")),
    };
    let mut sql = Vec::new();
    for change in &req.changes {
        let q = qualified(&req.schema, &table);
        match change {
            StructureChange::CreateTable {
                name,
                columns,
                comment,
            } => {
                if state.is_some() || !table.is_empty() {
                    return Err(AppError::msg("Table already exists in plan"));
                }
                ident(name)?;
                if columns.is_empty() {
                    return Err(AppError::msg("At least one column is required"));
                }
                let mut seen = std::collections::HashSet::new();
                let defs = columns
                    .iter()
                    .map(|c| {
                        if !seen.insert(&c.name) {
                            return Err(AppError::msg("Duplicate column"));
                        }
                        column(c)
                    })
                    .collect::<AppResult<Vec<_>>>()?;
                table = name.clone();
                let q = qualified(&req.schema, &table);
                sql.push(format!("CREATE TABLE {q} ({})", defs.join(", ")));
                for c in columns {
                    if c.comment.is_some() {
                        sql.push(format!(
                            "COMMENT ON COLUMN {q}.{} IS {}",
                            ident(&c.name)?,
                            literal(&c.comment)
                        ));
                    }
                }
                if comment.is_some() {
                    sql.push(format!("COMMENT ON TABLE {q} IS {}", literal(comment)));
                }
                state = Some(TableStructure {
                    schema: req.schema.clone(),
                    name: table.clone(),
                    comment: comment.clone(),
                    kind: "r".into(),
                    columns: columns
                        .iter()
                        .map(|c| StructureColumn {
                            name: c.name.clone(),
                            data_type: c.data_type.clone(),
                            nullable: c.nullable,
                            default: c.default.clone(),
                            identity: c.identity.clone(),
                            comment: c.comment.clone(),
                            generated: false,
                        })
                        .collect(),
                    constraints: vec![],
                    indexes: vec![],
                });
            }
            _ if state.is_none() => return Err(AppError::msg("Create the table first")),
            StructureChange::RenameTable { name } => {
                ident(name)?;
                sql.push(format!("ALTER TABLE {q} RENAME TO {}", ident(name)?));
                table = name.clone();
                state.as_mut().unwrap().name = name.clone();
            }
            StructureChange::SetTableComment { comment } => {
                sql.push(format!("COMMENT ON TABLE {q} IS {}", literal(comment)))
            }
            StructureChange::AddColumn { column: c } => {
                let s = state.as_mut().unwrap();
                if s.columns.iter().any(|x| x.name == c.name) {
                    return Err(AppError::msg("Duplicate column"));
                }
                sql.push(format!("ALTER TABLE {q} ADD COLUMN {}", column(c)?));
                if c.comment.is_some() {
                    sql.push(format!(
                        "COMMENT ON COLUMN {q}.{} IS {}",
                        ident(&c.name)?,
                        literal(&c.comment)
                    ));
                }
                s.columns.push(StructureColumn {
                    name: c.name.clone(),
                    data_type: c.data_type.clone(),
                    nullable: c.nullable,
                    default: c.default.clone(),
                    identity: c.identity.clone(),
                    comment: c.comment.clone(),
                    generated: false,
                });
            }
            StructureChange::AlterColumn { name, column: c } => {
                column(c)?;
                let s = state.as_mut().unwrap();
                let pos = s
                    .columns
                    .iter()
                    .position(|x| x.name == *name)
                    .ok_or_else(|| AppError::msg("Column not found"))?;
                let old = s.columns[pos].clone();
                if old.generated {
                    return Err(AppError::msg("Generated columns cannot be edited"));
                }
                if c.name != *name && s.columns.iter().any(|x| x.name == c.name) {
                    return Err(AppError::msg("Duplicate column"));
                }
                let oldq = ident(name)?;
                if !old.identity.is_empty() && c.identity.is_empty() {
                    sql.push(format!("ALTER TABLE {q} ALTER COLUMN {oldq} DROP IDENTITY"));
                }
                if old.default != c.default && old.default.is_some() {
                    sql.push(format!("ALTER TABLE {q} ALTER COLUMN {oldq} DROP DEFAULT"));
                }
                if old.data_type != c.data_type {
                    sql.push(format!(
                        "ALTER TABLE {q} ALTER COLUMN {oldq} TYPE {}",
                        c.data_type
                    ));
                }
                if old.nullable && !c.nullable {
                    sql.push(format!("ALTER TABLE {q} ALTER COLUMN {oldq} SET NOT NULL"));
                }
                if old.identity != c.identity && !c.identity.is_empty() {
                    let mode = if c.identity == "always" {
                        "ALWAYS"
                    } else {
                        "BY DEFAULT"
                    };
                    if old.identity.is_empty() {
                        sql.push(format!(
                            "ALTER TABLE {q} ALTER COLUMN {oldq} ADD GENERATED {mode} AS IDENTITY"
                        ));
                    } else {
                        sql.push(format!(
                            "ALTER TABLE {q} ALTER COLUMN {oldq} SET GENERATED {mode}"
                        ));
                    }
                }
                if old.default != c.default
                    && let Some(d) = &c.default
                {
                    sql.push(format!(
                        "ALTER TABLE {q} ALTER COLUMN {oldq} SET DEFAULT ({d})"
                    ));
                }
                if !old.nullable && c.nullable {
                    sql.push(format!("ALTER TABLE {q} ALTER COLUMN {oldq} DROP NOT NULL"));
                }
                if c.name != *name {
                    sql.push(format!(
                        "ALTER TABLE {q} RENAME COLUMN {oldq} TO {}",
                        ident(&c.name)?
                    ));
                }
                if old.comment != c.comment {
                    sql.push(format!(
                        "COMMENT ON COLUMN {q}.{} IS {}",
                        ident(&c.name)?,
                        literal(&c.comment)
                    ));
                }
                s.columns[pos] = StructureColumn {
                    name: c.name.clone(),
                    data_type: c.data_type.clone(),
                    nullable: c.nullable,
                    default: c.default.clone(),
                    identity: c.identity.clone(),
                    comment: c.comment.clone(),
                    generated: false,
                };
            }
            StructureChange::DropColumn { name } => {
                let s = state.as_mut().unwrap();
                let pos = s
                    .columns
                    .iter()
                    .position(|x| x.name == *name)
                    .ok_or_else(|| AppError::msg("Column not found"))?;
                sql.push(format!("ALTER TABLE {q} DROP COLUMN {}", ident(name)?));
                s.columns.remove(pos);
            }
            StructureChange::AddConstraint { constraint: c } => {
                let s = state.as_mut().unwrap();
                if s.constraints.iter().any(|x| x.name == c.name) {
                    return Err(AppError::msg("Duplicate constraint"));
                }
                sql.push(format!("ALTER TABLE {q} ADD {}", constraint(c)?));
                s.constraints.push(StructureConstraint {
                    name: c.name.clone(),
                    kind: c.kind.clone(),
                    columns: c.columns.clone(),
                    ref_schema: c.ref_schema.clone(),
                    ref_table: c.ref_table.clone(),
                    ref_columns: c.ref_columns.clone(),
                    on_update: c.on_update.clone(),
                    on_delete: c.on_delete.clone(),
                    expression: c.expression.clone(),
                    definition: String::new(),
                });
            }
            StructureChange::DropConstraint { name } => {
                let s = state.as_mut().unwrap();
                let pos = s
                    .constraints
                    .iter()
                    .position(|x| x.name == *name)
                    .ok_or_else(|| AppError::msg("Constraint not found"))?;
                if s.constraints[pos].kind == "other" {
                    return Err(AppError::msg("This constraint has unsupported properties"));
                }
                sql.push(format!("ALTER TABLE {q} DROP CONSTRAINT {}", ident(name)?));
                s.constraints.remove(pos);
            }
            StructureChange::AddIndex { index: i } => {
                let s = state.as_mut().unwrap();
                if s.indexes.iter().any(|x| x.name == i.name) {
                    return Err(AppError::msg("Duplicate index"));
                }
                let method = match i.method.as_str() {
                    "btree" | "hash" | "gist" | "gin" | "spgist" | "brin" => &i.method,
                    _ => return Err(AppError::msg("Unsupported index method")),
                };
                sql.push(format!(
                    "CREATE {}INDEX {} ON {q} USING {} ({})",
                    if i.unique { "UNIQUE " } else { "" },
                    ident(&i.name)?,
                    ident(method)?,
                    names(&i.columns)?
                ));
                s.indexes.push(StructureIndex {
                    name: i.name.clone(),
                    columns: i.columns.clone(),
                    unique: i.unique,
                    method: i.method.clone(),
                    definition: String::new(),
                    managed: false,
                });
            }
            StructureChange::DropIndex { name } => {
                let s = state.as_mut().unwrap();
                let pos = s
                    .indexes
                    .iter()
                    .position(|x| x.name == *name)
                    .ok_or_else(|| AppError::msg("Index not found"))?;
                if s.indexes[pos].managed {
                    return Err(AppError::msg(
                        "This index is managed or uses an unsupported definition",
                    ));
                }
                ident(name)?;
                sql.push(format!("DROP INDEX {}", qualified(&req.schema, name)));
                s.indexes.remove(pos);
            }
        }
    }
    if state.is_none() {
        return Err(AppError::msg("Create the table first"));
    }
    Ok((sql, table))
}

#[tauri::command]
pub async fn table_structure(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
) -> AppResult<TableStructure> {
    ident(&schema)?;
    ident(&table)?;
    let conn = manager.meta(&id, &database).await?;
    load(&conn.client, &schema, &table).await
}
#[tauri::command]
pub fn preview_structure(request: StructureRequest) -> AppResult<Vec<String>> {
    plan(&request).map(|x| x.0)
}
#[tauri::command]
pub async fn apply_structure(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    request: StructureRequest,
) -> AppResult<String> {
    let read_only = manager.store.get(&id)?.read_only;
    if read_only {
        return Err(AppError::msg("Connection is read-only"));
    }
    let conn = manager.open(&id, &database).await?;
    apply(&conn.client, &request, read_only).await
}

async fn apply(client: &Client, request: &StructureRequest, read_only: bool) -> AppResult<String> {
    if read_only {
        return Err(AppError::msg("Connection is read-only"));
    }
    let (statements, table) = plan(request)?;
    client
        .batch_execute("BEGIN; SET LOCAL lock_timeout = '2s'")
        .await?;
    let result: AppResult<()> = async {
        if let Some(original) = &request.table {
            client
                .batch_execute(&format!(
                    "LOCK TABLE {} IN ACCESS EXCLUSIVE MODE",
                    qualified(&request.schema, original)
                ))
                .await?;
            let current = load(client, &request.schema, original).await?;
            if Some(&current) != request.expected.as_ref() {
                return Err(AppError::msg(
                    "Table structure changed; reload before applying",
                ));
            }
        }
        for statement in &statements {
            client.batch_execute(statement).await?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            client.batch_execute("COMMIT").await?;
            Ok(table)
        }
        Err(e) => {
            let _ = client.batch_execute("ROLLBACK").await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragments_reject_statement_injection() {
        for s in [
            "1); DROP TABLE x; --",
            "1 -- x",
            "'unterminated",
            "1 /* x */",
            "$$x$$",
        ] {
            assert!(fragment(s).is_err(), "{s}");
        }
        assert!(fragment("coalesce('a''b', now()::text)").is_ok());
    }
    #[test]
    fn names_are_quoted_and_types_reject_injection() {
        assert_eq!(
            ident("x\"; DROP TABLE t").unwrap(),
            "\"x\"\"; DROP TABLE t\""
        );
        assert!(data_type("text DEFAULT 1").is_err());
        assert!(data_type("int, injected text").is_err());
        assert!(fragment("'a\\'; DROP TABLE t; SELECT ''").is_err());
        assert!(data_type("numeric(10, 2)[]").is_ok());
    }
    #[test]
    fn rejects_without_snapshot() {
        let r = StructureRequest {
            schema: "public".into(),
            table: Some("t".into()),
            expected: None,
            changes: vec![],
        };
        assert!(plan(&r).is_err());
    }
    #[tokio::test]
    async fn metadata_and_plans_on_disposable_postgres() {
        let Ok(url) = std::env::var("GUCKPG_STRUCTURE_TEST_URL") else {
            return;
        };
        let config: tokio_postgres::Config = url.parse().expect("invalid test URL");
        assert!(config.get_hosts().iter().all(|h| matches!(h, tokio_postgres::config::Host::Tcp(host) if host == "localhost" || host == "127.0.0.1" || host == "::1")), "test database must be localhost");
        let (client, connection) = config.connect(tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(async move {
            connection.await.unwrap();
        });
        client.batch_execute("CREATE TEMP TABLE structure_test (id int PRIMARY KEY, value text DEFAULT 'x'); CREATE INDEX structure_test_value ON structure_test(value)").await.unwrap();
        let schema: String = client
            .query_one(
                "SELECT nspname FROM pg_namespace WHERE oid = pg_my_temp_schema()",
                &[],
            )
            .await
            .unwrap()
            .get(0);
        client.batch_execute("CREATE TEMP TABLE advanced (id int, CONSTRAINT advanced_unique UNIQUE (id) DEFERRABLE); CREATE INDEX advanced_index ON advanced (id) WITH (fillfactor=80); CREATE TEMP TABLE inherited_parent (id int); CREATE TEMP TABLE inherited_child () INHERITS (inherited_parent)").await.unwrap();
        let advanced = load(&client, &schema, "advanced").await.unwrap();
        assert_eq!(
            advanced
                .constraints
                .iter()
                .find(|c| c.name == "advanced_unique")
                .unwrap()
                .kind,
            "other"
        );
        assert!(
            advanced
                .indexes
                .iter()
                .find(|i| i.name == "advanced_index")
                .unwrap()
                .managed
        );
        assert_eq!(
            load(&client, &schema, "inherited_parent")
                .await
                .unwrap()
                .kind,
            "inherited"
        );
        assert_eq!(
            load(&client, &schema, "inherited_child")
                .await
                .unwrap()
                .kind,
            "inherited"
        );
        let col = |name: &str, data_type: &str, nullable: bool| ColumnInput {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            default: None,
            identity: String::new(),
            comment: None,
        };
        let con = |name: &str, kind: &str, columns: Vec<&str>| ConstraintInput {
            name: name.into(),
            kind: kind.into(),
            columns: columns.into_iter().map(str::to_string).collect(),
            ref_schema: None,
            ref_table: None,
            ref_columns: vec![],
            on_update: None,
            on_delete: None,
            expression: None,
        };
        let mut check = con("ck_value", "check", vec![]);
        check.expression = Some("value > 0".into());
        let mut foreign = con("fk_id", "foreignKey", vec!["id"]);
        foreign.ref_schema = Some(schema.clone());
        foreign.ref_table = Some("structure_test".into());
        foreign.ref_columns = vec!["id".into()];
        let index = IndexInput {
            name: "design_idx".into(),
            columns: vec!["value".into()],
            unique: false,
            method: "btree".into(),
        };
        let create = StructureRequest {
            schema: schema.clone(),
            table: None,
            expected: None,
            changes: vec![
                StructureChange::CreateTable {
                    name: "design_test".into(),
                    columns: vec![col("id", "integer", false), col("value", "integer", true)],
                    comment: Some("table comment".into()),
                },
                StructureChange::AddConstraint {
                    constraint: con("design_pk", "primaryKey", vec!["id"]),
                },
                StructureChange::AddConstraint {
                    constraint: con("design_unique", "unique", vec!["value"]),
                },
                StructureChange::AddConstraint { constraint: check },
                StructureChange::AddConstraint {
                    constraint: foreign,
                },
                StructureChange::AddIndex { index },
            ],
        };
        apply(&client, &create, false).await.unwrap();
        let design = load(&client, &schema, "design_test").await.unwrap();
        assert!(design.constraints.len() >= 4);
        let mut changed = col("renamed", "bigint", false);
        changed.default = Some("1".into());
        changed.comment = Some("column comment".into());
        let modify = StructureRequest {
            schema: schema.clone(),
            table: Some("design_test".into()),
            expected: Some(design),
            changes: vec![
                StructureChange::DropConstraint {
                    name: "design_unique".into(),
                },
                StructureChange::DropConstraint {
                    name: "ck_value".into(),
                },
                StructureChange::DropConstraint {
                    name: "fk_id".into(),
                },
                StructureChange::DropConstraint {
                    name: "design_pk".into(),
                },
                StructureChange::DropIndex {
                    name: "design_idx".into(),
                },
                StructureChange::AlterColumn {
                    name: "value".into(),
                    column: changed,
                },
                StructureChange::AddIndex {
                    index: IndexInput {
                        name: "design_idx".into(),
                        columns: vec!["renamed".into()],
                        unique: true,
                        method: "btree".into(),
                    },
                },
                StructureChange::AddColumn {
                    column: ColumnInput {
                        identity: "byDefault".into(),
                        ..col("serial_id", "integer", false)
                    },
                },
                StructureChange::DropColumn { name: "id".into() },
                StructureChange::RenameTable {
                    name: "design_done".into(),
                },
            ],
        };
        apply(&client, &modify, false).await.unwrap();
        let design = load(&client, &schema, "design_done").await.unwrap();
        assert_eq!(design.columns[0].name, "renamed");
        assert_eq!(design.columns[0].comment.as_deref(), Some("column comment"));
        assert_eq!(design.columns[1].identity, "byDefault");
        assert!(design.indexes[0].unique);
        let snapshot = load(&client, &schema, "structure_test").await.unwrap();
        assert_eq!(snapshot.columns.len(), 2);
        assert!(snapshot.constraints.iter().any(|c| c.kind == "primaryKey"));
        assert_eq!(snapshot.indexes.len(), 2);
        let request = StructureRequest {
            schema: schema.clone(),
            table: Some("structure_test".into()),
            expected: Some(snapshot.clone()),
            changes: vec![
                StructureChange::DropIndex {
                    name: "structure_test_value".into(),
                },
                StructureChange::RenameTable {
                    name: "structure_renamed".into(),
                },
            ],
        };
        assert!(apply(&client, &request, true).await.is_err());
        assert_eq!(
            apply(&client, &request, false).await.unwrap(),
            "structure_renamed"
        );
        assert!(apply(&client, &request, false).await.is_err());
        let snapshot = load(&client, &schema, "structure_renamed").await.unwrap();
        let failed = StructureRequest {
            schema: schema.clone(),
            table: Some("structure_renamed".into()),
            expected: Some(snapshot.clone()),
            changes: vec![
                StructureChange::SetTableComment {
                    comment: Some("rollback".into()),
                },
                StructureChange::AddColumn {
                    column: ColumnInput {
                        name: "bad".into(),
                        data_type: "no_such_type".into(),
                        nullable: true,
                        default: None,
                        identity: "".into(),
                        comment: None,
                    },
                },
            ],
        };
        assert!(apply(&client, &failed, false).await.is_err());
        assert_eq!(
            load(&client, &schema, "structure_renamed").await.unwrap(),
            snapshot
        );
        client
            .batch_execute("ALTER TABLE structure_renamed ADD COLUMN external_change int")
            .await
            .unwrap();
        assert!(apply(&client, &failed, false).await.is_err());
    }
}
