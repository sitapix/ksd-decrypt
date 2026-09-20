use crate::{
    batch::{self, BatchResult, RecoveryEvent},
    error::{AppError, Result, blocking},
    selection::{Selection, collect_paths},
    session::{AppState, Session},
};
use std::path::PathBuf;
use tauri::{AppHandle, State, ipc::Channel};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub(crate) async fn choose_inputs(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: bool,
) -> Result<Selection> {
    let guard = state.begin()?;
    let state = state.inner().clone();
    blocking(move || {
        let _guard = guard;
        let paths = crate::picker::choose_inputs(&app, folder)?;
        collect_paths(paths, &state)
    })
    .await
}

#[tauri::command]
pub(crate) async fn add_paths(
    paths: Vec<PathBuf>,
    state: State<'_, AppState>,
) -> Result<Selection> {
    let guard = state.begin()?;
    let state = state.inner().clone();
    blocking(move || {
        let _guard = guard;
        collect_paths(paths, &state)
    })
    .await
}

#[tauri::command]
pub(crate) async fn choose_output(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<PathBuf>> {
    let guard = state.begin()?;
    let state = state.inner().clone();
    blocking(move || {
        let _guard = guard;
        let path = app
            .dialog()
            .file()
            .set_title("Save to")
            .blocking_pick_folder()
            .map(|path| {
                path.into_path()
                    .map_err(|error| AppError::Native(error.to_string()))
            })
            .transpose()?;
        if let Some(path) = &path {
            state.session()?.destination = Some(path.clone());
        }
        Ok(path)
    })
    .await
}

#[tauri::command]
pub(crate) async fn recover(
    state: State<'_, AppState>,
    ids: Vec<String>,
    on_event: Channel<RecoveryEvent>,
) -> Result<BatchResult> {
    let guard = state.begin_recovery()?;
    let state = state.inner().clone();
    blocking(move || {
        let _guard = guard;
        batch::recover_batch(&state, ids, |event| {
            // A disappeared webview must not leave an orphaned recovery running.
            if on_event.send(event).is_err() {
                state.cancel();
            }
        })
    })
    .await
}

#[tauri::command]
pub(crate) fn cancel_recovery(state: State<'_, AppState>) -> Result<()> {
    state.cancel();
    Ok(())
}

#[tauri::command]
pub(crate) fn clear_selection(state: State<'_, AppState>) -> Result<()> {
    let _guard = state.begin()?;
    *state.session()? = Session::default();
    Ok(())
}

#[tauri::command]
pub(crate) async fn open_result(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Option<String>,
    reveal: Option<bool>,
) -> Result<()> {
    let paths = {
        let session = state.session()?;
        match &id {
            Some(id) => vec![
                session
                    .results
                    .get(id)
                    .and_then(|result| result.output.clone())
                    .ok_or(AppError::NoResult)?,
            ],
            None => session.last_outputs.clone(),
        }
    };
    if paths.is_empty() {
        return Err(AppError::NoResult);
    }
    blocking(move || {
        if id.is_some() && !reveal.unwrap_or(false) {
            app.opener()
                .open_path(paths[0].to_string_lossy().into_owned(), None::<&str>)
        } else {
            app.opener().reveal_items_in_dir(paths)
        }
        .map_err(|error| AppError::Native(error.to_string()))
    })
    .await
}
