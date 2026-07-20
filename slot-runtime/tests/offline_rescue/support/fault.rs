use uclone_slot_runtime::rescue::{RescueError, RescueFaultInjector, RescuePhase};

#[derive(Debug, Clone, Copy)]
pub(crate) struct CrashOnce {
    phase: Option<RescuePhase>,
    gate_restore: bool,
    fired: bool,
}

impl CrashOnce {
    pub(crate) const fn after_phase(phase: RescuePhase) -> Self {
        Self {
            phase: Some(phase),
            gate_restore: false,
            fired: false,
        }
    }

    pub(crate) const fn after_gate_restore() -> Self {
        Self {
            phase: None,
            gate_restore: true,
            fired: false,
        }
    }

    pub(crate) const fn never() -> Self {
        Self {
            phase: None,
            gate_restore: false,
            fired: false,
        }
    }
}

impl RescueFaultInjector for CrashOnce {
    fn after_phase(&mut self, phase: RescuePhase) -> Result<(), RescueError> {
        if !self.fired && self.phase == Some(phase) {
            self.fired = true;
            Err(RescueError::InjectedCrash(phase))
        } else {
            Ok(())
        }
    }

    fn after_gate_restore(&mut self) -> Result<(), RescueError> {
        if !self.fired && self.gate_restore {
            self.fired = true;
            Err(RescueError::InjectedCrash(RescuePhase::BaseCommitted))
        } else {
            Ok(())
        }
    }
}
