use crate::{
    error::Result,
    recovery::{self, InputFile},
    session::AppState,
};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub(crate) struct Selection {
    pub files: Vec<InputFile>,
    pub warnings: Vec<String>,
    pub message: Option<&'static str>,
}

fn is_ksd(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ksd"))
}

// Called on a blocking worker with an operation reservation held by the caller.
pub(crate) fn collect_paths(paths: Vec<PathBuf>, state: &AppState) -> Result<Selection> {
    let has_input = !paths.is_empty();
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in walkdir::WalkDir::new(path).follow_links(false) {
                match entry {
                    Ok(entry) if entry.file_type().is_file() && is_ksd(entry.path()) => {
                        candidates.push(entry.into_path());
                    }
                    Err(error) => warnings.push(format!("Some files could not be read: {error}")),
                    _ => (),
                }
            }
        } else if path.is_file() {
            if is_ksd(&path) {
                candidates.push(path);
            }
        } else {
            warnings.push(format!("File not found: {}", path.display()));
        }
    }
    let message = (has_input && candidates.is_empty() && warnings.is_empty())
        .then_some("No .ksd files found.");
    candidates.sort();
    let (mut seen, first_id) = {
        let session = state.session()?;
        (
            session
                .inputs
                .iter()
                .map(|file| file.path.clone())
                .collect::<HashSet<_>>(),
            session.inputs.len(),
        )
    };
    // Inspect files outside the session lock, so window events never wait on I/O.
    let mut added = Vec::new();
    for path in candidates {
        match path.canonicalize() {
            Ok(path) if seen.insert(path.clone()) => {
                let id = format!("file-{}", first_id + added.len());
                added.push(recovery::inspect(path, id));
            }
            Err(error) => warnings.push(error.to_string()),
            _ => (),
        }
    }
    let mut session = state.session()?;
    session.inputs.extend(added);
    Ok(Selection {
        files: session.inputs.clone(),
        warnings,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    #[test]
    fn mixed_file_and_folder_selection_recurses_and_deduplicates() {
        let source = tempfile::tempdir().unwrap();
        let folder = source.path().join("Backup");
        fs::create_dir_all(folder.join("Nested")).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/sample.jpg.ksd");
        let nested = folder.join("Nested/photo.ksd");
        let separate = source.path().join("Separate.KSD");
        fs::copy(&fixture, &nested).unwrap();
        fs::copy(&fixture, &separate).unwrap();
        let unrelated = folder.join("ignore.md");
        fs::write(&unrelated, "not a vault file").unwrap();
        let state = AppState::default();
        let _guard = state.begin().unwrap();
        let selection = collect_paths(
            vec![folder, unrelated.clone(), nested.clone(), separate.clone()],
            &state,
        )
        .unwrap();
        assert_eq!(selection.files.len(), 2);
        assert!(selection.warnings.is_empty());
        assert!(selection.message.is_none());
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "not a vault file");
        assert!(selection.files.iter().all(|file| file.issue.is_none()));
        assert_eq!(
            selection
                .files
                .iter()
                .map(|file| &file.path)
                .collect::<HashSet<_>>(),
            HashSet::from([
                &nested.canonicalize().unwrap(),
                &separate.canonicalize().unwrap()
            ])
        );
        assert_eq!(
            selection
                .files
                .iter()
                .map(|file| &file.id)
                .collect::<HashSet<_>>()
                .len(),
            2
        );
    }

    #[test]
    fn unrelated_files_and_empty_folders_keep_the_queue_empty() {
        let folder = tempfile::tempdir().unwrap();
        let document = folder.path().join("notes.md");
        fs::write(&document, "notes").unwrap();
        let state = AppState::default();
        let _guard = state.begin().unwrap();
        for paths in [vec![document.clone()], vec![folder.path().to_path_buf()]] {
            let selection = collect_paths(paths, &state).unwrap();
            assert!(selection.files.is_empty());
            assert!(selection.warnings.is_empty());
            assert_eq!(selection.message, Some("No .ksd files found."));
        }
        assert_eq!(fs::read_to_string(document).unwrap(), "notes");
        let cancelled = collect_paths(Vec::new(), &state).unwrap();
        assert!(cancelled.message.is_none());
    }

    #[test]
    fn unsupported_ksd_stays_visible_and_readding_it_is_quiet() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("unsupported.ksd");
        fs::write(&file, vec![0; 4096]).unwrap();
        let state = AppState::default();
        let _guard = state.begin().unwrap();
        for _ in 0..2 {
            let selection = collect_paths(vec![file.clone()], &state).unwrap();
            assert_eq!(selection.files.len(), 1);
            assert_eq!(
                selection.files[0].issue.as_deref(),
                Some("Unsupported format.")
            );
            assert!(selection.message.is_none());
        }
        let unrelated = folder.path().join("notes.md");
        fs::write(&unrelated, "notes").unwrap();
        let selection = collect_paths(vec![unrelated], &state).unwrap();
        assert_eq!(selection.files.len(), 1);
        assert_eq!(selection.files[0].name, "unsupported.ksd");
    }
}
