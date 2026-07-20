use serde::{Deserialize, Serialize};

use super::{JournalError, TransactionSpec};
use crate::domain::CommitNonce;

#[doc = "Append-only transaction event persisted around one side-effect boundary."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalEvent {
    #[doc = "The transaction exists durably and no side effect has begun."]
    Prepared {
        #[doc = "The validated immutable transaction specification."]
        spec: Box<TransactionSpec>,
    },
    #[doc = "The package execution gate is verified held."]
    GateHeld,
    #[doc = "Every target package process is confirmed stopped."]
    ProcessesQuiesced,
    #[doc = "The runtime may be between CE and DE view application steps."]
    Applying,
    #[doc = "Canonical, mirror, Zygote, CE, and DE views agree with the target."]
    ViewVerified,
    #[doc = "The next Registry revision and nonce were declared before commit."]
    Committing {
        #[doc = "The commit nonce that must appear in the Registry revision."]
        nonce: CommitNonce,
    },
    #[doc = "The Registry commit point exists durably."]
    RegistryCommitted {
        #[doc = "The nonce found in the durable Registry revision."]
        nonce: CommitNonce,
    },
    #[doc = "The original enabled state was restored after a verified view."]
    GateReleased,
    #[doc = "The journal is terminal; its validated recovery lease retires separately."]
    Completed,
    #[doc = "The runtime is restoring the previous slot."]
    RollingBack,
    #[doc = "The previous slot is verified restored while the gate remains held."]
    RolledBack,
    #[doc = "The runtime cannot prove either view and must keep the gate held."]
    RecoveryRequired {
        #[doc = "A stable machine-readable recovery reason code."]
        reason: String,
    },
}

impl JournalEvent {
    pub(super) const fn can_follow(&self, previous: &Self) -> bool {
        match previous {
            Self::Prepared { .. } => matches!(self, Self::GateHeld | Self::RecoveryRequired { .. }),
            Self::GateHeld => matches!(
                self,
                Self::ProcessesQuiesced | Self::RollingBack | Self::RecoveryRequired { .. }
            ),
            Self::ProcessesQuiesced => matches!(
                self,
                Self::Applying | Self::RollingBack | Self::RecoveryRequired { .. }
            ),
            Self::Applying => matches!(
                self,
                Self::ViewVerified | Self::RollingBack | Self::RecoveryRequired { .. }
            ),
            Self::ViewVerified => matches!(
                self,
                Self::Committing { .. } | Self::RollingBack | Self::RecoveryRequired { .. }
            ),
            Self::Committing { .. } => matches!(
                self,
                Self::RegistryCommitted { .. } | Self::RollingBack | Self::RecoveryRequired { .. }
            ),
            Self::RegistryCommitted { .. } | Self::RolledBack => {
                matches!(self, Self::GateReleased | Self::RecoveryRequired { .. })
            }
            Self::GateReleased => matches!(self, Self::Completed | Self::RecoveryRequired { .. }),
            Self::RollingBack => matches!(self, Self::RolledBack | Self::RecoveryRequired { .. }),
            Self::RecoveryRequired { .. } => matches!(self, Self::RollingBack),
            Self::Completed => false,
        }
    }

    pub(super) fn validate_after(&self, previous: &Self) -> Result<(), JournalError> {
        if !self.can_follow(previous) {
            return Err(JournalError::IllegalTransition {
                previous: event_name(previous),
                next: event_name(self),
            });
        }
        if let (Self::RegistryCommitted { nonce: committed }, Self::Committing { nonce: declared }) =
            (self, previous)
            && committed != declared
        {
            return Err(JournalError::Corrupt(
                "Registry commit nonce differs from the declared nonce".to_owned(),
            ));
        }
        if let Self::RecoveryRequired { reason } = self
            && !valid_recovery_reason(reason)
        {
            return Err(JournalError::Corrupt(
                "invalid recovery reason code".to_owned(),
            ));
        }
        Ok(())
    }
}

fn valid_recovery_reason(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

const fn event_name(event: &JournalEvent) -> &'static str {
    match event {
        JournalEvent::Prepared { .. } => "prepared",
        JournalEvent::GateHeld => "gate_held",
        JournalEvent::ProcessesQuiesced => "processes_quiesced",
        JournalEvent::Applying => "applying",
        JournalEvent::ViewVerified => "view_verified",
        JournalEvent::Committing { .. } => "committing",
        JournalEvent::RegistryCommitted { .. } => "registry_committed",
        JournalEvent::GateReleased => "gate_released",
        JournalEvent::Completed => "completed",
        JournalEvent::RollingBack => "rolling_back",
        JournalEvent::RolledBack => "rolled_back",
        JournalEvent::RecoveryRequired { .. } => "recovery_required",
    }
}
