use crate::catalog::PathSecurityProof;
use crate::domain::{ManagedPackage, SlotId};
use crate::lifecycle::LifecycleState;

use super::{
    BaseAnchor, DataDomain, DomainCopyProof, MaterializationError, SlotMaterializationProof,
};

pub(super) fn request(package: &ManagedPackage, slot: &SlotId) -> Result<(), MaterializationError> {
    if slot.as_str() != crate::target::PREVIEW_SLOT {
        return Err(MaterializationError::UnsupportedSlot(slot.clone()));
    }
    if !package.active_slot().is_base()
        || package.active_inodes() != package.base_inodes()
        || package.lifecycle_state() != LifecycleState::Normal
    {
        return Err(MaterializationError::SourceNotBase);
    }
    Ok(())
}

pub(super) fn initial_base(
    package: &ManagedPackage,
    base: &BaseAnchor,
) -> Result<(), MaterializationError> {
    if base.inodes() != package.base_inodes()
        || base.security().ce().uid() != package.identity().uid()
        || base.security().de().uid() != package.identity().uid()
    {
        return Err(MaterializationError::BaseAnchorMismatch);
    }
    Ok(())
}

pub(super) fn copied_domain(
    base: &BaseAnchor,
    domain: DataDomain,
    proof: &DomainCopyProof,
) -> Result<(), MaterializationError> {
    if proof.domain() != domain {
        return Err(MaterializationError::CopyProofDomainMismatch(domain));
    }
    if let Some(artifact) = proof.safety().first_violation() {
        return Err(MaterializationError::UnsafeTree { domain, artifact });
    }
    if proof.content() != base.domain(domain).content() {
        return Err(MaterializationError::ContentMismatch(domain));
    }
    Ok(())
}

pub(super) fn staging(
    package: &ManagedPackage,
    base: &BaseAnchor,
    proof: &SlotMaterializationProof,
) -> Result<(), MaterializationError> {
    if !proof.is_complete() || reuses_base_or_pair(package, proof) {
        return Err(MaterializationError::StagingIncomplete);
    }
    for domain in [DataDomain::Ce, DataDomain::De] {
        if let Some(artifact) = proof.safety(domain).first_violation() {
            return Err(MaterializationError::UnsafeTree { domain, artifact });
        }
        if proof.domain(domain).device_id() != base.domain(domain).device_id() {
            return Err(MaterializationError::DeviceMismatch(domain));
        }
        if proof.domain(domain).content() != base.domain(domain).content() {
            return Err(MaterializationError::ContentMismatch(domain));
        }
        validate_security(base, proof, domain)?;
    }
    Ok(())
}

pub(super) fn unchanged_base(
    expected: &BaseAnchor,
    observed: &BaseAnchor,
) -> Result<(), MaterializationError> {
    if expected == observed {
        Ok(())
    } else {
        Err(MaterializationError::BaseChanged)
    }
}

fn reuses_base_or_pair(package: &ManagedPackage, proof: &SlotMaterializationProof) -> bool {
    let base = package.base_inodes();
    let slot = proof.inodes();
    slot.ce() == slot.de()
        || slot.ce() == base.ce()
        || slot.ce() == base.de()
        || slot.de() == base.ce()
        || slot.de() == base.de()
}

fn validate_security(
    base: &BaseAnchor,
    proof: &SlotMaterializationProof,
    domain: DataDomain,
) -> Result<(), MaterializationError> {
    let expected = path_security(base, domain);
    let observed = path_security_slot(proof, domain);
    if expected.uid() != observed.uid()
        || expected.gid() != observed.gid()
        || expected.mode() != observed.mode()
        || expected.selinux_context() != observed.selinux_context()
    {
        return Err(MaterializationError::SecurityMismatch(domain));
    }
    if expected.fscrypt_policy_sha256() != observed.fscrypt_policy_sha256() {
        return Err(MaterializationError::FscryptMismatch(domain));
    }
    Ok(())
}

const fn path_security(base: &BaseAnchor, domain: DataDomain) -> &PathSecurityProof {
    match domain {
        DataDomain::Ce => base.security().ce(),
        DataDomain::De => base.security().de(),
    }
}

const fn path_security_slot(
    proof: &SlotMaterializationProof,
    domain: DataDomain,
) -> &PathSecurityProof {
    match domain {
        DataDomain::Ce => proof.security().ce(),
        DataDomain::De => proof.security().de(),
    }
}
