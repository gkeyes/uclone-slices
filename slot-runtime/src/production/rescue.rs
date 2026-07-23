use crate::android::PackageProbe;
use crate::domain::PackageKey;
use crate::layout::RuntimeLayout;
use crate::materializer::MaterializationBackend;
use crate::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};
use crate::rescue::{
    OfflineRescuePlatform, RescueError, RescueExecution, RescueJournalStore, RescueMetadataSource,
    RescueStartup, RescueStatus,
};
use crate::service::PackageState;
use crate::service::ServiceError;
use std::collections::BTreeSet;
use std::fs;

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource + RescueMetadataSource,
{
    pub(super) fn do_rescue_to_base(
        &mut self,
        key: &PackageKey,
    ) -> Result<RescueExecution, ServiceError> {
        let outcome = {
            let mut rescue =
                OfflineRescuePlatform::open_fixed(&mut self.runtime, &mut self.metadata);
            require_authorized(rescue.authorize_existing_target(key))?;
            rescue.rescue_to_base(key)
        };
        Ok(finish_rescue(&mut self.recovery_overrides, key, outcome))
    }

    pub(super) fn do_rescue_startup(&mut self, key: &PackageKey) -> RescueStartup {
        OfflineRescuePlatform::open_fixed(&mut self.runtime, &mut self.metadata)
            .reconcile_startup(key)
    }
}

pub(super) fn require_authorized(
    authorization: Result<bool, RescueError>,
) -> Result<(), ServiceError> {
    let authorized = authorization.map_err(|_| ServiceError::RecoveryRequired)?;
    if authorized {
        Ok(())
    } else {
        Err(ServiceError::NotFound)
    }
}

pub(super) fn finish_rescue(
    overrides: &mut BTreeSet<crate::domain::PackageName>,
    key: &PackageKey,
    outcome: RescueExecution,
) -> RescueExecution {
    if outcome == RescueExecution::CompletedBase {
        overrides.remove(key.package_name());
    }
    outcome
}

pub(super) fn rescue_status(key: &PackageKey) -> Result<Option<RescueStatus>, ServiceError> {
    let package_root = RuntimeLayout::rescue_journal_root()
        .join("packages")
        .join(key.package_name().as_str());
    match fs::symlink_metadata(&package_root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ServiceError::RecoveryRequired),
        Ok(_) => {}
    }
    RescueJournalStore::fixed_for(key.package_name())
        .and_then(|journal| journal.load())
        .map(|transaction| transaction.map(|value| value.status()))
        .map_err(|_| ServiceError::RecoveryRequired)
}

pub(super) const fn package_state(status: Option<RescueStatus>) -> Option<PackageState> {
    match status {
        Some(_) => Some(PackageState::RecoveryRequired),
        None => None,
    }
}
