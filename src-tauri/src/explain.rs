use tauri::State;

use crate::{error::AppResult, pg::QueryOutput, session::Manager};

#[tauri::command]
pub async fn explain_query(
    manager: State<'_, Manager>,
    tab_id: String,
    id: String,
    database: String,
    sql: String,
) -> AppResult<QueryOutput> {
    let conn = manager.tab(&tab_id, &id, &database).await?;
    let statement = format!("EXPLAIN (FORMAT JSON) {sql}");
    // Extended-protocol preparation rejects scripts before the simple-protocol runner sees them.
    conn.client.prepare(&statement).await?;
    conn.run_editable(&statement, 1).await
}
