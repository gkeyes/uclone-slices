#[doc = "Deterministic failure boundaries around every materialization state transition."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    #[doc = "After gate, quiescence, and immutable base proof."]
    BaseVerified,
    #[doc = "After paired staging creation."]
    StagingCreated,
    #[doc = "After CE copy validation."]
    CeCopied,
    #[doc = "After DE copy validation."]
    DeCopied,
    #[doc = "After staging security metadata application."]
    SecurityApplied,
    #[doc = "After complete staging evidence collection."]
    StagingInspected,
    #[doc = "After CE durability synchronization."]
    CeSynced,
    #[doc = "After DE durability synchronization."]
    DeSynced,
    #[doc = "After final base and staging verification."]
    PairVerified,
    #[doc = "After ready publication but before returning catalog input."]
    ReadyPublished,
}

impl FaultPoint {
    #[doc = "Every deterministic crash boundary in execution order."]
    pub const ALL: [Self; 10] = [
        Self::BaseVerified,
        Self::StagingCreated,
        Self::CeCopied,
        Self::DeCopied,
        Self::SecurityApplied,
        Self::StagingInspected,
        Self::CeSynced,
        Self::DeSynced,
        Self::PairVerified,
        Self::ReadyPublished,
    ];
}

#[doc = "Injected materialization failure policy used for deterministic crash tests."]
pub trait FaultInjector: core::fmt::Debug {
    #[doc = "Returns true exactly when the coordinator must stop at this boundary."]
    fn should_fail(&mut self, point: FaultPoint) -> bool;
}

#[doc = "Production default that never injects a failure."]
#[derive(Debug, Clone, Copy, Default)]
pub struct NoFault;

impl FaultInjector for NoFault {
    fn should_fail(&mut self, _: FaultPoint) -> bool {
        false
    }
}

#[doc = "Single-boundary injector for host and device fault-matrix tests."]
#[derive(Debug, Clone, Copy, Default)]
pub struct ScriptedFaultInjector {
    remaining: Option<FaultPoint>,
}

impl ScriptedFaultInjector {
    #[doc = "Injects exactly once at the selected boundary."]
    pub const fn once(point: FaultPoint) -> Self {
        Self {
            remaining: Some(point),
        }
    }

    #[doc = "Creates an injector that never fails."]
    pub const fn never() -> Self {
        Self { remaining: None }
    }
}

impl FaultInjector for ScriptedFaultInjector {
    fn should_fail(&mut self, point: FaultPoint) -> bool {
        if self.remaining == Some(point) {
            self.remaining = None;
            true
        } else {
            false
        }
    }
}
