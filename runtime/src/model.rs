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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageAggregate {
    package: PackageName,
    identity: PackageIdentity,
    slots: Vec<Slot>,
    active_slot: SlotId,
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

    pub fn has_slot(&self, id: &SlotId) -> bool {
        self.slot(id).is_some()
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Ready)
    }

    fn slot(&self, id: &SlotId) -> Option<&Slot> {
        self.slots.iter().find(|slot| slot.id() == id)
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
            Lifecycle::Ready | Lifecycle::Activating { .. } => None,
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
            Lifecycle::Ready | Lifecycle::Creating { .. } => None,
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
            Lifecycle::Ready | Lifecycle::Creating { .. } | Lifecycle::Activating { .. } => {
                Err(ModelError::StateConflict)
            }
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
        self.validate()?;
        self.require_ready()?;
        Ok(PackageSnapshot {
            package: self.package.clone(),
            active_slot: self.active_slot.clone(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSnapshot {
    pub id: SlotId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSnapshot {
    pub package: PackageName,
    pub active_slot: SlotId,
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
}
