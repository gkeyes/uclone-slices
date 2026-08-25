use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write as _;
#[cfg(not(target_os = "android"))]
use std::os::unix::fs::symlink;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, chown, lchown};
use std::path::{Path, PathBuf};
#[cfg(target_os = "android")]
use std::process::Command;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::{
    AccountIoIntent, AccountIoToken, ArchiveAccountId, PackageAggregate, PackageBinding,
    PackageName, RebindIntent, RestoreBatchResult, RestoreDomain, RestorePolicy, RestoreStaging,
    SeedMode, Slot, SlotId, TransferId,
};
use crate::ports::{AdapterError, PackageStore, SlotStorage};

#[derive(Debug)]
pub(crate) struct FilePackageStore {
    packages_root: PathBuf,
}

impl FilePackageStore {
    pub(crate) fn open(runtime_root: impl Into<PathBuf>) -> Result<Self, AdapterError> {
        let packages_root = runtime_root.into().join("packages");
        fs::create_dir_all(&packages_root).map_err(io_error)?;
        Ok(Self { packages_root })
    }

    fn aggregate_path(&self, package: &PackageName) -> PathBuf {
        self.packages_root
            .join(package.as_str())
            .join("aggregate.json")
    }

    fn binding_path(&self, package: &PackageName) -> PathBuf {
        self.packages_root
            .join(package.as_str())
            .join("binding-v1.json")
    }

    fn rebind_intent_path(&self, package: &PackageName) -> PathBuf {
        self.packages_root
            .join(package.as_str())
            .join("rebind-intent.json")
    }

    fn account_io_intent_path(&self, package: &PackageName) -> PathBuf {
        self.packages_root
            .join(package.as_str())
            .join("account-io-intent-v1.json")
    }

    fn account_io_result_path(&self, package: &PackageName) -> PathBuf {
        self.packages_root
            .join(package.as_str())
            .join("account-io-result-v1.json")
    }

    fn backup_path(&self, package: &PackageName) -> Result<PathBuf, AdapterError> {
        let runtime_root = self
            .packages_root
            .parent()
            .ok_or_else(|| AdapterError::new("packages root has no parent"))?;
        Ok(runtime_root
            .join("state-backups")
            .join("pre-0.1.7")
            .join(package.as_str())
            .join("aggregate.json"))
    }

