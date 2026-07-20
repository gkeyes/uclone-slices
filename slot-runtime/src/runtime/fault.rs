use super::RuntimeError;

#[doc = "A deterministic crash boundary in the switch transaction."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    #[doc = "After the prepared Journal record is durable."]
    Prepared,
    #[doc = "After the held gate is verified and journaled."]
    GateHeld,
    #[doc = "After all processes are quiesced and journaled."]
    ProcessesQuiesced,
    #[doc = "After Applying is durable but before view mutation."]
    Applying,
    #[doc = "After the target view is applied but before verification."]
    TargetApplied,
    #[doc = "After the target view verification is journaled."]
    ViewVerified,
    #[doc = "After the commit nonce declaration is durable."]
    Committing,
    #[doc = "After Registry publication but before Journal acknowledgement."]
    RegistryPublished,
    #[doc = "After Registry acknowledgement is journaled."]
    RegistryCommitted,
    #[doc = "After exact gate restoration but before `GateReleased` is durable."]
    GateRestored,
    #[doc = "After `GateReleased` is durable while the recovery lease is retained."]
    GateReleased,
    #[doc = "After `Completed` is durable but before recovery-lease retirement."]
    Completed,
    #[doc = "After the completed transaction's recovery lease is retired."]
    GateLeaseRetired,
}

#[doc = "Deterministic single-point crash injector used by recovery tests."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaultInjector {
    crash_at: Option<FaultPoint>,
}

impl FaultInjector {
    #[doc = "Creates an injector that never interrupts execution."]
    pub const fn disabled() -> Self {
        Self { crash_at: None }
    }

    #[doc = "Creates an injector that crashes at one exact boundary."]
    pub const fn crash_at(point: FaultPoint) -> Self {
        Self {
            crash_at: Some(point),
        }
    }

    pub(super) fn check(self, point: FaultPoint) -> Result<(), RuntimeError> {
        if self.crash_at == Some(point) {
            Err(RuntimeError::InjectedCrash(point))
        } else {
            Ok(())
        }
    }
}

impl Default for FaultInjector {
    fn default() -> Self {
        Self::disabled()
    }
}
