use std::path::Path;

use crate::domain::{ManagedPackage, UserId};
use crate::materializer::{BackendFailure, DataDomain, MaterializationPaths};
use crate::target;

const ANCHOR_CE: &str = "/data/misc_ce/0";
const ANCHOR_DE: &str = "/data/misc_de/0";

pub(super) fn ensure_supported(package: &ManagedPackage) -> Result<(), BackendFailure> {
    if package.user_id() != UserId::PRIMARY {
        return Err(BackendFailure::new("user_not_supported"));
    }
    if package.package_name().as_str() != target::PACKAGE {
        return Err(BackendFailure::new("package_not_allowlisted"));
    }
    Ok(())
}

pub(super) fn validate_paths(paths: &MaterializationPaths) -> Result<(), BackendFailure> {
    if paths.ready_ce() != Path::new(target::PREVIEW_CE)
        || paths.ready_de() != Path::new(target::PREVIEW_DE)
        || paths.staging_ce() != Path::new(target::STAGING_CE)
        || paths.staging_de() != Path::new(target::STAGING_DE)
    {
        return Err(BackendFailure::new("materialization_paths_rejected"));
    }
    Ok(())
}

pub(super) fn base(domain: DataDomain) -> &'static Path {
    match domain {
        DataDomain::Ce => Path::new(target::TARGET_CE),
        DataDomain::De => Path::new(target::TARGET_DE),
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

pub(super) fn parent(domain: DataDomain) -> &'static Path {
    match domain {
        DataDomain::Ce => Path::new(target::TARGET_CE_SLOT_ROOT),
        DataDomain::De => Path::new(target::TARGET_DE_SLOT_ROOT),
    }
}
