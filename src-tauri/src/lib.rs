mod catalog;
mod commands;
mod config;
mod editing;
mod error;
#[cfg(test)]
mod it;
mod pg;
mod session;
mod sql;
mod ssh;

use tauri::Manager as _;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let dir = app.path().app_config_dir()?;
            app.manage(session::Manager::new(
                config::ConnectionStore::new(&dir),
                ssh::KnownHosts::new(&dir),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::secret_status,
            commands::save_connection,
            commands::delete_connection,
            commands::duplicate_connection,
            commands::arrange_connections,
            commands::test_connection,
            commands::trust_host_key,
            commands::connect,
            commands::disconnect,
            commands::list_databases,
            commands::list_schemas,
            commands::list_relations,
            commands::list_columns,
            commands::schema_diagram,
            commands::completion_tables,
            commands::run_query,
            commands::browse_table,
            commands::apply_changes,
            commands::preview_changes,
            commands::cancel_query,
            commands::close_tab,
        ])
        .run(tauri::generate_context!())
        .expect("error while running guckpg");
}
