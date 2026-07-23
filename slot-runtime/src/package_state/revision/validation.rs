use crate::domain::ManagedUpdateContext;
use crate::lifecycle::LifecycleState;

use super::{PackageStateRevision, V1_SCHEMA_VERSION, V2_SCHEMA_VERSION, semantics};
use crate::package_state::{PackageStateError, PackageStateReason};

pub(super) fn verify(
    revision: &PackageStateRevision,
    previous: Option<&PackageStateRevision>,
) -> Result<(), PackageStateError> {
    verify_link(revision, previous)?;
    match (
        previous.map(PackageStateRevision::schema_version),
        revision.schema_version,
    ) {
        (None, V1_SCHEMA_VERSION) => verify_v1(revision, true),
        (Some(V1_SCHEMA_VERSION), V1_SCHEMA_VERSION) => verify_v1_transition(revision, previous),
        (None, V2_SCHEMA_VERSION) => verify_v2_initial(revision),
        (Some(V1_SCHEMA_VERSION), V2_SCHEMA_VERSION) => verify_v1_migration(revision, previous),
        (Some(V2_SCHEMA_VERSION), V2_SCHEMA_VERSION) => verify_v2_transition(revision, previous),
        _ => Err(PackageStateError::Corrupt(
            "unsupported package state schema transition".to_owned(),
        )),
    }
}

fn verify_link(
    revision: &PackageStateRevision,
    previous: Option<&PackageStateRevision>,
) -> Result<(), PackageStateError> {
    if revision.generation == 0 {
        return Err(PackageStateError::Corrupt("zero generation".to_owned()));
    }
    let expected_generation = previous.map_or(Ok(1), |value| {
        value
            .generation
            .checked_add(1)
            .ok_or_else(|| PackageStateError::Corrupt("generation overflow".to_owned()))
    })?;
    if revision.generation != expected_generation
        || revision.previous_sha256.as_deref() != previous.map(PackageStateRevision::sha256)
    {
        return Err(PackageStateError::Corrupt(
            "generation or previous digest mismatch".to_owned(),
        ));
    }
    if revision.digest()? != revision.sha256 {
        return Err(PackageStateError::Corrupt("digest mismatch".to_owned()));
    }
    if let Some(previous) = previous
        && (revision.package_name != previous.package_name || revision.user_id != previous.user_id)
    {
        return Err(PackageStateError::Corrupt(
            "revision changes package stream identity".to_owned(),
        ));
    }
    Ok(())
}

fn verify_v1(revision: &PackageStateRevision, first: bool) -> Result<(), PackageStateError> {
    if revision.accepted_artifact.is_some()
        || revision.managed_update_context.is_some()
        || !semantics::reason_matches_v1(revision.lifecycle_state, revision.reason, first)
    {
        return Err(PackageStateError::Corrupt(
            "invalid schema-v1 lifecycle revision".to_owned(),
        ));
    }
    Ok(())
}

fn verify_v1_transition(
    revision: &PackageStateRevision,
    previous: Option<&PackageStateRevision>,
) -> Result<(), PackageStateError> {
    let previous = previous.ok_or_else(|| PackageStateError::Corrupt("missing head".to_owned()))?;
    verify_v1(revision, false)?;
    semantics::verify_lifecycle_transition(previous, revision)
}

fn verify_v2_initial(revision: &PackageStateRevision) -> Result<(), PackageStateError> {
    verify_v2_shape(revision)?;
    if revision.lifecycle_state != LifecycleState::Normal
        || revision.reason != PackageStateReason::Enrolled
    {
        return Err(PackageStateError::Corrupt(
            "schema-v2 stream must initialize in normal state".to_owned(),
        ));
    }
    Ok(())
}

fn verify_v1_migration(
    revision: &PackageStateRevision,
    previous: Option<&PackageStateRevision>,
) -> Result<(), PackageStateError> {
    let previous = previous.ok_or_else(|| PackageStateError::Corrupt("missing head".to_owned()))?;
    verify_v2_shape(revision)?;
    if previous.lifecycle_state != LifecycleState::Normal
        || revision.lifecycle_state != LifecycleState::Normal
        || revision.reason != PackageStateReason::Enrolled
        || revision.managed_update_context.is_some()
    {
        return Err(PackageStateError::Corrupt(
            "invalid schema-v1 to schema-v2 normal migration".to_owned(),
        ));
    }
    Ok(())
}

