fn main() {
    // "google-signin" is a small plugin of the app itself (Android: GoogleSignIn.kt); its one
    // command is allowed for the app window.
    tauri_build::try_build(
        tauri_build::Attributes::new().plugin(
            "google-signin",
            tauri_build::InlinedPlugin::new()
                .commands(&["sign_in"])
                .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands),
        ),
    )
    .expect("failed to run tauri-build");
}
