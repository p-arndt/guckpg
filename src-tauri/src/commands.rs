use serde::Serialize;
use tauri::State;

use crate::catalog::{self, Column, CompletionTable, Diagram, Relation, Role, SchemaObject};
use crate::config::{self, ConnectionConfig, Placement, Secrets};
use crate::editing;
use crate::error::{AppError, AppResult};
use crate::pg::QueryOutput;
use crate::session::Manager;
use crate::sql::{BrowseRequest, Change, browse_sql};

/// Upper bound for rows kept per result set, so a careless `SELECT *` on a
/// huge table cannot exhaust memory.
const MAX_ROWS_LIMIT: usize = 100_000;

#[tauri::command]
pub fn list_connections(manager: State<'_, Manager>) -> AppResult<Vec<ConnectionConfig>> {
    manager.store.list()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatus {
    password: bool,
    ssh_password: bool,
    ssh_passphrase: bool,
}

/// Which secrets are stored, so the form can show "saved" without ever
/// sending a secret to the frontend.
#[tauri::command]
pub fn secret_status(id: String) -> AppResult<SecretStatus> {
    let secrets = config::load_secrets(&id)?;
    Ok(SecretStatus {
        password: secrets.password.is_some(),
        ssh_password: secrets.ssh_password.is_some(),
        ssh_passphrase: secrets.ssh_passphrase.is_some(),
    })
}

#[tauri::command]
pub async fn save_connection(
    manager: State<'_, Manager>,
    mut config: ConnectionConfig,
    secrets: Secrets,
) -> AppResult<ConnectionConfig> {
    if config.id.is_empty() {
        config.id = uuid::Uuid::new_v4().to_string();
    }
    config.validate()?;
    let previous = manager.store.get(&config.id).ok();
    config::store_secrets_for(&config, previous.as_ref(), &secrets)?;
    manager.store.upsert(config.clone())?;
    // Open sessions still use the old settings.
    manager.disconnect(&config.id).await;
    Ok(config)
}

#[tauri::command]
pub async fn delete_connection(manager: State<'_, Manager>, id: String) -> AppResult<()> {
    manager.disconnect(&id).await;
    manager.store.remove(&id)?;
    config::delete_secrets(&id)
}

#[tauri::command]
pub fn duplicate_connection(
    manager: State<'_, Manager>,
    id: String,
) -> AppResult<ConnectionConfig> {
    let new_id = uuid::Uuid::new_v4().to_string();
    config::copy_secrets(&id, &new_id)?;
    let copy = manager.store.duplicate(&id, new_id.clone());
    if copy.is_err() {
        let _ = config::delete_secrets(&new_id);
    }
    copy
}

/// Sidebar order and folders. Leaves live sessions alone: nothing a session
/// uses changes.
#[tauri::command]
pub fn arrange_connections(
    manager: State<'_, Manager>,
    placements: Vec<Placement>,
) -> AppResult<Vec<ConnectionConfig>> {
    manager.store.arrange(&placements)
}

#[tauri::command]
pub async fn test_connection(
    manager: State<'_, Manager>,
    config: ConnectionConfig,
    secrets: Secrets,
) -> AppResult<String> {
    manager.test(&config, &secrets).await
}

#[tauri::command]
pub fn trust_host_key(
    manager: State<'_, Manager>,
    host: String,
    port: u16,
    key: String,
) -> AppResult<()> {
    manager.known_hosts.trust(&host, port, &key)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    version: String,
    database: String,
}

#[tauri::command]
pub async fn connect(manager: State<'_, Manager>, id: String) -> AppResult<ServerInfo> {
    let database = manager.store.get(&id)?.database;
    let conn = manager.meta(&id, &database).await?;
    Ok(ServerInfo {
        version: catalog::server_version(&conn.client).await?,
        database,
    })
}

#[tauri::command]
pub async fn disconnect(manager: State<'_, Manager>, id: String) -> AppResult<()> {
    manager.disconnect(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn list_databases(manager: State<'_, Manager>, id: String) -> AppResult<Vec<String>> {
    let conn = manager.meta(&id, "").await?;
    catalog::databases(&conn.client).await
}

#[tauri::command]
pub async fn list_roles(manager: State<'_, Manager>, id: String) -> AppResult<Vec<Role>> {
    let conn = manager.meta(&id, "").await?;
    catalog::roles(&conn.client).await
}

#[tauri::command]
pub async fn list_schemas(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    include_system: bool,
) -> AppResult<Vec<String>> {
    let conn = manager.meta(&id, &database).await?;
    catalog::schemas(&conn.client, include_system).await
}

#[tauri::command]
pub async fn list_relations(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
) -> AppResult<Vec<Relation>> {
    let conn = manager.meta(&id, &database).await?;
    catalog::relations(&conn.client, &schema).await
}

#[tauri::command]
pub async fn list_objects(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
) -> AppResult<Vec<SchemaObject>> {
    let conn = manager.meta(&id, &database).await?;
    catalog::objects(&conn.client, &schema).await
}

#[tauri::command]
pub async fn object_definition(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    kind: String,
    oid: u32,
) -> AppResult<String> {
    let conn = manager.meta(&id, &database).await?;
    catalog::object_definition(&conn.client, &schema, &kind, oid).await
}

#[tauri::command]
pub async fn relation_definition(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    name: String,
) -> AppResult<String> {
    let conn = manager.meta(&id, &database).await?;
    catalog::relation_definition(&conn.client, &schema, &name).await
}

#[tauri::command]
pub async fn view_details(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    name: String,
) -> AppResult<catalog::ViewDetails> {
    let conn = manager.meta(&id, &database).await?;
    catalog::view_details(&conn.client, &schema, &name).await
}

#[tauri::command]
pub async fn refresh_materialized_view(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    name: String,
) -> AppResult<()> {
    manager.ensure_writable(&id)?;
    // REFRESH can run for minutes and waits for an exclusive lock; on the
    // tree's shared connection that would freeze the whole sidebar.
    let conn = manager.open(&id, &database).await?;
    conn.client.batch_execute("SET lock_timeout = '2s'").await?;
    catalog::refresh_materialized_view(&conn.client, &schema, &name).await
}

#[tauri::command]
pub async fn list_columns(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
) -> AppResult<Vec<Column>> {
    let conn = manager.meta(&id, &database).await?;
    catalog::columns(&conn.client, &schema, &table).await
}

#[tauri::command]
pub async fn schema_diagram(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
) -> AppResult<Diagram> {
    let conn = manager.meta(&id, &database).await?;
    catalog::diagram(&conn.client, &schema).await
}

#[tauri::command]
pub async fn completion_tables(
    manager: State<'_, Manager>,
    id: String,
    database: String,
) -> AppResult<Vec<CompletionTable>> {
    let conn = manager.meta(&id, &database).await?;
    catalog::completion(&conn.client).await
}

#[tauri::command]
pub async fn run_query(
    manager: State<'_, Manager>,
    tab_id: String,
    id: String,
    database: String,
    sql: String,
    max_rows: usize,
) -> AppResult<QueryOutput> {
    let conn = manager.tab(&tab_id, &id, &database).await?;
    conn.run_editable(&sql, max_rows.min(MAX_ROWS_LIMIT)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseOutput {
    sql: String,
    #[serde(flatten)]
    output: QueryOutput,
}

#[tauri::command]
pub async fn browse_table(
    manager: State<'_, Manager>,
    tab_id: String,
    id: String,
    database: String,
    mut request: BrowseRequest,
) -> AppResult<BrowseOutput> {
    request.limit = request.limit.min(MAX_ROWS_LIMIT as u32);
    let order = {
        let meta = manager.meta(&id, &database).await?;
        editing::row_order(&meta.client, &request.schema, &request.table).await?
    };
    let sql = browse_sql(&request, &order);
    let conn = manager.tab(&tab_id, &id, &database).await?;
    let output = conn.run_editable(&sql, request.limit as usize).await?;
    Ok(BrowseOutput { sql, output })
}

/// Save grid edits. Runs on its own connection: never inside a transaction
/// the user opened in an editor tab, and never sharing a transaction with
/// another save or blocking the tree while it waits for row locks.
#[tauri::command]
pub async fn apply_changes(
    manager: State<'_, Manager>,
    id: String,
    database: String,
    schema: String,
    table: String,
    changes: Vec<Change>,
) -> AppResult<usize> {
    manager.ensure_writable(&id)?;
    let conn = manager.open(&id, &database).await?;
    editing::apply(&conn.client, &schema, &table, &changes).await
}

/// The exact SQL `apply_changes` would run, for review before saving.
#[tauri::command]
pub fn preview_changes(
    schema: String,
    table: String,
    changes: Vec<Change>,
) -> AppResult<Vec<String>> {
    changes
        .iter()
        .map(|c| crate::sql::change_sql(&schema, &table, c).map_err(AppError::msg))
        .collect()
}

#[tauri::command]
pub async fn cancel_query(manager: State<'_, Manager>, tab_id: String) -> AppResult<()> {
    manager.cancel(&tab_id).await
}

#[tauri::command]
pub async fn close_tab(manager: State<'_, Manager>, tab_id: String) -> AppResult<()> {
    manager.close_tab(&tab_id).await;
    Ok(())
}
