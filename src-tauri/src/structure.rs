use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_postgres::Client;

use crate::catalog::{CONSTRAINT_COLUMNS, REFERENCED_COLUMNS};
use crate::error::{AppError, AppResult};
use crate::session::Manager;
use crate::sql::{checked_ident, in_tx, qualified, quote_ident, quote_literal};

/// What `pg_class.relkind` says about the table, with ordinary tables that
/// take part in inheritance told apart: the designer cannot edit those.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TableKind {
    #[serde(rename = "r")]
    Ordinary,
    #[serde(rename = "inherited")]
    Inherited,
    #[serde(rename = "p")]
    Partitioned,
    #[serde(rename = "v")]
    View,
    #[serde(rename = "m")]
    Materialized,
    #[serde(rename = "f")]
    Foreign,
}

impl TableKind {
    fn from_catalog(kind: &str) -> AppResult<Self> {
        Ok(match kind {
            "r" => Self::Ordinary,
            "inherited" => Self::Inherited,
            "p" => Self::Partitioned,
            "v" => Self::View,
            "m" => Self::Materialized,
            "f" => Self::Foreign,
            _ => return Err(AppError::msg("Unsupported relation kind")),
        })
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Identity {
    #[default]
    #[serde(rename = "")]
    None,
    Always,
    ByDefault,
}

impl Identity {
    fn is_none(self) -> bool {
        self == Self::None
    }

    fn mode(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Always => Some("ALWAYS"),
            Self::ByDefault => Some("BY DEFAULT"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConstraintKind {
    PrimaryKey,
    Unique,
    ForeignKey,
    Check,
    /// Anything the designer cannot recreate faithfully (deferrable, not
    /// valid, exclusion, ...); shown but never dropped or added.
    Other,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum FkAction {
    #[default]
    #[serde(rename = "NO ACTION")]
    NoAction,
    #[serde(rename = "RESTRICT")]
    Restrict,
    #[serde(rename = "CASCADE")]
    Cascade,
    #[serde(rename = "SET NULL")]
    SetNull,
    #[serde(rename = "SET DEFAULT")]
    SetDefault,
}

impl FkAction {
    fn sql(self) -> &'static str {
        match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
        }
    }

    /// `pg_constraint.confupdtype` / `confdeltype`.
    fn from_code(code: &str) -> Self {
        match code {
            "r" => Self::Restrict,
            "c" => Self::Cascade,
            "n" => Self::SetNull,
            "d" => Self::SetDefault,
            _ => Self::NoAction,
        }
    }
}

/// The access methods the designer can create indexes with. Loaded indexes
/// keep their method as text, since any installed method can show up there.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IndexMethod {
    Btree,
    Hash,
    Gist,
    Gin,
    Spgist,
    Brin,
}

impl IndexMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Btree => "btree",
            Self::Hash => "hash",
            Self::Gist => "gist",
            Self::Gin => "gin",
            Self::Spgist => "spgist",
            Self::Brin => "brin",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableStructure {
    pub schema: String,
    pub name: String,
    pub comment: Option<String>,
    pub kind: TableKind,
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
    pub identity: Identity,
    pub comment: Option<String>,
    pub generated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructureConstraint {
    pub name: String,
    pub kind: ConstraintKind,
    pub columns: Vec<String>,
    pub ref_schema: Option<String>,
    pub ref_table: Option<String>,
    pub ref_columns: Vec<String>,
    pub on_update: Option<FkAction>,
    pub on_delete: Option<FkAction>,
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
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInput {
    name: String,
    data_type: String,
    nullable: bool,
    default: Option<String>,
    identity: Identity,
    comment: Option<String>,
    /// How existing values convert when `alterColumn` changes the type;
    /// ignored everywhere else.
    #[serde(default)]
    using: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintInput {
    name: String,
    kind: ConstraintKind,
    columns: Vec<String>,
    ref_schema: Option<String>,
    ref_table: Option<String>,
    ref_columns: Vec<String>,
    on_update: Option<FkAction>,
    on_delete: Option<FkAction>,
    expression: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInput {
    name: String,
    columns: Vec<String>,
    unique: bool,
    method: IndexMethod,
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

impl From<&ColumnInput> for StructureColumn {
    fn from(c: &ColumnInput) -> Self {
        StructureColumn {
            name: c.name.clone(),
            data_type: c.data_type.clone(),
            nullable: c.nullable,
            default: c.default.clone(),
            identity: c.identity,
            comment: c.comment.clone(),
            generated: false,
        }
    }
}

fn ident(s: &str) -> AppResult<String> {
    // Blank names are legal in Postgres but never what the designer meant.
    if s.trim().is_empty() {
        return Err(AppError::msg("Invalid identifier"));
    }
    checked_ident(s).map(quote_ident)
}
fn literal(s: &Option<String>) -> String {
    s.as_deref()
        .map(quote_literal)
        .unwrap_or_else(|| "NULL".into())
}
// Fragments must remain inside the surrounding expression; comments and dollar quotes are rejected.
// Backslashes are rejected because whether they escape a quote depends on
// server settings this scanner cannot see.
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
                ';' | '$' => return Err(AppError::msg("Invalid SQL expression")),
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

/// Words that would turn a type into a column constraint or option.
const TYPE_STOP_WORDS: &[&str] = &[
    "default",
    "constraint",
    "primary",
    "references",
    "unique",
    "check",
    "not",
    "null",
    "generated",
    "collate",
    "using",
    "include",
    "exclude",
    "deferrable",
    "initially",
];

/// A type name, possibly schema-qualified, with modifiers and array bounds.
/// Quoted identifiers may contain anything but NUL and backslashes.
fn data_type(s: &str) -> AppResult<()> {
    let invalid = || AppError::msg("Invalid data type");
    fragment(s)?;
    let mut depth = 0;
    let mut quoted = false;
    let mut unquoted = String::new();
    for c in s.chars() {
        if c == '"' {
            // A doubled quote inside an identifier toggles twice.
            quoted = !quoted;
            unquoted.push(' ');
            continue;
        }
        if quoted {
            continue;
        }
        if !(c.is_alphanumeric() || c.is_whitespace() || "_.,()[]".contains(c)) {
            return Err(invalid());
        }
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Err(invalid()),
            _ => {}
        }
        unquoted.push(c);
    }
    let lower = unquoted.to_ascii_lowercase();
    if lower
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| TYPE_STOP_WORDS.contains(&word))
    {
        return Err(invalid());
    }
    Ok(())
}

fn check_identity(c: &ColumnInput) -> AppResult<()> {
    if !c.identity.is_none() && c.nullable {
        return Err(AppError::msg("Identity columns must be not null"));
    }
    if !c.identity.is_none() && c.default.is_some() {
        return Err(AppError::msg("Identity cannot have a default"));
    }
    Ok(())
}

/// The column definition for CREATE TABLE / ADD COLUMN, named `name` (which
/// differs from `c.name` while a rename is pending).
fn column(c: &ColumnInput, name: &str) -> AppResult<String> {
    let name = ident(name)?;
    data_type(&c.data_type)?;
    if let Some(d) = &c.default {
        fragment(d)?;
    }
    check_identity(c)?;
    let identity = c
        .identity
        .mode()
        .map(|mode| format!(" GENERATED {mode} AS IDENTITY"))
        .unwrap_or_default();
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
fn constraint(c: &ConstraintInput) -> AppResult<String> {
    let n = ident(&c.name)?;
    let body = match c.kind {
        ConstraintKind::PrimaryKey => format!("PRIMARY KEY ({})", names(&c.columns)?),
        ConstraintKind::Unique => format!("UNIQUE ({})", names(&c.columns)?),
        ConstraintKind::ForeignKey => {
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
                c.on_update.unwrap_or_default().sql(),
                c.on_delete.unwrap_or_default().sql()
            )
        }
        ConstraintKind::Check => {
            let e = c
                .expression
                .as_deref()
                .ok_or_else(|| AppError::msg("Missing check expression"))?;
            fragment(e)?;
            format!("CHECK ({e})")
        }
        ConstraintKind::Other => return Err(AppError::msg("Unsupported constraint")),
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
    Ok(TableStructure {
        schema: schema.into(),
        name: table.into(),
        comment: row.get(2),
        kind: TableKind::from_catalog(row.get(1))?,
        columns: load_columns(client, oid).await?,
        constraints: load_constraints(client, oid).await?,
        indexes: load_indexes(client, oid).await?,
    })
}

async fn load_columns(client: &Client, oid: i64) -> AppResult<Vec<StructureColumn>> {
    let rows = client
        .query(r#"
            SELECT a.attname, format_type(a.atttypid,a.atttypmod), NOT a.attnotnull, pg_get_expr(d.adbin,d.adrelid), a.attidentity::text, col_description(a.attrelid,a.attnum), a.attgenerated <> ''
            FROM pg_attribute a
            LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
            WHERE a.attrelid=$1::int8::oid AND a.attnum>0 AND NOT a.attisdropped
            ORDER BY a.attnum
        "#, &[&oid])
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| StructureColumn {
            name: r.get(0),
            data_type: r.get(1),
            nullable: r.get(2),
            default: r.get(3),
            identity: match r.get::<_, &str>(4) {
                "a" => Identity::Always,
                "d" => Identity::ByDefault,
                _ => Identity::None,
            },
            comment: r.get(5),
            generated: r.get(6),
        })
        .collect())
}

async fn load_constraints(client: &Client, oid: i64) -> AppResult<Vec<StructureConstraint>> {
    let rows = client
        .query(&format!(r#"
            SELECT con.conname, con.contype::text, {CONSTRAINT_COLUMNS}, rn.nspname, rc.relname, {REFERENCED_COLUMNS}, con.confupdtype::text, con.confdeltype::text, CASE WHEN con.contype='c' THEN pg_get_expr(con.conbin,con.conrelid) END, pg_get_constraintdef(con.oid), (con.condeferrable OR NOT con.convalidated OR NOT con.conislocal OR (con.contype = 'f' AND con.confmatchtype <> 's') OR pg_get_constraintdef(con.oid) LIKE '%NULLS NOT DISTINCT%')
            FROM pg_constraint con
            LEFT JOIN pg_class rc ON rc.oid=con.confrelid
            LEFT JOIN pg_namespace rn ON rn.oid=rc.relnamespace
            WHERE con.conrelid=$1::int8::oid
            ORDER BY con.conname
        "#), &[&oid])
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let contype: &str = r.get(1);
            let foreign = contype == "f";
            let kind = if r.get::<_, bool>(10) {
                ConstraintKind::Other
            } else {
                match contype {
                    "p" => ConstraintKind::PrimaryKey,
                    "u" => ConstraintKind::Unique,
                    "f" => ConstraintKind::ForeignKey,
                    "c" => ConstraintKind::Check,
                    _ => ConstraintKind::Other,
                }
            };
            StructureConstraint {
                name: r.get(0),
                kind,
                columns: r.get(2),
                ref_schema: r.get(3),
                ref_table: r.get(4),
                ref_columns: r.get(5),
                on_update: foreign.then(|| FkAction::from_code(r.get(6))),
                on_delete: foreign.then(|| FkAction::from_code(r.get(7))),
                expression: r.get(8),
                definition: r.get(9),
            }
        })
        .collect())
}

async fn load_indexes(client: &Client, oid: i64) -> AppResult<Vec<StructureIndex>> {
    let rows = client
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
    Ok(rows
        .into_iter()
        .map(|r| StructureIndex {
            name: r.get(0),
            columns: r.get(1),
            unique: r.get(2),
            method: r.get(3),
            definition: r.get(4),
            managed: r.get(5),
        })
        .collect())
}

/// SQL being planned for one table, plus column renames that wait for their
/// target name to become free (swaps and chains like a→b, b→a).
#[derive(Default)]
struct Planner {
    sql: Vec<String>,
    /// (temporary name, final name)
    pending_renames: Vec<(String, String)>,
}

impl Planner {
    fn table_name(t: &TableStructure) -> String {
        qualified(&t.schema, &t.name)
    }

    fn rename_table(&mut self, t: &mut TableStructure, name: &str) -> AppResult<()> {
        self.sql.push(format!(
            "ALTER TABLE {} RENAME TO {}",
            Self::table_name(t),
            ident(name)?
        ));
        t.name = name.into();
        Ok(())
    }

    fn set_table_comment(&mut self, t: &TableStructure, comment: &Option<String>) {
        self.sql.push(format!(
            "COMMENT ON TABLE {} IS {}",
            Self::table_name(t),
            literal(comment)
        ));
    }

    /// The name `target` can be given right now: itself when free, a
    /// temporary name when a later change moves the current holder away.
    fn claim_column_name(
        &self,
        t: &TableStructure,
        target: &str,
        later: &[StructureChange],
    ) -> AppResult<String> {
        let taken = t.columns.iter().any(|x| x.name == target);
        let promised = self.pending_renames.iter().any(|(_, f)| f == target);
        if promised || (taken && !vacated_later(later, target)) {
            return Err(AppError::msg("Duplicate column"));
        }
        if !taken {
            return Ok(target.into());
        }
        let used = |name: &str| {
            t.columns.iter().any(|x| x.name == name)
                || later.iter().any(|change| match change {
                    StructureChange::AddColumn { column }
                    | StructureChange::AlterColumn { column, .. } => column.name == name,
                    _ => false,
                })
        };
        let temp = (0..)
            .map(|n| format!("guckpg_rename_{n}"))
            .find(|name| !used(name))
            .ok_or_else(|| AppError::msg("No temporary column name available"))?;
        Ok(temp)
    }

    fn defer_rename(&mut self, current: &str, target: &str) {
        if current != target {
            self.pending_renames.push((current.into(), target.into()));
        }
    }

    /// Finishes every pending rename whose target name is free by now.
    fn finish_renames(&mut self, t: &mut TableStructure) -> AppResult<()> {
        while let Some(i) = self
            .pending_renames
            .iter()
            .position(|(_, target)| !t.columns.iter().any(|c| c.name == *target))
        {
            let (temp, target) = self.pending_renames.remove(i);
            self.sql.push(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                Self::table_name(t),
                ident(&temp)?,
                ident(&target)?
            ));
            if let Some(c) = t.columns.iter_mut().find(|c| c.name == temp) {
                c.name = target;
            }
        }
        Ok(())
    }

    fn add_column(
        &mut self,
        t: &mut TableStructure,
        c: &ColumnInput,
        later: &[StructureChange],
    ) -> AppResult<()> {
        let name = self.claim_column_name(t, &c.name, later)?;
        let q = Self::table_name(t);
        self.sql
            .push(format!("ALTER TABLE {q} ADD COLUMN {}", column(c, &name)?));
        if c.comment.is_some() {
            self.sql.push(format!(
                "COMMENT ON COLUMN {q}.{} IS {}",
                ident(&name)?,
                literal(&c.comment)
            ));
        }
        t.columns.push(StructureColumn {
            name: name.clone(),
            ..StructureColumn::from(c)
        });
        self.defer_rename(&name, &c.name);
        Ok(())
    }

    fn alter_column(
        &mut self,
        t: &mut TableStructure,
        name: &str,
        c: &ColumnInput,
        later: &[StructureChange],
    ) -> AppResult<()> {
        let pos = t
            .columns
            .iter()
            .position(|x| x.name == name)
            .ok_or_else(|| AppError::msg("Column not found"))?;
        let old = &t.columns[pos];
        if old.generated {
            return Err(AppError::msg("Generated columns cannot be edited"));
        }
        let new_name = if c.name == name {
            ident(name)?;
            name.to_string()
        } else {
            ident(&c.name)?;
            self.claim_column_name(t, &c.name, later)?
        };
        let statements = alter_column_sql(&Self::table_name(t), &t.columns[pos], c, &new_name)?;
        self.sql.extend(statements);
        t.columns[pos] = StructureColumn {
            name: new_name.clone(),
            ..StructureColumn::from(c)
        };
        self.defer_rename(&new_name, &c.name);
        Ok(())
    }

    fn drop_column(&mut self, t: &mut TableStructure, name: &str) -> AppResult<()> {
        let pos = t
            .columns
            .iter()
            .position(|x| x.name == name)
            .ok_or_else(|| AppError::msg("Column not found"))?;
        self.sql.push(format!(
            "ALTER TABLE {} DROP COLUMN {}",
            Self::table_name(t),
            ident(name)?
        ));
        t.columns.remove(pos);
        Ok(())
    }

    fn add_constraint(&mut self, t: &mut TableStructure, c: &ConstraintInput) -> AppResult<()> {
        if t.constraints.iter().any(|x| x.name == c.name) {
            return Err(AppError::msg("Duplicate constraint"));
        }
        self.sql.push(format!(
            "ALTER TABLE {} ADD {}",
            Self::table_name(t),
            constraint(c)?
        ));
        t.constraints.push(StructureConstraint {
            name: c.name.clone(),
            kind: c.kind,
            columns: c.columns.clone(),
            ref_schema: c.ref_schema.clone(),
            ref_table: c.ref_table.clone(),
            ref_columns: c.ref_columns.clone(),
            on_update: c.on_update,
            on_delete: c.on_delete,
            expression: c.expression.clone(),
            definition: String::new(),
        });
        Ok(())
    }

    fn drop_constraint(&mut self, t: &mut TableStructure, name: &str) -> AppResult<()> {
        let pos = t
            .constraints
            .iter()
            .position(|x| x.name == name)
            .ok_or_else(|| AppError::msg("Constraint not found"))?;
        if t.constraints[pos].kind == ConstraintKind::Other {
            return Err(AppError::msg("This constraint has unsupported properties"));
        }
        self.sql.push(format!(
            "ALTER TABLE {} DROP CONSTRAINT {}",
            Self::table_name(t),
            ident(name)?
        ));
        t.constraints.remove(pos);
        Ok(())
    }

    fn add_index(&mut self, t: &mut TableStructure, i: &IndexInput) -> AppResult<()> {
        if t.indexes.iter().any(|x| x.name == i.name) {
            return Err(AppError::msg("Duplicate index"));
        }
        self.sql.push(format!(
            "CREATE {}INDEX {} ON {} USING {} ({})",
            if i.unique { "UNIQUE " } else { "" },
            ident(&i.name)?,
            Self::table_name(t),
            i.method.as_str(),
            names(&i.columns)?
        ));
        t.indexes.push(StructureIndex {
            name: i.name.clone(),
            columns: i.columns.clone(),
            unique: i.unique,
            method: i.method.as_str().into(),
            definition: String::new(),
            managed: false,
        });
        Ok(())
    }

    fn drop_index(&mut self, t: &mut TableStructure, name: &str) -> AppResult<()> {
        let pos = t
            .indexes
            .iter()
            .position(|x| x.name == name)
            .ok_or_else(|| AppError::msg("Index not found"))?;
        if t.indexes[pos].managed {
            return Err(AppError::msg(
                "This index is managed or uses an unsupported definition",
            ));
        }
        ident(name)?;
        self.sql
            .push(format!("DROP INDEX {}", qualified(&t.schema, name)));
        t.indexes.remove(pos);
        Ok(())
    }

    fn apply(
        &mut self,
        t: &mut TableStructure,
        change: &StructureChange,
        later: &[StructureChange],
    ) -> AppResult<()> {
        match change {
            StructureChange::CreateTable { .. } => {
                Err(AppError::msg("Table already exists in plan"))
            }
            StructureChange::RenameTable { name } => self.rename_table(t, name),
            StructureChange::SetTableComment { comment } => {
                self.set_table_comment(t, comment);
                Ok(())
            }
            StructureChange::AddColumn { column } => self.add_column(t, column, later),
            StructureChange::AlterColumn { name, column } => {
                self.alter_column(t, name, column, later)
            }
            StructureChange::DropColumn { name } => self.drop_column(t, name),
            StructureChange::AddConstraint { constraint } => self.add_constraint(t, constraint),
            StructureChange::DropConstraint { name } => self.drop_constraint(t, name),
            StructureChange::AddIndex { index } => self.add_index(t, index),
            StructureChange::DropIndex { name } => self.drop_index(t, name),
        }
    }
}

/// Whether a later change frees `name` by renaming or dropping its column.
fn vacated_later(later: &[StructureChange], name: &str) -> bool {
    later.iter().any(|change| match change {
        StructureChange::AlterColumn { name: from, column } => from == name && column.name != name,
        StructureChange::DropColumn { name: dropped } => dropped == name,
        _ => false,
    })
}

/// The statements turning `old` into `c`, renaming it to `new_name` (the
/// final name, or a temporary one while a swap is in progress). Only the
/// parts that change are validated, so a column whose existing default or
/// type the validators would reject can still get e.g. a new comment.
fn alter_column_sql(
    q: &str,
    old: &StructureColumn,
    c: &ColumnInput,
    new_name: &str,
) -> AppResult<Vec<String>> {
    let type_changed = old.data_type != c.data_type;
    let default_changed = old.default != c.default;
    if type_changed {
        data_type(&c.data_type)?;
    }
    if default_changed && let Some(d) = &c.default {
        fragment(d)?;
    }
    let using = c.using.as_deref().filter(|_| type_changed);
    if let Some(u) = using {
        fragment(u)?;
    }
    check_identity(c)?;

    let col = ident(&old.name)?;
    let alter = |action: String| format!("ALTER TABLE {q} ALTER COLUMN {col} {action}");
    let mut sql = Vec::new();
    // Identity and default go first, and come back after the type change,
    // so neither has to be valid for both the old and the new type.
    if !old.identity.is_none() && c.identity.is_none() {
        sql.push(alter("DROP IDENTITY".into()));
    }
    if default_changed && old.default.is_some() {
        sql.push(alter("DROP DEFAULT".into()));
    }
    if type_changed {
        sql.push(alter(match using {
            Some(u) => format!("TYPE {} USING ({u})", c.data_type),
            None => format!("TYPE {}", c.data_type),
        }));
    }
    if old.nullable && !c.nullable {
        sql.push(alter("SET NOT NULL".into()));
    }
    if old.identity != c.identity
        && let Some(mode) = c.identity.mode()
    {
        sql.push(alter(if old.identity.is_none() {
            format!("ADD GENERATED {mode} AS IDENTITY")
        } else {
            format!("SET GENERATED {mode}")
        }));
    }
    if default_changed && let Some(d) = &c.default {
        sql.push(alter(format!("SET DEFAULT ({d})")));
    }
    if !old.nullable && c.nullable {
        sql.push(alter("DROP NOT NULL".into()));
    }
    let new_col = ident(new_name)?;
    if new_name != old.name {
        sql.push(format!("ALTER TABLE {q} RENAME COLUMN {col} TO {new_col}"));
    }
    if old.comment != c.comment {
        sql.push(format!(
            "COMMENT ON COLUMN {q}.{new_col} IS {}",
            literal(&c.comment)
        ));
    }
    Ok(sql)
}

fn create_table(
    schema: &str,
    name: &str,
    columns: &[ColumnInput],
    comment: &Option<String>,
    sql: &mut Vec<String>,
) -> AppResult<TableStructure> {
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
            column(c, &c.name)
        })
        .collect::<AppResult<Vec<_>>>()?;
    let q = qualified(schema, name);
    sql.push(format!("CREATE TABLE {q} ({})", defs.join(", ")));
    for c in columns.iter().filter(|c| c.comment.is_some()) {
        sql.push(format!(
            "COMMENT ON COLUMN {q}.{} IS {}",
            ident(&c.name)?,
            literal(&c.comment)
        ));
    }
    if comment.is_some() {
        sql.push(format!("COMMENT ON TABLE {q} IS {}", literal(comment)));
    }
    Ok(TableStructure {
        schema: schema.into(),
        name: name.into(),
        comment: comment.clone(),
        kind: TableKind::Ordinary,
        columns: columns.iter().map(StructureColumn::from).collect(),
        constraints: vec![],
        indexes: vec![],
    })
}

/// The table as the request expects to find it, or None for a new table.
fn snapshot(req: &StructureRequest) -> AppResult<Option<TableStructure>> {
    match (&req.table, &req.expected) {
        (Some(name), Some(s)) if s.schema == req.schema && s.name == *name => {
            ident(name)?;
            if s.kind != TableKind::Ordinary {
                return Err(AppError::msg("Only ordinary tables can be edited"));
            }
            Ok(Some(s.clone()))
        }
        (None, None) => Ok(None),
        _ => Err(AppError::msg("A matching table snapshot is required")),
    }
}

fn plan(req: &StructureRequest) -> AppResult<(Vec<String>, String)> {
    ident(&req.schema)?;
    let mut state = snapshot(req)?;
    let mut planner = Planner::default();
    for (i, change) in req.changes.iter().enumerate() {
        let Some(table) = state.as_mut() else {
            let StructureChange::CreateTable {
                name,
                columns,
                comment,
            } = change
            else {
                return Err(AppError::msg("Create the table first"));
            };
            state = Some(create_table(
                &req.schema,
                name,
                columns,
                comment,
                &mut planner.sql,
            )?);
            continue;
        };
        planner.apply(table, change, &req.changes[i + 1..])?;
        planner.finish_renames(table)?;
    }
    if !planner.pending_renames.is_empty() {
        return Err(AppError::msg("Duplicate column"));
    }
    let table = state.ok_or_else(|| AppError::msg("Create the table first"))?;
    Ok((planner.sql, table.name))
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
    manager.ensure_writable(&id)?;
    let conn = manager.open(&id, &database).await?;
    apply(&conn.client, &request).await
}

/// Fails fast on locks like `BEGIN_GUARDED`, but without its statement
/// timeout: rewriting or indexing a large table may legitimately take longer.
const BEGIN_STRUCTURE: &str = "BEGIN; SET LOCAL lock_timeout = '2s'";

async fn apply(client: &Client, request: &StructureRequest) -> AppResult<String> {
    let (statements, table) = plan(request)?;
    in_tx(client, BEGIN_STRUCTURE, || async {
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
        Ok(table)
    })
    .await
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

    fn input(name: &str, data_type: &str) -> ColumnInput {
        ColumnInput {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            ..Default::default()
        }
    }
    fn existing(columns: &[ColumnInput]) -> TableStructure {
        TableStructure {
            schema: "s".into(),
            name: "t".into(),
            comment: None,
            kind: TableKind::Ordinary,
            columns: columns.iter().map(StructureColumn::from).collect(),
            constraints: vec![],
            indexes: vec![],
        }
    }
    fn plan_on(table: TableStructure, changes: Vec<StructureChange>) -> AppResult<Vec<String>> {
        plan(&StructureRequest {
            schema: table.schema.clone(),
            table: Some(table.name.clone()),
            expected: Some(table),
            changes,
        })
        .map(|(sql, _)| sql)
    }
    fn alter(name: &str, column: ColumnInput) -> StructureChange {
        StructureChange::AlterColumn {
            name: name.into(),
            column,
        }
    }

    #[test]
    fn creates_tables_with_comments() {
        let mut id = input("id", "integer");
        id.nullable = false;
        id.identity = Identity::ByDefault;
        let mut name = input("name", "text");
        name.default = Some("'x'".into());
        name.comment = Some("it's".into());
        let (sql, table) = plan(&StructureRequest {
            schema: "s".into(),
            table: None,
            expected: None,
            changes: vec![StructureChange::CreateTable {
                name: "t".into(),
                columns: vec![id, name],
                comment: Some("table".into()),
            }],
        })
        .unwrap();
        assert_eq!(table, "t");
        assert_eq!(
            sql,
            [
                "CREATE TABLE \"s\".\"t\" (\"id\" integer GENERATED BY DEFAULT AS IDENTITY NOT NULL, \"name\" text DEFAULT ('x'))",
                "COMMENT ON COLUMN \"s\".\"t\".\"name\" IS 'it''s'",
                "COMMENT ON TABLE \"s\".\"t\" IS 'table'",
            ]
        );
    }

    #[test]
    fn alter_column_orders_every_part_of_the_diff() {
        let mut old = input("v", "text");
        old.default = Some("'x'::text".into());
        let mut new = input("w", "integer");
        new.nullable = false;
        new.default = Some("0".into());
        new.comment = Some("count".into());
        new.using = Some("v::integer".into());
        let sql = plan_on(existing(&[old]), vec![alter("v", new)]).unwrap();
        let q = "ALTER TABLE \"s\".\"t\" ALTER COLUMN \"v\"";
        assert_eq!(
            sql,
            [
                format!("{q} DROP DEFAULT"),
                format!("{q} TYPE integer USING (v::integer)"),
                format!("{q} SET NOT NULL"),
                format!("{q} SET DEFAULT (0)"),
                "ALTER TABLE \"s\".\"t\" RENAME COLUMN \"v\" TO \"w\"".into(),
                "COMMENT ON COLUMN \"s\".\"t\".\"w\" IS 'count'".into(),
            ]
        );
    }

    #[test]
    fn alter_column_switches_identity_around_defaults_and_nullability() {
        let q = "ALTER TABLE \"s\".\"t\" ALTER COLUMN \"id\"";
        let mut plain = input("id", "integer");
        plain.default = Some("1".into());
        let mut always = input("id", "integer");
        always.nullable = false;
        always.identity = Identity::Always;
        assert_eq!(
            plan_on(
                existing(std::slice::from_ref(&plain)),
                vec![alter("id", always.clone())]
            )
            .unwrap(),
            [
                format!("{q} DROP DEFAULT"),
                format!("{q} SET NOT NULL"),
                format!("{q} ADD GENERATED ALWAYS AS IDENTITY"),
            ]
        );
        let by_default = ColumnInput {
            identity: Identity::ByDefault,
            ..always.clone()
        };
        assert_eq!(
            plan_on(
                existing(std::slice::from_ref(&always)),
                vec![alter("id", by_default)]
            )
            .unwrap(),
            [format!("{q} SET GENERATED BY DEFAULT")]
        );
        assert_eq!(
            plan_on(existing(&[always]), vec![alter("id", plain)]).unwrap(),
            [
                format!("{q} DROP IDENTITY"),
                format!("{q} SET DEFAULT (1)"),
                format!("{q} DROP NOT NULL"),
            ]
        );
    }

    #[test]
    fn alter_column_validates_only_what_changes() {
        let mut old = input("path", "\"my-domain\"");
        old.default = Some("'C:\\tmp'::text".into());
        let commented = ColumnInput {
            comment: Some("where".into()),
            ..old.clone()
        };
        assert_eq!(
            plan_on(
                existing(std::slice::from_ref(&old)),
                vec![alter("path", commented)]
            )
            .unwrap(),
            ["COMMENT ON COLUMN \"s\".\"t\".\"path\" IS 'where'"]
        );
        let new_default = ColumnInput {
            default: Some("'D:\\tmp'".into()),
            ..old.clone()
        };
        assert!(
            plan_on(
                existing(std::slice::from_ref(&old)),
                vec![alter("path", new_default)]
            )
            .is_err()
        );
        let new_type = ColumnInput {
            data_type: "text DEFAULT 1".into(),
            ..old.clone()
        };
        assert!(plan_on(existing(&[old]), vec![alter("path", new_type)]).is_err());
    }

    #[test]
    fn using_applies_only_to_type_changes_and_is_validated() {
        let old = input("v", "text");
        let same_type = ColumnInput {
            using: Some("v::integer".into()),
            comment: Some("c".into()),
            ..old.clone()
        };
        assert_eq!(
            plan_on(
                existing(std::slice::from_ref(&old)),
                vec![alter("v", same_type)]
            )
            .unwrap(),
            ["COMMENT ON COLUMN \"s\".\"t\".\"v\" IS 'c'"]
        );
        let new_type = input("v", "integer");
        assert_eq!(
            plan_on(
                existing(std::slice::from_ref(&old)),
                vec![alter("v", new_type)]
            )
            .unwrap(),
            ["ALTER TABLE \"s\".\"t\" ALTER COLUMN \"v\" TYPE integer"]
        );
        let injected = ColumnInput {
            using: Some("1); DROP TABLE x; --".into()),
            ..input("v", "integer")
        };
        assert!(plan_on(existing(&[old]), vec![alter("v", injected)]).is_err());
        let json: ColumnInput = serde_json::from_value(serde_json::json!({
            "name": "v", "dataType": "text", "nullable": true, "default": null,
            "identity": "", "comment": null,
        }))
        .unwrap();
        assert_eq!(json.using, None);
    }

    #[test]
    fn data_types_allow_quoted_identifiers() {
        for ok in [
            "\"my-domain\"",
            "public.\"my-domain\"[]",
            "\"not null\"",
            "\"a\"\"b\"",
            "timestamp with time zone",
            "character varying(20)",
        ] {
            assert!(data_type(ok).is_ok(), "{ok}");
        }
        for bad in [
            "text DEFAULT(1)",
            "\"x\" DEFAULT 1",
            "\"x\"NOT NULL",
            "int-4",
            "\"open",
            "text COLLATE \"C\"",
        ] {
            assert!(data_type(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn column_swaps_go_through_a_temporary_name() {
        let table = existing(&[input("a", "text"), input("b", "text")]);
        let r = "ALTER TABLE \"s\".\"t\" RENAME COLUMN";
        assert_eq!(
            plan_on(
                table.clone(),
                vec![
                    alter("a", input("b", "text")),
                    alter("b", input("a", "text"))
                ]
            )
            .unwrap(),
            [
                format!("{r} \"a\" TO \"guckpg_rename_0\""),
                format!("{r} \"b\" TO \"a\""),
                format!("{r} \"guckpg_rename_0\" TO \"b\""),
            ]
        );
        // a→b, b→c in either order.
        assert_eq!(
            plan_on(
                table.clone(),
                vec![
                    alter("a", input("b", "text")),
                    alter("b", input("c", "text"))
                ]
            )
            .unwrap(),
            [
                format!("{r} \"a\" TO \"guckpg_rename_0\""),
                format!("{r} \"b\" TO \"c\""),
                format!("{r} \"guckpg_rename_0\" TO \"b\""),
            ]
        );
        assert_eq!(
            plan_on(
                table.clone(),
                vec![
                    alter("b", input("c", "text")),
                    alter("a", input("b", "text"))
                ]
            )
            .unwrap(),
            [format!("{r} \"b\" TO \"c\""), format!("{r} \"a\" TO \"b\"")]
        );
    }

    #[test]
    fn renamed_away_names_can_be_reused_by_new_columns() {
        let table = existing(&[input("b", "text")]);
        let sql = plan_on(
            table,
            vec![
                StructureChange::AddColumn {
                    column: input("b", "integer"),
                },
                alter("b", input("c", "text")),
            ],
        )
        .unwrap();
        assert_eq!(
            sql,
            [
                "ALTER TABLE \"s\".\"t\" ADD COLUMN \"guckpg_rename_0\" integer",
                "ALTER TABLE \"s\".\"t\" RENAME COLUMN \"b\" TO \"c\"",
                "ALTER TABLE \"s\".\"t\" RENAME COLUMN \"guckpg_rename_0\" TO \"b\"",
            ]
        );
    }

    #[test]
    fn real_duplicates_are_still_rejected() {
        let table = existing(&[input("a", "text"), input("b", "text"), input("c", "text")]);
        assert!(plan_on(table.clone(), vec![alter("a", input("b", "text"))]).is_err());
        assert!(
            plan_on(
                table.clone(),
                vec![
                    alter("a", input("x", "text")),
                    alter("b", input("x", "text"))
                ]
            )
            .is_err()
        );
        // Both want b, which is only vacated once.
        assert!(
            plan_on(
                table.clone(),
                vec![
                    alter("a", input("b", "text")),
                    alter("c", input("b", "text")),
                    alter("b", input("y", "text")),
                ]
            )
            .is_err()
        );
        assert!(
            plan_on(
                table,
                vec![StructureChange::AddColumn {
                    column: input("c", "text")
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn constraints_indexes_and_table_renames() {
        let mut table = existing(&[input("id", "integer"), input("v", "text")]);
        table.constraints.push(StructureConstraint {
            name: "odd".into(),
            kind: ConstraintKind::Other,
            columns: vec![],
            ref_schema: None,
            ref_table: None,
            ref_columns: vec![],
            on_update: None,
            on_delete: None,
            expression: None,
            definition: String::new(),
        });
        table.indexes.push(StructureIndex {
            name: "t_pkey".into(),
            columns: vec!["id".into()],
            unique: true,
            method: "btree".into(),
            definition: String::new(),
            managed: true,
        });
        table.indexes.push(StructureIndex {
            managed: false,
            name: "t_v".into(),
            ..table.indexes[0].clone()
        });
        let fk = ConstraintInput {
            name: "t_fk".into(),
            kind: ConstraintKind::ForeignKey,
            columns: vec!["id".into()],
            ref_schema: Some("s".into()),
            ref_table: Some("p".into()),
            ref_columns: vec!["id".into()],
            on_update: None,
            on_delete: Some(FkAction::Cascade),
            expression: None,
        };
        let check = ConstraintInput {
            name: "t_check".into(),
            kind: ConstraintKind::Check,
            columns: vec![],
            ref_schema: None,
            ref_table: None,
            ref_columns: vec![],
            on_update: None,
            on_delete: None,
            expression: Some("id > 0".into()),
        };
        let sql = plan_on(
            table.clone(),
            vec![
                StructureChange::RenameTable { name: "u".into() },
                StructureChange::SetTableComment { comment: None },
                StructureChange::DropIndex { name: "t_v".into() },
                StructureChange::AddConstraint { constraint: fk },
                StructureChange::AddConstraint { constraint: check },
                StructureChange::AddIndex {
                    index: IndexInput {
                        name: "u_v".into(),
                        columns: vec!["v".into()],
                        unique: true,
                        method: IndexMethod::Gin,
                    },
                },
                StructureChange::DropColumn { name: "v".into() },
            ],
        )
        .unwrap();
        assert_eq!(
            sql,
            [
                "ALTER TABLE \"s\".\"t\" RENAME TO \"u\"",
                "COMMENT ON TABLE \"s\".\"u\" IS NULL",
                "DROP INDEX \"s\".\"t_v\"",
                "ALTER TABLE \"s\".\"u\" ADD CONSTRAINT \"t_fk\" FOREIGN KEY (\"id\") REFERENCES \"s\".\"p\" (\"id\") ON UPDATE NO ACTION ON DELETE CASCADE",
                "ALTER TABLE \"s\".\"u\" ADD CONSTRAINT \"t_check\" CHECK (id > 0)",
                "CREATE UNIQUE INDEX \"u_v\" ON \"s\".\"u\" USING gin (\"v\")",
                "ALTER TABLE \"s\".\"u\" DROP COLUMN \"v\"",
            ]
        );
        for refused in [
            StructureChange::DropConstraint { name: "odd".into() },
            StructureChange::DropIndex {
                name: "t_pkey".into(),
            },
            StructureChange::DropColumn {
                name: "missing".into(),
            },
            StructureChange::CreateTable {
                name: "again".into(),
                columns: vec![input("x", "text")],
                comment: None,
            },
        ] {
            assert!(plan_on(table.clone(), vec![refused]).is_err());
        }
        let mut inherited = table;
        inherited.kind = TableKind::Inherited;
        assert!(plan_on(inherited, vec![]).is_err());
    }

    #[test]
    fn plans_need_a_table() {
        let r = StructureRequest {
            schema: "s".into(),
            table: None,
            expected: None,
            changes: vec![StructureChange::DropColumn { name: "x".into() }],
        };
        assert!(plan(&r).is_err());
    }

    #[test]
    fn wire_format_is_unchanged() {
        let table = TableStructure {
            schema: "s".into(),
            name: "t".into(),
            comment: None,
            kind: TableKind::Ordinary,
            columns: vec![StructureColumn {
                identity: Identity::ByDefault,
                ..StructureColumn::from(&input("id", "integer"))
            }],
            constraints: vec![StructureConstraint {
                name: "fk".into(),
                kind: ConstraintKind::ForeignKey,
                columns: vec!["id".into()],
                ref_schema: Some("s".into()),
                ref_table: Some("p".into()),
                ref_columns: vec!["id".into()],
                on_update: Some(FkAction::NoAction),
                on_delete: Some(FkAction::SetNull),
                expression: None,
                definition: "FOREIGN KEY".into(),
            }],
            indexes: vec![],
        };
        let json = serde_json::to_value(&table).unwrap();
        assert_eq!(json["kind"], "r");
        assert_eq!(json["columns"][0]["identity"], "byDefault");
        assert_eq!(json["constraints"][0]["kind"], "foreignKey");
        assert_eq!(json["constraints"][0]["onUpdate"], "NO ACTION");
        assert_eq!(json["constraints"][0]["onDelete"], "SET NULL");
        assert_eq!(
            serde_json::from_value::<TableStructure>(json).unwrap(),
            table
        );
        for (kind, wire) in [
            (TableKind::Inherited, "inherited"),
            (TableKind::Partitioned, "p"),
            (TableKind::View, "v"),
            (TableKind::Materialized, "m"),
            (TableKind::Foreign, "f"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), wire);
            assert_eq!(TableKind::from_catalog(wire).unwrap(), kind);
        }
        assert_eq!(serde_json::to_value(Identity::None).unwrap(), "");
        assert_eq!(serde_json::to_value(IndexMethod::Spgist).unwrap(), "spgist");
    }

    #[tokio::test]
    async fn metadata_and_plans_on_disposable_postgres() {
        let Some((cfg, password)) = crate::testutil::test_db("structure") else {
            return;
        };
        let conn = crate::pg::PgConn::connect(
            &cfg,
            &cfg.database,
            Some(&password),
            crate::pg::Transport::Direct,
        )
        .await
        .unwrap();
        let client = &conn.client;
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
        let advanced = load(client, &schema, "advanced").await.unwrap();
        assert_eq!(
            advanced
                .constraints
                .iter()
                .find(|c| c.name == "advanced_unique")
                .unwrap()
                .kind,
            ConstraintKind::Other
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
            load(client, &schema, "inherited_parent")
                .await
                .unwrap()
                .kind,
            TableKind::Inherited
        );
        assert_eq!(
            load(client, &schema, "inherited_child").await.unwrap().kind,
            TableKind::Inherited
        );
        let col = |name: &str, data_type: &str, nullable: bool| ColumnInput {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            default: None,
            identity: Identity::None,
            comment: None,
            using: None,
        };
        let con = |name: &str, kind: ConstraintKind, columns: Vec<&str>| ConstraintInput {
            name: name.into(),
            kind,
            columns: columns.into_iter().map(str::to_string).collect(),
            ref_schema: None,
            ref_table: None,
            ref_columns: vec![],
            on_update: None,
            on_delete: None,
            expression: None,
        };
        let mut check = con("ck_value", ConstraintKind::Check, vec![]);
        check.expression = Some("value > 0".into());
        let mut foreign = con("fk_id", ConstraintKind::ForeignKey, vec!["id"]);
        foreign.ref_schema = Some(schema.clone());
        foreign.ref_table = Some("structure_test".into());
        foreign.ref_columns = vec!["id".into()];
        let index = IndexInput {
            name: "design_idx".into(),
            columns: vec!["value".into()],
            unique: false,
            method: IndexMethod::Btree,
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
                    constraint: con("design_pk", ConstraintKind::PrimaryKey, vec!["id"]),
                },
                StructureChange::AddConstraint {
                    constraint: con("design_unique", ConstraintKind::Unique, vec!["value"]),
                },
                StructureChange::AddConstraint { constraint: check },
                StructureChange::AddConstraint {
                    constraint: foreign,
                },
                StructureChange::AddIndex { index },
            ],
        };
        apply(client, &create).await.unwrap();
        let design = load(client, &schema, "design_test").await.unwrap();
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
                        method: IndexMethod::Btree,
                    },
                },
                StructureChange::AddColumn {
                    column: ColumnInput {
                        identity: Identity::ByDefault,
                        ..col("serial_id", "integer", false)
                    },
                },
                StructureChange::DropColumn { name: "id".into() },
                StructureChange::RenameTable {
                    name: "design_done".into(),
                },
            ],
        };
        apply(client, &modify).await.unwrap();
        let design = load(client, &schema, "design_done").await.unwrap();
        assert_eq!(design.columns[0].name, "renamed");
        assert_eq!(design.columns[0].comment.as_deref(), Some("column comment"));
        assert_eq!(design.columns[1].identity, Identity::ByDefault);
        assert!(design.indexes[0].unique);
        let snapshot = load(client, &schema, "structure_test").await.unwrap();
        assert_eq!(snapshot.columns.len(), 2);
        assert!(
            snapshot
                .constraints
                .iter()
                .any(|c| c.kind == ConstraintKind::PrimaryKey)
        );
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
        assert_eq!(apply(client, &request).await.unwrap(), "structure_renamed");
        assert!(apply(client, &request).await.is_err());
        let snapshot = load(client, &schema, "structure_renamed").await.unwrap();
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
                        identity: Identity::None,
                        comment: None,
                        using: None,
                    },
                },
            ],
        };
        assert!(apply(client, &failed).await.is_err());
        assert_eq!(
            load(client, &schema, "structure_renamed").await.unwrap(),
            snapshot
        );
        client
            .batch_execute("ALTER TABLE structure_renamed ADD COLUMN external_change int")
            .await
            .unwrap();
        assert!(apply(client, &failed).await.is_err());
    }
}
