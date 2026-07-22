use crate::domain::PackageKey;
use crate::enrollment_attempt::EnrollmentAttempt;
use crate::service::ServiceError;

use super::stores::ProductionStores;

pub(super) fn publications_match(
    stores: &ProductionStores,
    key: &PackageKey,
    attempt: &EnrollmentAttempt,
) -> Result<bool, ServiceError> {
    let Some(anchors) = attempt.committed() else {
        return Ok(false);
    };
    let Some(enrollment) = stores
        .enrollment
        .load(key.package_name())
        .map_err(|_| ServiceError::RecoveryRequired)?
    else {
        return Ok(false);
    };
    let catalog = stores
        .catalog
        .list(key)
        .map_err(|_| ServiceError::RecoveryRequired)?;
    let Some(state) = stores
        .package_state
        .latest(key)
        .map_err(|_| ServiceError::RecoveryRequired)?
    else {
        return Ok(false);
    };
    if enrollment.identity() != anchors.identity()
        || enrollment.base_inodes() != anchors.base_inodes()
        || catalog.len() != 1
        || !catalog.first().is_some_and(|entry| {
            entry.slot_id().is_base()
                && entry.inodes() == anchors.base_inodes()
                && entry.enrolled_identity() == anchors.identity()
        })
    {
        return Ok(false);
    }
    let digests = stores.published_digests(key, &state)?;
    Ok(digests.enrollment == anchors.enrollment_sha256()
        && digests.compatibility_policy == anchors.compatibility_policy_sha256()
        && digests.base_catalog == anchors.base_catalog_sha256()
        && digests.package_state == anchors.package_state_sha256())
}