    fn backup_slot_manifest_path(&self, package: &PackageName) -> Result<PathBuf, AdapterError> {
        Ok(self.backup_path(package)?.with_file_name("slot-pairs.json"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SlotPairBackup {
    ce: Vec<SlotId>,
    de: Vec<SlotId>,
}

impl PackageStore for FilePackageStore {
    fn list(&self) -> Result<Vec<PackageName>, AdapterError> {
        let mut packages = Vec::new();
        for entry in fs::read_dir(&self.packages_root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            if !entry.file_type().map_err(io_error)?.is_dir() {
                continue;
            }
            if !entry.path().join("aggregate.json").is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                return Err(AdapterError::new("package directory is not UTF-8"));
            };
            packages.push(
                PackageName::new(name).map_err(|error| AdapterError::new(error.to_string()))?,
            );
        }
        packages.sort();
        Ok(packages)
    }

    fn load(&self, package: &PackageName) -> Result<Option<PackageAggregate>, AdapterError> {
        let path = self.aggregate_path(package);
        match fs::read(path) {
            Ok(bytes) => {
                let aggregate: PackageAggregate = serde_json::from_slice(&bytes)
                    .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
                aggregate
                    .validate()
                    .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
                (aggregate.package() == package)
                    .then_some(Some(aggregate))
                    .ok_or_else(|| {
                        AdapterError::state_conflict("aggregate package does not match its path")
                    })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(error)),
        }
    }

    fn save(&mut self, aggregate: &PackageAggregate) -> Result<(), AdapterError> {
        aggregate
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        write_json_atomic(&self.aggregate_path(aggregate.package()), aggregate)
    }

    fn load_binding(&self, package: &PackageName) -> Result<Option<PackageBinding>, AdapterError> {
        let binding = read_optional_json::<PackageBinding>(&self.binding_path(package))?;
        if let Some(binding) = &binding {
            binding
                .validate()
                .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        }
        Ok(binding)
    }

    fn save_binding(
        &mut self,
        package: &PackageName,
        binding: &PackageBinding,
    ) -> Result<(), AdapterError> {
        binding
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        write_json_atomic(&self.binding_path(package), binding)
    }

    fn load_rebind_intent(
        &self,
        package: &PackageName,
    ) -> Result<Option<RebindIntent>, AdapterError> {
        let intent = read_optional_json::<RebindIntent>(&self.rebind_intent_path(package))?;
        if let Some(intent) = &intent {
            intent
                .validate()
                .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
            if &intent.package != package {
                return Err(AdapterError::state_conflict(
                    "rebind intent package does not match its path",
                ));
            }
        }
        Ok(intent)
    }

    fn save_rebind_intent(&mut self, intent: &RebindIntent) -> Result<(), AdapterError> {
        intent
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        write_json_atomic(&self.rebind_intent_path(&intent.package), intent)
    }

    fn clear_rebind_intent(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        match fs::remove_file(self.rebind_intent_path(package)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    fn list_account_io_intents(&self) -> Result<Vec<AccountIoIntent>, AdapterError> {
        let mut intents = Vec::new();
        for entry in fs::read_dir(&self.packages_root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            if !entry.file_type().map_err(io_error)?.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                return Err(AdapterError::state_conflict(
                    "package directory is not UTF-8",
                ));
            };
            let package = PackageName::new(name)
                .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
            if let Some(intent) = self.load_account_io_intent(&package)? {
                intents.push(intent);
            }
        }
        intents.sort_by(|left, right| left.package.cmp(&right.package));
        Ok(intents)
    }

    fn load_account_io_intent(
        &self,
        package: &PackageName,
    ) -> Result<Option<AccountIoIntent>, AdapterError> {
        let intent = read_optional_json::<AccountIoIntent>(&self.account_io_intent_path(package))?;
        if let Some(intent) = &intent {
            intent
                .validate()
                .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
            if &intent.package != package {
                return Err(AdapterError::state_conflict(
                    "account I/O intent package does not match its path",
                ));
            }
        }
        Ok(intent)
    }

    fn save_account_io_intent(&mut self, intent: &AccountIoIntent) -> Result<(), AdapterError> {
        intent
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        write_json_atomic(&self.account_io_intent_path(&intent.package), intent)
    }

    fn clear_account_io_intent(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        remove_file_and_sync_parent(&self.account_io_intent_path(package))
    }

    fn save_account_io_result(
        &mut self,
        package: &PackageName,
        result: &RestoreBatchResult,
    ) -> Result<(), AdapterError> {
        if &result.package != package {
            return Err(AdapterError::state_conflict(
                "account I/O result package does not match its path",
            ));
        }
        write_json_atomic(&self.account_io_result_path(package), result)
    }

    fn backup_before_v1_binding(
        &mut self,
        aggregate: &PackageAggregate,
        complete_pairs: &[SlotId],
    ) -> Result<(), AdapterError> {
        let aggregate_path = self.backup_path(aggregate.package())?;
        if !aggregate_path.is_file() {
            write_json_atomic(&aggregate_path, aggregate)?;
        }
        let manifest_path = self.backup_slot_manifest_path(aggregate.package())?;
        if !manifest_path.is_file() {
            let manifest = SlotPairBackup {
                ce: complete_pairs.to_vec(),
                de: complete_pairs.to_vec(),
            };
            write_json_atomic(&manifest_path, &manifest)?;
        }
        Ok(())
    }

    fn remove(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        let package_root = self
            .aggregate_path(package)
            .parent()
            .ok_or_else(|| AdapterError::new("aggregate path has no parent"))?
            .to_path_buf();
        match fs::remove_dir_all(package_root) {
            Ok(()) => {
                File::open(&self.packages_root)
                    .and_then(|directory| directory.sync_all())
                    .map_err(io_error)?;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }
}

fn read_optional_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, AdapterError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| AdapterError::state_conflict(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

fn remove_file_and_sync_parent(path: &Path) -> Result<(), AdapterError> {
    match fs::remove_file(path) {
        Ok(()) => {
            let parent = path
                .parent()
                .ok_or_else(|| AdapterError::new("state path has no parent"))?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(io_error)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

fn write_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), AdapterError> {
    let parent = path
        .parent()
        .ok_or_else(|| AdapterError::new("state path has no parent"))?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let bytes = serde_json::to_vec(value).map_err(|error| AdapterError::new(error.to_string()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AdapterError::new("state path is not UTF-8"))?;
    let temporary = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

#[derive(Debug)]
pub(crate) struct FileSlotStorage {
    ce_slots_root: PathBuf,
    de_slots_root: PathBuf,
    canonical_ce_root: PathBuf,
    canonical_de_root: PathBuf,
    owner: DirectoryOwner,
    #[cfg(test)]
    available_bytes_by_device: std::collections::BTreeMap<u64, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryOwner {
    uid: u32,
    gid: u32,
}

impl DirectoryOwner {
    const fn new(uid: u32, gid: u32) -> Self {
        Self { uid, gid }
    }
}

impl FileSlotStorage {
    pub(crate) fn open(
        ce_slots_root: impl Into<PathBuf>,
        de_slots_root: impl Into<PathBuf>,
        canonical_ce_root: impl Into<PathBuf>,
        canonical_de_root: impl Into<PathBuf>,
    ) -> Result<Self, AdapterError> {
        Self::open_with_owner(
            ce_slots_root,
            de_slots_root,
            canonical_ce_root,
            canonical_de_root,
            DirectoryOwner::new(0, 0),
        )
    }

    fn open_with_owner(
        ce_slots_root: impl Into<PathBuf>,
        de_slots_root: impl Into<PathBuf>,
        canonical_ce_root: impl Into<PathBuf>,
        canonical_de_root: impl Into<PathBuf>,
        owner: DirectoryOwner,
    ) -> Result<Self, AdapterError> {
        let ce_slots_root = ce_slots_root.into();
        let de_slots_root = de_slots_root.into();
        secure_storage_tree(&ce_slots_root, owner)?;
        secure_storage_tree(&de_slots_root, owner)?;
        Ok(Self {
            ce_slots_root,
            de_slots_root,
            canonical_ce_root: canonical_ce_root.into(),
            canonical_de_root: canonical_de_root.into(),
            owner,
            #[cfg(test)]
            available_bytes_by_device: std::collections::BTreeMap::new(),
        })
    }

    #[cfg(test)]
    fn set_available_bytes(&mut self, path: &Path, available: u64) -> Result<(), AdapterError> {
        let device = fs::metadata(path).map_err(io_error)?.dev();
        self.available_bytes_by_device.insert(device, available);
        Ok(())
    }

    fn slot_paths(&self, package: &PackageName, slot: &SlotId) -> (PathBuf, PathBuf) {
        (
            self.ce_slots_root
                .join(package.as_str())
                .join(slot.as_str()),
            self.de_slots_root
                .join(package.as_str())
                .join(slot.as_str()),
        )
    }

    fn storage_roots(&self) -> Result<(PathBuf, PathBuf), AdapterError> {
        Ok((
            self.ce_slots_root
                .parent()
                .ok_or_else(|| AdapterError::new("CE slots root has no storage parent"))?
                .to_path_buf(),
            self.de_slots_root
                .parent()
                .ok_or_else(|| AdapterError::new("DE slots root has no storage parent"))?
                .to_path_buf(),
        ))
    }

    fn transfer_account_paths(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
        account: &ArchiveAccountId,
    ) -> Result<(PathBuf, PathBuf), AdapterError> {
        let (ce_root, de_root) = self.storage_roots()?;
        Ok((
            ce_root
                .join("transfers")
                .join(token.as_str())
                .join(package.as_str())
                .join(account.as_str()),
            de_root
                .join("transfers")
                .join(token.as_str())
                .join(package.as_str())
                .join(account.as_str()),
        ))
    }

    fn maintenance_paths(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(PathBuf, PathBuf), AdapterError> {
        let (ce_root, de_root) = self.storage_roots()?;
        Ok((
            ce_root
                .join("maintenance")
                .join(token.as_str())
                .join(package.as_str()),
            de_root
                .join("maintenance")
                .join(token.as_str())
                .join(package.as_str()),
        ))
    }

    fn base_alias_paths(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(PathBuf, PathBuf), AdapterError> {
        let (ce_root, de_root) = self.storage_roots()?;
        Ok((
            ce_root
                .join("maintenance-base")
                .join(token.as_str())
                .join(package.as_str()),
            de_root
                .join("maintenance-base")
                .join(token.as_str())
                .join(package.as_str()),
        ))
    }

    fn rollback_paths(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(PathBuf, PathBuf), AdapterError> {
        let (ce_root, de_root) = self.storage_roots()?;
        Ok((
            ce_root
                .join("rollback")
                .join(token.as_str())
                .join(package.as_str())
                .join(target.as_str()),
            de_root
                .join("rollback")
                .join(token.as_str())
                .join(package.as_str())
                .join(target.as_str()),
        ))
    }

    #[cfg(target_os = "android")]
    fn replacement_target_paths(
        &self,
        package: &PackageName,
        _token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(PathBuf, PathBuf), AdapterError> {
        if target.is_base() {
            let canonical_ce = self.canonical_ce_root.join(package.as_str());
            let canonical_de = self.canonical_de_root.join(package.as_str());
            let (ce_mounted, de_mounted) = path_mount_presence(&canonical_ce, &canonical_de)?;
            if ce_mounted || de_mounted {
                return Err(AdapterError::state_conflict(
                    "base recovery target still has a managed view mounted",
                ));
            }
            if !real_directory(&canonical_ce)? || !real_directory(&canonical_de)? {
                return Err(AdapterError::state_conflict(
                    "base recovery target is unavailable",
                ));
            }
            Ok((canonical_ce, canonical_de))
        } else {
            Ok(self.slot_paths(package, target))
        }
    }

    #[cfg(not(target_os = "android"))]
    fn replacement_target_paths(
        &self,
        package: &PackageName,
        _token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(PathBuf, PathBuf), AdapterError> {
        if target.is_base() {
            Ok((
                self.canonical_ce_root.join(package.as_str()),
                self.canonical_de_root.join(package.as_str()),
            ))
        } else {
            Ok(self.slot_paths(package, target))
        }
    }

    fn available_bytes(&self, path: &Path) -> Result<(u64, u64), AdapterError> {
        let device = fs::metadata(path).map_err(io_error)?.dev();
        #[cfg(test)]
        if let Some(available) = self.available_bytes_by_device.get(&device) {
            return Ok((device, *available));
        }
        let statistics =
            rustix::fs::statvfs(path).map_err(|error| io_error(std::io::Error::from(error)))?;
        let fragment_size = if statistics.f_frsize == 0 {
            statistics.f_bsize
        } else {
            statistics.f_frsize
        };
        let available = statistics
            .f_bavail
            .checked_mul(fragment_size)
            .ok_or_else(|| AdapterError::state_conflict("available storage size overflow"))?;
        Ok((device, available))
    }

    fn ensure_base_restore_capacity(
        &self,
        staged_ce: &Path,
        staged_de: &Path,
        target_ce: &Path,
        target_de: &Path,
        policy: &RestorePolicy,
    ) -> Result<(), AdapterError> {
        let mut requirements = Vec::with_capacity(2);
        for (staged, target, domain) in [
            (staged_ce, target_ce, RestoreDomain::Ce),
            (staged_de, target_de, RestoreDomain::De),
        ] {
            let target_device = fs::metadata(target).map_err(io_error)?.dev();
            let (staging_device, available) = self.available_bytes(staged)?;
            if target_device != staging_device {
                return Err(AdapterError::state_conflict(
                    "Base restore staging and target are on different filesystems",
                ));
            }
            // This gate runs before any Base mutation. Reserve the staged account and
            // a rollback copy of only the data that the transaction will replace.
            // Preserved resource trees stay on the same filesystem and are moved by
            // rename, so copying their multi-gigabyte contents is neither needed nor
            // included in the capacity requirement.
            let required = copied_tree_bytes_excluding(target, &policy.paths_for(domain))?
                .checked_add(copied_tree_bytes(staged)?)
                .ok_or_else(|| AdapterError::state_conflict("restore size overflow"))?;
            requirements.push((staging_device, required, available));
        }
        ensure_filesystem_capacity(requirements)
    }
}

impl SlotStorage for FileSlotStorage {
    fn materialize(
        &mut self,
        package: &PackageName,
        slot: &Slot,
        seed: SeedMode,
    ) -> Result<(), AdapterError> {
        let ce_package_root = self.ce_slots_root.join(package.as_str());
        let de_package_root = self.de_slots_root.join(package.as_str());
        secure_private_directory(&ce_package_root, self.owner)?;
        secure_private_directory(&de_package_root, self.owner)?;
        let (target_ce, target_de) = self.slot_paths(package, slot.id());
        if target_ce.exists() || target_de.exists() {
            return Err(AdapterError::new("slot already exists"));
        }
        let temporary_ce = ce_package_root.join(format!(
            ".{}.tmp-{}",
            slot.id().as_str(),
            std::process::id()
        ));
        let temporary_de = de_package_root.join(format!(
            ".{}.tmp-{}",
            slot.id().as_str(),
            std::process::id()
        ));
        remove_tree_if_exists(&temporary_ce)?;
        remove_tree_if_exists(&temporary_de)?;
        let result = (|| {
            fs::create_dir(&temporary_ce).map_err(io_error)?;
            fs::create_dir(&temporary_de).map_err(io_error)?;
            populate_domain(
                &self.canonical_ce_root.join(package.as_str()),
                &temporary_ce,
                seed,
            )?;
            populate_domain(
                &self.canonical_de_root.join(package.as_str()),
                &temporary_de,
                seed,
            )?;
            fs::rename(&temporary_ce, &target_ce).map_err(io_error)?;
            fs::rename(&temporary_de, &target_de).map_err(io_error)?;
            Ok(())
        })();
        if result.is_err() {
            let _ce_temporary = remove_tree_if_exists(&temporary_ce);
            let _de_temporary = remove_tree_if_exists(&temporary_de);
            let _ce_target = remove_tree_if_exists(&target_ce);
            let _de_target = remove_tree_if_exists(&target_de);
        }
        result
    }

    fn discard(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError> {
        let ce_result = remove_slot_artifacts(&self.ce_slots_root, package, slot);
        let de_result = remove_slot_artifacts(&self.de_slots_root, package, slot);
        ce_result.and(de_result)
    }

    fn discard_package(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        let ce_result = remove_tree_if_exists(&self.ce_slots_root.join(package.as_str()));
        let de_result = remove_tree_if_exists(&self.de_slots_root.join(package.as_str()));
        ce_result.and(de_result)
    }

    fn require_complete_pair(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(), AdapterError> {
        let (ce, de) = self.slot_paths(package, slot);
        (ce.is_dir() && de.is_dir())
            .then_some(())
            .ok_or_else(|| AdapterError::new("slot CE/DE pair is incomplete"))
    }

    fn account_paths(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(String, String), AdapterError> {
        let (ce, de) = if slot.is_base() {
            (
                self.canonical_ce_root.join(package.as_str()),
                self.canonical_de_root.join(package.as_str()),
            )
        } else {
            self.require_complete_pair(package, slot)?;
            self.slot_paths(package, slot)
        };
        if !real_directory(&ce)? || !real_directory(&de)? {
            return Err(AdapterError::new("account CE/DE pair is unavailable"));
        }
        Ok((path_text_owned(&ce)?, path_text_owned(&de)?))
    }

    fn prepare_restore_staging(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        accounts: &[ArchiveAccountId],
    ) -> Result<Vec<RestoreStaging>, AdapterError> {
        if accounts.is_empty() {
            return Err(AdapterError::state_conflict(
                "restore requires at least one account",
            ));
        }
        let mut staging = Vec::with_capacity(accounts.len());
        for account in accounts {
            let (ce, de) = self.transfer_account_paths(package, token, account)?;
            for path in [&ce, &de] {
                if path.exists() {
                    return Err(AdapterError::state_conflict(
                        "restore staging path already exists",
                    ));
                }
                let parent = path
                    .parent()
                    .ok_or_else(|| AdapterError::new("staging path has no parent"))?;
                fs::create_dir_all(parent).map_err(io_error)?;
                secure_private_directory(parent, self.owner)?;
                fs::create_dir(path).map_err(io_error)?;
                secure_private_directory(path, self.owner)?;
                fs::write(parent.join("transfer-id"), transfer_id.as_str()).map_err(io_error)?;
                sync_directory(parent)?;
            }
            staging.push(RestoreStaging {
                archive_account_id: account.clone(),
                ce_path: path_text_owned(&ce)?,
                de_path: path_text_owned(&de)?,
            });
        }
        Ok(staging)
    }

    fn validate_staged_account(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        account: &ArchiveAccountId,
        policy: &RestorePolicy,
    ) -> Result<(), AdapterError> {
        let (ce, de) = self.transfer_account_paths(package, token, account)?;
        for path in [&ce, &de] {
            if !real_directory(path)? {
                return Err(AdapterError::state_conflict(
                    "staged account domain is not a real directory",
                ));
            }
            let parent = path
                .parent()
                .ok_or_else(|| AdapterError::new("staging path has no parent"))?;
            let recorded = fs::read_to_string(parent.join("transfer-id")).map_err(io_error)?;
            if recorded != transfer_id.as_str() {
                return Err(AdapterError::state_conflict(
                    "staged account transfer ID does not match",
                ));
            }
            validate_staged_tree(path)?;
        }
        for (path, domain) in [(&ce, RestoreDomain::Ce), (&de, RestoreDomain::De)] {
            if !resolve_preserve_roots(path, &policy.paths_for(domain))?.is_empty() {
                return Err(AdapterError::state_conflict(
                    "staged account overlaps a preserved resource path",
                ));
            }
        }
        Ok(())
    }

    fn ensure_restore_capacity(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        account: &ArchiveAccountId,
        target: &SlotId,
        policy: &RestorePolicy,
    ) -> Result<(), AdapterError> {
        if !target.is_base() {
            return Ok(());
        }
        self.validate_staged_account(package, token, transfer_id, account, policy)?;
        let (staged_ce, staged_de) = self.transfer_account_paths(package, token, account)?;
        let (target_ce, target_de) = self.replacement_target_paths(package, token, target)?;
        self.ensure_base_restore_capacity(&staged_ce, &staged_de, &target_ce, &target_de, policy)
    }

    fn prepare_maintenance(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError> {
        let canonical_ce = self.canonical_ce_root.join(package.as_str());
        let canonical_de = self.canonical_de_root.join(package.as_str());
        if !real_directory(&canonical_ce)? || !real_directory(&canonical_de)? {
            return Err(AdapterError::new("base CE/DE pair is unavailable"));
        }
        let (maintenance_ce, maintenance_de) = self.maintenance_paths(package, token)?;
        for (source, maintenance) in [
            (&canonical_ce, &maintenance_ce),
            (&canonical_de, &maintenance_de),
        ] {
            if maintenance.exists() {
                return Err(AdapterError::state_conflict(
                    "maintenance path already exists",
                ));
            }
            let parent = maintenance
                .parent()
                .ok_or_else(|| AdapterError::new("maintenance path has no parent"))?;
            fs::create_dir_all(parent).map_err(io_error)?;
            secure_private_directory(parent, self.owner)?;
            fs::create_dir(maintenance).map_err(io_error)?;
            apply_root_profile(source, maintenance)?;
        }
        Ok(())
    }

    fn replace_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        account: &ArchiveAccountId,
        target: &SlotId,
        policy: &RestorePolicy,
    ) -> Result<(), AdapterError> {
        self.validate_staged_account(package, token, transfer_id, account, policy)?;
        let (staged_ce, staged_de) = self.transfer_account_paths(package, token, account)?;
        let (target_ce, target_de) = self.replacement_target_paths(package, token, target)?;
        if target.is_base() {
            self.ensure_base_restore_capacity(
                &staged_ce, &staged_de, &target_ce, &target_de, policy,
            )?;
        }
        normalize_restored_tree(&self.canonical_ce_root.join(package.as_str()), &staged_ce)?;
        normalize_restored_tree(&self.canonical_de_root.join(package.as_str()), &staged_de)?;
        let (rollback_ce, rollback_de) = self.rollback_paths(package, token, target)?;
        let ce_result = replace_domain(
            &staged_ce,
            &target_ce,
            &rollback_ce,
            target.is_base(),
            &policy.paths_for(RestoreDomain::Ce),
        );
        if let Err(error) = ce_result {
            let _rollback = rollback_domain(&target_ce, &rollback_ce, target.is_base());
            return Err(error);
        }
        let de_result = replace_domain(
            &staged_de,
            &target_de,
            &rollback_de,
            target.is_base(),
            &policy.paths_for(RestoreDomain::De),
        );
        if let Err(error) = de_result {
            let de_rollback = rollback_domain(&target_de, &rollback_de, target.is_base());
            let ce_rollback = rollback_domain(&target_ce, &rollback_ce, target.is_base());
            if de_rollback.is_err() || ce_rollback.is_err() {
                return Err(AdapterError::state_conflict(format!(
                    "DE replacement failed and paired rollback was incomplete: {error}"
                )));
            }
            return Err(error);
        }
        Ok(())
    }

    fn rollback_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        target: &SlotId,
        _policy: &RestorePolicy,
    ) -> Result<(), AdapterError> {
        let (rollback_ce, rollback_de) = self.rollback_paths(package, token, target)?;
        if !rollback_ce.exists() && !rollback_de.exists() {
            return Ok(());
        }
        let (target_ce, target_de) = self.replacement_target_paths(package, token, target)?;
        let ce = rollback_domain(&target_ce, &rollback_ce, target.is_base());
        let de = rollback_domain(&target_de, &rollback_de, target.is_base());
        ce.and(de)
    }

    fn finalize_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(), AdapterError> {
        let (ce, de) = self.rollback_paths(package, token, target)?;
        remove_tree_if_exists(&ce)?;
        remove_tree_if_exists(&de)?;
        Ok(())
    }

    fn cleanup_account_io(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError> {
        let (alias_ce, alias_de) = self.base_alias_paths(package, token)?;
        unmount_base_aliases(&alias_ce, &alias_de)?;
        let (ce_root, de_root) = self.storage_roots()?;
        for root in [&ce_root, &de_root] {
            for category in ["transfers", "maintenance", "maintenance-base", "rollback"] {
                remove_tree_if_exists(
                    &root
                        .join(category)
                        .join(token.as_str())
                        .join(package.as_str()),
                )?;
            }
        }
        Ok(())
    }
}

fn secure_storage_tree(slots_root: &Path, owner: DirectoryOwner) -> Result<(), AdapterError> {
    let storage_root = slots_root
        .parent()
        .ok_or_else(|| AdapterError::new("slots root has no storage parent"))?;
    secure_private_directory(storage_root, owner)?;
    secure_private_directory(slots_root, owner)?;
    for entry in fs::read_dir(slots_root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        secure_private_directory(&entry.path(), owner)?;
    }
    Ok(())
}

fn secure_private_directory(path: &Path, owner: DirectoryOwner) -> Result<(), AdapterError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(io_error)?;
            fs::symlink_metadata(path).map_err(io_error)?
        }
        Err(error) => return Err(io_error(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(AdapterError::state_conflict(format!(
            "private storage ancestor is not a real directory: {}",
            path.display()
        )));
    }
    if metadata.uid() != owner.uid || metadata.gid() != owner.gid {
        chown(path, Some(owner.uid), Some(owner.gid)).map_err(io_error)?;
    }
    if metadata.mode() & 0o7777 != 0o700 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
    }
    let secured = fs::symlink_metadata(path).map_err(io_error)?;
    verify_private_directory(&secured, owner)
}

fn verify_private_directory(
    metadata: &fs::Metadata,
    owner: DirectoryOwner,
) -> Result<(), AdapterError> {
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(AdapterError::state_conflict(
            "private storage ancestor is not a real directory",
        ));
    }
    if metadata.uid() != owner.uid || metadata.gid() != owner.gid {
        return Err(AdapterError::state_conflict(
            "private storage ancestor has the wrong owner",
        ));
    }
    if metadata.mode() & 0o7777 != 0o700 {
        return Err(AdapterError::state_conflict(
            "private storage ancestor has the wrong mode",
        ));
    }
    Ok(())
}

fn real_directory(path: &Path) -> Result<bool, AdapterError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_dir() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn path_text_owned(path: &Path) -> Result<String, AdapterError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AdapterError::new("path is not UTF-8"))
}

fn sync_directory(path: &Path) -> Result<(), AdapterError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

fn validate_staged_tree(root: &Path) -> Result<(), AdapterError> {
    const MAX_ENTRIES: usize = 1_000_000;
    const MAX_PATH_BYTES: usize = 4096;
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            entries = entries.saturating_add(1);
            if entries > MAX_ENTRIES {
                return Err(AdapterError::state_conflict(
                    "staged account contains too many entries",
                ));
            }
            let path = entry.path();
            if path.as_os_str().as_encoded_bytes().len() > MAX_PATH_BYTES {
                return Err(AdapterError::state_conflict(
                    "staged account path is too long",
                ));
            }
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            let kind = metadata.file_type();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                if metadata.nlink() != 1 {
                    return Err(AdapterError::state_conflict(
                        "staged account contains a hard link",
                    ));
                }
                if metadata.len() != 0 && metadata.blocks().saturating_mul(512) < metadata.len() {
                    return Err(AdapterError::state_conflict(
                        "staged account contains a sparse file",
                    ));
                }
            } else if kind.is_symlink() {
                let target = fs::read_link(&path).map_err(io_error)?;
                if target.is_absolute() || !relative_link_stays_inside(root, &path, &target) {
                    return Err(AdapterError::state_conflict(
                        "staged account contains an escaping symbolic link",
                    ));
                }
            } else {
                return Err(AdapterError::state_conflict(
                    "staged account contains a special file",
                ));
            }
        }
    }
    Ok(())
}

fn normalize_restored_tree(profile_source: &Path, root: &Path) -> Result<(), AdapterError> {
    let profile = fs::metadata(profile_source).map_err(io_error)?;
    let uid = profile.uid();
    let gid = profile.gid();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() {
            if metadata.uid() != uid || metadata.gid() != gid {
                lchown(&path, Some(uid), Some(gid)).map_err(io_error)?;
            }
            continue;
        }
        if metadata.uid() != uid || metadata.gid() != gid {
            chown(&path, Some(uid), Some(gid)).map_err(io_error)?;
        }
        let safe_mode = metadata.mode() & 0o1777;
        if metadata.mode() & 0o7777 != safe_mode {
            fs::set_permissions(&path, fs::Permissions::from_mode(safe_mode)).map_err(io_error)?;
        }
        if metadata.file_type().is_dir() {
            for entry in fs::read_dir(&path).map_err(io_error)? {
                pending.push(entry.map_err(io_error)?.path());
            }
        }
    }
    apply_android_context(profile_source, root)
}

fn relative_link_stays_inside(root: &Path, link: &Path, target: &Path) -> bool {
    use std::path::Component;

    let Some(parent) = link.parent() else {
        return false;
    };
    let Ok(relative_parent) = parent.strip_prefix(root) else {
        return false;
    };
    let mut depth = 0_usize;
    for component in relative_parent.components().chain(target.components()) {
        match component {
            Component::Normal(_) => depth = depth.saturating_add(1),
            Component::CurDir => {}
            Component::ParentDir => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            Component::Prefix(_) | Component::RootDir => return false,
        }
    }
    true
}

fn replace_domain(
    staged: &Path,
    target: &Path,
    rollback: &Path,
    keep_target_root: bool,
    preserve_patterns: &[&str],
) -> Result<(), AdapterError> {
    if rollback.exists() {
        return Err(AdapterError::state_conflict(
            "restore rollback directory already exists",
        ));
    }
    let rollback_parent = rollback
        .parent()
        .ok_or_else(|| AdapterError::new("rollback path has no parent"))?;
    fs::create_dir_all(rollback_parent).map_err(io_error)?;
    fs::create_dir(rollback).map_err(io_error)?;
    let preserved = resolve_preserve_roots(target, preserve_patterns)?;
    if !preserved.is_empty() {
        write_json_atomic(&rollback.join("preserve-plan.json"), &preserved)?;
        if !keep_target_root {
            fs::create_dir(rollback.join("preserved")).map_err(io_error)?;
            move_preserved_to_holding(target, rollback, &preserved)?;
        }
    }
    let old = rollback.join("old");
    let missing = rollback.join("old-missing");
    if keep_target_root {
        if !real_directory(target)? {
            return Err(AdapterError::state_conflict(
                "base replacement target is unavailable",
            ));
        }
        fs::create_dir(&old).map_err(io_error)?;
        if preserved.is_empty() {
            copy_tree(target, &old)?;
        } else {
            copy_tree_excluding(target, &old, &preserved)?;
        }
        sync_tree(&old)?;
        fs::write(rollback.join("backup-complete"), b"").map_err(io_error)?;
        sync_directory(rollback)?;
        if preserved.is_empty() {
            remove_directory_contents(target)?;
        } else {
            remove_directory_contents_excluding(target, &preserved)?;
        }
        copy_tree(staged, target)?;
        if preserved.is_empty() {
            sync_tree(target)?;
        } else {
            sync_tree_excluding(target, &preserved)?;
        }
        remove_tree_if_exists(staged)?;
        sync_directory(target)?;
    } else {
        let target_parent = target
            .parent()
            .ok_or_else(|| AdapterError::new("replacement target has no parent"))?;
        fs::create_dir_all(target_parent).map_err(io_error)?;
        if target.exists() {
            rename_path(target, &old).map_err(io_error)?;
        } else {
            fs::write(&missing, b"").map_err(io_error)?;
        }
        rename_path(staged, target).map_err(io_error)?;
        sync_directory(target_parent)?;
    }
    if !keep_target_root {
        restore_preserved_from_holding(target, rollback, &preserved)?;
    }
    sync_directory(rollback)?;
    sync_directory(rollback_parent)
}

fn copied_tree_bytes(root: &Path) -> Result<u64, AdapterError> {
    let metadata = fs::symlink_metadata(root).map_err(io_error)?;
    let mut bytes = metadata
        .blocks()
        .checked_mul(512)
        .ok_or_else(|| AdapterError::state_conflict("restore size overflow"))?
        .max(metadata.len())
        .max(metadata.blksize());
    if metadata.file_type().is_dir() {
        for entry in fs::read_dir(root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            bytes = bytes
                .checked_add(copied_tree_bytes(&entry.path())?)
                .ok_or_else(|| AdapterError::state_conflict("restore size overflow"))?;
        }
    }
    Ok(bytes)
}

fn copied_tree_bytes_excluding(root: &Path, patterns: &[&str]) -> Result<u64, AdapterError> {
    let excluded = resolve_preserve_roots(root, patterns)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    copied_tree_bytes_filtered(root, root, &excluded)
}

fn copied_tree_bytes_filtered(
    root: &Path,
    path: &Path,
    excluded: &BTreeSet<PathBuf>,
) -> Result<u64, AdapterError> {
    if path != root {
        let relative = path
            .strip_prefix(root)
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        if excluded.contains(relative) {
            return Ok(0);
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    let mut bytes = metadata
        .blocks()
        .checked_mul(512)
        .ok_or_else(|| AdapterError::state_conflict("restore size overflow"))?
        .max(metadata.len())
        .max(metadata.blksize());
    if metadata.file_type().is_dir() {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            bytes = bytes
                .checked_add(copied_tree_bytes_filtered(root, &entry.path(), excluded)?)
                .ok_or_else(|| AdapterError::state_conflict("restore size overflow"))?;
        }
    }
    Ok(bytes)
}

fn ensure_filesystem_capacity(
    requirements: impl IntoIterator<Item = (u64, u64, u64)>,
) -> Result<(), AdapterError> {
    let mut filesystems = BTreeMap::<u64, (u64, u64)>::new();
    for (device, required, available) in requirements {
        let entry = filesystems.entry(device).or_insert((0, available));
        entry.0 = entry
            .0
            .checked_add(required)
            .ok_or_else(|| AdapterError::state_conflict("restore size overflow"))?;
        entry.1 = entry.1.min(available);
    }
    if filesystems
        .values()
        .any(|(required, available)| required > available)
    {
        return Err(AdapterError::insufficient_storage(
            "insufficient storage for Base rollback and replacement copies",
        ));
    }
    Ok(())
}

fn rollback_domain(
    target: &Path,
    rollback: &Path,
    keep_target_root: bool,
) -> Result<(), AdapterError> {
    if !rollback.exists() {
        return Ok(());
    }
    let preserved = read_optional_json::<Vec<PathBuf>>(&rollback.join("preserve-plan.json"))?
        .unwrap_or_default();
    let old = rollback.join("old");
    let old_missing = rollback.join("old-missing").is_file();
    let restore_started = rollback.join("restore-started");
    let old_available = real_directory(&old)?;
    if keep_target_root {
        if !real_directory(target)? {
            return Err(AdapterError::state_conflict(
                "base rollback target is unavailable",
            ));
        }
        if !rollback.join("backup-complete").is_file() {
            remove_tree_if_exists(rollback)?;
            let parent = rollback
                .parent()
                .ok_or_else(|| AdapterError::new("rollback path has no parent"))?;
            return sync_directory(parent);
        }
        if !restore_started.is_file() && !old_available {
            return Err(AdapterError::state_conflict(
                "base rollback has no complete prior-state copy",
            ));
        }
        if !restore_started.is_file() {
            if preserved.is_empty() {
                remove_directory_contents(target)?;
            } else {
                remove_directory_contents_excluding(target, &preserved)?;
            }
            fs::write(&restore_started, b"").map_err(io_error)?;
            sync_directory(rollback)?;
        }
        if old_available {
            copy_tree(&old, target)?;
            if preserved.is_empty() {
                sync_tree(target)?;
            } else {
                sync_tree_excluding(target, &preserved)?;
            }
        }
        sync_directory(target)?;
        return retire_completed_rollback(rollback);
    } else {
        if old_available {
            if !restore_started.is_file() {
                move_preserved_to_holding(target, rollback, &preserved)?;
                remove_tree_if_exists(target)?;
                if let Some(parent) = target.parent() {
                    sync_directory(parent)?;
                }
                fs::write(&restore_started, b"").map_err(io_error)?;
                sync_directory(rollback)?;
            }
            rename_path(&old, target).map_err(io_error)?;
        } else if old_missing {
            remove_tree_if_exists(target)?;
        } else if restore_started.is_file() && real_directory(target)? {
            // The old tree was already atomically renamed before interruption.
        } else {
            restore_preserved_from_holding(target, rollback, &preserved)?;
            remove_tree_if_exists(rollback)?;
            let parent = rollback
                .parent()
                .ok_or_else(|| AdapterError::new("rollback path has no parent"))?;
            return sync_directory(parent);
        }
        restore_preserved_from_holding(target, rollback, &preserved)?;
        if let Some(parent) = target.parent() {
            sync_directory(parent)?;
        }
    }
    remove_tree_if_exists(rollback)?;
    let parent = rollback
        .parent()
        .ok_or_else(|| AdapterError::new("rollback path has no parent"))?;
    sync_directory(parent)
}

fn resolve_preserve_roots(root: &Path, patterns: &[&str]) -> Result<Vec<PathBuf>, AdapterError> {
    if patterns.is_empty() || !root.exists() {
        return Ok(Vec::new());
    }
    if !real_directory(root)? {
        return Err(AdapterError::state_conflict(
            "preserve source is not a real directory",
        ));
    }
    let mut resolved = BTreeSet::new();
    for pattern in patterns {
        let segments = pattern.split('/').collect::<Vec<_>>();
        resolve_preserve_pattern(root, Path::new(""), &segments, 0, &mut resolved)?;
    }
    let resolved = resolved.into_iter().collect::<Vec<_>>();
    for (index, left) in resolved.iter().enumerate() {
        if resolved
            .iter()
            .skip(index + 1)
            .any(|right| left.starts_with(right) || right.starts_with(left))
        {
            return Err(AdapterError::state_conflict(
                "preserved resource paths overlap",
            ));
        }
    }
    Ok(resolved)
}

fn resolve_preserve_pattern(
    root: &Path,
    relative: &Path,
    segments: &[&str],
    index: usize,
    resolved: &mut BTreeSet<PathBuf>,
) -> Result<(), AdapterError> {
    if index == segments.len() {
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(AdapterError::state_conflict(
                "preserved resource root is not a real directory",
            ));
        }
        resolved.insert(relative.to_path_buf());
        return Ok(());
    }
    let current = root.join(relative);
    let segment = segments[index];
    if segment == "*" {
        let mut children = fs::read_dir(&current)
            .map_err(io_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io_error)?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let metadata = fs::symlink_metadata(child.path()).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err(AdapterError::state_conflict(
                    "preserved resource wildcard crosses a symbolic link",
                ));
            }
            if metadata.file_type().is_dir() {
                resolve_preserve_pattern(
                    root,
                    &relative.join(child.file_name()),
                    segments,
                    index + 1,
                    resolved,
                )?;
            }
        }
        return Ok(());
    }
    let next = relative.join(segment);
    match fs::symlink_metadata(root.join(&next)) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(AdapterError::state_conflict(
                    "preserved resource path crosses a symbolic link",
                ));
            }
            if !metadata.file_type().is_dir() {
                return Err(AdapterError::state_conflict(
                    "preserved resource path is not a directory",
                ));
            }
            resolve_preserve_pattern(root, &next, segments, index + 1, resolved)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

fn move_preserved_to_holding(
    target: &Path,
    rollback: &Path,
    paths: &[PathBuf],
) -> Result<(), AdapterError> {
    let holding = rollback.join("preserved");
    for relative in paths {
        let source = target.join(relative);
        let destination = holding.join(relative);
        let source_exists = real_directory(&source)?;
        let destination_exists = real_directory(&destination)?;
        match (source_exists, destination_exists) {
            (true, false) => {
                let parent = destination
                    .parent()
                    .ok_or_else(|| AdapterError::new("preserved path has no parent"))?;
                fs::create_dir_all(parent).map_err(io_error)?;
                rename_path(&source, &destination).map_err(io_error)?;
                if let Some(parent) = source.parent() {
                    sync_directory(parent)?;
                }
                sync_directory(parent)?;
            }
            (false, true) => {}
            (true, true) => {
                return Err(AdapterError::state_conflict(
                    "preserved resource exists in target and rollback",
                ));
            }
            (false, false) => {
                return Err(AdapterError::state_conflict(
                    "preserved resource disappeared during restore",
                ));
            }
        }
    }
    Ok(())
}

fn restore_preserved_from_holding(
    target: &Path,
    rollback: &Path,
    paths: &[PathBuf],
) -> Result<(), AdapterError> {
    let holding = rollback.join("preserved");
    for relative in paths {
        let source = holding.join(relative);
        let destination = target.join(relative);
        let source_exists = real_directory(&source)?;
        let destination_exists = real_directory(&destination)?;
        match (source_exists, destination_exists) {
            (true, false) => {
                let parent = destination
                    .parent()
                    .ok_or_else(|| AdapterError::new("restored resource has no parent"))?;
                fs::create_dir_all(parent).map_err(io_error)?;
                rename_path(&source, &destination).map_err(io_error)?;
                if let Some(parent) = source.parent() {
                    sync_directory(parent)?;
                }
                sync_directory(parent)?;
            }
            (false, true) => {}
            (true, true) => {
                return Err(AdapterError::state_conflict(
                    "restored account overlaps a preserved resource",
                ));
            }
            (false, false) => {
                return Err(AdapterError::state_conflict(
                    "preserved resource is unavailable",
                ));
            }
        }
    }
    Ok(())
}

fn retire_completed_rollback(rollback: &Path) -> Result<(), AdapterError> {
    let parent = rollback
        .parent()
        .ok_or_else(|| AdapterError::new("rollback path has no parent"))?;
    let name = rollback
        .file_name()
        .ok_or_else(|| AdapterError::new("rollback path has no file name"))?;
    let retired = parent.join(format!(".{}.restored", name.to_string_lossy()));
    if retired.exists() {
        return Err(AdapterError::state_conflict(
            "retired rollback directory already exists",
        ));
    }
    fs::rename(rollback, &retired).map_err(io_error)?;
    sync_directory(parent)?;
    remove_tree_if_exists(&retired)?;
    sync_directory(parent)
}

#[cfg(not(test))]
fn rename_path(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

#[cfg(test)]
thread_local! {
    static TEST_RENAME_FAILURE: std::cell::RefCell<Option<TestRenameFailure>> =
        const { std::cell::RefCell::new(None) };
    static TEST_COPY_FAILURE_PATHS: std::cell::RefCell<Option<(PathBuf, PathBuf)>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
#[derive(Clone)]
struct TestRenameFailure {
    target_prefixes: Vec<PathBuf>,
    triggered: std::rc::Rc<std::cell::Cell<bool>>,
}

#[cfg(test)]
struct TestRenameFailureGuard {
    previous: Option<TestRenameFailure>,
    triggered: std::rc::Rc<std::cell::Cell<bool>>,
}

#[cfg(test)]
impl TestRenameFailureGuard {
    fn was_triggered(&self) -> bool {
        self.triggered.get()
    }
}

#[cfg(test)]
impl Drop for TestRenameFailureGuard {
    fn drop(&mut self) {
        TEST_RENAME_FAILURE.with(|failure| {
            *failure.borrow_mut() = self.previous.take();
        });
    }
}

#[cfg(test)]
fn fail_next_rename_into(target_prefixes: &[&Path]) -> TestRenameFailureGuard {
    let triggered = std::rc::Rc::new(std::cell::Cell::new(false));
    let failure = TestRenameFailure {
        target_prefixes: target_prefixes
            .iter()
            .map(|path| path.to_path_buf())
            .collect(),
        triggered: triggered.clone(),
    };
    let previous = TEST_RENAME_FAILURE.with(|configured| configured.borrow_mut().replace(failure));
    TestRenameFailureGuard {
        previous,
        triggered,
    }
}

#[cfg(test)]
struct TestCopyFailureGuard {
    previous: Option<(PathBuf, PathBuf)>,
}

#[cfg(test)]
impl Drop for TestCopyFailureGuard {
    fn drop(&mut self) {
        TEST_COPY_FAILURE_PATHS.with(|paths| {
            *paths.borrow_mut() = self.previous.take();
        });
    }
}

#[cfg(test)]
fn fail_next_copy(source: &Path, target: &Path) -> TestCopyFailureGuard {
    let previous = TEST_COPY_FAILURE_PATHS.with(|configured| {
        configured
            .borrow_mut()
            .replace((source.to_path_buf(), target.to_path_buf()))
    });
    TestCopyFailureGuard { previous }
}

#[cfg(test)]
fn take_injected_copy_failure(source: &Path, target: &Path) -> bool {
    TEST_COPY_FAILURE_PATHS.with(|configured| {
        let mut configured = configured.borrow_mut();
        configured
            .as_ref()
            .is_some_and(|(expected_source, expected_target)| {
                expected_source == source && expected_target == target
            })
            .then(|| configured.take())
            .flatten()
            .is_some()
    })
}

#[cfg(test)]
fn rename_path(source: &Path, target: &Path) -> std::io::Result<()> {
    let should_fail = TEST_RENAME_FAILURE.with(|configured| {
        let mut configured = configured.borrow_mut();
        let matches = configured.as_ref().is_some_and(|failure| {
            failure
                .target_prefixes
                .iter()
                .any(|prefix| target.starts_with(prefix))
        });
        if matches && let Some(failure) = configured.take() {
            failure.triggered.set(true);
        }
        matches
    });
    if should_fail {
        return Err(std::io::Error::new(
            std::io::ErrorKind::CrossesDevices,
            "Cross-device link (os error 18)",
        ));
    }
    fs::rename(source, target)
}

fn sync_tree(root: &Path) -> Result<(), AdapterError> {
    let mut directories = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        directories.push(directory.clone());
        for entry in fs::read_dir(&directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                pending.push(entry.path());
            } else if metadata.file_type().is_file() {
                File::open(entry.path())
                    .and_then(|file| file.sync_all())
                    .map_err(io_error)?;
            }
        }
    }
    for directory in directories.into_iter().rev() {
        sync_directory(&directory)?;
    }
    Ok(())
}

fn sync_tree_excluding(root: &Path, excluded: &[PathBuf]) -> Result<(), AdapterError> {
    let excluded = excluded.iter().cloned().collect::<BTreeSet<_>>();
    let mut directories = Vec::new();
    let mut pending = vec![(root.to_path_buf(), PathBuf::new())];
    while let Some((directory, relative)) = pending.pop() {
        directories.push(directory.clone());
        for entry in fs::read_dir(&directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let child_relative = relative.join(entry.file_name());
            if excluded.contains(&child_relative) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                pending.push((entry.path(), child_relative));
            } else if metadata.file_type().is_file() {
                File::open(entry.path())
                    .and_then(|file| file.sync_all())
                    .map_err(io_error)?;
            }
        }
    }
    for directory in directories.into_iter().rev() {
        sync_directory(&directory)?;
    }
    Ok(())
}

fn remove_directory_contents(path: &Path) -> Result<(), AdapterError> {
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(entry.path()).map_err(io_error)?;
        } else {
            fs::remove_file(entry.path()).map_err(io_error)?;
        }
    }
    sync_directory(path)
}

fn remove_directory_contents_excluding(
    root: &Path,
    excluded: &[PathBuf],
) -> Result<(), AdapterError> {
    remove_directory_contents_excluding_at(root, Path::new(""), excluded)
}

fn remove_directory_contents_excluding_at(
    directory: &Path,
    relative: &Path,
    excluded: &[PathBuf],
) -> Result<(), AdapterError> {
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let child_relative = relative.join(entry.file_name());
        if excluded.iter().any(|path| path == &child_relative) {
            continue;
        }
        let contains_preserved = excluded
            .iter()
            .any(|path| path.starts_with(&child_relative));
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if contains_preserved {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                return Err(AdapterError::state_conflict(
                    "preserved resource ancestor is not a real directory",
                ));
            }
            remove_directory_contents_excluding_at(&entry.path(), &child_relative, excluded)?;
        } else if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(entry.path()).map_err(io_error)?;
        } else {
            fs::remove_file(entry.path()).map_err(io_error)?;
        }
    }
    sync_directory(directory)
}

#[cfg(target_os = "android")]
fn unmount_base_aliases(alias_ce: &Path, alias_de: &Path) -> Result<(), AdapterError> {
    let mut first_error = None;
    for target in [alias_ce, alias_de] {
        if let Err(error) = unmount_all_path_layers(target)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    let (ce_remaining, de_remaining) = path_mount_presence(alias_ce, alias_de)?;
    if ce_remaining || de_remaining {
        Err(first_error.unwrap_or_else(|| {
            AdapterError::state_conflict("Base maintenance alias remained mounted")
        }))
    } else {
        first_error.map_or(Ok(()), Err)
    }
}

#[cfg(target_os = "android")]
fn unmount_all_path_layers(target: &Path) -> Result<(), AdapterError> {
    drain_mount_layers(target, path_mount_layer_count, |path| {
        let status = Command::new("/system/bin/umount")
            .arg(path)
            .status()
            .map_err(io_error)?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| AdapterError::new("could not unmount the Base maintenance alias"))
    })
}

#[cfg(any(target_os = "android", test))]
fn drain_mount_layers(
    target: &Path,
    mut layer_count: impl FnMut(&Path) -> Result<usize, AdapterError>,
    mut unmount_once: impl FnMut(&Path) -> Result<(), AdapterError>,
) -> Result<(), AdapterError> {
    let mut remaining = layer_count(target)?;
    while remaining > 0 {
        unmount_once(target)?;
        let next = layer_count(target)?;
        if next >= remaining {
            return Err(AdapterError::state_conflict(
                "Base maintenance alias unmount made no progress",
            ));
        }
        remaining = next;
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn unmount_base_aliases(_alias_ce: &Path, _alias_de: &Path) -> Result<(), AdapterError> {
    Ok(())
}

#[cfg(target_os = "android")]
fn path_mount_presence(first: &Path, second: &Path) -> Result<(bool, bool), AdapterError> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").map_err(io_error)?;
    Ok((
        mountinfo_path_layer_count(&mountinfo, first) > 0,
        mountinfo_path_layer_count(&mountinfo, second) > 0,
    ))
}

#[cfg(any(target_os = "android", test))]
fn mountinfo_path_layer_count(mountinfo: &str, path: &Path) -> usize {
    let Some(expected) = path.to_str() else {
        return 0;
    };
    mountinfo
        .lines()
        .filter(|line| {
            line.split_once(" - ").is_some() && line.split_whitespace().nth(4) == Some(expected)
        })
        .count()
}

#[cfg(target_os = "android")]
fn path_mount_layer_count(path: &Path) -> Result<usize, AdapterError> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").map_err(io_error)?;
    Ok(mountinfo_path_layer_count(&mountinfo, path))
}

fn populate_domain(source: &Path, target: &Path, seed: SeedMode) -> Result<(), AdapterError> {
    if !source.is_dir() {
        return Err(AdapterError::new(format!(
            "base directory is missing: {}",
            source.display()
        )));
    }
    if seed == SeedMode::CloneBase {
        copy_tree(source, target)?;
    }
    apply_root_profile(source, target)
}

fn apply_root_profile(source: &Path, target: &Path) -> Result<(), AdapterError> {
    let source_metadata = fs::metadata(source).map_err(io_error)?;
    let target_metadata = fs::metadata(target).map_err(io_error)?;
    if source_metadata.uid() != target_metadata.uid()
        || source_metadata.gid() != target_metadata.gid()
    {
        chown(
            target,
            Some(source_metadata.uid()),
            Some(source_metadata.gid()),
        )
        .map_err(io_error)?;
    }
    fs::set_permissions(target, source_metadata.permissions()).map_err(io_error)?;
    apply_android_context(source, target)
}

#[cfg(target_os = "android")]
fn apply_android_context(source: &Path, target: &Path) -> Result<(), AdapterError> {
    let output = Command::new("/system/bin/getfattr")
        .args(["--only-values", "-n", "security.selinux"])
        .arg(source)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(AdapterError::new("could not read Android data context"));
    }
    let context = String::from_utf8(output.stdout)
        .map_err(|error| AdapterError::new(error.to_string()))?
        .trim_matches(['\0', '\n', '\r'])
        .to_owned();
    if context.is_empty() {
        return Err(AdapterError::new("Android data context is empty"));
    }
    let status = Command::new("/system/bin/chcon")
        .args(["-R", "--", &context])
        .arg(target)
        .status()
        .map_err(io_error)?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| AdapterError::new("could not apply Android data context"))
}

#[cfg(not(target_os = "android"))]
fn apply_android_context(_source: &Path, _target: &Path) -> Result<(), AdapterError> {
    Ok(())
}

fn copy_tree_excluding(
    source: &Path,
    target: &Path,
    excluded: &[PathBuf],
) -> Result<(), AdapterError> {
    copy_tree_excluding_at(source, target, Path::new(""), excluded)
}

fn copy_tree_excluding_at(
    source: &Path,
    target: &Path,
    relative: &Path,
    excluded: &[PathBuf],
) -> Result<(), AdapterError> {
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let child_relative = relative.join(entry.file_name());
        if excluded.iter().any(|path| path == &child_relative) {
            continue;
        }
        let destination = target.join(entry.file_name());
        let contains_preserved = excluded
            .iter()
            .any(|path| path.starts_with(&child_relative));
        if contains_preserved {
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                return Err(AdapterError::state_conflict(
                    "preserved resource ancestor is not a real directory",
                ));
            }
            match fs::symlink_metadata(&destination) {
                Ok(metadata)
                    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(AdapterError::state_conflict(
                        "copy destination type does not match source directory",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&destination).map_err(io_error)?;
                }
                Err(error) => return Err(io_error(error)),
            }
            copy_tree_excluding_at(&entry.path(), &destination, &child_relative, excluded)?;
            apply_filtered_directory_profile(&entry.path(), &destination)?;
        } else {
            copy_path_preserving(&entry.path(), &destination)?;
        }
    }
    Ok(())
}

