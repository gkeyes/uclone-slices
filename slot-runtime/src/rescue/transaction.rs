use super::event::RescuePhase;
use super::{RescueError, RescueEvent, RescueSpec, RescueStep};

#[doc = "Verified emergency-rescue position relative to its durable commit point."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescueStatus {
    #[doc = "Native base has not reached the durable supersession point."]
    PreCommit,
    #[doc = "Native base is authoritative, but the original gate is still held."]
    Committed,
    #[doc = "The exact gate is restored, but the terminal marker is not durable."]
    Completed,
    #[doc = "The terminal marker retires ordinary Slots state onto native base."]
    BaseRetired,
}

#[doc = "A complete verified rescue specification and append-only event chain."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RescueTransaction {
    spec: RescueSpec,
    steps: Vec<RescueStep>,
}

impl RescueTransaction {
    pub(super) fn new(spec: RescueSpec, steps: Vec<RescueStep>) -> Result<Self, RescueError> {
        let transaction = Self { spec, steps };
        transaction.validate_transitions()?;
        Ok(transaction)
    }

    #[doc = "Returns the immutable rescue specification."]
    pub const fn spec(&self) -> &RescueSpec {
        &self.spec
    }

    #[doc = "Returns every verified generation in order."]
    pub fn steps(&self) -> &[RescueStep] {
        &self.steps
    }

    #[doc = "Classifies the latest proven phase around native-base supersession."]
    pub fn status(&self) -> RescueStatus {
        match self.phase() {
            RescuePhase::Prepared
            | RescuePhase::GateHeld
            | RescuePhase::ProcessesQuiesced
            | RescuePhase::BaseApplying
            | RescuePhase::BaseVerified => RescueStatus::PreCommit,
            RescuePhase::BaseCommitted => RescueStatus::Committed,
            RescuePhase::GateReleased => RescueStatus::Completed,
            RescuePhase::Completed => RescueStatus::BaseRetired,
        }
    }

    #[doc = "Returns the exact latest proven side-effect boundary, ignoring failure records."]
    pub fn phase(&self) -> RescuePhase {
        self.steps
            .iter()
            .rev()
            .find_map(|step| step.event().advanced_phase())
            .unwrap_or(RescuePhase::Prepared)
    }

    fn validate_transitions(&self) -> Result<(), RescueError> {
        let mut events = self.steps.iter().map(RescueStep::event);
        let Some(RescueEvent::Prepared { spec }) = events.next() else {
            return Err(RescueError::Corrupt(
                "first rescue event is not prepared".to_owned(),
            ));
        };
        if spec.as_ref() != &self.spec {
            return Err(RescueError::Corrupt(
                "prepared specification differs from transaction".to_owned(),
            ));
        }
        let mut phase = RescuePhase::Prepared;
        for event in events {
            event.validate_after(phase, &self.spec)?;
            if let Some(next) = event.advanced_phase() {
                phase = next;
            }
        }
        Ok(())
    }
}
