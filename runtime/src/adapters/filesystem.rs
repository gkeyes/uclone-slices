use std::fs::{self, File};
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, chown};
use std::path::{Path, PathBuf};
#[cfg(target_os = "android")]
use std::process::Command;

use crate::model::{PackageAggregate, PackageName, SeedMode, Slot, SlotId};
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
                    .map_err(|error| AdapterError::new(error.to_string()))?;
                aggregate
                    .validate()
                    .map_err(|error| AdapterError::new(error.to_string()))?;
                (aggregate.package() == package)
                    .then_some(Some(aggregate))
                    .ok_or_else(|| AdapterError::new("aggregate package does not match its path"))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(error)),
        }
    }

    fn save(&mut self, aggregate: &PackageAggregate) -> Result<(), AdapterError> {
        aggregate
            .validate()
            .map_err(|error| AdapterError::new(error.to_string()))?;
        let path = self.aggregate_path(aggregate.package());
        let parent = path
            .parent()
            .ok_or_else(|| AdapterError::new("aggregate path has no parent"))?;
        fs::create_dir_all(parent).map_err(io_error)?;
        let bytes =
            serde_json::to_vec(aggregate).map_err(|error| AdapterError::new(error.to_string()))?;
        let temporary = parent.join(format!(".aggregate.json.tmp-{}", std::process::id()));
        let mut file = File::create(&temporary).map_err(io_error)?;
        file.write_all(&bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        fs::rename(&temporary, &path).map_err(io_error)?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(io_error)?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct FileSlotStorage {
    ce_slots_root: PathBuf,
    de_slots_root: PathBuf,
    canonical_ce_root: PathBuf,
    canonical_de_root: PathBuf,
}

impl FileSlotStorage {
    pub(crate) fn open(
        ce_slots_root: impl Into<PathBuf>,
        de_slots_root: impl Into<PathBuf>,
        canonical_ce_root: impl Into<PathBuf>,
        canonical_de_root: impl Into<PathBuf>,
    ) -> Result<Self, AdapterError> {
        let ce_slots_root = ce_slots_root.into();
        let de_slots_root = de_slots_root.into();
        fs::create_dir_all(&ce_slots_root).map_err(io_error)?;
        fs::create_dir_all(&de_slots_root).map_err(io_error)?;
        Ok(Self {
            ce_slots_root,
            de_slots_root,
            canonical_ce_root: canonical_ce_root.into(),
            canonical_de_root: canonical_de_root.into(),
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
        fs::create_dir_all(&ce_package_root).map_err(io_error)?;
        fs::create_dir_all(&de_package_root).map_err(io_error)?;
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
        let (ce, de) = self.slot_paths(package, slot);
        let ce_result = remove_tree_if_exists(&ce);
        let de_result = remove_tree_if_exists(&de);
        match (ce_result, de_result) {
            (Err(error), _) | (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
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

fn io_error(error: std::io::Error) -> AdapterError {
    AdapterError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::model::{DisplayName, PackageAggregate, PackageIdentity};
    use std::os::unix::fs::PermissionsExt as _;

    fn identity() -> PackageIdentity {
        PackageIdentity::new(10_000, "/data/app/example/base.apk", 1, 2).unwrap()
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
        let mut storage = FileSlotStorage::open(&ce_slots, &de_slots, &ce, &de).unwrap();
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
        let mut storage = FileSlotStorage::open(
            root.path().join("slots-ce"),
            root.path().join("slots-de"),
            &ce,
            &de,
        )
        .unwrap();
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
        let mut storage = FileSlotStorage::open(
            root.path().join("slots-ce"),
            root.path().join("slots-de"),
            &ce,
            &de,
        )
        .unwrap();
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
}
