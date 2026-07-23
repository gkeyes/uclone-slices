use serde::{Deserialize, Serialize};

/// Typed Runtime mode and device capability report.
#[allow(
    clippy::struct_excessive_bools,
    reason = "the wire report preserves independently auditable runtime gates"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeReport {
    ready: bool,
    user_unlocked: bool,
    ce_de_supported: bool,
    #[serde(default)]
    recovery_only: bool,
    runtime_version: String,
    build_id: String,
}

impl ProbeReport {
    /// Creates a bounded capability report.
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "constructor mirrors the stable wire report's independent runtime gates"
    )]
    pub fn new(
        ready: bool,
        user_unlocked: bool,
        ce_de_supported: bool,
        recovery_only: bool,
    ) -> Self {
        Self {
            ready,
            user_unlocked,
            ce_de_supported,
            recovery_only,
            runtime_version: env!("CARGO_PKG_VERSION").to_owned(),
            build_id: crate::protocol::RUNTIME_BUILD_ID.to_owned(),
        }
    }

    /// Returns whether all runtime gates are ready.
    pub const fn ready(&self) -> bool {
        self.ready
    }

    /// Returns whether user 0 CE is unlocked.
    pub const fn user_unlocked(&self) -> bool {
        self.user_unlocked
    }

    /// Returns whether paired CE and DE views are supported.
    pub const fn ce_de_supported(&self) -> bool {
        self.ce_de_supported
    }

    /// Returns whether only bounded recovery operations are exposed.
    pub const fn recovery_only(&self) -> bool {
        self.recovery_only
    }

    /// Returns the semantic Runtime version embedded in this binary.
    pub fn runtime_version(&self) -> &str {
        &self.runtime_version
    }

    /// Returns the immutable paired-build identity embedded by CI.
    pub fn build_id(&self) -> &str {
        &self.build_id
    }
}
