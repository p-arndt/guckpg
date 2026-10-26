//! Role inspection and editing: attributes, memberships and object privileges.
//! Changes are planned as SQL first so the user can review them, then applied
//! in one transaction.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_postgres::Client;
use tokio_postgres::types::Type;

use crate::catalog::RelKind;
use crate::error::{AppError, AppResult};
use crate::session::Manager;
use crate::sql::{checked_ident, in_tx, literal_or_null, qualified, quote_ident, quote_literal};

const DATABASE_PRIVILEGES: &[&str] = &["CONNECT", "CREATE", "TEMPORARY"];
const SCHEMA_PRIVILEGES: &[&str] = &["USAGE", "CREATE"];
const TABLE_PRIVILEGES: &[&str] = &[
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "TRUNCATE",
    "REFERENCES",
    "TRIGGER",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDetails {
    name: String,
    can_login: bool,
    superuser: bool,
    create_db: bool,
    create_role: bool,
    inherit: bool,
    replication: bool,
    bypass_rls: bool,
    /// -1 means unlimited.
    connection_limit: i32,
    /// None means the password never expires.
    valid_until: Option<String>,
    comment: Option<String>,
    member_of: Vec<Membership>,
    members: Vec<Membership>,
    databases: Vec<ObjectPrivileges>,
    /// Every other role, including predefined `pg_*` ones, as membership candidates.
    other_roles: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Membership {
    role: String,
    admin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivilegeObjectKind {
    Database,
    Schema,
    #[serde(untagged)]
    Relation(RelKind),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectPrivileges {
    schema: Option<String>,
    name: String,
    kind: PrivilegeObjectKind,
    owner: bool,
    /// Granted to the role directly.
    granted: Vec<String>,
    /// Who granted each entry of `granted`. A REVOKE only removes grants
    /// made by the revoking role (or the owner, for superusers), so
    /// privileges granted by someone else survive it.
    grants: Vec<Grant>,
    /// What the role can actually do, including ownership, PUBLIC and memberships.
    effective: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    privilege: String,
    grantor: String,
}

/// The `granted` privileges and their grantors as two aligned arrays, for
/// [`object_privileges`]. `$1` is the role's oid.
fn direct_grants_sql(acl: &str) -> String {
    let rows = format!(
        "FROM pg_catalog.aclexplode({acl}) a WHERE a.grantee = $1::oid
         ORDER BY a.privilege_type, a.grantor"
    );
    format!(
        "ARRAY(SELECT a.privilege_type::text {rows}),
         ARRAY(SELECT pg_catalog.pg_get_userbyid(a.grantor)::text {rows})"
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RolePrivileges {
    schemas: Vec<ObjectPrivileges>,
    relations: Vec<ObjectPrivileges>,
}

pub async fn details(client: &Client, name: &str) -> AppResult<RoleDetails> {
    let row = client
        .query_opt(
            "SELECT r.oid, r.rolname::text, r.rolcanlogin, r.rolsuper, r.rolcreatedb,
                    r.rolcreaterole, r.rolinherit, r.rolreplication, r.rolbypassrls,
                    r.rolconnlimit,
                    CASE WHEN r.rolvaliduntil IS NULL OR r.rolvaliduntil = 'infinity' THEN NULL
                         ELSE r.rolvaliduntil::text END,
                    pg_catalog.shobj_description(r.oid, 'pg_authid')
             FROM pg_catalog.pg_roles r WHERE r.rolname = $1",
            &[&name],
        )
        .await?
        .ok_or_else(|| AppError::msg(format!("Role \"{name}\" does not exist.")))?;
    let oid: u32 = row.get(0);
    let memberships = |query: &'static str| async move {
        let rows = client.query(query, &[&oid]).await?;
        AppResult::Ok(
            rows.iter()
                .map(|r| Membership {
                    role: r.get(0),
                    admin: r.get(1),
                })
                .collect::<Vec<_>>(),
        )
    };
    let member_of = memberships(
        "SELECT g.rolname::text, bool_or(m.admin_option)
         FROM pg_catalog.pg_auth_members m JOIN pg_catalog.pg_roles g ON g.oid = m.roleid
         WHERE m.member = $1::oid GROUP BY g.rolname ORDER BY 1",
    )
    .await?;
    let members = memberships(
        "SELECT u.rolname::text, bool_or(m.admin_option)
         FROM pg_catalog.pg_auth_members m JOIN pg_catalog.pg_roles u ON u.oid = m.member
         WHERE m.roleid = $1::oid GROUP BY u.rolname ORDER BY 1",
    )
    .await?;
    let databases = client
        .query(
            &format!(
                "SELECT NULL::text, d.datname::text, 'database', d.datdba = $1::oid,
                        {},
                        ARRAY(SELECT p FROM unnest(ARRAY['CONNECT', 'CREATE', 'TEMPORARY']) p
                              WHERE pg_catalog.has_database_privilege($1::oid, d.oid, p))
                 FROM pg_catalog.pg_database d
                 WHERE d.datallowconn AND NOT d.datistemplate ORDER BY 2",
                direct_grants_sql("COALESCE(d.datacl, pg_catalog.acldefault('d', d.datdba))")
            ),
            &[&oid],
        )
        .await?;
    let other_roles = client
        .query(
            "SELECT rolname::text FROM pg_catalog.pg_roles WHERE oid <> $1::oid ORDER BY 1",
            &[&oid],
        )
        .await?;
    Ok(RoleDetails {
        name: row.get(1),
        can_login: row.get(2),
        superuser: row.get(3),
        create_db: row.get(4),
        create_role: row.get(5),
        inherit: row.get(6),
        replication: row.get(7),
        bypass_rls: row.get(8),
        connection_limit: row.get(9),
        valid_until: row.get(10),
        comment: row.get(11),
        member_of,
        members,
        databases: databases.iter().map(object_privileges).collect(),
        other_roles: other_roles.iter().map(|r| r.get(0)).collect(),
    })
}

fn object_kind(kind: &str) -> PrivilegeObjectKind {
    match kind {
        "database" => PrivilegeObjectKind::Database,
        "schema" => PrivilegeObjectKind::Schema,
        relkind => PrivilegeObjectKind::Relation(RelKind::from_code(
            relkind.bytes().next().unwrap_or_default(),
        )),
    }
}

fn object_privileges(row: &tokio_postgres::Row) -> ObjectPrivileges {
    let granted: Vec<String> = row.get(4);
    let grantors: Vec<String> = row.get(5);
    ObjectPrivileges {
        schema: row.get(0),
        name: row.get(1),
        kind: object_kind(row.get(2)),
        owner: row.get(3),
        grants: granted
            .iter()
            .zip(grantors)
            .map(|(privilege, grantor)| Grant {
                privilege: privilege.clone(),
                grantor,
            })
            .collect(),
        granted,
        effective: row.get(6),
    }
}

/// Schema and table privileges of a role in the database `client` is connected to.
pub async fn privileges(client: &Client, name: &str) -> AppResult<RolePrivileges> {
    let oid: u32 = client
        .query_opt(
            "SELECT oid FROM pg_catalog.pg_roles WHERE rolname = $1",
            &[&name],
        )
        .await?
        .ok_or_else(|| AppError::msg(format!("Role \"{name}\" does not exist.")))?
        .get(0);
    let schemas = client
        .query(
            &format!(
                "SELECT NULL::text, n.nspname::text, 'schema', n.nspowner = $1::oid,
                        {},
                        ARRAY(SELECT p FROM unnest(ARRAY['USAGE', 'CREATE']) p
                              WHERE pg_catalog.has_schema_privilege($1::oid, n.oid, p))
                 FROM pg_catalog.pg_namespace n
                 WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
                 ORDER BY 2",
                direct_grants_sql("COALESCE(n.nspacl, pg_catalog.acldefault('n', n.nspowner))")
            ),
            &[&oid],
        )
        .await?;
    let relations = client
        .query(
            &format!(
                "SELECT n.nspname::text, c.relname::text, c.relkind::text, c.relowner = $1::oid,
                        {},
                        ARRAY(SELECT p FROM unnest(ARRAY['SELECT', 'INSERT', 'UPDATE', 'DELETE',
                                                         'TRUNCATE', 'REFERENCES', 'TRIGGER']) p
                              WHERE pg_catalog.has_table_privilege($1::oid, c.oid, p))
                 FROM pg_catalog.pg_class c
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                 WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f')
                   AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
                 ORDER BY 1, 2",
                direct_grants_sql("COALESCE(c.relacl, pg_catalog.acldefault('r', c.relowner))")
            ),
            &[&oid],
        )
        .await?;
    Ok(RolePrivileges {
        schemas: schemas.iter().map(object_privileges).collect(),
        relations: relations.iter().map(object_privileges).collect(),
    })
}

/// Only the attributes that change, so editing e.g. the connection limit
/// does not need the rights to touch SUPERUSER.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeChanges {
    can_login: Option<bool>,
    superuser: Option<bool>,
    create_db: Option<bool>,
    create_role: Option<bool>,
    inherit: Option<bool>,
    replication: Option<bool>,
    bypass_rls: Option<bool>,
    connection_limit: Option<i32>,
    /// Some(None) removes the expiry.
    #[serde(default, with = "double_option")]
    valid_until: Option<Option<String>>,
}

mod double_option {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PrivilegeTarget {
    Database { name: String },
    Schema { name: String },
    Table { schema: String, name: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegeChange {
    target: PrivilegeTarget,
    privilege: String,
    grant: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleChanges {
    name: String,
    #[serde(default)]
    rename: Option<String>,
    #[serde(default)]
    attributes: AttributeChanges,
    /// A new password; hashed client-side before it is sent.
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    remove_password: bool,
    #[serde(default, with = "double_option")]
    comment: Option<Option<String>>,
    #[serde(default)]
    grant_roles: Vec<String>,
    #[serde(default)]
    revoke_roles: Vec<String>,
    #[serde(default)]
    privileges: Vec<PrivilegeChange>,
}

fn check_text(value: &str) -> AppResult<()> {
    if value.contains('\0') {
        return Err(AppError::msg("Text must not contain NUL characters."));
    }
    Ok(())
}

/// The privileges granted or revoked on one object.
struct PrivilegeGroup<'a> {
    revoke: bool,
    target: &'a PrivilegeTarget,
    /// The object as it appears after `ON`.
    object: String,
    privileges: Vec<&'static str>,
}

/// One GRANT and one REVOKE per object keeps the script short and readable.
fn privilege_groups(changes: &RoleChanges) -> AppResult<Vec<PrivilegeGroup<'_>>> {
    let mut grouped: BTreeMap<(bool, u8, String), PrivilegeGroup<'_>> = BTreeMap::new();
    for change in &changes.privileges {
        let (order, object, allowed) = match &change.target {
            PrivilegeTarget::Database { name } => (
                0,
                format!("DATABASE {}", quote_ident(name)),
                DATABASE_PRIVILEGES,
            ),
            PrivilegeTarget::Schema { name } => (
                1,
                format!("SCHEMA {}", quote_ident(name)),
                SCHEMA_PRIVILEGES,
            ),
            PrivilegeTarget::Table { schema, name } => (
                2,
                format!("TABLE {}", qualified(schema, name)),
                TABLE_PRIVILEGES,
            ),
        };
        let privilege = *allowed
            .iter()
            .find(|p| p.eq_ignore_ascii_case(&change.privilege))
            .ok_or_else(|| AppError::msg(format!("Unknown privilege {}.", change.privilege)))?;
        let group = grouped
            .entry((!change.grant, order, object.clone()))
            .or_insert_with(|| PrivilegeGroup {
                revoke: !change.grant,
                target: &change.target,
                object,
                privileges: Vec::new(),
            });
        if !group.privileges.contains(&privilege) {
            group.privileges.push(privilege);
        }
    }
    let mut groups: Vec<_> = grouped.into_values().collect();
    for group in &mut groups {
        group.privileges.sort_by_key(|p| {
            [DATABASE_PRIVILEGES, SCHEMA_PRIVILEGES, TABLE_PRIVILEGES]
                .iter()
                .find_map(|list| list.iter().position(|x| x == p))
        });
    }
    Ok(groups)
}

/// The statements for `changes`. `hash` turns a password into what gets sent;
/// previews pass a placeholder so the password never shows up.
pub fn plan(changes: &RoleChanges, hash: impl Fn(&str) -> String) -> AppResult<Vec<String>> {
    checked_ident(&changes.name)?;
    let role = quote_ident(&changes.name);
    let mut statements = Vec::new();

    let a = &changes.attributes;
    let mut options = Vec::new();
    let flags = [
        (a.superuser, "SUPERUSER"),
        (a.create_db, "CREATEDB"),
        (a.create_role, "CREATEROLE"),
        (a.inherit, "INHERIT"),
        (a.can_login, "LOGIN"),
        (a.replication, "REPLICATION"),
        (a.bypass_rls, "BYPASSRLS"),
    ];
    for (value, keyword) in flags {
        if let Some(on) = value {
            options.push(if on {
                keyword.to_string()
            } else {
                format!("NO{keyword}")
            });
        }
    }
    if let Some(limit) = a.connection_limit {
        if limit < -1 {
            return Err(AppError::msg(
                "Connection limit must be -1 (unlimited) or more.",
            ));
        }
        options.push(format!("CONNECTION LIMIT {limit}"));
    }
    if let Some(valid_until) = &a.valid_until {
        let value = valid_until
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        if let Some(v) = value {
            check_text(v)?;
        }
        options.push(format!(
            "VALID UNTIL {}",
            quote_literal(value.unwrap_or("infinity"))
        ));
    }
    if let Some(password) = &changes.password {
        if password.is_empty() || password.contains('\0') {
            return Err(AppError::msg("Invalid password."));
        }
        options.push(format!("PASSWORD {}", quote_literal(&hash(password))));
    } else if changes.remove_password {
        options.push("PASSWORD NULL".into());
    }
    if !options.is_empty() {
        statements.push(format!("ALTER ROLE {role} WITH {}", options.join(" ")));
    }

    if let Some(comment) = &changes.comment {
        let value = comment.as_deref().map(str::trim).filter(|v| !v.is_empty());
        if let Some(v) = value {
            check_text(v)?;
        }
        statements.push(format!(
            "COMMENT ON ROLE {role} IS {}",
            literal_or_null(value)
        ));
    }

    for group in &changes.grant_roles {
        checked_ident(group)?;
        statements.push(format!("GRANT {} TO {role}", quote_ident(group)));
    }
    for group in &changes.revoke_roles {
        checked_ident(group)?;
        statements.push(format!("REVOKE {} FROM {role}", quote_ident(group)));
    }

    for group in privilege_groups(changes)? {
        let list = group.privileges.join(", ");
        let object = group.object;
        statements.push(if group.revoke {
            format!("REVOKE {list} ON {object} FROM {role}")
        } else {
            format!("GRANT {list} ON {object} TO {role}")
        });
    }

    // Last, so every statement above can use the old name.
    if let Some(new_name) = changes.rename.as_deref().filter(|n| *n != changes.name) {
        checked_ident(new_name)?;
        statements.push(format!(
            "ALTER ROLE {role} RENAME TO {}",
            quote_ident(new_name)
        ));
    }
    Ok(statements)
}

#[tauri::command]
pub async fn role_details(
    manager: State<'_, Manager>,
    id: String,
    name: String,
) -> AppResult<RoleDetails> {
    let conn = manager.meta(&id, "").await?;
    details(&conn.client, &name).await
}

#[tauri::command]
pub async fn role_privileges(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    name: String,
) -> AppResult<RolePrivileges> {
    let conn = manager.meta(&id, &database).await?;
    privileges(&conn.client, &name).await
}

#[tauri::command]
pub fn preview_role_changes(changes: RoleChanges) -> AppResult<Vec<String>> {
    plan(&changes, |_| {
        "<SCRAM-SHA-256 hash of the new password>".into()
    })
}

#[tauri::command]
pub async fn apply_role_changes(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    changes: RoleChanges,
) -> AppResult<()> {
    manager.ensure_writable(&id)?;
    let statements = plan(&changes, scram_hash)?;
    if statements.is_empty() {
        return Ok(());
    }
    let conn = manager.open(&id, &database).await?;
    apply_verified(&conn.client, &statements, &changes).await
}

/// The verifier is computed from the password, so the server never sees
/// (or logs) the plaintext.
fn scram_hash(password: &str) -> String {
    postgres_protocol::password::scram_sha_256(password.as_bytes())
}

/// Runs `statements` in one transaction and fails (rolling back) when a
/// revoked privilege is still granted afterwards, which happens when someone
/// other than the current user granted it: PostgreSQL then skips it without
/// an error.
pub async fn apply_verified(
    client: &Client,
    statements: &[String],
    changes: &RoleChanges,
) -> AppResult<()> {
    in_tx(client, "BEGIN", || async {
        for statement in statements {
            client.batch_execute(statement).await?;
        }
        verify_revocations(client, changes).await
    })
    .await
}

async fn verify_revocations(client: &Client, changes: &RoleChanges) -> AppResult<()> {
    // Runs after the statements, so the rename (always last) has happened.
    let role = changes.rename.as_deref().unwrap_or(&changes.name);
    for group in privilege_groups(changes)?.iter().filter(|g| g.revoke) {
        let (acl_source, name, schema) = match group.target {
            PrivilegeTarget::Database { name } => (
                "pg_catalog.pg_database o, pg_catalog.aclexplode(o.datacl) a
                 WHERE o.datname = $2",
                name,
                None,
            ),
            PrivilegeTarget::Schema { name } => (
                "pg_catalog.pg_namespace o, pg_catalog.aclexplode(o.nspacl) a
                 WHERE o.nspname = $2",
                name,
                None,
            ),
            PrivilegeTarget::Table { schema, name } => (
                "pg_catalog.pg_class o
                 JOIN pg_catalog.pg_namespace n ON n.oid = o.relnamespace,
                 pg_catalog.aclexplode(o.relacl) a
                 WHERE o.relname = $2 AND n.nspname = $3",
                name,
                Some(schema.as_str()),
            ),
        };
        let rows = client
            .query_typed(
                &format!(
                    "SELECT a.privilege_type::text, pg_catalog.pg_get_userbyid(a.grantor)::text
                     FROM {acl_source}
                       AND a.grantee = (SELECT oid FROM pg_catalog.pg_roles WHERE rolname = $1)
                       AND a.privilege_type = ANY ($4)
                     ORDER BY 1, 2"
                ),
                &[
                    (&role, Type::TEXT),
                    (name, Type::TEXT),
                    (&schema, Type::TEXT),
                    (&group.privileges, Type::TEXT_ARRAY),
                ],
            )
            .await?;
        let surviving: Vec<Grant> = rows
            .iter()
            .map(|r| Grant {
                privilege: r.get(0),
                grantor: r.get(1),
            })
            .collect();
        if let Some(message) = surviving_grants_message(&group.object, &surviving) {
            return Err(AppError::msg(message));
        }
    }
    Ok(())
}

fn surviving_grants_message(object: &str, surviving: &[Grant]) -> Option<String> {
    if surviving.is_empty() {
        return None;
    }
    let list = surviving
        .iter()
        .map(|g| format!("{} (granted by {})", g.privilege, quote_ident(&g.grantor)))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "Could not revoke {list} on {object}: only the role that granted a privilege can revoke it. Nothing was changed."
    ))
}

pub fn create_role_sql(
    name: &str,
    password: Option<&str>,
    login: bool,
    create_db: bool,
    create_role: bool,
    hash: impl Fn(&str) -> String,
) -> AppResult<String> {
    checked_ident(name)?;
    let mut sql = format!(
        "CREATE ROLE {} {} {} {}",
        quote_ident(name),
        if login { "LOGIN" } else { "NOLOGIN" },
        if create_db { "CREATEDB" } else { "NOCREATEDB" },
        if create_role {
            "CREATEROLE"
        } else {
            "NOCREATEROLE"
        }
    );
    if let Some(password) = password {
        if password.is_empty() || password.contains('\0') {
            return Err(AppError::msg("Invalid password."));
        }
        sql.push_str(&format!(" PASSWORD {}", quote_literal(&hash(password))));
    }
    Ok(sql)
}

#[tauri::command]
pub async fn create_role(
    manager: State<'_, Manager>,
    id: String,
    name: String,
    password: Option<String>,
    login: bool,
    create_db: bool,
    create_role: bool,
) -> AppResult<()> {
    manager.ensure_writable(&id)?;
    let sql = create_role_sql(
        &name,
        password.as_deref(),
        login,
        create_db,
        create_role,
        scram_hash,
    )?;
    let conn = manager.meta(&id, "").await?;
    conn.client.batch_execute(&sql).await?;
    Ok(())
}

#[tauri::command]
pub async fn drop_role(manager: State<'_, Manager>, id: String, name: String) -> AppResult<()> {
    manager.ensure_writable(&id)?;
    checked_ident(&name)?;
    let conn = manager.meta(&id, "").await?;
    conn.client
        .batch_execute(&format!("DROP ROLE {}", quote_ident(&name)))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changes(json: serde_json::Value) -> RoleChanges {
        serde_json::from_value(json).unwrap()
    }

    fn preview(json: serde_json::Value) -> AppResult<Vec<String>> {
        plan(&changes(json), |_| "HASH".into())
    }

    #[test]
    fn nothing_to_do_plans_nothing() {
        assert!(
            preview(serde_json::json!({ "name": "app" }))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn alters_only_changed_attributes() {
        let sql = preview(serde_json::json!({
            "name": "a\"pp",
            "attributes": { "canLogin": false, "createDb": true, "connectionLimit": 5 },
        }))
        .unwrap();
        assert_eq!(
            sql,
            ["ALTER ROLE \"a\"\"pp\" WITH CREATEDB NOLOGIN CONNECTION LIMIT 5"]
        );
    }

    #[test]
    fn expiry_and_password() {
        let sql = preview(serde_json::json!({
            "name": "app",
            "attributes": { "validUntil": "2027-01-01'" },
            "password": "secret",
        }))
        .unwrap();
        assert_eq!(
            sql,
            ["ALTER ROLE \"app\" WITH VALID UNTIL '2027-01-01''' PASSWORD 'HASH'"]
        );
        let sql = preview(serde_json::json!({
            "name": "app",
            "attributes": { "validUntil": null },
            "removePassword": true,
        }))
        .unwrap();
        assert_eq!(
            sql,
            ["ALTER ROLE \"app\" WITH VALID UNTIL 'infinity' PASSWORD NULL"]
        );
    }

    #[test]
    fn missing_expiry_leaves_it_alone() {
        let sql = preview(serde_json::json!({ "name": "app", "attributes": {} })).unwrap();
        assert!(sql.is_empty());
    }

    #[test]
    fn groups_privileges_per_object_and_renames_last() {
        let sql = preview(serde_json::json!({
            "name": "app",
            "rename": "web",
            "grantRoles": ["readers"],
            "revokeRoles": ["writers"],
            "privileges": [
                { "target": { "kind": "table", "schema": "public", "name": "t" }, "privilege": "insert", "grant": true },
                { "target": { "kind": "table", "schema": "public", "name": "t" }, "privilege": "SELECT", "grant": true },
                { "target": { "kind": "table", "schema": "public", "name": "t" }, "privilege": "DELETE", "grant": false },
                { "target": { "kind": "schema", "name": "public" }, "privilege": "USAGE", "grant": true },
                { "target": { "kind": "database", "name": "db" }, "privilege": "CONNECT", "grant": true },
            ],
        }))
        .unwrap();
        assert_eq!(
            sql,
            [
                "GRANT \"readers\" TO \"app\"",
                "REVOKE \"writers\" FROM \"app\"",
                "GRANT CONNECT ON DATABASE \"db\" TO \"app\"",
                "GRANT USAGE ON SCHEMA \"public\" TO \"app\"",
                "GRANT SELECT, INSERT ON TABLE \"public\".\"t\" TO \"app\"",
                "REVOKE DELETE ON TABLE \"public\".\"t\" FROM \"app\"",
                "ALTER ROLE \"app\" RENAME TO \"web\"",
            ]
        );
    }

    #[test]
    fn comments() {
        assert_eq!(
            preview(serde_json::json!({ "name": "app", "comment": "it's" })).unwrap(),
            ["COMMENT ON ROLE \"app\" IS 'it''s'"]
        );
        assert_eq!(
            preview(serde_json::json!({ "name": "app", "comment": null })).unwrap(),
            ["COMMENT ON ROLE \"app\" IS NULL"]
        );
    }

    #[test]
    fn rejects_bad_input() {
        let bad_privilege = serde_json::json!({
            "name": "app",
            "privileges": [{ "target": { "kind": "schema", "name": "s" }, "privilege": "SELECT; DROP", "grant": true }],
        });
        assert!(preview(bad_privilege).is_err());
        let wrong_level = serde_json::json!({
            "name": "app",
            "privileges": [{ "target": { "kind": "database", "name": "d" }, "privilege": "SELECT", "grant": true }],
        });
        assert!(preview(wrong_level).is_err());
        assert!(preview(serde_json::json!({ "name": "" })).is_err());
        assert!(preview(serde_json::json!({ "name": "app", "rename": "a".repeat(64) })).is_err());
        assert!(preview(serde_json::json!({ "name": "app", "password": "" })).is_err());
        assert!(
            preview(serde_json::json!({ "name": "app", "attributes": { "connectionLimit": -2 } }))
                .is_err()
        );
    }

    #[test]
    fn creates_roles_with_hashed_password_and_explicit_privileges() {
        assert_eq!(
            create_role_sql("a\"b", Some("a'b\\c"), true, false, true, |p| format!(
                "HASH({p})"
            ))
            .unwrap(),
            "CREATE ROLE \"a\"\"b\" LOGIN NOCREATEDB CREATEROLE PASSWORD E'HASH(a''b\\\\c)'"
        );
        assert_eq!(
            create_role_sql("app", None, false, true, false, |_| unreachable!()).unwrap(),
            "CREATE ROLE \"app\" NOLOGIN CREATEDB NOCREATEROLE"
        );
        assert!(create_role_sql("", None, true, false, false, |p| p.into()).is_err());
        assert!(create_role_sql(&"a".repeat(64), None, true, false, false, |p| p.into()).is_err());
        assert!(
            create_role_sql("role", Some("bad\0password"), true, false, false, |p| p
                .into())
            .is_err()
        );
        assert!(create_role_sql("role", Some(""), true, false, false, |p| p.into()).is_err());
    }

    #[test]
    fn created_roles_never_carry_the_plaintext_password() {
        let sql = create_role_sql("app", Some("hunter2"), true, false, false, scram_hash).unwrap();
        assert!(!sql.contains("hunter2"));
        assert!(sql.contains("PASSWORD 'SCRAM-SHA-256$"));
    }

    #[test]
    fn privilege_kinds_serialize_like_before() {
        for (code, wire) in [
            ("database", "database"),
            ("schema", "schema"),
            ("r", "table"),
            ("p", "partitioned"),
            ("v", "view"),
            ("m", "materialized"),
            ("f", "foreign"),
            ("S", "other"),
        ] {
            assert_eq!(serde_json::to_value(object_kind(code)).unwrap(), wire);
        }
    }

    #[test]
    fn revocations_are_grouped_per_object() {
        let changes = changes(serde_json::json!({
            "name": "app",
            "privileges": [
                { "target": { "kind": "table", "schema": "s", "name": "t" }, "privilege": "delete", "grant": false },
                { "target": { "kind": "table", "schema": "s", "name": "t" }, "privilege": "SELECT", "grant": false },
                { "target": { "kind": "table", "schema": "s", "name": "t" }, "privilege": "INSERT", "grant": true },
            ],
        }));
        let groups = privilege_groups(&changes).unwrap();
        let revoked: Vec<_> = groups.iter().filter(|g| g.revoke).collect();
        assert_eq!(revoked.len(), 1);
        assert_eq!(revoked[0].object, "TABLE \"s\".\"t\"");
        assert_eq!(revoked[0].privileges, ["SELECT", "DELETE"]);
        assert!(matches!(
            revoked[0].target,
            PrivilegeTarget::Table { schema, name } if schema == "s" && name == "t"
        ));
    }

    #[test]
    fn explains_privileges_a_revoke_could_not_remove() {
        assert_eq!(surviving_grants_message("TABLE \"t\"", &[]), None);
        let message = surviving_grants_message(
            "TABLE \"t\"",
            &[Grant {
                privilege: "SELECT".into(),
                grantor: "alice".into(),
            }],
        )
        .unwrap();
        assert!(message.contains("SELECT (granted by \"alice\") on TABLE \"t\""));
        assert!(message.contains("Nothing was changed"));
    }

    #[test]
    fn preview_hides_the_password() {
        let sql = preview_role_changes(changes(
            serde_json::json!({ "name": "app", "password": "hunter2" }),
        ))
        .unwrap();
        assert!(!sql[0].contains("hunter2"));
    }
}
