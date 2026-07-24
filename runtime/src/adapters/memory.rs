use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::model::{
    Capabilities, ObservedView, PackageAggregate, PackageIdentity, PackageInspection, PackageName,
    SeedMode, Slot, SlotId,
};
use crate::ports::{AdapterError, AndroidOps, PackageStore, SlotStorage};

#[derive(Debug, Default)]
pub(crate) struct MemoryPackageStore {
    packages: BTreeMap<PackageName, PackageAggregate>,
    save_calls: usize,
    failed_save_calls: BTreeSet<usize>,
}

impl MemoryPackageStore {
    pub(crate) fn fail_next_save(&mut self) {
        self.failed_save_calls.insert(self.save_calls + 1);
    }

    pub(crate) fn fail_save_call(&mut self, call: usize) {
        self.failed_save_calls.insert(call);
    }

    pub(crate) fn save_calls(&self) -> usize {
        self.save_calls
    }
}

impl PackageStore for MemoryPackageStore {
    fn list(&self) -> Result<Vec<PackageName>, AdapterError> {
        Ok(self.packages.keys().cloned().collect())
    }

    fn load(&self, package: &PackageName) -> Result<Option<PackageAggregate>, AdapterError> {
        Ok(self.packages.get(package).cloned())
    }

    fn save(&mut self, aggregate: &PackageAggregate) -> Result<(), AdapterError> {
        self.save_calls += 1;
        if self.failed_save_calls.remove(&self.save_calls) {
            return Err(AdapterError::new("injected package-store failure"));
        }
        aggregate
            .validate()
            .map_err(|error| AdapterError::new(error.to_string()))?;
        self.packages
            .insert(aggregate.package().clone(), aggregate.clone());
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotFailure {
    MaterializeCe,
    MaterializeDe,
    Discard,
    Require,
}

#[derive(Debug, Default)]
pub(crate) struct MemorySlotStorage {
    domains: BTreeMap<(PackageName, SlotId), (bool, bool)>,
    failures: VecDeque<SlotFailure>,
}

impl MemorySlotStorage {
    pub(crate) fn contains(&self, package: &PackageName, slot: &SlotId) -> bool {
        self.domains
            .get(&(package.clone(), slot.clone()))
            .is_some_and(|pair| *pair == (true, true))
    }

    pub(crate) fn remove_de(&mut self, package: &PackageName, slot: &SlotId) {
        if let Some(pair) = self.domains.get_mut(&(package.clone(), slot.clone())) {
            pair.1 = false;
        }
    }

    pub(crate) fn fail_next(&mut self, failure: SlotFailure) {
        self.failures.push_back(failure);
    }

    fn take_failure(&mut self, expected: SlotFailure) -> bool {
        if self.failures.front() == Some(&expected) {
            self.failures.pop_front();
            true
        } else {
            false
        }
    }
}

impl SlotStorage for MemorySlotStorage {
    fn materialize(
        &mut self,
        package: &PackageName,
        slot: &Slot,
        _seed: SeedMode,
    ) -> Result<(), AdapterError> {
        let key = (package.clone(), slot.id().clone());
        if self.domains.contains_key(&key) {
            return Err(AdapterError::new("slot already materialized"));
        }
        if self.take_failure(SlotFailure::MaterializeCe) {
            return Err(AdapterError::new("injected CE materialization failure"));
        }
        self.domains.insert(key.clone(), (true, false));
        if self.take_failure(SlotFailure::MaterializeDe) {
            return Err(AdapterError::new("injected DE materialization failure"));
        }
        self.domains.insert(key, (true, true));
        Ok(())
    }

    fn discard(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError> {
        if self.take_failure(SlotFailure::Discard) {
            return Err(AdapterError::new("injected slot discard failure"));
        }
        self.domains.remove(&(package.clone(), slot.clone()));
        Ok(())
    }

    fn require_complete_pair(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(), AdapterError> {
        if self.failures.front() == Some(&SlotFailure::Require) {
            return Err(AdapterError::new("injected pair verification failure"));
        }
        self.contains(package, slot)
            .then_some(())
            .ok_or_else(|| AdapterError::new("slot CE/DE pair is incomplete"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AndroidFailure {
    Inspect,
    ForceStop,
    Observe,
    Apply,
    Launch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AndroidCall {
    Inspect(PackageName),
    ForceStop(PackageName),
    Observe(PackageName),
    Apply(PackageName, SlotId),
    Launch(PackageName, SlotId),
}

#[derive(Debug)]
pub(crate) struct MemoryAndroidOps {
    build_id: String,
    inspections: BTreeMap<PackageName, PackageInspection>,
    views: BTreeMap<PackageName, ObservedView>,
    running: BTreeSet<PackageName>,
    failures: VecDeque<AndroidFailure>,
    calls: Vec<AndroidCall>,
}

impl Default for MemoryAndroidOps {
    fn default() -> Self {
        Self {
            build_id: "memory".to_owned(),
            inspections: BTreeMap::new(),
            views: BTreeMap::new(),
            running: BTreeSet::new(),
            failures: VecDeque::new(),
            calls: Vec::new(),
        }
    }
}

impl MemoryAndroidOps {
    pub(crate) fn install(&mut self, package: PackageName) {
        let Ok(identity) =
            PackageIdentity::new(10_000, format!("/data/app/{package}/base.apk"), 1, 2)
        else {
            return;
        };
        self.inspections.insert(
            package.clone(),
            PackageInspection {
                identity,
                was_running: true,
            },
        );
        self.views.insert(package.clone(), ObservedView::Base);
        self.running.insert(package);
    }

    pub(crate) fn fail_next(&mut self, failure: AndroidFailure) {
        self.failures.push_back(failure);
    }

    pub(crate) fn set_view(&mut self, package: &PackageName, view: ObservedView) {
        self.views.insert(package.clone(), view);
    }

    pub(crate) fn change_identity(&mut self, package: &PackageName) {
        if let Some(inspection) = self.inspections.get_mut(package) {
            let Ok(identity) =
                PackageIdentity::new(10_000, format!("/data/app/{package}/base.apk"), 1, 99)
            else {
                return;
            };
            inspection.identity = identity;
        }
    }

    pub(crate) fn calls(&self) -> &[AndroidCall] {
        &self.calls
    }

    pub(crate) fn view(&self, package: &PackageName) -> Option<&ObservedView> {
        self.views.get(package)
    }

    pub(crate) fn is_running(&self, package: &PackageName) -> bool {
        self.running.contains(package)
    }

    fn take_failure(&mut self, expected: AndroidFailure) -> bool {
        if self.failures.front() == Some(&expected) {
            self.failures.pop_front();
            true
        } else {
            false
        }
    }

    fn require_installed(&self, package: &PackageName) -> Result<(), AdapterError> {
        self.inspections
            .contains_key(package)
            .then_some(())
            .ok_or_else(|| AdapterError::new("package is not installed"))
    }
}

impl AndroidOps for MemoryAndroidOps {
    fn probe(&mut self) -> Result<Capabilities, AdapterError> {
        Ok(Capabilities {
            build_id: self.build_id.clone(),
        })
    }

    fn inspect(&mut self, package: &PackageName) -> Result<PackageInspection, AdapterError> {
        self.calls.push(AndroidCall::Inspect(package.clone()));
        if self.take_failure(AndroidFailure::Inspect) {
            return Err(AdapterError::new("injected package inspection failure"));
        }
        let mut inspection = self
            .inspections
            .get(package)
            .cloned()
            .ok_or_else(|| AdapterError::new("package is not installed"))?;
        inspection.was_running = self.running.contains(package);
        Ok(inspection)
    }

    fn force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.calls.push(AndroidCall::ForceStop(package.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::ForceStop) {
            return Err(AdapterError::new("injected force-stop failure"));
        }
        self.running.remove(package);
        Ok(())
    }

    fn observe_view(&mut self, package: &PackageName) -> Result<ObservedView, AdapterError> {
        self.calls.push(AndroidCall::Observe(package.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::Observe) {
            return Err(AdapterError::new("injected view observation failure"));
        }
        self.views
            .get(package)
            .cloned()
            .ok_or_else(|| AdapterError::new("package view is unavailable"))
    }

    fn apply_view(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError> {
        self.calls
            .push(AndroidCall::Apply(package.clone(), slot.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::Apply) {
            return Err(AdapterError::new("injected view application failure"));
        }
        self.views
            .insert(package.clone(), ObservedView::for_slot(slot));
        Ok(())
    }

    fn launch_verified(
        &mut self,
        package: &PackageName,
        expected: &SlotId,
    ) -> Result<(), AdapterError> {
        self.calls
            .push(AndroidCall::Launch(package.clone(), expected.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::Launch) {
            return Err(AdapterError::new("injected launch failure"));
        }
        self.views
            .get(package)
            .is_some_and(|view| view.matches(expected))
            .then_some(())
            .ok_or_else(|| AdapterError::new("launched process has the wrong view"))?;
        self.running.insert(package.clone());
        Ok(())
    }
}
