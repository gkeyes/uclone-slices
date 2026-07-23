use crate::domain::PackageName;

/// Package identities recoverable from transaction preparation records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalPackageScan {
    packages: Vec<PackageName>,
    unattributed_corruption: bool,
}

impl JournalPackageScan {
    pub(super) fn new(mut packages: Vec<PackageName>, unattributed_corruption: bool) -> Self {
        packages.sort();
        packages.dedup();
        Self {
            packages,
            unattributed_corruption,
        }
    }

    /// Returns package names proved by valid first transaction records.
    pub fn package_names(&self) -> &[PackageName] {
        &self.packages
    }

    /// Returns true when corruption could not be assigned to any package.
    pub const fn unattributed_corruption(&self) -> bool {
        self.unattributed_corruption
    }
}
