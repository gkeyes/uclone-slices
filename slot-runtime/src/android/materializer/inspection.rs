use std::path::Path;

use crate::catalog::{PathSecurityProof, SecurityProfileProof};
use crate::domain::DataInodes;
use crate::materializer::{
    BackendFailure, BaseAnchor, ContentProof, DataBytes, DataDomain, DirectoryAnchor,
    SlotMaterializationProof, TreeSafetyProof,
};

use super::evidence::capture_security;
use super::executor::MaterializerExecutor;
use super::limits::MaterializerLimits;
use super::tree::{TreeInspection, inspect_tree};

#[derive(Debug)]
struct DomainInspection {
    tree: TreeInspection,
    security: PathSecurityProof,
}

pub(super) fn inspect_base<E: MaterializerExecutor>(
    executor: &mut E,
    ce_path: &Path,
    de_path: &Path,
    limits: MaterializerLimits,
) -> Result<BaseAnchor, BackendFailure> {
    let ce = inspect_domain(executor, ce_path, DataDomain::Ce, limits)?;
    let de = inspect_domain(executor, de_path, DataDomain::De, limits)?;
    if ce.tree.safety() != TreeSafetyProof::clean() || de.tree.safety() != TreeSafetyProof::clean()
    {
        return Err(BackendFailure::new("base_tree_unsafe"));
    }
    BaseAnchor::new(
        inodes(&ce, &de)?,
        anchor(&ce)?,
        anchor(&de)?,
        SecurityProfileProof::new(ce.security, de.security),
        DataBytes::new(ce.tree.total_bytes(), de.tree.total_bytes()),
    )
    .map_err(|_| BackendFailure::new("base_proof_invalid"))
}

pub(super) fn inspect_pair<E: MaterializerExecutor>(
    executor: &mut E,
    ce_path: &Path,
    de_path: &Path,
    limits: MaterializerLimits,
) -> Result<SlotMaterializationProof, BackendFailure> {
    let ce = inspect_domain(executor, ce_path, DataDomain::Ce, limits)?;
    let de = inspect_domain(executor, de_path, DataDomain::De, limits)?;
    let contents = ContentProof::new(ce.tree.digest(), de.tree.digest())
        .map_err(|_| BackendFailure::new("content_proof_invalid"))?;
    SlotMaterializationProof::new(
        inodes(&ce, &de)?,
        anchor(&ce)?,
        anchor(&de)?,
        contents,
        SecurityProfileProof::new(ce.security, de.security),
        ce.tree.safety(),
        de.tree.safety(),
        true,
    )
    .map_err(|_| BackendFailure::new("slot_proof_invalid"))
}

fn inspect_domain<E: MaterializerExecutor>(
    executor: &mut E,
    path: &Path,
    domain: DataDomain,
    limits: MaterializerLimits,
) -> Result<DomainInspection, BackendFailure> {
    let tree = inspect_tree(path, limits).map_err(|error| BackendFailure::new(error.code()))?;
    let security = capture_security(executor, path, domain, &tree)?;
    Ok(DomainInspection { tree, security })
}

fn inodes(ce: &DomainInspection, de: &DomainInspection) -> Result<DataInodes, BackendFailure> {
    DataInodes::new(ce.tree.inode(), de.tree.inode())
        .map_err(|_| BackendFailure::new("inode_proof_invalid"))
}

fn anchor(domain: &DomainInspection) -> Result<DirectoryAnchor, BackendFailure> {
    DirectoryAnchor::new(domain.tree.device_id(), domain.tree.digest())
        .map_err(|_| BackendFailure::new("directory_anchor_invalid"))
}
