fn main() {
    tauri_build::build();

    // On Windows the test executable of this crate fails to start (STATUS_ENTRYPOINT_NOT_FOUND)
    // unless it asks for the version 6 common controls that Tauri's window code needs. The
    // manifest tauri-build embeds only reaches the app itself.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-tests.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
