#![allow(clippy::unwrap_used)]

use std::process::Command;

#[test]
fn mismatched_module_metadata_is_rejected_before_runtime_files_are_created() {
    let root = tempfile::tempdir().unwrap();
    let runtime_root = root.path().join("runtime-not-started");

    let output = Command::new(env!("CARGO_BIN_EXE_ucloned"))
        .env("UCLONE_BUILD_ID", "pending-module-version")
        .env("UCLONE_RUNTIME_ROOT", &runtime_root)
        .env_remove("UCLONE_RUNTIME_SOCKET")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!runtime_root.exists());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("module configuration does not match the executable build")
    );
}