fn apply_filtered_directory_profile(source: &Path, target: &Path) -> Result<(), AdapterError> {
    apply_root_profile(source, target)?;
    let source_metadata = fs::metadata(source).map_err(io_error)?;
    filetime::set_file_mtime(
        target,
        filetime::FileTime::from_last_modification_time(&source_metadata),
    )
    .map_err(io_error)
}

#[cfg(target_os = "android")]
fn copy_path_preserving(source: &Path, destination: &Path) -> Result<(), AdapterError> {
    if destination.exists() {
        return Err(AdapterError::state_conflict(
            "filtered copy destination already exists",
        ));
    }
    let status = Command::new("/system/bin/cp")
        .args(["-a", "--"])
        .arg(source)
        .arg(destination)
        .status()
        .map_err(io_error)?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| AdapterError::new("could not copy filtered Android data tree"))
}

#[cfg(not(target_os = "android"))]
fn copy_path_preserving(source: &Path, destination: &Path) -> Result<(), AdapterError> {
    let metadata = fs::symlink_metadata(source).map_err(io_error)?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::create_dir(destination).map_err(io_error)?;
        copy_tree(source, destination)?;
        apply_root_profile(source, destination)
    } else if metadata.file_type().is_file() {
        fs::copy(source, destination).map_err(io_error)?;
        apply_root_profile(source, destination)
    } else if metadata.file_type().is_symlink() {
        symlink(fs::read_link(source).map_err(io_error)?, destination).map_err(io_error)
    } else {
        Err(AdapterError::new("unsupported base artifact"))
    }
}

