#![doc = "Production composition for the fixed user-zero Slots Preview runtime."]

mod composition;
mod containment;
mod enrollment;
mod enrollment_ops;
mod management;
mod mapping;
mod metadata;
mod reconciliation;
mod reconciliation_pending;
mod reconciliation_safety;
mod reconciliation_validation;
mod rescue;
mod slot_lifecycle;
mod state;
mod stores;
mod switching;

pub use composition::ProductionPlatform;
pub use metadata::{MetadataSource, SystemMetadataSource};

#[cfg(test)]
mod tests {
    mod orphan_gate;

    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;

    use tempfile::tempdir;

    use super::composition::ProductionPlatform;
    use super::metadata::SystemMetadataSource;
    use super::stores::ProductionStores;
    use crate::android::{
        AndroidBackend, AndroidMaterializer, SystemCommandRunner, SystemMaterializerExecutor,
        SystemPackageProbe,
    };
    use crate::domain::{PackageKey, PackageName, UserId};
    use crate::enrollment::EnrollmentStore;
    use crate::enrollment_attempt::EnrollmentAttemptStore;
    use crate::journal::JournalStore;
    use crate::package_state::PackageStateStore;
    use crate::protocol::ALLOWED_PACKAGE;
    use crate::registry::RegistryStore;

    #[test]
    fn clean_production_startup_is_serveable_before_first_enroll() {
        // Given: a pristine control plane with no enrollment or recovery artifacts.
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let stores = ProductionStores {
            enrollment: EnrollmentStore::new(root.path().join("enrollment")).unwrap(),
            compatibility_policy: crate::compatibility_policy::CompatibilityPolicyStore::new(
                root.path().join("compatibility-policy"),
            )
            .unwrap(),
            attempts: EnrollmentAttemptStore::new(root.path().join("enrollment-attempts")).unwrap(),
            catalog: crate::catalog::CatalogStore::new(root.path().join("catalog")).unwrap(),
            package_state: PackageStateStore::new(root.path().join("package-state")).unwrap(),
            journal: JournalStore::new(root.path().join("journal")).unwrap(),
            registry: RegistryStore::new(root.path().join("registry")).unwrap(),
            slot_metadata: crate::slot_metadata::SlotMetadataStore::new(
                root.path().join("slot-metadata"),
            )
            .unwrap(),
        };
        let runtime = AndroidBackend::new(SystemCommandRunner::new(), SystemPackageProbe::new());
        let materializer =
            AndroidMaterializer::new(SystemMaterializerExecutor, SystemPackageProbe::new());
        let mut platform = ProductionPlatform {
            runtime,
            materializer,
            probe: std::cell::RefCell::new(SystemPackageProbe::new()),
            metadata: SystemMetadataSource::new(),
            stores,
        };
        let package = PackageName::parse(ALLOWED_PACKAGE).unwrap();
        let key = PackageKey::new(package, UserId::PRIMARY);

        // When: production reconciliation runs before the first enrollment.
        let result = platform.do_reconcile(&key);

        // Then: an artifact-free daemon remains serveable for the enroll command.
        assert!(result.is_ok(), "clean startup failed: {result:?}");
    }
}
