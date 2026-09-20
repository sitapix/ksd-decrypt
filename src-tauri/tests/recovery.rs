use ksd_decrypt_lib::recovery::{
    Format, inspect, probe, recover_beside_source, recover_one, safe_filename, write_report,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

#[test]
fn independent_python_fixtures_recover_byte_for_byte() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fixtures().join("manifest.json")).unwrap()).unwrap();
    let output = tempfile::tempdir().unwrap();
    for (i, fixture) in manifest.as_array().unwrap().iter().enumerate() {
        let name = fixture["name"].as_str().unwrap();
        let path = fixtures().join(name);
        let before = hash(&path);
        let input = inspect(path.clone(), i.to_string());
        assert!(input.issue.is_none(), "{name}: {:?}", input.issue);
        let recovered = recover_one(&input, output.path(), &AtomicBool::new(false), |_, _| {})
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            recovered.recovered_sha256.as_deref().unwrap(),
            fixture["sha256"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            fs::metadata(recovered.output.unwrap()).unwrap().len(),
            fixture["size"].as_u64().unwrap()
        );
        assert_eq!(before, hash(&path), "Source changed: {name}");
    }
}

#[test]
fn cancelled_file_is_not_published_and_source_is_unchanged() {
    let output = tempfile::tempdir().unwrap();
    let path = fixtures().join("chunk-boundary.jpg.ksd");
    let before = hash(&path);
    let input = inspect(path.clone(), "cancel".into());
    let cancel = AtomicBool::new(false);
    let error = recover_one(&input, output.path(), &cancel, |_, _| {
        cancel.store(true, Ordering::Relaxed);
    })
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    assert_eq!(
        fs::read_dir(output.path().join("Photos")).unwrap().count(),
        0
    );
    assert_eq!(before, hash(&path));
}

#[test]
fn duplicate_names_do_not_overwrite_previous_recoveries() {
    let output = tempfile::tempdir().unwrap();
    let input = inspect(fixtures().join("sample.jpg.ksd"), "same".into());
    let first = recover_one(&input, output.path(), &AtomicBool::new(false), |_, _| {})
        .unwrap()
        .output
        .unwrap();
    let second = recover_one(&input, output.path(), &AtomicBool::new(false), |_, _| {})
        .unwrap()
        .output
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(hash(&first), hash(&second));
}

#[test]
fn copies_save_beside_each_source_without_replacing_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let expected = "abdfdc3fe4e311cf5220d2e496e5e4d23db79cedfc481e372e0ca35b1d19d6f8";
    for folder_name in ["First backup", "Second backup"] {
        let folder = temp.path().join(folder_name);
        fs::create_dir(&folder).unwrap();
        let source = folder.join("sample.jpg.ksd");
        fs::copy(fixtures().join("sample.jpg.ksd"), &source).unwrap();
        fs::write(folder.join("sample.jpg"), b"existing photo").unwrap();
        let before = hash(&source);
        let input = inspect(source.clone(), folder_name.into());
        for suffix in [2, 3] {
            let recovered =
                recover_beside_source(&input, &AtomicBool::new(false), |_, _| {}).unwrap();
            let output = recovered.output.unwrap();
            assert_eq!(output, folder.join(format!("sample ({suffix}).jpg")));
            assert_eq!(hash(&output), expected);
        }
        assert_eq!(hash(&source), before);
        assert_eq!(
            fs::read(folder.join("sample.jpg")).unwrap(),
            b"existing photo"
        );
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 4);
    }
}

#[test]
fn cancelled_sibling_recovery_leaves_only_the_original() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("sample.jpg.ksd");
    fs::copy(fixtures().join("chunk-boundary.jpg.ksd"), &source).unwrap();
    let before = hash(&source);
    let input = inspect(source.clone(), "cancel".into());
    let cancel = AtomicBool::new(false);
    let error = recover_beside_source(&input, &cancel, |_, _| {
        cancel.store(true, Ordering::Relaxed);
    })
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    assert_eq!(hash(&source), before);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn read_only_source_folder_can_be_retried_in_another_location() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("sample.jpg.ksd");
    fs::copy(fixtures().join("sample.jpg.ksd"), &source).unwrap();
    let before = hash(&source);
    let input = inspect(source.clone(), "read-only".into());
    let permissions = fs::metadata(temp.path()).unwrap().permissions();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let attempt = recover_beside_source(&input, &AtomicBool::new(false), |_, _| {});
    fs::set_permissions(temp.path(), permissions).unwrap();
    let error = attempt.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(error.to_string().contains("Choose another output folder"));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    let elsewhere = tempfile::tempdir().unwrap();
    let recovered =
        recover_one(&input, elsewhere.path(), &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(
        recovered.output.unwrap(),
        elsewhere.path().join("Photos/sample.jpg")
    );
    assert_eq!(hash(&source), before);
}

#[test]
fn reports_do_not_replace_previous_reports() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("KSD Decrypt report.json");
    fs::write(&existing, b"previous report").unwrap();
    let report = serde_json::json!({ "files": [], "cancelled": false });
    for suffix in [2, 3] {
        let output = write_report(temp.path(), &report).unwrap();
        assert_eq!(
            output,
            temp.path()
                .join(format!("KSD Decrypt report ({suffix}).json"))
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(output).unwrap()).unwrap(),
            report
        );
    }
    assert_eq!(fs::read(existing).unwrap(), b"previous report");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
}

#[test]
fn damaged_header_and_wrong_key_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("broken.ksd");
    let mut data = fs::read(fixtures().join("sample.jpg.ksd")).unwrap();
    data[48] ^= 1;
    fs::write(&path, &data).unwrap();
    assert!(probe(&path).unwrap_err().to_string().contains("checksum"));
    let mut data = fs::read(fixtures().join("sample.jpg.ksd")).unwrap();
    data[3705] ^= 1;
    fs::write(&path, &data).unwrap();
    assert!(probe(&path).is_err());
}

#[test]
fn truncated_jpeg_is_not_published() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("truncated.jpg.ksd");
    let mut data = fs::read(fixtures().join("sample.jpg.ksd")).unwrap();
    data.truncate(data.len() - 30);
    fs::write(&path, data).unwrap();
    let output = tempfile::tempdir().unwrap();
    let input = inspect(path, "truncated".into());
    assert!(recover_one(&input, output.path(), &AtomicBool::new(false), |_, _| {}).is_err());
    assert_eq!(
        fs::read_dir(output.path().join("Photos")).unwrap().count(),
        0
    );
}

#[test]
fn output_names_are_portable_and_preserve_jpeg_aliases() {
    let format = Format {
        extension: "jpg".into(),
        mime: "image/jpeg".into(),
    };
    assert_eq!(
        safe_filename(Path::new("0000000000000.photo.jpeg.ksd"), &format),
        "photo.jpeg"
    );
    assert_eq!(
        safe_filename(Path::new("CON.jpg.ksd"), &format),
        "Recovered-CON.jpg"
    );
    assert_eq!(
        safe_filename(Path::new("a:b?.jpg.ksd"), &format),
        "a_b_.jpg"
    );
}
