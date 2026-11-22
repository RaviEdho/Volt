fn main() {
    #[cfg(windows)]
    {
        // The Windows resource compiler misreads apostrophes in Tauri's icon path.
        let icon_path = std::fs::canonicalize("icons/icon.ico").expect("missing Volt icon");
        if icon_path.to_string_lossy().contains('\'') {
            println!("cargo:rerun-if-changed=icons/icon.ico");
            let temp = tempfile::tempdir().expect("failed to create a temporary icon directory");
            let staged_icon = temp.path().join("volt.ico");
            std::fs::copy(icon_path, &staged_icon).expect("failed to stage the Volt icon");

            let windows = tauri_build::WindowsAttributes::new().window_icon_path(staged_icon);
            tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
                .expect("failed to prepare Volt's build resources");
            return;
        }
    }

    tauri_build::build();
}
