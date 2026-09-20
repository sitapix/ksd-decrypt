use crate::error::{AppError, Result};
use std::path::PathBuf;
use tauri::AppHandle;

// Called from a blocking worker. Only panel creation/presentation runs on the
// main thread; directory traversal and probing remain in the selection module.
#[cfg(target_os = "macos")]
pub(crate) fn choose_inputs(app: &AppHandle, folders_only: bool) -> Result<Vec<PathBuf>> {
    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel, NSWindow};
    use objc2_foundation::ns_string;
    use std::{cell::RefCell, sync::mpsc};
    use tauri::Manager;

    let (sender, receiver) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let Some(main_thread) = MainThreadMarker::new() else {
            let _ = sender.send(Err(AppError::Native(
                "The file picker could not open.".into(),
            )));
            return;
        };
        let panel = NSOpenPanel::openPanel(main_thread);
        panel.setTitle(Some(if folders_only {
            ns_string!("Choose folders")
        } else {
            ns_string!("Choose files or folders")
        }));
        panel.setPrompt(Some(ns_string!("Add")));
        panel.setCanChooseFiles(!folders_only);
        panel.setCanChooseDirectories(true);
        panel.setAllowsMultipleSelection(true);
        panel.setCanCreateDirectories(false);

        // Take the retained panel when the callback fires, breaking the
        // panel → completion → panel ownership cycle after dismissal.
        let retained_panel = RefCell::new(Some(panel.clone()));
        let completion = RcBlock::new(move |response| {
            let Some(panel) = retained_panel.borrow_mut().take() else {
                return;
            };
            let result = if response == NSModalResponseOK {
                panel
                    .URLs()
                    .iter()
                    .map(|url| {
                        url.to_file_path().ok_or_else(|| {
                            AppError::Native(
                                "The selected item is not a local file or folder.".into(),
                            )
                        })
                    })
                    .collect()
            } else if response == NSModalResponseCancel {
                Ok(Vec::new())
            } else {
                Err(AppError::Native(
                    "The file picker could not open. Please try again.".into(),
                ))
            };
            let _ = sender.send(result);
        });
        let window = handle.get_webview_window("main");
        let parent = window.as_ref().and_then(|window| window.ns_window().ok());
        if let Some(parent) = parent.filter(|parent| !parent.is_null()) {
            // Tauri owns this NSWindow. We use the pointer only on the main
            // thread, while the WebviewWindow handle above remains alive.
            let parent = unsafe { &*parent.cast::<NSWindow>() };
            panel.beginSheetModalForWindow_completionHandler(parent, &completion);
        } else {
            panel.beginWithCompletionHandler(&completion);
        }
    })
    .map_err(|error| AppError::Native(error.to_string()))?;
    receiver
        .recv()
        .map_err(|_| AppError::Native("The file picker closed unexpectedly.".into()))?
}

// Other desktop pickers do not expose mixed selection through Tauri. Keep both
// choices reachable from the main button, then use the platform's native picker.
#[cfg(not(target_os = "macos"))]
pub(crate) fn choose_inputs(app: &AppHandle, folders_only: bool) -> Result<Vec<PathBuf>> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};

    let folders = if folders_only {
        true
    } else {
        match app
            .dialog()
            .message("What would you like to add?")
            .title("Choose files or folders")
            .buttons(MessageDialogButtons::YesNoCancelCustom(
                "Files…".into(),
                "Folders…".into(),
                "Cancel".into(),
            ))
            .blocking_show_with_result()
        {
            MessageDialogResult::Custom(choice) if choice == "Files…" => false,
            MessageDialogResult::Custom(choice) if choice == "Folders…" => true,
            MessageDialogResult::Yes => false,
            MessageDialogResult::No => true,
            _ => return Ok(Vec::new()),
        }
    };
    let dialog = app.dialog().file();
    let selected = if folders {
        dialog.set_title("Choose folders").blocking_pick_folders()
    } else {
        dialog
            .set_title("Choose files")
            .add_filter("KeepSafe files", &["ksd"])
            .add_filter("All files", &["*"])
            .blocking_pick_files()
    };
    selected
        .unwrap_or_default()
        .into_iter()
        .map(|path| {
            path.into_path()
                .map_err(|error| AppError::Native(error.to_string()))
        })
        .collect()
}
