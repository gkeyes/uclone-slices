use crate::journal::JournalError;
use crate::registry::{PackageRevision, RegistryError};

use super::PlatformError;

#[doc = "Typed cause retained when fail-closed recovery is required."]
#[derive(Debug, thiserror::Error)]
pub enum RecoveryCause {
    #[doc = "A platform operation left the durable view uncertain."]
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[doc = "A Journal operation failed after the gate was acquired."]
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[doc = "Registry publication failed after Committing became durable."]
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[doc = "The original failure was followed by a failed previous-view rollback."]
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed {
        #[doc = "The platform failure that initiated rollback."]
        original: PlatformError,
        #[doc = "The platform failure that made rollback unprovable."]
        rollback: PlatformError,
    },
}

#[doc = "Observable result of one coordinated switch attempt."]
#[derive(Debug)]
pub enum SwitchOutcome {
    #[doc = "The target view and Registry commit are durable and the original gate is restored."]
    Committed {
        #[doc = "The committed Registry revision."]
        revision: Box<PackageRevision>,
    },
    #[doc = "A pre-commit platform failure restored the previous view and exact gate state."]
    RolledBack {
        #[doc = "The original target-side platform failure."]
        cause: PlatformError,
    },
    #[doc = "The runtime retained the gate because neither safe completion could be proved."]
    RecoveryRequired {
        #[doc = "The typed failure that requires reconciliation."]
        cause: RecoveryCause,
    },
}
