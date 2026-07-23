use crate::domain::SlotView;
use crate::protocol::{Ack, AckOperation, ReconcileOutcome as ProtocolOutcome, ResponsePayload};
use crate::reconcile::ReconcileOutcome;
use crate::rescue::RescueExecution;

use super::{ServiceError, SwitchExecution};

pub(super) fn proved_switch(
    expected: &SlotView,
    execution: SwitchExecution,
) -> Result<SlotView, ServiceError> {
    match execution {
        SwitchExecution::Committed(proved) if proved == *expected => Ok(proved),
        SwitchExecution::Committed(_) | SwitchExecution::RecoveryRequired => {
            Err(ServiceError::RecoveryRequired)
        }
        SwitchExecution::RolledBack => Err(ServiceError::Conflict),
        SwitchExecution::Quarantined => Err(ServiceError::Quarantined),
    }
}

pub(super) const fn rescued(execution: RescueExecution) -> Result<ResponsePayload, ServiceError> {
    match execution {
        RescueExecution::CompletedBase => {
            Ok(ResponsePayload::Ack(Ack::new(AckOperation::RescueToBase)))
        }
        RescueExecution::RecoveryRequired => Err(ServiceError::RecoveryRequired),
        RescueExecution::Quarantined => Err(ServiceError::Quarantined),
        RescueExecution::ContainmentFailed => Err(ServiceError::Internal),
    }
}

pub(super) fn reconcile(outcome: ReconcileOutcome) -> ProtocolOutcome {
    match outcome {
        ReconcileOutcome::Held => ProtocolOutcome::Held,
        ReconcileOutcome::Locked => ProtocolOutcome::Locked,
        ReconcileOutcome::RestoredBase => ProtocolOutcome::RestoredBase,
        ReconcileOutcome::RestoredSlot(slot) => ProtocolOutcome::RestoredSlot { slot },
        ReconcileOutcome::RolledBack => ProtocolOutcome::RolledBack,
        ReconcileOutcome::RolledForward => ProtocolOutcome::RolledForward,
        ReconcileOutcome::RecoveryRequired(reason) => ProtocolOutcome::RecoveryRequired { reason },
        ReconcileOutcome::Quarantined => ProtocolOutcome::Quarantined,
    }
}
