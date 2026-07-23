#[doc = "Bounded result of an independent native-base rescue."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescueExecution {
    #[doc = "Native base is authoritative and the exact gate is restored."]
    CompletedBase,
    #[doc = "Uncertainty remains and package execution must stay disabled."]
    RecoveryRequired,
    #[doc = "The installed UID or signing identity no longer owns the anchors."]
    Quarantined,
    #[doc = "Recovery failed and package execution could not be proved contained."]
    ContainmentFailed,
}

#[doc = "Early-start decision made before any ordinary control-plane store is opened."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescueStartup {
    #[doc = "No rescue epoch exists, so ordinary reconciliation may be opened."]
    OpenOrdinary,
    #[doc = "A completed native-base retirement blocks all ordinary slot state."]
    BaseRetired,
    #[doc = "The rescue could not be proved and the package remains disabled."]
    RecoveryRequired,
    #[doc = "Installed identity no longer owns the immutable base anchors."]
    Quarantined,
    #[doc = "Startup failed and package execution could not be proved contained."]
    ContainmentFailed,
}

/// Result of the pre-store package execution gate acquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupGateOutcome {
    /// No trusted management evidence was found for this package.
    NotManaged,
    /// Management evidence was valid and the gate is held.
    Held,
    /// Evidence could not be attributed completely; the gate is held fail-closed.
    HeldRecovery,
}
