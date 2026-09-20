use crate::{
    error::{AppError, Result},
    recovery::{self, InputFile, RecoveredFile},
    session::AppState,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Progress {
    id: String,
    stage: String,
    completed: usize,
    total: usize,
    processed_bytes: u64,
    total_bytes: u64,
}

#[derive(Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub(crate) enum RecoveryEvent {
    Progress(Progress),
    File(RecoveredFile),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchResult {
    pub files: Vec<RecoveredFile>,
    pub folder: Option<PathBuf>,
    pub cancelled: bool,
    pub report_warning: Option<String>,
}

fn create_output_folder(destination: &Path) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = format!("KSD Decrypt recovery {stamp}");
    for suffix in 1u64.. {
        let folder = destination.join(if suffix == 1 {
            name.clone()
        } else {
            format!("{name} ({suffix})")
        });
        match fs::create_dir(&folder) {
            Ok(()) => return Ok(folder),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(AppError::Native(format!(
                    "Cannot create the output folder: {error}"
                )));
            }
        }
    }
    unreachable!("output folder suffix exhausted")
}

fn write_reports(
    root: Option<&Path>,
    files: &[InputFile],
    results: &[RecoveredFile],
    cancelled: bool,
) -> Option<String> {
    let finished: HashSet<_> = results.iter().map(|result| &result.id).collect();
    let mut reports: BTreeMap<&Path, Vec<&RecoveredFile>> = BTreeMap::new();
    // Include unprocessed files, even if cancellation happens before the first
    // result, so every affected source folder gets an accurate report.
    for file in files {
        if let Some(folder) = root.or_else(|| file.path.parent()) {
            reports.entry(folder).or_default();
        }
    }
    for result in results {
        if let Some(folder) = root.or_else(|| result.source.parent()) {
            reports.entry(folder).or_default().push(result);
        }
    }
    let mut warnings = Vec::new();
    for (folder, folder_results) in reports {
        let remaining: Vec<_> = files
            .iter()
            .filter(|file| {
                !finished.contains(&file.id)
                    && (root.is_some() || file.path.parent() == Some(folder))
            })
            .map(|file| &file.path)
            .collect();
        let report = serde_json::json!({
            "application": concat!("KSD Decrypt ", env!("CARGO_PKG_VERSION")),
            "format": "Legacy Android KeepSafe AES-256-CTR",
            "cancelled": cancelled,
            "files": folder_results,
            "not_processed": remaining,
            "note": "Original files were opened read-only. Recognized format and structure checks cannot guarantee all media content is intact; this legacy format has no authentication tag."
        });
        if let Err(error) = recovery::write_report(folder, &report) {
            warnings.push(format!(
                "Could not save the recovery report in {}: {error}",
                folder.display()
            ));
        }
    }
    (!warnings.is_empty()).then(|| warnings.join(" "))
}