fn verify_v2_transition(
    revision: &PackageStateRevision,
    previous: Option<&PackageStateRevision>,
) -> Result<(), PackageStateError> {
    let previous = previous.ok_or_else(|| PackageStateError::Corrupt("missing head".to_owned()))?;
    verify_v2_shape(revision)?;
    semantics::verify_lifecycle_transition(previous, revision)?;
    if !semantics::reason_matches_v2(
        previous.lifecycle_state,
        revision.lifecycle_state,
        revision.reason,
    ) {
        return Err(PackageStateError::Corrupt(
            "lifecycle state and reason do not match".to_owned(),
        ));
    }
    verify_accepted_artifact(previous, revision)?;
    verify_update_continuity(previous, revision)
}

fn verify_v2_shape(revision: &PackageStateRevision) -> Result<(), PackageStateError> {
    if revision.accepted_artifact.is_none() {
        return Err(PackageStateError::Corrupt(
            "schema-v2 revision requires accepted artifact".to_owned(),
        ));
    }
    let context = revision.managed_update_context.as_ref();
    let valid = match revision.lifecycle_state {
        LifecycleState::Normal
        | LifecycleState::LifecycleDrifted
        | LifecycleState::RepairWaiting
        | LifecycleState::RecoveryRequired
        | LifecycleState::Quarantined => context.is_none(),
        LifecycleState::UpdatePreparing | LifecycleState::UpdateWindowOpen => {
            context.is_some_and(|value| value.candidate_artifact().is_none())
        }
        LifecycleState::UpdateVerifying => {
            context.is_some_and(|value| value.candidate_artifact().is_some())
        }
    };
    if !valid {
        return Err(PackageStateError::Corrupt(
            "schema-v2 lifecycle/update context mismatch".to_owned(),
        ));
    }
    Ok(())
}

fn verify_accepted_artifact(
    previous: &PackageStateRevision,
    revision: &PackageStateRevision,
) -> Result<(), PackageStateError> {
    let previous_artifact = previous.accepted_artifact.as_ref().ok_or_else(|| {
        PackageStateError::Corrupt("previous v2 revision is missing accepted artifact".to_owned())
    })?;
    let expected = if previous.lifecycle_state == LifecycleState::UpdateVerifying
        && revision.lifecycle_state == LifecycleState::Normal
    {
        previous
            .managed_update_context
            .as_ref()
            .and_then(ManagedUpdateContext::candidate_artifact)
            .ok_or_else(|| {
                PackageStateError::Corrupt(
                    "verifying revision is missing candidate artifact".to_owned(),
                )
            })?
    } else {
        previous_artifact
    };
    if revision.accepted_artifact.as_ref() != Some(expected) {
        return Err(PackageStateError::Corrupt(
            "accepted artifact changed outside verified update completion".to_owned(),
        ));
    }
    Ok(())
}

fn verify_update_continuity(
    previous: &PackageStateRevision,
    revision: &PackageStateRevision,
) -> Result<(), PackageStateError> {
    let continues = matches!(
        (previous.lifecycle_state, revision.lifecycle_state),
        (
            LifecycleState::UpdatePreparing,
            LifecycleState::UpdateWindowOpen
        ) | (
            LifecycleState::UpdateWindowOpen,
            LifecycleState::UpdateVerifying
        )
    );
    if !continues {
        return Ok(());
    }
    let before = previous.managed_update_context.as_ref().ok_or_else(|| {
        PackageStateError::Corrupt("previous update context is missing".to_owned())
    })?;
    let after = revision
        .managed_update_context
        .as_ref()
        .ok_or_else(|| PackageStateError::Corrupt("next update context is missing".to_owned()))?;
    if before.token() != after.token() || before.gate_snapshot() != after.gate_snapshot() {
        return Err(PackageStateError::Corrupt(
            "managed update token or Gate snapshot changed".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
