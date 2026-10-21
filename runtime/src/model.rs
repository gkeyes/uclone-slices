use core::fmt;
use std::collections::BTreeSet;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::archive::{ArchiveAccountKind, ArchiveDomain, ArchiveScope};
use crate::validation;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ModelError {
    #[error("invalid package name")]
    InvalidPackage,
    #[error("invalid slot identifier")]
    InvalidSlot,
    #[error("invalid display name")]
    InvalidDisplayName,
    #[error("invalid signing identity")]
    InvalidSigningIdentity,
    #[error("invalid package aggregate")]
    InvalidState,
    #[error("package state is not ready")]
    StateConflict,
    #[error("slot does not exist")]
    SlotNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct PackageName(String);

impl PackageName {
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        validation::is_package_name(&value)
            .then_some(Self(value))
            .ok_or(ModelError::InvalidPackage)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PackageName {
    type Err = ModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for PackageName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Account name recorded for the base space in `begin_backup_io` sources and in archive
/// manifests.
///
/// This is data, not UI copy: it is persisted in every existing archive's `manifest.json`,
/// shipped Manager builds show it verbatim in the restore preview, and they send it back as
/// the slot name when a base account is restored into a new slot. Changing it would change
/// what new archives contain and how they render on already-installed Managers.
pub(crate) const BASE_ACCOUNT_NAME: &str = "系统原始空间";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SlotId(String);

impl SlotId {
    pub fn base() -> Self {
        Self("base".to_owned())
    }

    pub fn numbered(number: u64) -> Self {
        Self(format!("slot-{number}"))
    }

    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        validation::is_slot_id(&value)
            .then_some(Self(value))
            .ok_or(ModelError::InvalidSlot)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_base(&self) -> bool {
        self.0 == "base"
    }

    fn number(&self) -> Option<u64> {
        self.0.strip_prefix("slot-")?.parse().ok()
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SlotId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct DisplayName(String);

impl DisplayName {
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        let trimmed = value.trim();
        let valid = !trimmed.is_empty() && !trimmed.chars().any(char::is_control);
        valid
            .then(|| Self(trimmed.to_owned()))
            .ok_or(ModelError::InvalidDisplayName)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for DisplayName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedMode {
    Blank,
    CloneBase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageIdentity {
    uid: u32,
    apk_path: String,
    apk_device: u64,
    apk_inode: u64,
}

impl PackageIdentity {
    pub fn new(
        uid: u32,
        apk_path: impl Into<String>,
        apk_device: u64,
        apk_inode: u64,
    ) -> Result<Self, ModelError> {
        let identity = Self {
            uid,
            apk_path: apk_path.into(),
            apk_device,
            apk_inode,
        };
        identity.validate()?;
        Ok(identity)
    }

    fn validate(&self) -> Result<(), ModelError> {
        (self.uid != 0 && !self.apk_path.is_empty() && self.apk_device != 0 && self.apk_inode != 0)
            .then_some(())
            .ok_or(ModelError::InvalidState)
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }

    pub(crate) fn apk_stamp(&self) -> (&str, u64, u64) {
        (&self.apk_path, self.apk_device, self.apk_inode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningKind {
    Lineage,
    Multiple,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SigningIdentity {
    kind: SigningKind,
    sha256: Vec<String>,
}

impl SigningIdentity {
    #[cfg(test)]
    pub fn new(kind: SigningKind, sha256: Vec<String>) -> Result<Self, ModelError> {
        let identity = Self { kind, sha256 };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        validation::is_signing_digest_set(self.kind == SigningKind::Multiple, &self.sha256)
            .then_some(())
            .ok_or(ModelError::InvalidSigningIdentity)
    }

    pub fn is_compatible_with(&self, persisted: &Self) -> bool {
        if self.validate().is_err() || persisted.validate().is_err() || self.kind != persisted.kind
        {
            return false;
        }
        match self.kind {
            SigningKind::Multiple => self.sha256 == persisted.sha256,
            SigningKind::Lineage => {
                lineage_extends(&self.sha256, &persisted.sha256)
                    || lineage_extends(&persisted.sha256, &self.sha256)
            }
        }
    }
}

fn lineage_extends(longer: &[String], shorter: &[String]) -> bool {
    longer.starts_with(shorter) || longer.ends_with(shorter)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageBinding {
    schema_version: u8,
    signing: SigningIdentity,
}

impl PackageBinding {
    pub fn v1(signing: SigningIdentity) -> Result<Self, ModelError> {
        signing.validate()?;
        Ok(Self {
            schema_version: 1,
            signing,
        })
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != 1 {
            return Err(ModelError::InvalidState);
        }
        self.signing.validate()
    }

    pub fn signing(&self) -> &SigningIdentity {
        &self.signing
    }
}

/// The trash directory an unenroll finalizes into, chosen once after every domain is pending and
/// kept until the package state is removed, so every retry reuses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrashIntent {
    pub package: PackageName,
    /// The `<unix seconds>[-N]` directory below each domain's trash.
    pub name: String,
    /// Whether the CE domain had slot data pending when the name was chosen.
    pub ce: bool,
    /// Whether the DE domain had slot data pending when the name was chosen.
    pub de: bool,
}

impl TrashIntent {
    pub fn validate(&self) -> Result<(), ModelError> {
        validation::is_trash_name(&self.name)
            .then_some(())
            .ok_or(ModelError::InvalidState)
    }
}

/// Trash that a purge keeps however old it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashHold {
    /// `trash/<name>/<package>`, which an unenroll still in progress is finalizing.
    Entry { package: PackageName, name: String },
    /// Every trash entry of a package whose trash intent cannot be read.
    Package(PackageName),
}

impl TrashHold {
    pub fn holds(&self, name: &str, package: &str) -> bool {
        match self {
            Self::Entry {
                package: held,
                name: held_name,
            } => held.as_str() == package && held_name == name,
            Self::Package(held) => held.as_str() == package,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebindIntent {
    pub package: PackageName,
    pub previous_identity: PackageIdentity,
    pub target_identity: PackageIdentity,
    pub signing: SigningIdentity,
    pub active_slot: SlotId,
}

impl RebindIntent {
    pub fn validate(&self) -> Result<(), ModelError> {
        self.previous_identity.validate()?;
        self.target_identity.validate()?;
        self.signing.validate()?;
        if self.previous_identity.uid() != self.target_identity.uid() {
            return Err(ModelError::InvalidState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInspection {
    pub identity: PackageIdentity,
    pub was_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedView {
    Base,
    Slot(SlotId),
    Maintenance(AccountIoToken),
    Inconsistent,
}

impl ObservedView {
    pub fn for_slot(slot: &SlotId) -> Self {
        if slot.is_base() {
            Self::Base
        } else {
            Self::Slot(slot.clone())
        }
    }

    pub fn matches(&self, slot: &SlotId) -> bool {
        self == &Self::for_slot(slot)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct AccountIoToken(String);

impl AccountIoToken {
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        validation::is_io_token(&value)
            .then_some(Self(value))
            .ok_or(ModelError::InvalidState)
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        let mut value = String::with_capacity(32);
        for byte in bytes {
            use core::fmt::Write as _;
            let _ignored = write!(value, "{byte:02x}");
        }
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountIoToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AccountIoToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ArchiveAccountId(String);

impl ArchiveAccountId {
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        validation::is_identifier(&value)
            .then_some(Self(value))
            .ok_or(ModelError::InvalidState)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArchiveAccountId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArchiveAccountId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct TransferId(String);

impl TransferId {
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        validation::is_identifier(&value)
            .then_some(Self(value))
            .ok_or(ModelError::InvalidState)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for TransferId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountIoScope {
    Account { slot: SlotId },
    AllAccounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountIoKind {
    Backup,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestorePath {
    pub domain: ArchiveDomain,
    pub path: String,
}

impl RestorePath {
    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        validation::is_path_pattern(&self.path)
            .then_some(())
            .ok_or(ModelError::InvalidState)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum RestorePolicy {
    #[default]
    Replace,
    PreservePaths {
        paths: Vec<RestorePath>,
    },
}

impl RestorePolicy {
    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        let Self::PreservePaths { paths } = self else {
            return Ok(());
        };
        if paths.is_empty() || paths.len() > validation::MAX_PATH_RULES {
            return Err(ModelError::InvalidState);
        }
        for path in paths {
            path.validate()?;
        }
        if paths.iter().collect::<BTreeSet<_>>().len() != paths.len() {
            return Err(ModelError::InvalidState);
        }
        if validation::any_patterns_overlap(
            paths.iter().map(|path| (path.domain, path.path.as_str())),
        ) {
            return Err(ModelError::InvalidState);
        }
        Ok(())
    }

    pub(crate) fn paths_for(&self, domain: ArchiveDomain) -> Vec<&str> {
        match self {
            Self::Replace => Vec::new(),
            Self::PreservePaths { paths } => paths
                .iter()
                .filter(|path| path.domain == domain)
                .map(|path| path.path.as_str())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RestoreTarget {
    Existing { slot: SlotId },
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreMapping {
    pub archive_account_id: ArchiveAccountId,
    pub archive_kind: ArchiveAccountKind,
    pub name: DisplayName,
    pub target: RestoreTarget,
    #[serde(default)]
    pub restore_policy: RestorePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedState {
    pub scope: ArchiveScope,
    pub active_account_id: Option<ArchiveAccountId>,
    #[serde(default)]
    pub launch_after_reboot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageEnabledState {
    Default,
    Enabled,
    Disabled,
    DisabledUser,
    DisabledUntilUsed,
}

impl PackageEnabledState {
    /// Whether the app may be launched in this state.
    pub fn allows_launch(self) -> bool {
        !matches!(
            self,
            Self::Disabled | Self::DisabledUser | Self::DisabledUntilUsed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountIoPhase {
    Preparing,
    Ready,
    Replacing {
        archive_account_id: ArchiveAccountId,
        target_slot: SlotId,
    },
    Finalizing,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreItemState {
    Restored,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreItemResult {
    pub archive_account_id: ArchiveAccountId,
    pub target_slot: Option<SlotId>,
    pub state: RestoreItemState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreBatchResult {
    pub package: PackageName,
    pub items: Vec<RestoreItemResult>,
    pub active_slot: SlotId,
    pub launch_after_reboot: bool,
    pub app_started: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountIoSource {
    pub archive_account_id: ArchiveAccountId,
    pub slot: SlotId,
    pub name: String,
    pub ce_path: String,
    pub de_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreStaging {
    pub archive_account_id: ArchiveAccountId,
    pub ce_path: String,
    pub de_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountIoLease {
    pub io_token: AccountIoToken,
    pub package: PackageName,
    pub kind: AccountIoKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<AccountIoSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staging: Vec<RestoreStaging>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountIoStatus {
    pub io_token: AccountIoToken,
    pub package: PackageName,
    pub kind: AccountIoKind,
    pub phase: AccountIoPhase,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed: Vec<RestoreItemResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolvedRestoreMapping {
    pub archive_account_id: ArchiveAccountId,
    pub archive_kind: ArchiveAccountKind,
    pub name: DisplayName,
    pub previous_name: Option<DisplayName>,
    pub target_slot: SlotId,
    pub create_slot: bool,
    #[serde(default)]
    pub restore_policy: RestorePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccountIoIntent {
    pub schema_version: u8,
    pub io_token: AccountIoToken,
    pub package: PackageName,
    pub kind: AccountIoKind,
    pub scope: Option<AccountIoScope>,
    pub transfer_id: Option<TransferId>,
    #[serde(default)]
    pub mappings: Vec<ResolvedRestoreMapping>,
    pub archived_state: Option<ArchivedState>,
    pub previous_active_slot: SlotId,
    pub previous_launch_after_reboot: bool,
    pub previous_was_running: bool,
    pub previous_enabled_state: Option<PackageEnabledState>,
    #[serde(default)]
    pub maintenance_active: bool,
    pub phase: AccountIoPhase,
    #[serde(default)]
    pub completed: Vec<RestoreItemResult>,
}

impl AccountIoIntent {
    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        if !matches!(self.schema_version, 1 | 2) {
            return Err(ModelError::InvalidState);
        }
        match self.kind {
            AccountIoKind::Backup => {
                if self.scope.is_none()
                    || self.transfer_id.is_some()
                    || !self.mappings.is_empty()
                    || self.archived_state.is_some()
                {
                    return Err(ModelError::InvalidState);
                }
            }
            AccountIoKind::Restore => {
                if self.scope.is_some()
                    || self.transfer_id.is_none()
                    || self.mappings.is_empty()
                    || self.archived_state.is_none()
                {
                    return Err(ModelError::InvalidState);
                }
                let source_ids = self
                    .mappings
                    .iter()
                    .map(|mapping| &mapping.archive_account_id)
                    .collect::<BTreeSet<_>>();
                let targets = self
                    .mappings
                    .iter()
                    .map(|mapping| &mapping.target_slot)
                    .collect::<BTreeSet<_>>();
                if source_ids.len() != self.mappings.len() || targets.len() != self.mappings.len() {
                    return Err(ModelError::InvalidState);
                }
                for mapping in &self.mappings {
                    mapping.restore_policy.validate()?;
                    if self.schema_version == 1 && mapping.restore_policy != RestorePolicy::Replace
                    {
                        return Err(ModelError::InvalidState);
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn status(&self) -> AccountIoStatus {
        AccountIoStatus {
            io_token: self.io_token.clone(),
            package: self.package.clone(),
            kind: self.kind,
            phase: self.phase.clone(),
            completed: self.completed.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    id: SlotId,
    display_name: DisplayName,
}

impl Slot {
    pub fn id(&self) -> &SlotId {
        &self.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Lifecycle {
    Ready,
    Creating {
        target: SlotId,
        restore_running: bool,
    },
    Activating {
        previous: SlotId,
        target: SlotId,
    },
    Deleting {
        target: SlotId,
    },
    Unenrolling,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageAggregate {
    package: PackageName,
    identity: PackageIdentity,
    slots: Vec<Slot>,
    active_slot: SlotId,
    #[serde(default)]
    launch_after_reboot: bool,
    #[serde(default)]
    default_slot: Option<SlotId>,
    next_slot_number: u64,
    lifecycle: Lifecycle,
}

impl PackageAggregate {
    pub fn enrolled(package: PackageName, identity: PackageIdentity) -> Self {
        Self {
            package,
            identity,
            slots: vec![Slot {
                id: SlotId::base(),
                display_name: DisplayName("Base".to_owned()),
            }],
            active_slot: SlotId::base(),
            launch_after_reboot: false,
            default_slot: None,
            next_slot_number: 1,
            lifecycle: Lifecycle::Ready,
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        self.identity.validate()?;
        let mut ids = BTreeSet::new();
        let mut base_count = 0_u8;
        let mut highest_number = 0_u64;
        for slot in &self.slots {
            if !ids.insert(slot.id.clone()) {
                return Err(ModelError::InvalidState);
            }
            if slot.id.is_base() {
                base_count = base_count.saturating_add(1);
            } else {
                highest_number = highest_number.max(
                    slot.id
                        .number()
                        .filter(|number| *number != 0)
                        .ok_or(ModelError::InvalidState)?,
                );
            }
        }
        if base_count != 1
            || !ids.contains(&self.active_slot)
            || self
                .default_slot
                .as_ref()
                .is_some_and(|slot| !ids.contains(slot))
            || self.next_slot_number == 0
            || self.next_slot_number <= highest_number
        {
            return Err(ModelError::InvalidState);
        }
        match &self.lifecycle {
            Lifecycle::Ready => {}
            Lifecycle::Creating { target, .. } => {
                if target.is_base() || ids.contains(target) {
                    return Err(ModelError::InvalidState);
                }
            }
            Lifecycle::Activating { previous, target } => {
                if previous != &self.active_slot
                    || previous == target
                    || !ids.contains(previous)
                    || !ids.contains(target)
                {
                    return Err(ModelError::InvalidState);
                }
            }
            Lifecycle::Deleting { target } => {
                if target.is_base() || target == &self.active_slot || !ids.contains(target) {
                    return Err(ModelError::InvalidState);
                }
            }
            Lifecycle::Unenrolling => {}
        }
        Ok(())
    }

    pub fn package(&self) -> &PackageName {
        &self.package
    }

    pub fn identity(&self) -> &PackageIdentity {
        &self.identity
    }

    pub fn active_slot(&self) -> &SlotId {
        &self.active_slot
    }

    pub fn rebind_identity(&mut self, identity: PackageIdentity) -> Result<(), ModelError> {
        self.require_ready()?;
        if self.identity.uid() != identity.uid() {
            return Err(ModelError::StateConflict);
        }
        self.identity = identity;
        self.validate()
    }

    pub fn launch_after_reboot(&self) -> bool {
        self.launch_after_reboot
    }

    pub fn set_launch_after_reboot(&mut self, enabled: bool) -> Result<(), ModelError> {
        self.require_ready()?;
        self.launch_after_reboot = enabled;
        self.validate()
    }

    pub fn default_slot(&self) -> Option<&SlotId> {
        self.default_slot.as_ref()
    }

    pub fn set_default_slot(&mut self, slot: Option<SlotId>) -> Result<(), ModelError> {
        self.require_ready()?;
        if slot.as_ref().is_some_and(|slot| !self.has_slot(slot)) {
            return Err(ModelError::SlotNotFound);
        }
        self.default_slot = slot;
        self.validate()
    }

    pub fn has_slot(&self, id: &SlotId) -> bool {
        self.slot(id).is_some()
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Ready)
    }

    fn slot(&self, id: &SlotId) -> Option<&Slot> {
        self.slots.iter().find(|slot| slot.id() == id)
    }

    pub fn rename_slot(
        &mut self,
        target: &SlotId,
        display_name: DisplayName,
    ) -> Result<(), ModelError> {
        self.require_ready()?;
        if target.is_base() {
            return Err(ModelError::StateConflict);
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.id() == target)
            .ok_or(ModelError::SlotNotFound)?;
        slot.display_name = display_name;
        self.validate()
    }

    pub fn begin_deletion(&mut self, target: &SlotId) -> Result<(), ModelError> {
        self.require_ready()?;
        if target.is_base() || target == &self.active_slot {
            return Err(ModelError::StateConflict);
        }
        if self.slot(target).is_none() {
            return Err(ModelError::SlotNotFound);
        }
        self.lifecycle = Lifecycle::Deleting {
            target: target.clone(),
        };
        self.validate()
    }

    pub fn pending_deletion(&self) -> Option<&SlotId> {
        match &self.lifecycle {
            Lifecycle::Deleting { target } => Some(target),
            Lifecycle::Ready
            | Lifecycle::Creating { .. }
            | Lifecycle::Activating { .. }
            | Lifecycle::Unenrolling => None,
        }
    }

    pub fn finish_deletion(&mut self, target: &SlotId) -> Result<(), ModelError> {
        if !matches!(
            &self.lifecycle,
            Lifecycle::Deleting { target: pending } if pending == target
        ) {
            return Err(ModelError::StateConflict);
        }
        let index = self
            .slots
            .iter()
            .position(|slot| slot.id() == target)
            .ok_or(ModelError::StateConflict)?;
        self.slots.remove(index);
        if self.default_slot.as_ref() == Some(target) {
            self.default_slot = None;
        }
        self.lifecycle = Lifecycle::Ready;
        self.validate()
    }

    pub fn begin_unenrollment(&mut self) -> Result<(), ModelError> {
        self.require_ready()?;
        self.lifecycle = Lifecycle::Unenrolling;
        self.validate()
    }

    pub fn is_unenrolling(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Unenrolling)
    }

    pub fn reserve_slot(&mut self, display_name: DisplayName) -> Result<Slot, ModelError> {
        self.require_ready()?;
        let id = SlotId::numbered(self.next_slot_number);
        if self.slot(&id).is_some() {
            return Err(ModelError::StateConflict);
        }
        self.next_slot_number = self
            .next_slot_number
            .checked_add(1)
            .ok_or(ModelError::StateConflict)?;
        Ok(Slot { id, display_name })
    }

    pub(crate) fn reserve_restored_slot(
        &mut self,
        display_name: DisplayName,
    ) -> Result<SlotId, ModelError> {
        let slot = self.reserve_slot(display_name)?;
        Ok(slot.id)
    }

    pub(crate) fn install_restored_slot(
        &mut self,
        target: &SlotId,
        display_name: DisplayName,
    ) -> Result<(), ModelError> {
        self.require_ready()?;
        if target.is_base() || self.slot(target).is_some() {
            return Err(ModelError::StateConflict);
        }
        self.slots.push(Slot {
            id: target.clone(),
            display_name,
        });
        self.validate()
    }

    pub(crate) fn remove_restored_slot(&mut self, target: &SlotId) -> Result<(), ModelError> {
        self.require_ready()?;
        if target.is_base() || target == &self.active_slot {
            return Err(ModelError::StateConflict);
        }
        let index = self
            .slots
            .iter()
            .position(|slot| slot.id() == target)
            .ok_or(ModelError::SlotNotFound)?;
        self.slots.remove(index);
        self.validate()
    }

    pub(crate) fn set_restored_active_slot(&mut self, target: &SlotId) -> Result<(), ModelError> {
        self.require_ready()?;
        if self.slot(target).is_none() {
            return Err(ModelError::SlotNotFound);
        }
        self.active_slot = target.clone();
        self.validate()
    }

    pub fn begin_creation(&mut self, slot: &Slot, restore_running: bool) -> Result<(), ModelError> {
        self.require_ready()?;
        if self.slot(slot.id()).is_some() || slot.id().is_base() {
            return Err(ModelError::StateConflict);
        }
        self.lifecycle = Lifecycle::Creating {
            target: slot.id().clone(),
            restore_running,
        };
        Ok(())
    }

    pub fn finish_creation(&mut self, slot: Slot) -> Result<(), ModelError> {
        if !matches!(
            &self.lifecycle,
            Lifecycle::Creating { target, .. } if target == slot.id()
        ) || self.slot(slot.id()).is_some()
        {
            return Err(ModelError::StateConflict);
        }
        self.slots.push(slot);
        self.lifecycle = Lifecycle::Ready;
        self.validate()
    }

    pub fn pending_creation(&self) -> Option<(&SlotId, bool)> {
        match &self.lifecycle {
            Lifecycle::Creating {
                target,
                restore_running,
            } => Some((target, *restore_running)),
            Lifecycle::Ready
            | Lifecycle::Activating { .. }
            | Lifecycle::Deleting { .. }
            | Lifecycle::Unenrolling => None,
        }
    }

    pub fn abort_creation(&mut self, target: &SlotId) -> Result<(), ModelError> {
        if !matches!(
            &self.lifecycle,
            Lifecycle::Creating { target: pending, .. } if pending == target
        ) {
            return Err(ModelError::StateConflict);
        }
        self.lifecycle = Lifecycle::Ready;
        self.validate()
    }

    pub fn begin_activation(&mut self, target: &SlotId) -> Result<(), ModelError> {
        self.require_ready()?;
        if self.slot(target).is_none() {
            return Err(ModelError::SlotNotFound);
        }
        if target == &self.active_slot {
            return Err(ModelError::StateConflict);
        }
        self.lifecycle = Lifecycle::Activating {
            previous: self.active_slot.clone(),
            target: target.clone(),
        };
        Ok(())
    }

    pub fn pending_activation(&self) -> Option<(&SlotId, &SlotId)> {
        match &self.lifecycle {
            Lifecycle::Activating { previous, target } => Some((previous, target)),
            Lifecycle::Ready
            | Lifecycle::Creating { .. }
            | Lifecycle::Deleting { .. }
            | Lifecycle::Unenrolling => None,
        }
    }

    pub fn finish_activation(&mut self, target: &SlotId) -> Result<(), ModelError> {
        match &self.lifecycle {
            Lifecycle::Activating {
                target: expected, ..
            } if expected == target => {
                self.active_slot = target.clone();
                self.lifecycle = Lifecycle::Ready;
                self.validate()
            }
            Lifecycle::Ready if &self.active_slot == target => Ok(()),
            Lifecycle::Ready
            | Lifecycle::Creating { .. }
            | Lifecycle::Activating { .. }
            | Lifecycle::Deleting { .. }
            | Lifecycle::Unenrolling => Err(ModelError::StateConflict),
        }
    }

    pub fn abort_activation(&mut self, previous: &SlotId) -> Result<(), ModelError> {
        if !matches!(
            &self.lifecycle,
            Lifecycle::Activating { previous: expected, .. } if expected == previous
        ) {
            return Err(ModelError::StateConflict);
        }
        self.active_slot = previous.clone();
        self.lifecycle = Lifecycle::Ready;
        self.validate()
    }

    pub fn snapshot(&self) -> Result<PackageSnapshot, ModelError> {
        self.snapshot_with_binding(BindingState::Ready)
    }

    pub fn snapshot_with_binding(
        &self,
        binding_state: BindingState,
    ) -> Result<PackageSnapshot, ModelError> {
        self.validate()?;
        self.require_ready()?;
        Ok(PackageSnapshot {
            package: self.package.clone(),
            active_slot: self.active_slot.clone(),
            launch_after_reboot: self.launch_after_reboot,
            default_slot: self.default_slot.clone(),
            binding_state,
            slots: self
                .slots
                .iter()
                .map(|slot| SlotSnapshot {
                    id: slot.id.clone(),
                    name: slot.display_name.as_str().to_owned(),
                })
                .collect(),
        })
    }

    fn require_ready(&self) -> Result<(), ModelError> {
        matches!(self.lifecycle, Lifecycle::Ready)
            .then_some(())
            .ok_or(ModelError::StateConflict)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingState {
    Ready,
    LegacyUnbound,
    RebindRequired,
    LegacyConfirmationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSnapshot {
    pub id: SlotId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSnapshot {
    pub package: PackageName,
    pub active_slot: SlotId,
    pub launch_after_reboot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_slot: Option<SlotId>,
    pub binding_state: BindingState,
    pub slots: Vec<SlotSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub build_id: String,
    pub binary_build_id: String,
    pub launch_gate_ready: bool,
    pub process_view_ready: bool,
}

#[cfg(test)]
mod tests;
