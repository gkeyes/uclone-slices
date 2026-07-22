use serde::{Deserialize, Serialize};

use crate::domain::{DataInodes, PackageName, PackageSupportLevel, SlotId};
use crate::lifecycle::LifecycleState;
use crate::service::{ManagedAppInfo, PackageInspection, SlotInfo};
use crate::slot_metadata::{SlotDisplayName, SlotRecordState, SlotSeedMode};

#[doc = "Installed-package identity and compatibility result."]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the report exposes each compatibility blocker independently to the UI"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageInspectionReport {
    package: PackageName,
    uid: u32,
    signature_sha256: String,
    version_code: u64,
    code_path: String,
    base_inodes: DataInodes,
    compatible: bool,
    support_level: PackageSupportLevel,
    system_app: bool,
    shared_uid: bool,
    direct_boot_aware: bool,
}

impl From<PackageInspection> for PackageInspectionReport {
    fn from(value: PackageInspection) -> Self {
        let compatibility = value.compatibility();
        Self {
            package: value.package().clone(),
            uid: value.identity().uid(),
            signature_sha256: value.identity().signature_sha256().to_owned(),
            version_code: value.identity().version_code(),
            code_path: value.identity().code_path().to_owned(),
            base_inodes: value.base_inodes(),
            compatible: value.compatible(),
            support_level: value.support_level(),
            system_app: compatibility.system_app(),
            shared_uid: compatibility.shared_uid(),
            direct_boot_aware: compatibility.direct_boot_aware(),
        }
    }
}

impl PackageInspectionReport {
    #[doc = "Returns the inspected package."]
    pub const fn package(&self) -> &PackageName {
        &self.package
    }
    #[doc = "Returns whether the first Preview contract is satisfied."]
    pub const fn compatible(&self) -> bool {
        self.compatible
    }
    #[doc = "Returns ordinary, conditional Direct Boot, or blocked support."]
    pub const fn support_level(&self) -> PackageSupportLevel {
        self.support_level
    }
}

#[doc = "One row in the durable managed-app list."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedAppSummary {
    package: PackageName,
    active_slot: SlotId,
    lifecycle: LifecycleState,
}

impl From<ManagedAppInfo> for ManagedAppSummary {
    fn from(value: ManagedAppInfo) -> Self {
        Self {
            package: value.package().clone(),
            active_slot: value.active_slot().clone(),
            lifecycle: value.lifecycle(),
        }
    }
}

impl ManagedAppSummary {
    #[doc = "Returns the managed package."]
    pub const fn package(&self) -> &PackageName {
        &self.package
    }
    #[doc = "Returns the committed active slot."]
    pub const fn active_slot(&self) -> &SlotId {
        &self.active_slot
    }
    #[doc = "Returns the package lifecycle state."]
    pub const fn lifecycle(&self) -> LifecycleState {
        self.lifecycle
    }
}

#[doc = "Bounded list of all durable managed packages."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedAppsReport {
    apps: Vec<ManagedAppSummary>,
}

impl ManagedAppsReport {
    #[doc = "Creates a sorted managed-app report."]
    pub fn new(mut apps: Vec<ManagedAppSummary>) -> Self {
        apps.sort_by(|left, right| left.package().cmp(right.package()));
        Self { apps }
    }
    #[doc = "Returns all managed-app rows."]
    pub fn apps(&self) -> &[ManagedAppSummary] {
        &self.apps
    }
}

#[doc = "One Base or extension slot row."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotSummary {
    slot: SlotId,
    display_name: SlotDisplayName,
    seed_mode: SlotSeedMode,
    state: SlotRecordState,
    active: bool,
    created_version_code: u64,
    last_opened_version_code: u64,
    inodes: DataInodes,
}

impl From<SlotInfo> for SlotSummary {
    fn from(value: SlotInfo) -> Self {
        Self {
            slot: value.slot().clone(),
            display_name: value.display_name().clone(),
            seed_mode: value.seed_mode(),
            state: value.state(),
            active: value.active(),
            created_version_code: value.created_version_code(),
            last_opened_version_code: value.last_opened_version_code(),
            inodes: value.inodes(),
        }
    }
}

impl SlotSummary {
    #[doc = "Returns the immutable slot identifier."]
    pub const fn slot(&self) -> &SlotId {
        &self.slot
    }
    #[doc = "Returns the display-only slot label."]
    pub const fn display_name(&self) -> &SlotDisplayName {
        &self.display_name
    }
    #[doc = "Returns whether this is the committed active slot."]
    pub const fn active(&self) -> bool {
        self.active
    }
}

#[doc = "All visible slot rows for one managed package."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotsReport {
    package: PackageName,
    slots: Vec<SlotSummary>,
}

impl SlotsReport {
    #[doc = "Creates a package-scoped slot report."]
    pub const fn new(package: PackageName, slots: Vec<SlotSummary>) -> Self {
        Self { package, slots }
    }
    #[doc = "Returns the owning package."]
    pub const fn package(&self) -> &PackageName {
        &self.package
    }
    #[doc = "Returns Base and every visible extension slot."]
    pub fn slots(&self) -> &[SlotSummary] {
        &self.slots
    }
}