// The caller reserves the operation before starting the blocking worker.
pub(crate) fn recover_batch(
    state: &AppState,
    ids: Vec<String>,
    mut send: impl FnMut(RecoveryEvent),
) -> Result<BatchResult> {
    let (files, destination) = {
        let session = state.session()?;
        let requested: HashSet<_> = ids.into_iter().collect();
        let known: HashSet<_> = session.inputs.iter().map(|file| file.id.as_str()).collect();
        if requested.iter().any(|id| !known.contains(id.as_str())) {
            return Err(AppError::InvalidSelection);
        }
        let files: Vec<_> = session
            .inputs
            .iter()
            .filter(|file| {
                requested.contains(&file.id)
                    && file.issue.is_none()
                    && session
                        .results
                        .get(&file.id)
                        .is_none_or(|result| result.output.is_none())
            })
            .cloned()
            .collect();
        (files, session.destination.clone())
    };
    if files.is_empty() {
        return Err(AppError::EmptySelection);
    }
    let root = destination
        .as_deref()
        .map(create_output_folder)
        .transpose()?;
    let total_bytes = files.iter().fold(0u64, |total, file| {
        total.saturating_add(file.payload_bytes())
    });
    let mut processed = 0u64;
    let mut results = Vec::new();
    let mut cancelled = false;
    state.session()?.last_outputs.clear();
    for (index, file) in files.iter().enumerate() {
        if state.cancellation().load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let mut last_update = Instant::now() - Duration::from_secs(1);
        let progress = |bytes, stage: &str| {
            if last_update.elapsed() >= Duration::from_millis(80) || stage == "checking" {
                send(RecoveryEvent::Progress(Progress {
                    id: file.id.clone(),
                    stage: stage.into(),
                    completed: index,
                    total: files.len(),
                    processed_bytes: processed.saturating_add(bytes),
                    total_bytes,
                }));
                last_update = Instant::now();
            }
        };
        let result = match root.as_deref() {
            Some(root) => recovery::recover_one(file, root, state.cancellation(), progress),
            None => recovery::recover_beside_source(file, state.cancellation(), progress),
        };
        let result = match result {
            Ok(result) => result,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                cancelled = true;
                break;
            }
            Err(error) => RecoveredFile::failed(file, error),
        };
        {
            let mut session = state.session()?;
            if let Some(output) = &result.output {
                session.last_outputs.push(output.clone());
            }
            session.results.insert(result.id.clone(), result.clone());
        }
        send(RecoveryEvent::File(result.clone()));
        results.push(result);
        processed = processed.saturating_add(file.payload_bytes());
    }
    let report_warning = write_reports(root.as_deref(), &files, &results, cancelled);
    let outputs = &state.session()?.last_outputs;
    let folder = root.or_else(|| {
        let first = outputs.first()?.parent()?;
        outputs
            .iter()
            .all(|path| path.parent() == Some(first))
            .then(|| first.to_path_buf())
    });
    Ok(BatchResult {
        files: results,
        folder,
        cancelled,
        report_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected(names: &[&str]) -> (tempfile::TempDir, AppState) {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::default();
        for (index, name) in names.iter().enumerate() {
            let path = directory.path().join(name);
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/sample.jpg.ksd"),
                &path,
            )
            .unwrap();
            state
                .session()
                .unwrap()
                .inputs
                .push(recovery::inspect(path, index.to_string()));
        }
        (directory, state)
    }

    #[test]
    fn recovery_streams_results_and_skips_already_completed_ids() {
        let (directory, state) = selected(&["first.jpg.ksd", "second.jpg.ksd"]);
        let _guard = state.begin_recovery().unwrap();
        let mut events = Vec::new();
        let first = recover_batch(&state, vec!["0".into(), "0".into()], |event| {
            events.push(event)
        })
        .unwrap();
        assert_eq!(first.files.len(), 1);
        assert_eq!(first.folder.as_deref(), Some(directory.path()));
        assert!(matches!(events.first(), Some(RecoveryEvent::Progress(_))));
        assert!(matches!(events.last(), Some(RecoveryEvent::File(_))));
        let second = recover_batch(&state, vec!["0".into(), "1".into()], |_| {}).unwrap();
        assert_eq!(second.files.len(), 1);
        assert_eq!(second.files[0].id, "1");
        assert_eq!(state.session().unwrap().results.len(), 2);
        assert!(!directory.path().join("first (2).jpg").exists());
    }

    #[test]
    fn cancelling_before_the_first_result_reports_every_unprocessed_file() {
        let (directory, state) = selected(&["first.jpg.ksd", "second.jpg.ksd"]);
        let _guard = state.begin_recovery().unwrap();
        state.cancel();
        let batch = recover_batch(&state, vec!["0".into(), "1".into()], |_| {}).unwrap();
        assert!(batch.cancelled);
        assert!(batch.files.is_empty());
        let report: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.path().join("KSD Decrypt report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["not_processed"].as_array().unwrap().len(), 2);
        assert!(!directory.path().join("first.jpg").exists());
    }

    #[test]
    fn failed_files_do_not_prevent_other_files_from_recovering() {
        let (directory, state) = selected(&["first.jpg.ksd", "second.jpg.ksd"]);
        fs::remove_file(directory.path().join("first.jpg.ksd")).unwrap();
        let _guard = state.begin_recovery().unwrap();
        let batch = recover_batch(&state, vec!["0".into(), "1".into()], |_| {}).unwrap();
        assert_eq!(batch.files[0].status, "failed");
        assert_eq!(batch.files[1].status, "recovered");
        assert_eq!(state.session().unwrap().last_outputs.len(), 1);
    }

    #[test]
    fn invalid_ids_fail_before_creating_any_output() {
        let (directory, state) = selected(&["first.jpg.ksd"]);
        state.session().unwrap().destination = Some(directory.path().to_owned());
        let _guard = state.begin_recovery().unwrap();
        assert!(matches!(
            recover_batch(&state, vec!["stale".into()], |_| {}),
            Err(AppError::InvalidSelection)
        ));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn explicit_destination_uses_a_unique_grouped_folder() {
        let (_source, state) = selected(&["first.jpg.ksd"]);
        let destination = tempfile::tempdir().unwrap();
        state.session().unwrap().destination = Some(destination.path().to_owned());
        let _guard = state.begin_recovery().unwrap();
        let batch = recover_batch(&state, vec!["0".into()], |_| {}).unwrap();
        let root = batch.folder.unwrap();
        assert_eq!(root.parent(), Some(destination.path()));
        assert!(root.join("Photos/first.jpg").exists());
        assert!(root.join("KSD Decrypt report.json").exists());
        assert_ne!(create_output_folder(destination.path()).unwrap(), root);
    }

    #[test]
    fn channel_payloads_match_the_frontend_discriminated_union() {
        let event = RecoveryEvent::Progress(Progress {
            id: "one".into(),
            stage: "checking".into(),
            completed: 0,
            total: 1,
            processed_bytes: 42,
            total_bytes: 42,
        });
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({
                "event": "progress", "data": { "id": "one", "stage": "checking", "completed": 0,
                    "total": 1, "processedBytes": 42, "totalBytes": 42 }
            })
        );
    }
}
