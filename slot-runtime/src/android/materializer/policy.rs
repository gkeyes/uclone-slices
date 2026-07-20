use std::path::Path;

use crate::domain::{ManagedPackage, PackageName, SlotId, UserId};
use crate::layout::RuntimeLayout;
use crate::materializer::{BackendFailure, DataDomain, MaterializationPaths};

const ANCHOR_CE: &str = "/data/misc_ce/0";
const ANCHOR_DE: &str = "/data/misc_de/0";

pub(super) fn ensure_supported(package: &ManagedPackage) -> Result<(), BackendFailure> {
    if package.user_id() != UserId::PRIMARY {
        return Err(BackendFailure::new("user_not_supported"));
    }
    Ok(())
}

pub(super) fn validate_paths(paths: &MaterializationPaths) -> Result<(), BackendFailure> {
    if paths.slot().is_base() {
        return Err(BackendFailure::new("materialization_paths_rejected"));
    }
    let expected = RuntimeLayout::slot_paths(paths.package(), paths.slot());
    if paths.ready_ce() != expected.ce()
        || paths.ready_de() != expected.de()
        || paths.staging_ce()
            != expected
                .ce()
                .with_file_name(format!(".{}.staging", paths.slot().as_str()))
        || paths.staging_de()
            != expected
                .de()
                .with_file_name(format!(".{}.staging", paths.slot().as_str()))
    {
        return Err(BackendFailure::new("materialization_paths_rejected"));
    }
    Ok(())
}

pub(super) fn base(package: &PackageName, domain: DataDomain) -> std::path::PathBuf {
    let base = RuntimeLayout::slot_paths(package, &SlotId::base());
    match domain {
        DataDomain::Ce => base.ce().to_path_buf(),
        DataDomain::De => base.de().to_path_buf(),
    }
}

pub(super) fn staging(paths: &MaterializationPaths, domain: DataDomain) -> &Path {
    match domain {
        DataDomain::Ce => paths.staging_ce(),
        DataDomain::De => paths.staging_de(),
    }
}

pub(super) fn anchor(domain: DataDomain) -> &'static Path {
    match domain {
        DataDomain::Ce => Path::new(ANCHOR_CE),
        DataDomain::De => Path::new(ANCHOR_DE),
    }
}

pub(super) fn parent(paths: &MaterializationPaths, domain: DataDomain) -> &Path {
    match domain {
        DataDomain::Ce => paths
            .ready_ce()
            .parent()
            .unwrap_or_else(|| paths.ready_ce()),
        DataDomain::De => paths
            .ready_de()
            .parent()
            .unwrap_or_else(|| paths.ready_de()),
    }
}
