use crate::android::PackageProbe;
use crate::domain::PackageKey;
use crate::materializer::MaterializationBackend;
use crate::reconcile::RecoveryBackend;
use crate::service::ServiceError;

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn pristine_pending(&self, key: &PackageKey) -> Result<bool, ServiceError> {
        let no_enrollment = self
            .stores
            .enrollment
            .load(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_none();
        let no_catalog = self
            .stores
            .catalog
            .list(key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_empty();
        let no_state = self
            .stores
            .package_state
            .latest(key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_none();
        let no_registry = self
            .stores
            .registry
            .latest(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_none();
        let no_journal = self
            .stores
            .journal
            .list_for_package(key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_empty();
        let no_policy = self
            .stores
            .compatibility_policy
            .load(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_none();
        let no_slot_metadata = self
            .stores
            .slot_metadata
            .list(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_empty();
        Ok(no_enrollment
            && no_catalog
            && no_state
            && no_registry
            && no_journal
            && no_policy
            && no_slot_metadata)
    }
}
