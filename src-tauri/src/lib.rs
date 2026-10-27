mod ai;
mod backup;
mod catalog;
mod commands;
mod config;
mod csv_transfer;
mod editing;
mod error;
mod explain;
mod guard;
#[cfg(test)]
mod it;
mod navicat;
mod pg;
mod roles;
mod session;
mod sql;
mod ssh;
mod structure;
mod test_data;
#[cfg(test)]
mod testutil;
mod vault;

use tauri::Manager as _;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // One process-wide provider, so no rustls user depends on another having
    // installed it first. Fails only if one is already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();
    tauri::Builder::default()
        .setup(|app| {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(if cfg!(debug_assertions) {
                        log::LevelFilter::Info
                    } else {
                        log::LevelFilter::Warn
                    })
                    .build(),
            )?;
            let dir = app.path().app_config_dir()?;
            vault::init(&dir)?;
            app.manage(ai::AiState::new(&dir));
            app.manage(session::Manager::new(
                config::ConnectionStore::new(&dir),
                ssh::KnownHosts::new(&dir),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backup::backup_database,
            backup::restore_database,
            csv_transfer::import_csv,
            csv_transfer::export_csv,
            explain::explain_query,
            test_data::preview_test_data,
            test_data::insert_test_data,
            ai::ai_get_settings,
            ai::ai_ollama_models,
            ai::ai_save_settings,
            ai::ai_chat,
            ai::ai_cancel,
            ai::ai_decide_write,
            commands::list_connections,
            navicat::import_navicat,
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
            commands::list_roles,
            roles::create_role,
            commands::list_schemas,
            commands::list_relations,
            commands::list_objects,
            commands::object_definition,
            commands::relation_definition,
            commands::refresh_materialized_view,
            commands::list_columns,
            commands::schema_diagram,
            commands::completion_tables,
            commands::run_query,
            commands::browse_table,
            commands::apply_changes,
            commands::preview_changes,
            commands::cancel_query,
            commands::close_tab,
            structure::table_structure,
            structure::preview_structure,
            structure::apply_structure,
            roles::role_details,
            roles::role_privileges,
            roles::preview_role_changes,
            roles::apply_role_changes,
            roles::drop_role,
        ])
        .build(tauri::generate_context!())
        .expect("error while running guckpg")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let manager = app.state::<session::Manager>();
                tauri::async_runtime::block_on(manager.shutdown());
            }
        });
}
