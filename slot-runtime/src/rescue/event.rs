use serde::{Deserialize, Serialize};

use super::{RescueError, RescueSpec};
use crate::domain::CommitNonce;

#[doc = "One durable emergency-rescue event around a side-effect boundary."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RescueEvent {
    #[doc = "The immutable specification is durable and no gate mutation has begun."]
    Prepared {
        #[doc = "The complete independently anchored rescue specification."]
        spec: Box<RescueSpec>,
    },
    #[doc = "The exact application execution gate is durably held."]
    GateHeld,
    #[doc = "Every package process is proved absent."]
    ProcessesQuiesced,
    #[doc = "Validated Preview bind layers may be removed domain by domain."]
    BaseApplying,
    #[doc = "Canonical, mirror, and Zygote views prove native CE and DE base."]
    BaseVerified,
    #[doc = "Native base durably supersedes every ordinary control-plane record."]
    BaseCommitted {
        #[doc = "The nonce pinned by the immutable specification."]
        nonce: CommitNonce,
    },
    #[doc = "The exact original gate snapshot is proved restored."]
    GateReleased,
    #[doc = "The package is authoritatively retired to native base."]
    Completed,
    #[doc = "A bounded machine code records failure without advancing proven phase."]
    Failure {
        #[doc = "Stable lowercase machine-readable reason code."]
        code: String,
    },
}

#[doc = "Latest durable rescue boundary proved independently of failure records."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescuePhase {
    #[doc = "The immutable specification is durable."]
    Prepared,
    #[doc = "The exact package gate is held."]
    GateHeld,
    #[doc = "Every package process is absent."]
    ProcessesQuiesced,
    #[doc = "Validated native-base restoration has begun."]
    BaseApplying,
    #[doc = "Every native base view is verified while gated."]
    BaseVerified,
    #[doc = "Native base is the durable authoritative view."]
    BaseCommitted,
    #[doc = "The exact original package state is restored."]
    GateReleased,
    #[doc = "The package is durably retired to native base."]
    Completed,
}

impl RescueEvent {
    pub(super) fn validate_standalone(&self) -> Result<(), RescueError> {
        if let Self::Failure { code } = self
            && !valid_failure_code(code)
        {
            return Err(RescueError::Corrupt(
                "invalid rescue failure code".to_owned(),
            ));
        }
        Ok(())
    }

    pub(super) fn validate_after(
        &self,
        phase: RescuePhase,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        self.validate_standalone()?;
        if matches!(self, Self::Failure { .. }) && phase != RescuePhase::Completed {
            return Ok(());
        }
        let legal = matches!(
            (phase, self),
            (RescuePhase::Prepared, Self::GateHeld)
                | (RescuePhase::GateHeld, Self::ProcessesQuiesced)
                | (RescuePhase::ProcessesQuiesced, Self::BaseApplying)
                | (RescuePhase::BaseApplying, Self::BaseVerified)
                | (RescuePhase::BaseVerified, Self::BaseCommitted { .. })
                | (RescuePhase::BaseCommitted, Self::GateReleased)
                | (RescuePhase::GateReleased, Self::Completed)
        );
        if !legal {
            return Err(RescueError::IllegalTransition {
                previous: phase.name(),
                next: self.name(),
            });
        }
        if let Self::BaseCommitted { nonce } = self
            && nonce != spec.commit_nonce()
        {
            return Err(RescueError::Corrupt(
                "base commit nonce differs from immutable specification".to_owned(),
            ));
        }
        Ok(())
    }

    pub(super) const fn advanced_phase(&self) -> Option<RescuePhase> {
        match self {
            Self::Prepared { .. } => Some(RescuePhase::Prepared),
            Self::GateHeld => Some(RescuePhase::GateHeld),
            Self::ProcessesQuiesced => Some(RescuePhase::ProcessesQuiesced),
            Self::BaseApplying => Some(RescuePhase::BaseApplying),
            Self::BaseVerified => Some(RescuePhase::BaseVerified),
            Self::BaseCommitted { .. } => Some(RescuePhase::BaseCommitted),
            Self::GateReleased => Some(RescuePhase::GateReleased),
            Self::Completed => Some(RescuePhase::Completed),
            Self::Failure { .. } => None,
        }
    }

    const fn name(&self) -> &'static str {
        match self {
            Self::Prepared { .. } => "prepared",
            Self::GateHeld => "gate_held",
            Self::ProcessesQuiesced => "processes_quiesced",
            Self::BaseApplying => "base_applying",
            Self::BaseVerified => "base_verified",
            Self::BaseCommitted { .. } => "base_committed",
            Self::GateReleased => "gate_released",
            Self::Completed => "completed",
            Self::Failure { .. } => "failure",
        }
    }
}

impl RescuePhase {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::GateHeld => "gate_held",
            Self::ProcessesQuiesced => "processes_quiesced",
            Self::BaseApplying => "base_applying",
            Self::BaseVerified => "base_verified",
            Self::BaseCommitted => "base_committed",
            Self::GateReleased => "gate_released",
            Self::Completed => "completed",
        }
    }
}

fn valid_failure_code(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}
