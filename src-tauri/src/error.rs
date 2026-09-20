use serde::{Serialize, Serializer};

pub(crate) type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error("Wait until the current operation finishes.")]
    Busy,
    #[error("The recovery session is unavailable. Please restart the app.")]
    SessionUnavailable,
    #[error("Choose at least one supported file.")]
    EmptySelection,
    #[error("The selection has changed. Add your files again.")]
    InvalidSelection,
    #[error("No recovered file is available yet.")]
    NoResult,
    #[error("{0}")]
    Native(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("The background operation could not finish: {0}")]
    Worker(String),
}

// Keep the IPC error human-readable without leaking the internal enum layout.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| AppError::Worker(error.to_string()))?
}
