use core::fmt;
use std::collections::BTreeSet;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
        let valid = !value.is_empty()
            && value.contains('.')
            && !value.starts_with('.')
            && !value.ends_with('.')
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_'));
        valid
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
        let numbered = value.strip_prefix("slot-").is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        });
        (value == "base" || numbered)
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
    pub fn new(kind: SigningKind, sha256: Vec<String>) -> Result<Self, ModelError> {
        let identity = Self { kind, sha256 };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let valid_digest = |value: &String| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        };
        if self.sha256.is_empty() || self.sha256.len() > 16 || !self.sha256.iter().all(valid_digest)
        {
            return Err(ModelError::InvalidSigningIdentity);
        }
        let unique = self.sha256.iter().collect::<BTreeSet<_>>();
        if unique.len() != self.sha256.len() {
            return Err(ModelError::InvalidSigningIdentity);
        }
        if self.kind == SigningKind::Multiple
            && (self.sha256.len() < 2
                || !self
                    .sha256
                    .windows(2)
                    .all(|pair| pair[0].as_str() < pair[1].as_str()))
        {
            return Err(ModelError::InvalidSigningIdentity);
        }
        Ok(())
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
    desktop_shortcut_slot: Option<SlotId>,
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
            desktop_shortcut_slot: None,
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
            || self.next_slot_number == 0
            || self.next_slot_number <= highest_number
        {
            return Err(ModelError::InvalidState);
        }
        if self
            .desktop_shortcut_slot
            .as_ref()
            .is_some_and(|slot| slot.is_base() || !ids.contains(slot))
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

    pub fn desktop_shortcut_slot(&self) -> Option<&SlotId> {
        self.desktop_shortcut_slot.as_ref()
    }

    pub fn desktop_shortcut_target(&self) -> Result<&SlotId, ModelError> {
        self.require_ready()?;
        let bound = self
            .desktop_shortcut_slot()
            .ok_or(ModelError::SlotNotFound)?;
        if &self.active_slot == bound {
            self.slot(&SlotId::base())
                .map(Slot::id)
                .ok_or(ModelError::InvalidState)
        } else {
            Ok(bound)
        }
    }

    pub fn set_desktop_shortcut(&mut self, target: Option<SlotId>) -> Result<(), ModelError> {
        self.require_ready()?;
        if target
            .as_ref()
            .is_some_and(|slot| slot.is_base() || self.slot(slot).is_none())
        {
            return Err(ModelError::StateConflict);
        }
        self.desktop_shortcut_slot = target;
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
        if self.desktop_shortcut_slot.as_ref() == Some(target) {
            self.desktop_shortcut_slot = None;
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
            desktop_shortcut_slot: self.desktop_shortcut_slot.clone(),
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
    pub desktop_shortcut_slot: Option<SlotId>,
    pub binding_state: BindingState,
    pub slots: Vec<SlotSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub build_id: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn package() -> PackageName {
        PackageName::new("com.example.app").unwrap()
    }

    fn identity() -> PackageIdentity {
        PackageIdentity::new(10_000, "/data/app/example/base.apk", 1, 2).unwrap()
    }

    #[test]
    fn identifiers_only_enforce_operational_grammar() {
        assert!(PackageName::new("com.example.app").is_ok());
        assert!(PackageName::new("single").is_err());
        assert!(PackageName::new("../app").is_err());
        assert!(DisplayName::new("Work profile").is_ok());
        assert!(DisplayName::new("\n").is_err());
    }

    #[test]
    fn aggregate_owns_slot_lifecycle() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        let slot_id = slot.id().clone();
        aggregate.begin_creation(&slot, false).unwrap();
        aggregate.finish_creation(slot).unwrap();
        aggregate.begin_activation(&slot_id).unwrap();
        aggregate.finish_activation(&slot_id).unwrap();

        assert_eq!(aggregate.active_slot(), &slot_id);
        assert_eq!(aggregate.snapshot().unwrap().slots.len(), 2);
    }

    #[test]
    fn reboot_launch_defaults_off_and_survives_round_trip() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());

        assert!(!aggregate.launch_after_reboot());
        aggregate.set_launch_after_reboot(true).unwrap();

        let encoded = serde_json::to_string(&aggregate).unwrap();
        let restored: PackageAggregate = serde_json::from_str(&encoded).unwrap();
        assert!(restored.launch_after_reboot());
        assert!(restored.snapshot().unwrap().launch_after_reboot);
    }

    #[test]
    fn persisted_aggregate_without_reboot_launch_setting_migrates_off() {
        let aggregate = PackageAggregate::enrolled(package(), identity());
        let mut legacy = serde_json::to_value(aggregate).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("launch_after_reboot");

        let restored: PackageAggregate = serde_json::from_value(legacy).unwrap();

        assert!(!restored.launch_after_reboot());
        assert!(!restored.snapshot().unwrap().launch_after_reboot);
    }

    #[test]
    fn persisted_aggregate_without_desktop_shortcut_migrates_unbound() {
        let aggregate = PackageAggregate::enrolled(package(), identity());
        let mut legacy = serde_json::to_value(aggregate).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("desktop_shortcut_slot");

        let restored: PackageAggregate = serde_json::from_value(legacy).unwrap();

        assert_eq!(restored.desktop_shortcut_slot(), None);
        assert_eq!(restored.snapshot().unwrap().desktop_shortcut_slot, None);
    }

    #[test]
    fn desktop_shortcut_switches_between_bound_slot_and_base() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        let slot_id = slot.id().clone();
        aggregate.begin_creation(&slot, false).unwrap();
        aggregate.finish_creation(slot).unwrap();

        assert_eq!(
            aggregate.set_desktop_shortcut(Some(SlotId::base())),
            Err(ModelError::StateConflict)
        );
        aggregate
            .set_desktop_shortcut(Some(slot_id.clone()))
            .unwrap();
        assert_eq!(aggregate.desktop_shortcut_target().unwrap(), &slot_id);

        aggregate.begin_activation(&slot_id).unwrap();
        aggregate.finish_activation(&slot_id).unwrap();
        assert_eq!(
            aggregate.desktop_shortcut_target().unwrap(),
            &SlotId::base()
        );
    }

    #[test]
    fn aggregate_wire_format_stays_independent_from_the_v1_binding_sidecar() {
        let aggregate = PackageAggregate::enrolled(package(), identity());

        let encoded = serde_json::to_value(aggregate).unwrap();

        let fields = encoded.as_object().unwrap();
        assert!(!fields.contains_key("binding"));
        assert!(!fields.contains_key("signing"));
        assert!(!fields.contains_key("binding_state"));
    }

    #[test]
    fn signing_lineages_accept_verified_rotation_in_either_direction() {
        let old = SigningIdentity::new(SigningKind::Lineage, vec!["a".repeat(64)]).unwrap();
        let rotated =
            SigningIdentity::new(SigningKind::Lineage, vec!["a".repeat(64), "b".repeat(64)])
                .unwrap();

        assert!(old.is_compatible_with(&rotated));
        assert!(rotated.is_compatible_with(&old));
        let newest_first =
            SigningIdentity::new(SigningKind::Lineage, vec!["b".repeat(64), "a".repeat(64)])
                .unwrap();
        assert!(old.is_compatible_with(&newest_first));
    }

    #[test]
    fn multiple_signers_require_a_sorted_exact_set() {
        let first =
            SigningIdentity::new(SigningKind::Multiple, vec!["a".repeat(64), "b".repeat(64)])
                .unwrap();
        let same =
            SigningIdentity::new(SigningKind::Multiple, vec!["a".repeat(64), "b".repeat(64)])
                .unwrap();
        let different =
            SigningIdentity::new(SigningKind::Multiple, vec!["a".repeat(64), "c".repeat(64)])
                .unwrap();

        assert!(first.is_compatible_with(&same));
        assert!(!first.is_compatible_with(&different));
        assert!(
            SigningIdentity::new(SigningKind::Multiple, vec!["b".repeat(64), "a".repeat(64)],)
                .is_err()
        );
        assert!(SigningIdentity::new(SigningKind::Lineage, vec!["A".repeat(64)]).is_err());
        assert!(SigningIdentity::new(SigningKind::Lineage, vec!["a".repeat(63)]).is_err());
    }

    #[test]
    fn invalid_persisted_relationships_are_rejected() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());
        aggregate.active_slot = SlotId::numbered(9);

        assert_eq!(aggregate.validate(), Err(ModelError::InvalidState));
    }

    #[test]
    fn interrupted_activation_keeps_both_directions() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        let slot_id = slot.id().clone();
        aggregate.begin_creation(&slot, false).unwrap();
        aggregate.finish_creation(slot).unwrap();
        aggregate.begin_activation(&slot_id).unwrap();

        let base = SlotId::base();
        assert_eq!(aggregate.pending_activation(), Some((&base, &slot_id)));
        aggregate.abort_activation(&base).unwrap();
        assert!(aggregate.snapshot().is_ok());
    }

    #[test]
    fn rename_and_delete_preserve_slot_identity_and_monotonic_numbering() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        let slot_id = slot.id().clone();
        aggregate.begin_creation(&slot, false).unwrap();
        aggregate.finish_creation(slot).unwrap();

        aggregate
            .rename_slot(&slot_id, DisplayName::new("Personal").unwrap())
            .unwrap();
        assert_eq!(aggregate.snapshot().unwrap().slots[1].name, "Personal");

        aggregate.begin_deletion(&slot_id).unwrap();
        assert_eq!(aggregate.pending_deletion(), Some(&slot_id));
        assert_eq!(aggregate.snapshot(), Err(ModelError::StateConflict));
        aggregate.finish_deletion(&slot_id).unwrap();
        assert!(!aggregate.has_slot(&slot_id));

        let next = aggregate
            .reserve_slot(DisplayName::new("Next").unwrap())
            .unwrap();
        assert_eq!(next.id(), &SlotId::numbered(2));
    }

    #[test]
    fn deleting_the_bound_slot_clears_only_the_shortcut_binding() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        let slot_id = slot.id().clone();
        aggregate.begin_creation(&slot, false).unwrap();
        aggregate.finish_creation(slot).unwrap();
        aggregate
            .set_desktop_shortcut(Some(slot_id.clone()))
            .unwrap();

        aggregate.begin_deletion(&slot_id).unwrap();
        aggregate.finish_deletion(&slot_id).unwrap();

        assert_eq!(aggregate.desktop_shortcut_slot(), None);
        assert_eq!(aggregate.snapshot().unwrap().slots.len(), 1);
    }

    #[test]
    fn base_active_and_missing_slots_cannot_be_deleted() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());
        assert_eq!(
            aggregate.begin_deletion(&SlotId::base()),
            Err(ModelError::StateConflict)
        );
        assert_eq!(
            aggregate.begin_deletion(&SlotId::numbered(9)),
            Err(ModelError::SlotNotFound)
        );

        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        let slot_id = slot.id().clone();
        aggregate.begin_creation(&slot, false).unwrap();
        aggregate.finish_creation(slot).unwrap();
        aggregate.begin_activation(&slot_id).unwrap();
        aggregate.finish_activation(&slot_id).unwrap();

        assert_eq!(
            aggregate.begin_deletion(&slot_id),
            Err(ModelError::StateConflict)
        );
        aggregate
            .rename_slot(&slot_id, DisplayName::new("Current").unwrap())
            .unwrap();
        assert_eq!(aggregate.snapshot().unwrap().slots[1].name, "Current");
        assert_eq!(
            aggregate.rename_slot(&SlotId::base(), DisplayName::new("Root").unwrap()),
            Err(ModelError::StateConflict)
        );
    }

    #[test]
    fn unenrollment_is_a_durable_terminal_intent() {
        let mut aggregate = PackageAggregate::enrolled(package(), identity());

        aggregate.begin_unenrollment().unwrap();

        assert!(aggregate.is_unenrolling());
        assert_eq!(aggregate.snapshot(), Err(ModelError::StateConflict));
        assert_eq!(
            aggregate.begin_unenrollment(),
            Err(ModelError::StateConflict)
        );
        let encoded = serde_json::to_string(&aggregate).unwrap();
        assert!(encoded.contains(r#""state":"unenrolling""#));
        let restored: PackageAggregate = serde_json::from_str(&encoded).unwrap();
        assert!(restored.is_unenrolling());
        assert!(restored.validate().is_ok());
    }
}
