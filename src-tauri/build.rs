fn main() {
    #[cfg(windows)]
    {
        // Tauri's default manifest injection only covers the main binary. Embed
        // the manifest into every artifact so test binaries can also load the
        // Tauri dependencies (Common Controls v6 is required).
        embed_resource::compile_for_everything("windows-app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed the Windows manifest");
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("failed to run tauri-build");
    }

    #[cfg(not(windows))]
    {
        tauri_build::build();
    }
}
