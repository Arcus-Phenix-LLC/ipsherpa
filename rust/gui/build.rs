fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["list_scopes"])),
    )
    .expect("Failed to build IPSherpa GUI resources");
}
