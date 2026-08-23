use std::fs::{self, File};
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, chown};
use std::path::{Path, PathBuf};
#[cfg(target_os = "android")]
use std::process::Command;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::{
    PackageAggregate, PackageBinding, PackageName, RebindIntent, SeedMode, Slot, SlotId,
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
        })
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
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir(&destination).map_err(io_error)?;
            copy_tree(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &destination).map_err(io_error)?;
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
    use crate::model::{DisplayName, PackageAggregate, PackageIdentity};
    use std::os::unix::fs::symlink;

    fn identity() -> PackageIdentity {
        PackageIdentity::new(10_000, "/data/app/example/base.apk", 1, 2).unwrap()
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