#[cfg(target_os = "android")]
fn copy_tree(source: &Path, target: &Path) -> Result<(), AdapterError> {
    let status = Command::new("/system/bin/cp")
        .args(["-a", "--"])
        .arg(source.join("."))
        .arg(target)
        .status()
        .map_err(io_error)?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| AdapterError::new("could not copy Android data tree"))
}

#[cfg(not(target_os = "android"))]
fn copy_tree(source: &Path, target: &Path) -> Result<(), AdapterError> {
    #[cfg(test)]
    if take_injected_copy_failure(source, target) {
        return Err(AdapterError::new("injected copy failure"));
    }
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            match fs::symlink_metadata(&destination) {
                Ok(metadata)
                    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(AdapterError::state_conflict(
                        "copy destination type does not match source directory",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&destination).map_err(io_error)?;
                }
                Err(error) => return Err(io_error(error)),
            }
            copy_tree(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            if let Ok(metadata) = fs::symlink_metadata(&destination)
                && !metadata.file_type().is_file()
            {
                return Err(AdapterError::state_conflict(
                    "copy destination type does not match source file",
                ));
            }
            fs::copy(entry.path(), &destination).map_err(io_error)?;
        } else if file_type.is_symlink() {
            let link_target = fs::read_link(entry.path()).map_err(io_error)?;
            match fs::symlink_metadata(&destination) {
                Ok(metadata)
                    if metadata.file_type().is_symlink()
                        && fs::read_link(&destination).map_err(io_error)? == link_target => {}
                Ok(_) => {
                    return Err(AdapterError::state_conflict(
                        "copy destination type does not match source symbolic link",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    symlink(link_target, &destination).map_err(io_error)?;
                }
                Err(error) => return Err(io_error(error)),
            }
            continue;
        } else {
            return Err(AdapterError::new("unsupported base artifact"));
        }
        apply_root_profile(&entry.path(), &destination)?;
    }
    Ok(())
}

fn remove_tree_if_exists(path: &Path) -> Result<(), AdapterError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

fn remove_slot_artifacts(
    slots_root: &Path,
    package: &PackageName,
    slot: &SlotId,
) -> Result<(), AdapterError> {
    let package_root = slots_root.join(package.as_str());
    let mut first_error = remove_tree_if_exists(&package_root.join(slot.as_str())).err();
    let entries = match fs::read_dir(&package_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return first_error.map_or(Ok(()), Err);
        }
        Err(error) => {
            if first_error.is_none() {
                first_error = Some(io_error(error));
            }
            return first_error.map_or(Ok(()), Err);
        }
    };
    let temporary_prefix = format!(".{}.tmp-", slot.as_str());
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(io_error(error));
                }
                continue;
            }
        };
        let matches = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&temporary_prefix));
        let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if matches
            && is_directory
            && let Err(error) = remove_tree_if_exists(&entry.path())
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn io_error(error: std::io::Error) -> AdapterError {
    AdapterError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::model::{DisplayName, PackageAggregate, PackageIdentity, RestorePath};
    use std::os::unix::fs::symlink;

    fn identity() -> PackageIdentity {
        PackageIdentity::new(10_000, "/data/app/example/base.apk", 1, 2).unwrap()
    }

    fn resource_preserve_policy() -> RestorePolicy {
        RestorePolicy::PreservePaths {
            paths: vec![
                RestorePath {
                    domain: RestoreDomain::Ce,
                    path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer".to_owned(),
                },
                RestorePath {
                    domain: RestoreDomain::Ce,
                    path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/*/Paks".to_owned(),
                },
            ],
        }
    }

    fn open_slot_storage(
        ce_slots: &Path,
        de_slots: &Path,
        canonical_ce: &Path,
        canonical_de: &Path,
    ) -> FileSlotStorage {
        let existing_ancestor = ce_slots.ancestors().find(|path| path.exists()).unwrap();
        let metadata = fs::metadata(existing_ancestor).unwrap();
        FileSlotStorage::open_with_owner(
            ce_slots,
            de_slots,
            canonical_ce,
            canonical_de,
            DirectoryOwner::new(metadata.uid(), metadata.gid()),
        )
        .unwrap()
    }

    #[test]
    fn legacy_base_alias_mount_detection_counts_every_stacked_layer() {
        let mountinfo = concat!(
            "41 30 0:35 / /data/misc_ce/0/uclone-slices-v2/maintenance-base/token/pkg rw - ext4 /dev/block/data rw\n",
            "42 41 0:35 /maintenance/token/pkg /data/misc_ce/0/uclone-slices-v2/maintenance-base/token/pkg rw - ext4 /dev/block/data rw\n",
            "43 30 0:35 / /data/misc_de/0/uclone-slices-v2/maintenance-base/token/pkg rw - ext4 /dev/block/data rw\n",
        );
        assert_eq!(
            mountinfo_path_layer_count(
                mountinfo,
                Path::new("/data/misc_ce/0/uclone-slices-v2/maintenance-base/token/pkg"),
            ),
            2
        );
        assert_eq!(
            mountinfo_path_layer_count(
                mountinfo,
                Path::new("/data/misc_ce/0/uclone-slices-v2/maintenance-base/token"),
            ),
            0
        );
        assert_eq!(
            mountinfo_path_layer_count(
                mountinfo,
                Path::new("/data/misc_de/0/uclone-slices-v2/maintenance-base/token/pkg"),
            ),
            1
        );
    }

    #[test]
    fn legacy_base_alias_cleanup_unmounts_every_stacked_layer() {
        let target = Path::new("/legacy-alias");
        let mut layers = std::collections::VecDeque::from([3, 2, 1, 0]);
        let mut unmounts = 0;

        drain_mount_layers(
            target,
            |_path| Ok(layers.pop_front().unwrap()),
            |_path| {
                unmounts += 1;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(unmounts, 3);
        assert!(layers.is_empty());
    }

    #[test]
    fn restore_maintenance_does_not_create_a_base_alias() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();

        storage.prepare_maintenance(&package, &token).unwrap();

        let (maintenance_ce, maintenance_de) = storage.maintenance_paths(&package, &token).unwrap();
        let (alias_ce, alias_de) = storage.base_alias_paths(&package, &token).unwrap();
        assert!(maintenance_ce.is_dir());
        assert!(maintenance_de.is_dir());
        assert!(!alias_ce.exists());
        assert!(!alias_de.exists());
    }

    #[test]
    fn aggregate_round_trips_atomically() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let mut store = FilePackageStore::open(root.path()).unwrap();
        let aggregate = PackageAggregate::enrolled(package.clone(), identity());

        store.save(&aggregate).unwrap();

        assert_eq!(store.load(&package).unwrap(), Some(aggregate));
        assert_eq!(store.list().unwrap(), vec![package]);
    }

    #[test]
    fn pre_v1_binding_backup_records_aggregate_and_both_domain_inventories_once() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let mut store = FilePackageStore::open(root.path()).unwrap();
        let aggregate = PackageAggregate::enrolled(package.clone(), identity());
        let pairs = vec![SlotId::numbered(1), SlotId::numbered(2)];

        store.backup_before_v1_binding(&aggregate, &pairs).unwrap();

        let backup_root = root
            .path()
            .join("state-backups/pre-0.1.7")
            .join(package.as_str());
        let restored: PackageAggregate =
            serde_json::from_slice(&fs::read(backup_root.join("aggregate.json")).unwrap()).unwrap();
        let inventory: SlotPairBackup =
            serde_json::from_slice(&fs::read(backup_root.join("slot-pairs.json")).unwrap())
                .unwrap();
        assert_eq!(restored, aggregate);
        assert_eq!(inventory.ce, pairs);
        assert_eq!(inventory.de, pairs);
    }

    #[test]
    fn aggregate_removal_is_idempotent_and_does_not_touch_other_packages() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let other = PackageName::new("com.example.other").unwrap();
        let mut store = FilePackageStore::open(root.path()).unwrap();
        store
            .save(&PackageAggregate::enrolled(package.clone(), identity()))
            .unwrap();
        store
            .save(&PackageAggregate::enrolled(other.clone(), identity()))
            .unwrap();

        store.remove(&package).unwrap();
        store.remove(&package).unwrap();

        assert!(store.load(&package).unwrap().is_none());
        assert!(store.load(&other).unwrap().is_some());
        assert_eq!(store.list().unwrap(), vec![other]);
    }

    #[test]
    fn package_directory_without_an_aggregate_is_not_listed() {
        let root = tempfile::tempdir().unwrap();
        let store = FilePackageStore::open(root.path()).unwrap();
        fs::create_dir_all(root.path().join("packages/com.example.partial")).unwrap();

        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn base_clone_materializes_both_domains() {
        let root = tempfile::tempdir().unwrap();
        let ce = root.path().join("base-ce");
        let de = root.path().join("base-de");
        let package = PackageName::new("com.example.app").unwrap();
        fs::create_dir_all(ce.join(package.as_str())).unwrap();
        fs::create_dir_all(de.join(package.as_str())).unwrap();
        fs::write(ce.join(package.as_str()).join("ce.txt"), b"ce").unwrap();
        fs::write(de.join(package.as_str()).join("de.txt"), b"de").unwrap();
        let ce_slots = root.path().join("slots-ce");
        let de_slots = root.path().join("slots-de");
        let mut storage = open_slot_storage(&ce_slots, &de_slots, &ce, &de);
        let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Clone").unwrap())
            .unwrap();

        storage
            .materialize(&package, &slot, SeedMode::CloneBase)
            .unwrap();

        let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
        assert_eq!(fs::read(target_ce.join("ce.txt")).unwrap(), b"ce");
        assert_eq!(fs::read(target_de.join("de.txt")).unwrap(), b"de");
        assert_eq!(
            fs::metadata(target_ce.parent().unwrap()).unwrap().mode() & 0o777,
            0o700,
        );
        assert_eq!(
            fs::metadata(target_de.parent().unwrap()).unwrap().mode() & 0o777,
            0o700,
        );
    }

    #[test]
    fn blank_slot_inherits_both_base_root_modes() {
        let root = tempfile::tempdir().unwrap();
        let ce = root.path().join("base-ce");
        let de = root.path().join("base-de");
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce = ce.join(package.as_str());
        let canonical_de = de.join(package.as_str());
        fs::create_dir_all(&canonical_ce).unwrap();
        fs::create_dir_all(&canonical_de).unwrap();
        fs::set_permissions(&canonical_ce, fs::Permissions::from_mode(0o710)).unwrap();
        fs::set_permissions(&canonical_de, fs::Permissions::from_mode(0o750)).unwrap();
        let ce_slots = root.path().join("slots-ce");
        let de_slots = root.path().join("slots-de");
        let mut storage = open_slot_storage(&ce_slots, &de_slots, &ce, &de);
        let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Blank").unwrap())
            .unwrap();

        storage
            .materialize(&package, &slot, SeedMode::Blank)
            .unwrap();

        let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
        assert_eq!(fs::metadata(target_ce).unwrap().mode() & 0o777, 0o710);
        assert_eq!(fs::metadata(target_de).unwrap().mode() & 0o777, 0o750);
    }

    #[test]
    fn failed_pair_materialization_leaves_no_half_slot() {
        let root = tempfile::tempdir().unwrap();
        let ce = root.path().join("base-ce");
        let de = root.path().join("base-de");
        let package = PackageName::new("com.example.app").unwrap();
        fs::create_dir_all(ce.join(package.as_str())).unwrap();
        let ce_slots = root.path().join("slots-ce");
        let de_slots = root.path().join("slots-de");
        let mut storage = open_slot_storage(&ce_slots, &de_slots, &ce, &de);
        let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Clone").unwrap())
            .unwrap();

        let result = storage.materialize(&package, &slot, SeedMode::CloneBase);

        assert!(result.is_err());
        let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
        assert!(!target_ce.exists());
        assert!(!target_de.exists());
    }

    #[test]
    fn discard_removes_published_and_stale_temporary_slot_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let slot = SlotId::numbered(1);
        let ce_slots = root.path().join("slots-ce");
        let de_slots = root.path().join("slots-de");
        let ce_package = ce_slots.join(package.as_str());
        let de_package = de_slots.join(package.as_str());
        let published_ce = ce_package.join(slot.as_str());
        let stale_ce = ce_package.join(".slot-1.tmp-111");
        let stale_de = de_package.join(".slot-1.tmp-222");
        let unrelated = de_package.join(".slot-10.tmp-333");
        for directory in [&published_ce, &stale_ce, &stale_de, &unrelated] {
            fs::create_dir_all(directory).unwrap();
        }
        let mut storage = open_slot_storage(
            &ce_slots,
            &de_slots,
            &root.path().join("base-ce"),
            &root.path().join("base-de"),
        );

        storage.discard(&package, &slot).unwrap();

        assert!(!published_ce.exists());
        assert!(!stale_ce.exists());
        assert!(!stale_de.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn discard_package_removes_every_ce_de_slot_and_temporary_artifact() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let other = PackageName::new("com.example.other").unwrap();
        let ce_slots = root.path().join("slots-ce");
        let de_slots = root.path().join("slots-de");
        for slots_root in [&ce_slots, &de_slots] {
            for path in [
                slots_root.join(package.as_str()).join("slot-1"),
                slots_root.join(package.as_str()).join(".slot-2.tmp-111"),
                slots_root.join(other.as_str()).join("slot-1"),
            ] {
                fs::create_dir_all(path).unwrap();
            }
        }
        let mut storage = open_slot_storage(
            &ce_slots,
            &de_slots,
            &root.path().join("base-ce"),
            &root.path().join("base-de"),
        );

        storage.discard_package(&package).unwrap();

        assert!(!ce_slots.join(package.as_str()).exists());
        assert!(!de_slots.join(package.as_str()).exists());
        assert!(ce_slots.join(other.as_str()).join("slot-1").is_dir());
        assert!(de_slots.join(other.as_str()).join("slot-1").is_dir());
    }

    #[test]
    fn opening_storage_hardens_only_source_ancestors() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let ce_storage = root.path().join("ce/uclone-slices-v2");
        let de_storage = root.path().join("de/uclone-slices-v2");
        let ce_slots = ce_storage.join("slots");
        let de_slots = de_storage.join("slots");
        let ce_package = ce_slots.join(package.as_str());
        let de_package = de_slots.join(package.as_str());
        let ce_slot = ce_package.join("slot-1");
        let de_slot = de_package.join("slot-1");
        for directory in [&ce_slot, &de_slot] {
            fs::create_dir_all(directory).unwrap();
            fs::set_permissions(directory, fs::Permissions::from_mode(0o710)).unwrap();
            fs::write(directory.join("account.db"), b"preserve").unwrap();
            fs::set_permissions(
                directory.join("account.db"),
                fs::Permissions::from_mode(0o640),
            )
            .unwrap();
        }
        for directory in [
            &ce_storage,
            &de_storage,
            &ce_slots,
            &de_slots,
            &ce_package,
            &de_package,
        ] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o777)).unwrap();
        }

        let _storage = open_slot_storage(
            &ce_slots,
            &de_slots,
            &root.path().join("canonical-ce"),
            &root.path().join("canonical-de"),
        );

        for directory in [
            &ce_storage,
            &de_storage,
            &ce_slots,
            &de_slots,
            &ce_package,
            &de_package,
        ] {
            assert_eq!(fs::metadata(directory).unwrap().mode() & 0o777, 0o700);
        }
        for directory in [&ce_slot, &de_slot] {
            assert_eq!(fs::metadata(directory).unwrap().mode() & 0o777, 0o710);
            assert_eq!(
                fs::metadata(directory.join("account.db")).unwrap().mode() & 0o777,
                0o640,
            );
            assert_eq!(fs::read(directory.join("account.db")).unwrap(), b"preserve");
        }
    }

    #[test]
    fn source_ancestor_symlink_and_non_directory_are_rejected() {
        for kind in ["symlink", "file"] {
            let root = tempfile::tempdir().unwrap();
            let ce_storage = root.path().join("ce/uclone-slices-v2");
            fs::create_dir_all(ce_storage.parent().unwrap()).unwrap();
            if kind == "symlink" {
                let target = root.path().join("elsewhere");
                fs::create_dir(&target).unwrap();
                symlink(target, &ce_storage).unwrap();
            } else {
                fs::write(&ce_storage, b"not a directory").unwrap();
            }
            let ancestor = fs::metadata(root.path()).unwrap();

            let result = FileSlotStorage::open_with_owner(
                ce_storage.join("slots"),
                root.path().join("de/uclone-slices-v2/slots"),
                root.path().join("canonical-ce"),
                root.path().join("canonical-de"),
                DirectoryOwner::new(ancestor.uid(), ancestor.gid()),
            );

            assert!(result.is_err(), "kind: {kind}");
        }
    }

    #[test]
    fn restore_staging_replaces_and_rolls_back_a_ce_de_slot_pair() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let ce_slots = root.path().join("ce/uclone-slices-v2/slots");
        let de_slots = root.path().join("de/uclone-slices-v2/slots");
        let mut storage =
            open_slot_storage(&ce_slots, &de_slots, &canonical_ce_root, &canonical_de_root);
        let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        storage
            .materialize(&package, &slot, SeedMode::Blank)
            .unwrap();
        let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
        fs::write(target_ce.join("old-ce-a"), b"old-a").unwrap();
        fs::write(target_ce.join("old-ce-b"), b"old-b").unwrap();
        fs::write(target_de.join("old-de-a"), b"old-a").unwrap();
        fs::write(target_de.join("old-de-b"), b"old-b").unwrap();
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-1").unwrap();
        let account = ArchiveAccountId::new("account-1").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        fs::write(Path::new(&staging[0].ce_path).join("new-ce"), b"new").unwrap();
        fs::write(Path::new(&staging[0].de_path).join("new-de"), b"new").unwrap();
        storage
            .set_available_bytes(Path::new(&staging[0].ce_path), 0)
            .unwrap();

        let non_base_exdev = fail_next_rename_into(&[&target_ce]);
        let first_attempt = storage.replace_account(
            &package,
            &token,
            &transfer,
            &account,
            slot.id(),
            &RestorePolicy::Replace,
        );
        assert!(
            first_attempt
                .as_ref()
                .is_err_and(|error| error.to_string().contains("Cross-device link"))
        );
        assert!(non_base_exdev.was_triggered());
        drop(non_base_exdev);

        storage
            .replace_account(
                &package,
                &token,
                &transfer,
                &account,
                slot.id(),
                &RestorePolicy::Replace,
            )
            .unwrap();

        assert_eq!(fs::read(target_ce.join("new-ce")).unwrap(), b"new");
        assert_eq!(fs::read(target_de.join("new-de")).unwrap(), b"new");
        assert!(!target_ce.join("old-ce-a").exists());
        assert!(!target_de.join("old-de-a").exists());

        let (rollback_ce, rollback_de) =
            storage.rollback_paths(&package, &token, slot.id()).unwrap();
        remove_tree_if_exists(&target_ce).unwrap();
        remove_tree_if_exists(&target_de).unwrap();
        fs::write(rollback_ce.join("restore-started"), b"").unwrap();
        fs::write(rollback_de.join("restore-started"), b"").unwrap();
        fs::rename(rollback_de.join("old"), &target_de).unwrap();

        storage
            .rollback_account(&package, &token, slot.id(), &RestorePolicy::Replace)
            .unwrap();

        assert_eq!(fs::read(target_ce.join("old-ce-a")).unwrap(), b"old-a");
        assert_eq!(fs::read(target_ce.join("old-ce-b")).unwrap(), b"old-b");
        assert_eq!(fs::read(target_de.join("old-de-a")).unwrap(), b"old-a");
        assert_eq!(fs::read(target_de.join("old-de-b")).unwrap(), b"old-b");
        assert!(!target_ce.join("new-ce").exists());
        assert!(!target_de.join("new-de").exists());
    }

    #[test]
    fn slot_restore_preserves_resources_while_replacing_and_rolling_back_account_data() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.tencent.tmgp.dfm").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        storage
            .materialize(&package, &slot, SeedMode::Blank)
            .unwrap();
        let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
        fs::write(target_ce.join("old-account"), b"old-account").unwrap();
        fs::write(target_de.join("old-device"), b"old-device").unwrap();
        let puffer = target_ce
            .join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/download/resource.pak");
        let paks =
            target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/1.2.3/Paks/base.pak");
        fs::create_dir_all(puffer.parent().unwrap()).unwrap();
        fs::create_dir_all(paks.parent().unwrap()).unwrap();
        fs::write(&puffer, b"puffer-resource").unwrap();
        fs::write(&paks, b"dolphin-resource").unwrap();
        fs::create_dir_all(target_ce.join("cache")).unwrap();
        fs::write(target_ce.join("cache/transient"), b"discard").unwrap();

        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-preserve-slot").unwrap();
        let account = ArchiveAccountId::new("account-preserve-slot").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        fs::write(
            Path::new(&staging[0].ce_path).join("new-account"),
            b"new-account",
        )
        .unwrap();
        fs::write(
            Path::new(&staging[0].de_path).join("new-device"),
            b"new-device",
        )
        .unwrap();
        let policy = resource_preserve_policy();

        let (rollback_ce, _) = storage.rollback_paths(&package, &token, slot.id()).unwrap();
        let interrupted_resource =
            rollback_ce.join("preserved/files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer");
        let interrupted_move = fail_next_rename_into(&[&interrupted_resource]);
        let interrupted =
            storage.replace_account(&package, &token, &transfer, &account, slot.id(), &policy);
        assert!(
            interrupted
                .as_ref()
                .is_err_and(|error| error.to_string().contains("Cross-device link"))
        );
        assert!(interrupted_move.was_triggered());
        assert_eq!(
            fs::read(target_ce.join("old-account")).unwrap(),
            b"old-account"
        );
        assert_eq!(fs::read(&puffer).unwrap(), b"puffer-resource");
        assert_eq!(fs::read(&paks).unwrap(), b"dolphin-resource");
        assert!(!rollback_ce.exists());
        drop(interrupted_move);

        storage
            .replace_account(&package, &token, &transfer, &account, slot.id(), &policy)
            .unwrap();

        assert_eq!(
            fs::read(target_ce.join("new-account")).unwrap(),
            b"new-account"
        );
        assert_eq!(
            fs::read(target_de.join("new-device")).unwrap(),
            b"new-device"
        );
        assert_eq!(fs::read(&puffer).unwrap(), b"puffer-resource");
        assert_eq!(fs::read(&paks).unwrap(), b"dolphin-resource");
        assert!(!target_ce.join("old-account").exists());
        assert!(!target_ce.join("cache").exists());

        storage
            .rollback_account(&package, &token, slot.id(), &policy)
            .unwrap();

        assert_eq!(
            fs::read(target_ce.join("old-account")).unwrap(),
            b"old-account"
        );
        assert_eq!(
            fs::read(target_de.join("old-device")).unwrap(),
            b"old-device"
        );
        assert_eq!(fs::read(&puffer).unwrap(), b"puffer-resource");
        assert_eq!(fs::read(&paks).unwrap(), b"dolphin-resource");
        assert!(!target_ce.join("new-account").exists());
    }

    #[test]
    fn base_restore_capacity_excludes_preserved_resource_trees() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.tencent.tmgp.dfm").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        let target_ce = canonical_ce_root.join(package.as_str());
        let target_de = canonical_de_root.join(package.as_str());
        fs::create_dir_all(&target_ce).unwrap();
        fs::create_dir_all(&target_de).unwrap();
        fs::write(target_ce.join("old-account"), b"old-account").unwrap();
        fs::write(target_de.join("old-device"), b"old-device").unwrap();
        let resource =
            target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/resource.pak");
        fs::create_dir_all(resource.parent().unwrap()).unwrap();
        fs::write(&resource, vec![0x5a; 1024 * 1024]).unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-preserve-base").unwrap();
        let account = ArchiveAccountId::new("account-preserve-base").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        let staged_ce = Path::new(&staging[0].ce_path);
        let staged_de = Path::new(&staging[0].de_path);
        fs::write(staged_ce.join("new-account"), b"new-account").unwrap();
        fs::write(staged_de.join("new-device"), b"new-device").unwrap();
        let policy = resource_preserve_policy();
        let required_without_resources =
            copied_tree_bytes_excluding(&target_ce, &policy.paths_for(RestoreDomain::Ce)).unwrap()
                + copied_tree_bytes_excluding(&target_de, &policy.paths_for(RestoreDomain::De))
                    .unwrap()
                + copied_tree_bytes(staged_ce).unwrap()
                + copied_tree_bytes(staged_de).unwrap();
        assert!(copied_tree_bytes(&target_ce).unwrap() > required_without_resources);
        storage
            .set_available_bytes(staged_ce, required_without_resources)
            .unwrap();

        storage
            .replace_account(
                &package,
                &token,
                &transfer,
                &account,
                &SlotId::base(),
                &policy,
            )
            .unwrap();

        assert_eq!(
            fs::read(target_ce.join("new-account")).unwrap(),
            b"new-account"
        );
        assert_eq!(
            fs::read(target_de.join("new-device")).unwrap(),
            b"new-device"
        );
        assert_eq!(fs::metadata(&resource).unwrap().len(), 1024 * 1024);
        assert!(!target_ce.join("old-account").exists());

        storage
            .rollback_account(&package, &token, &SlotId::base(), &policy)
            .unwrap();

        assert_eq!(
            fs::read(target_ce.join("old-account")).unwrap(),
            b"old-account"
        );
        assert_eq!(
            fs::read(target_de.join("old-device")).unwrap(),
            b"old-device"
        );
        assert_eq!(fs::metadata(&resource).unwrap().len(), 1024 * 1024);
        assert!(!target_ce.join("new-account").exists());
    }

    #[test]
    fn preserve_policy_rejects_staged_overlap_and_target_symlink_before_mutation() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.tencent.tmgp.dfm").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        let target_ce = canonical_ce_root.join(package.as_str());
        let target_de = canonical_de_root.join(package.as_str());
        fs::create_dir_all(&target_ce).unwrap();
        fs::create_dir_all(&target_de).unwrap();
        fs::write(target_ce.join("old-account"), b"old-account").unwrap();
        fs::write(target_de.join("old-device"), b"old-device").unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-preserve-conflict").unwrap();
        let account = ArchiveAccountId::new("account-preserve-conflict").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        let staged_resource =
            Path::new(&staging[0].ce_path).join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer");
        fs::create_dir_all(&staged_resource).unwrap();
        let policy = resource_preserve_policy();
        assert!(
            storage
                .validate_staged_account(&package, &token, &transfer, &account, &policy)
                .is_err()
        );
        remove_tree_if_exists(&staged_resource).unwrap();

        let outside = root.path().join("outside-resource");
        fs::create_dir(&outside).unwrap();
        let target_resource = target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer");
        fs::create_dir_all(target_resource.parent().unwrap()).unwrap();
        symlink(&outside, &target_resource).unwrap();
        fs::write(
            Path::new(&staging[0].ce_path).join("new-account"),
            b"new-account",
        )
        .unwrap();
        fs::write(
            Path::new(&staging[0].de_path).join("new-device"),
            b"new-device",
        )
        .unwrap();

        assert!(
            storage
                .replace_account(
                    &package,
                    &token,
                    &transfer,
                    &account,
                    &SlotId::base(),
                    &policy,
                )
                .is_err()
        );
        assert_eq!(
            fs::read(target_ce.join("old-account")).unwrap(),
            b"old-account"
        );
        assert_eq!(
            fs::read(target_de.join("old-device")).unwrap(),
            b"old-device"
        );
        assert!(!target_ce.join("new-account").exists());
    }

    #[test]
    fn base_rollback_retries_from_a_complete_old_copy_after_partial_progress() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        let target_ce = canonical_ce_root.join(package.as_str());
        let target_de = canonical_de_root.join(package.as_str());
        fs::create_dir_all(&target_ce).unwrap();
        fs::create_dir_all(&target_de).unwrap();
        for name in ["old-ce-a", "old-ce-b"] {
            fs::write(target_ce.join(name), name.as_bytes()).unwrap();
        }
        fs::create_dir(target_ce.join("nested")).unwrap();
        fs::write(target_ce.join("nested/old-ce-c"), b"old-ce-c").unwrap();
        fs::write(target_ce.join("nested/old-ce-d"), b"old-ce-d").unwrap();
        for name in ["old-de-a", "old-de-b"] {
            fs::write(target_de.join(name), name.as_bytes()).unwrap();
        }
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-base").unwrap();
        let account = ArchiveAccountId::new("account-base").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        fs::write(Path::new(&staging[0].ce_path).join("new-ce"), b"new").unwrap();
        fs::write(Path::new(&staging[0].de_path).join("new-de"), b"new").unwrap();
        let base_rename_guard = fail_next_rename_into(&[&target_ce, &target_de]);
        storage
            .replace_account(
                &package,
                &token,
                &transfer,
                &account,
                &SlotId::base(),
                &RestorePolicy::Replace,
            )
            .unwrap();
        assert!(!base_rename_guard.was_triggered());
        assert_eq!(fs::read(target_ce.join("new-ce")).unwrap(), b"new");
        assert_eq!(fs::read(target_de.join("new-de")).unwrap(), b"new");

        let (rollback_ce, rollback_de) = storage
            .rollback_paths(&package, &token, &SlotId::base())
            .unwrap();
        remove_directory_contents(&target_ce).unwrap();
        remove_directory_contents(&target_de).unwrap();
        fs::write(rollback_ce.join("restore-started"), b"").unwrap();
        fs::write(rollback_de.join("restore-started"), b"").unwrap();
        fs::copy(rollback_ce.join("old/old-ce-a"), target_ce.join("old-ce-a")).unwrap();
        fs::create_dir(target_ce.join("nested")).unwrap();
        fs::copy(
            rollback_ce.join("old/nested/old-ce-c"),
            target_ce.join("nested/old-ce-c"),
        )
        .unwrap();
        fs::copy(rollback_de.join("old/old-de-a"), target_de.join("old-de-a")).unwrap();

        storage
            .rollback_account(&package, &token, &SlotId::base(), &RestorePolicy::Replace)
            .unwrap();
        assert!(!base_rename_guard.was_triggered());

        assert_eq!(fs::read(target_ce.join("old-ce-a")).unwrap(), b"old-ce-a");
        assert_eq!(fs::read(target_ce.join("old-ce-b")).unwrap(), b"old-ce-b");
        assert_eq!(
            fs::read(target_ce.join("nested/old-ce-c")).unwrap(),
            b"old-ce-c"
        );
        assert_eq!(
            fs::read(target_ce.join("nested/old-ce-d")).unwrap(),
            b"old-ce-d"
        );
        assert_eq!(fs::read(target_de.join("old-de-a")).unwrap(), b"old-de-a");
        assert_eq!(fs::read(target_de.join("old-de-b")).unwrap(), b"old-de-b");
        assert!(!target_ce.join("new-ce").exists());
        assert!(!target_de.join("new-de").exists());
    }

    #[test]
    fn base_de_copy_failure_rolls_back_the_successful_ce_and_de_pair() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        let target_ce = canonical_ce_root.join(package.as_str());
        let target_de = canonical_de_root.join(package.as_str());
        fs::create_dir_all(&target_ce).unwrap();
        fs::create_dir_all(&target_de).unwrap();
        fs::write(target_ce.join("old-ce"), b"old-ce").unwrap();
        fs::write(target_de.join("old-de"), b"old-de").unwrap();
        let resource =
            target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/resource.pak");
        fs::create_dir_all(resource.parent().unwrap()).unwrap();
        fs::write(&resource, b"resource").unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-base-de-failure").unwrap();
        let account = ArchiveAccountId::new("account-base-de-failure").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        let staged_ce = Path::new(&staging[0].ce_path);
        let staged_de = Path::new(&staging[0].de_path);
        fs::write(staged_ce.join("new-ce"), b"new-ce").unwrap();
        fs::write(staged_de.join("new-de"), b"new-de").unwrap();

        let _copy_failure = fail_next_copy(staged_de, &target_de);
        let policy = resource_preserve_policy();
        let result = storage.replace_account(
            &package,
            &token,
            &transfer,
            &account,
            &SlotId::base(),
            &policy,
        );

        assert!(result.is_err_and(|error| error.to_string().contains("injected copy failure")));
        assert_eq!(fs::read(target_ce.join("old-ce")).unwrap(), b"old-ce");
        assert_eq!(fs::read(target_de.join("old-de")).unwrap(), b"old-de");
        assert_eq!(fs::read(&resource).unwrap(), b"resource");
        assert!(!target_ce.join("new-ce").exists());
        assert!(!target_de.join("new-de").exists());
        let (rollback_ce, rollback_de) = storage
            .rollback_paths(&package, &token, &SlotId::base())
            .unwrap();
        assert!(!rollback_ce.exists());
        assert!(!rollback_de.exists());
    }

    #[test]
    fn base_restore_rejects_space_that_covers_rollback_but_not_replacement_copy() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        let target_ce = canonical_ce_root.join(package.as_str());
        let target_de = canonical_de_root.join(package.as_str());
        fs::create_dir_all(&target_ce).unwrap();
        fs::create_dir_all(&target_de).unwrap();
        fs::write(target_ce.join("old-ce"), b"old-ce").unwrap();
        fs::write(target_de.join("old-de"), b"old-de").unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-base-capacity").unwrap();
        let account = ArchiveAccountId::new("account-base-capacity").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        let staged_ce = Path::new(&staging[0].ce_path);
        let staged_de = Path::new(&staging[0].de_path);
        fs::write(staged_ce.join("new-ce"), b"new-ce").unwrap();
        fs::write(staged_de.join("new-de"), b"new-de").unwrap();
        let ce_required = copied_tree_bytes(&target_ce).unwrap();
        let de_required = copied_tree_bytes(&target_de).unwrap();
        let staged_ce_required = copied_tree_bytes(staged_ce).unwrap();
        let staged_de_required = copied_tree_bytes(staged_de).unwrap();
        assert_eq!(
            fs::metadata(staged_ce).unwrap().dev(),
            fs::metadata(staged_de).unwrap().dev()
        );
        let rollback_only = ce_required + de_required;
        let full_copy_requirement = rollback_only + staged_ce_required + staged_de_required;
        assert!(rollback_only < full_copy_requirement);
        storage
            .set_available_bytes(staged_ce, rollback_only)
            .unwrap();

        let result = storage.replace_account(
            &package,
            &token,
            &transfer,
            &account,
            &SlotId::base(),
            &RestorePolicy::Replace,
        );

        assert!(
            result
                .as_ref()
                .is_err_and(AdapterError::is_insufficient_storage)
        );
        assert_eq!(fs::read(target_ce.join("old-ce")).unwrap(), b"old-ce");
        assert_eq!(fs::read(target_de.join("old-de")).unwrap(), b"old-de");
        assert_eq!(fs::read(staged_ce.join("new-ce")).unwrap(), b"new-ce");
        assert_eq!(fs::read(staged_de.join("new-de")).unwrap(), b"new-de");
        let (rollback_ce, rollback_de) = storage
            .rollback_paths(&package, &token, &SlotId::base())
            .unwrap();
        assert!(!rollback_ce.exists());
        assert!(!rollback_de.exists());
    }

    #[test]
    fn capacity_requirements_are_combined_only_for_the_same_filesystem() {
        assert!(
            ensure_filesystem_capacity([(11, 6, 10), (11, 6, 10)])
                .is_err_and(|error| error.is_insufficient_storage())
        );
        assert!(ensure_filesystem_capacity([(11, 6, 10), (22, 6, 10)]).is_ok());
    }

    #[test]
    fn restore_staging_rejects_an_escaping_symlink_before_replacement() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce_root = root.path().join("canonical-ce");
        let canonical_de_root = root.path().join("canonical-de");
        fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
        fs::create_dir(root.path().join("ce")).unwrap();
        fs::create_dir(root.path().join("de")).unwrap();
        let mut storage = open_slot_storage(
            &root.path().join("ce/uclone-slices-v2/slots"),
            &root.path().join("de/uclone-slices-v2/slots"),
            &canonical_ce_root,
            &canonical_de_root,
        );
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let transfer = TransferId::new("transfer-1").unwrap();
        let account = ArchiveAccountId::new("account-1").unwrap();
        let staging = storage
            .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
            .unwrap();
        symlink(
            "../../../../outside",
            Path::new(&staging[0].ce_path).join("escape"),
        )
        .unwrap();

        assert!(
            storage
                .validate_staged_account(
                    &package,
                    &token,
                    &transfer,
                    &account,
                    &RestorePolicy::Replace,
                )
                .is_err()
        );
    }

    #[test]
    fn private_directory_verification_rejects_wrong_owner_and_mode() {
        let root = tempfile::tempdir().unwrap();
        let metadata = fs::metadata(root.path()).unwrap();
        let actual_owner = DirectoryOwner::new(metadata.uid(), metadata.gid());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
        let wrong_mode = fs::metadata(root.path()).unwrap();

        assert!(verify_private_directory(&wrong_mode, actual_owner).is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let secure = fs::metadata(root.path()).unwrap();
        let wrong_owner = DirectoryOwner::new(secure.uid().saturating_add(1), secure.gid());
        assert!(verify_private_directory(&secure, wrong_owner).is_err());
    }
}
