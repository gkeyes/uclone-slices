//! Generates immutable Rust target constants from the selected canonical profile.

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    let profile = env::var("UCLONE_TARGET_PROFILE").unwrap_or_else(|_| "slotprobe".to_owned());
    if !matches!(profile.as_str(), "slotprobe" | "fitness") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "UCLONE_TARGET_PROFILE must be exactly slotprobe or fitness",
        )
        .into());
    }

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "CARGO_MANIFEST_DIR is missing")
        })?);
    let repository_root = manifest_dir.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime has no repository parent",
        )
    })?;
    let renderer = repository_root.join("tools/render-target-profile.sh");
    let output_root = PathBuf::from(
        env::var_os("OUT_DIR")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is missing"))?,
    );
    let generated_root = output_root.join(format!("target-profile-{profile}"));
    if generated_root.exists() {
        fs::remove_dir_all(&generated_root)?;
    }
    let status = Command::new(&renderer)
        .arg(&profile)
        .arg(&output_root)
        .status()?;
    if !status.success() {
        return Err(io::Error::other("target profile renderer rejected the Cargo build").into());
    }
    let generated = generated_root.join("target_profile.rs");
    if !generated.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "renderer did not produce target_profile.rs",
        )
        .into());
    }

    println!("cargo:rerun-if-env-changed=UCLONE_TARGET_PROFILE");
    println!("cargo:rerun-if-changed={}", renderer.display());
    println!(
        "cargo:rerun-if-changed={}",
        repository_root
            .join(format!("slot-targets/{profile}.toml"))
            .display()
    );
    println!("cargo:rustc-env=UCLONE_TARGET_RS={}", generated.display());
    Ok(())
}
