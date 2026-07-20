use crate::android::PackageProbe;
use crate::domain::PackageKey;
use crate::materializer::MaterializationBackend;
use crate::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};
use crate::rescue::{OfflineRescuePlatform, RescueExecution, RescueMetadataSource, RescueStartup};

use super::composition::ProductionPlatform;
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource + RescueMetadataSource,
{
    pub(super) fn do_rescue_to_base(&mut self, key: &PackageKey) -> RescueExecution {
        OfflineRescuePlatform::open_fixed(&mut self.runtime, &mut self.metadata).rescue_to_base(key)
    }

    pub(super) fn do_rescue_startup(&mut self, key: &PackageKey) -> RescueStartup {
        OfflineRescuePlatform::open_fixed(&mut self.runtime, &mut self.metadata)
            .reconcile_startup(key)
    }
}
