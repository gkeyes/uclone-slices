use serde::Serialize;

use super::revision::SCHEMA_VERSION;
use super::{PackageRevision, RegistryError};
use crate::domain::{
    AppIdentity, CommitNonce, DataInodes, PackageName, SlotId, TransactionId, UserId,
};
use crate::integrity::digest_json;
use crate::lifecycle::LifecycleState;

pub(super) fn publish(
    draft: &PackageRevision,
    previous: Option<&PackageRevision>,
) -> Result<PackageRevision, RegistryError> {
    validate_draft(draft, previous)?;
    let generation = next_generation(previous)?;
    let previous_sha256 = previous.map(|value| value.sha256.clone());
    let mut revision = PackageRevision {
        schema_version: SCHEMA_VERSION,
        generation,
        package_name: draft.package_name.clone(),
        user_id: draft.user_id,
        identity: draft.identity.clone(),
        lifecycle_state: draft.lifecycle_state,
        base_inodes: draft.base_inodes,
        previous_slot: draft.previous_slot.clone(),
        previous_inodes: draft.previous_inodes,
        active_slot: draft.active_slot.clone(),
        active_inodes: draft.active_inodes,
        transaction_id: draft.transaction_id.clone(),
        commit_nonce: draft.commit_nonce.clone(),
        previous_sha256,
        sha256: String::new(),
    };
    revision.sha256 = revision_digest(&revision)?;
    Ok(revision)
}

pub(super) fn verify(
    revision: &PackageRevision,
    previous: Option<&PackageRevision>,
) -> Result<(), RegistryError> {
    if revision.schema_version != SCHEMA_VERSION || revision.generation == 0 {
        return Err(RegistryError::Corrupt(
            "unsupported schema or zero generation".to_owned(),
        ));
    }
    validate_slot_shapes(revision).map_err(|error| RegistryError::Corrupt(error.to_string()))?;
    let expected_generation = next_generation(previous)?;
    let expected_previous = previous.map(|value| value.sha256.as_str());
    if revision.generation != expected_generation
        || revision.previous_sha256.as_deref() != expected_previous
    {
        return Err(RegistryError::Corrupt(
            "generation or previous digest mismatch".to_owned(),
        ));
    }
    if revision_digest(revision)? != revision.sha256 {
        return Err(RegistryError::Corrupt("digest mismatch".to_owned()));
    }
    validate_chain_position(revision, previous)
}

pub(super) fn revision_digest(revision: &PackageRevision) -> Result<String, RegistryError> {
    let unsigned = UnsignedRevision::new(
        revision,
        revision.generation,
        revision.previous_sha256.as_deref(),
    );
    Ok(digest_json(&unsigned)?)
}

fn validate_draft(
    draft: &PackageRevision,
    previous: Option<&PackageRevision>,
) -> Result<(), RegistryError> {
    if draft.schema_version != SCHEMA_VERSION || draft.generation != 0 || !draft.sha256.is_empty() {
        return Err(RegistryError::InvalidRevision(
            "only unpublished revisions may be appended".to_owned(),
        ));
    }
    validate_slot_shapes(draft)?;
    match previous {
        Some(value) => {
            validate_stream_identity(draft, value)?;
            if draft.previous_slot != value.active_slot
                || draft.previous_inodes != value.active_inodes
            {
                return Err(RegistryError::InvalidRevision(
                    "revision does not continue the latest committed view".to_owned(),
                ));
            }
            if draft.transaction_id == value.transaction_id {
                return Err(RegistryError::InvalidRevision(
                    "revision reuses the previous transaction id".to_owned(),
                ));
            }
        }
        None if !starts_from_base(draft) => {
            return Err(RegistryError::InvalidRevision(
                "first revision must start from the base anchor".to_owned(),
            ));
        }
        None => {}
    }
    Ok(())
}

fn validate_chain_position(
    revision: &PackageRevision,
    previous: Option<&PackageRevision>,
) -> Result<(), RegistryError> {
    match previous {
        Some(value) => {
            validate_stream_identity(revision, value)
                .map_err(|error| RegistryError::Corrupt(error.to_string()))?;
            if revision.previous_slot != value.active_slot
                || revision.previous_inodes != value.active_inodes
            {
                return Err(RegistryError::Corrupt(
                    "revision chain skips a committed view".to_owned(),
                ));
            }
        }
        None if !starts_from_base(revision) => {
            return Err(RegistryError::Corrupt(
                "first revision does not start from base".to_owned(),
            ));
        }
        None => {}
    }
    Ok(())
}

fn validate_stream_identity(
    candidate: &PackageRevision,
    previous: &PackageRevision,
) -> Result<(), RegistryError> {
    if candidate.package_name != previous.package_name
        || candidate.user_id != previous.user_id
        || candidate.identity != previous.identity
        || candidate.lifecycle_state != previous.lifecycle_state
        || candidate.base_inodes != previous.base_inodes
    {
        return Err(RegistryError::InvalidRevision(
            "revision changes the Registry stream identity".to_owned(),
        ));
    }
    Ok(())
}

fn validate_slot_shapes(revision: &PackageRevision) -> Result<(), RegistryError> {
    if !view_matches_base_rule(
        &revision.previous_slot,
        revision.previous_inodes,
        revision.base_inodes,
    ) || !view_matches_base_rule(
        &revision.active_slot,
        revision.active_inodes,
        revision.base_inodes,
    ) {
        return Err(RegistryError::InvalidRevision(
            "slot views do not preserve the immutable base inode anchor".to_owned(),
        ));
    }
    if revision.previous_slot != revision.active_slot
        && revision.previous_inodes == revision.active_inodes
    {
        return Err(RegistryError::InvalidRevision(
            "distinct slots share the same inode pair".to_owned(),
        ));
    }
    Ok(())
}

fn starts_from_base(revision: &PackageRevision) -> bool {
    revision.previous_slot.is_base() && revision.previous_inodes == revision.base_inodes
}

fn view_matches_base_rule(slot: &SlotId, inodes: DataInodes, base_inodes: DataInodes) -> bool {
    slot.is_base() == (inodes == base_inodes)
}

fn next_generation(previous: Option<&PackageRevision>) -> Result<u64, RegistryError> {
    previous.map_or(Ok(1), |value| {
        value
            .generation
            .checked_add(1)
            .ok_or_else(|| RegistryError::Corrupt("generation overflow".to_owned()))
    })
}

#[derive(Serialize)]
struct UnsignedRevision<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    identity: &'a AppIdentity,
    lifecycle_state: LifecycleState,
    base_inodes: DataInodes,
    previous_slot: &'a SlotId,
    previous_inodes: DataInodes,
    active_slot: &'a SlotId,
    active_inodes: DataInodes,
    transaction_id: &'a TransactionId,
    commit_nonce: &'a CommitNonce,
    previous_sha256: Option<&'a str>,
}

impl<'a> UnsignedRevision<'a> {
    const fn new(
        value: &'a PackageRevision,
        generation: u64,
        previous_sha256: Option<&'a str>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generation,
            package_name: &value.package_name,
            user_id: value.user_id,
            identity: &value.identity,
            lifecycle_state: value.lifecycle_state,
            base_inodes: value.base_inodes,
            previous_slot: &value.previous_slot,
            previous_inodes: value.previous_inodes,
            active_slot: &value.active_slot,
            active_inodes: value.active_inodes,
            transaction_id: &value.transaction_id,
            commit_nonce: &value.commit_nonce,
            previous_sha256,
        }
    }
}
