fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_scopes",
            "list_records",
            "scope_capacity",
            "inspect_dhcp",
            "preview_reservation",
            "create_reservation",
        ]),
    ))
    .expect("Failed to build IPSherpa GUI resources");
}
