use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, AppResult};

#[tauri::command]
pub async fn choose_file(
    app: AppHandle,
    save: bool,
    filename: String,
    extension: String,
) -> AppResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = app
            .dialog()
            .file()
            .set_file_name(&filename)
            .add_filter("Files", &[&extension]);
        let selected = if save {
            dialog.blocking_save_file()
        } else {
            dialog.blocking_pick_file()
        };
        selected
            .map(|file| {
                let path = file.into_path().map_err(|e| AppError::msg(e.to_string()))?;
                path.to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| AppError::msg("File path is not valid UTF-8"))
            })
            .transpose()
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}
