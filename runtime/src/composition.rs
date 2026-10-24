use std::path::Path;
use std::sync::Arc;

use thiserror::Error;

use crate::adapters::{FilePackageStore, FileSlotStorage, SystemAndroidOps};
use crate::launch::LaunchGate;
use crate::layout;
use crate::model::SigningIdentity;
use crate::ports::AdapterError;
use crate::usecases::{Runtime, RuntimeError};

type InnerRuntime = Runtime<FilePackageStore, FileSlotStorage, SystemAndroidOps>;

#[derive(Debug, Error)]
#[error("could not compose production Runtime: {message}")]
pub struct CompositionError {
    message: String,
}

impl From<AdapterError> for CompositionError {
    fn from(error: AdapterError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ProductionRuntime {
    inner: InnerRuntime,
}

impl ProductionRuntime {
    pub fn reconcile_boot(&mut self) -> Result<(), RuntimeError> {
        self.inner.reconcile_boot()
    }

    pub fn restamp_renumbered_apks(&mut self) -> Result<(), RuntimeError> {
        self.inner.restamp_renumbered_apks()
    }

    pub fn prepare_default_account(
        &mut self,
        package: &str,
        still_foreground: impl FnMut() -> bool,
    ) -> Result<bool, RuntimeError> {
        let package = crate::model::PackageName::new(package)
            .map_err(|_error| RuntimeError::InvalidRequest)?;
        self.inner
            .prepare_default_account(&package, still_foreground)
    }

    /// Re-binds an App whose installed APK changed, as the Manager does when it opens, so a
    /// direct launch after an update or reboot can start without opening the Manager first.
    pub fn rebind_installed(
        &mut self,
        package: &str,
        signing: &SigningIdentity,
    ) -> Result<(), RuntimeError> {
        let package = crate::model::PackageName::new(package)
            .map_err(|_error| RuntimeError::InvalidRequest)?;
        self.inner
            .rebind_package(&package, signing.clone(), false)
            .map(|_snapshot| ())
    }

    pub(crate) fn inner_mut(&mut self) -> &mut InnerRuntime {
        &mut self.inner
    }
}

/// Whether a Ready backup lease of `package` covers every slot in `slots`. The archive helper
/// reads account data only under such a lease: the App is stopped and its view is on Base.
pub fn backup_lease_covers(runtime_root: &Path, package: &str, slots: &[&str]) -> bool {
    use crate::model::{AccountIoKind, AccountIoPhase, AccountIoScope};
    use crate::ports::PackageStore as _;
    let Ok(package) = crate::model::PackageName::new(package) else {
        return false;
    };
    let Ok(Some(intent)) = FilePackageStore::open(runtime_root)
        .and_then(|store| store.load_account_io_intent(&package))
    else {
        return false;
    };
    intent.kind == AccountIoKind::Backup
        && intent.phase == AccountIoPhase::Ready
        && match &intent.scope {
            Some(AccountIoScope::AllAccounts) => true,
            Some(AccountIoScope::Account { slot }) => {
                slots.iter().all(|requested| *requested == slot.as_str())
            }
            None => false,
        }
}

pub fn production(
    runtime_root: &Path,
    build_id: impl Into<String>,
) -> Result<ProductionRuntime, CompositionError> {
    compose(runtime_root, build_id, None)
}

pub fn production_with_launch_gate(
    runtime_root: &Path,
    build_id: impl Into<String>,
    gate: Arc<LaunchGate>,
) -> Result<ProductionRuntime, CompositionError> {
    compose(runtime_root, build_id, Some(gate))
}

fn compose(
    runtime_root: &Path,
    build_id: impl Into<String>,
    gate: Option<Arc<LaunchGate>>,
) -> Result<ProductionRuntime, CompositionError> {
    let packages = FilePackageStore::open(runtime_root)?;
    let slots = FileSlotStorage::open(
        layout::CE_SLOTS_ROOT,
        layout::DE_SLOTS_ROOT,
        layout::USER_CE_ROOT,
        layout::USER_DE_ROOT,
    )?;
    let mut android = SystemAndroidOps::new(
        build_id,
        layout::CE_SLOTS_ROOT,
        layout::DE_SLOTS_ROOT,
        layout::USER_CE_ROOT,
        layout::USER_DE_ROOT,
    );
    android.set_launch_gate(runtime_root, gate);
    Ok(ProductionRuntime {
        inner: Runtime::new(packages, slots, android),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::model::{
        AccountIoIntent, AccountIoKind, AccountIoPhase, AccountIoScope, AccountIoToken,
        PackageName, SlotId,
    };
    use crate::ports::PackageStore as _;

    fn save_backup_intent(root: &Path, scope: AccountIoScope, phase: AccountIoPhase) {
        let intent = AccountIoIntent {
            schema_version: 2,
            io_token: AccountIoToken::from_bytes([7; 16]),
            package: PackageName::new("com.example.app").unwrap(),
            kind: AccountIoKind::Backup,
            scope: Some(scope),
            transfer_id: None,
            mappings: Vec::new(),
            archived_state: None,
            previous_active_slot: SlotId::base(),
            previous_launch_after_reboot: false,
            previous_was_running: false,
            previous_enabled_state: None,
            maintenance_active: false,
            phase,
            completed: Vec::new(),
        };
        FilePackageStore::open(root)
            .unwrap()
            .save_account_io_intent(&intent)
            .unwrap();
    }

    #[test]
    fn the_archive_helper_reads_accounts_only_under_a_ready_backup_lease() {
        let root = tempfile::tempdir().unwrap();
        let package = "com.example.app";
        assert!(!backup_lease_covers(root.path(), package, &["base"]));

        save_backup_intent(
            root.path(),
            AccountIoScope::AllAccounts,
            AccountIoPhase::Preparing,
        );
        assert!(!backup_lease_covers(root.path(), package, &["base"]));

        save_backup_intent(
            root.path(),
            AccountIoScope::AllAccounts,
            AccountIoPhase::Ready,
        );
        assert!(backup_lease_covers(
            root.path(),
            package,
            &["base", "slot-1"]
        ));
        assert!(!backup_lease_covers(
            root.path(),
            "com.example.other",
            &["base"]
        ));

        let slot = SlotId::new("slot-1").unwrap();
        save_backup_intent(
            root.path(),
            AccountIoScope::Account { slot },
            AccountIoPhase::Ready,
        );
        assert!(backup_lease_covers(root.path(), package, &["slot-1"]));
        assert!(!backup_lease_covers(
            root.path(),
            package,
            &["base", "slot-1"]
        ));
    }
}
