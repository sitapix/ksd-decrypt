fn main() {
    // Windows resources must be regenerated when the icon artwork changes.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "choose_inputs",
            "add_paths",
            "choose_output",
            "recover",
            "cancel_recovery",
            "clear_selection",
            "open_result",
        ]),
    ))
    .expect("Unable to build the Tauri application manifest");
}
