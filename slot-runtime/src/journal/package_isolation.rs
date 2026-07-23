use super::{JournalError, JournalPackageScan, JournalStore, Transaction};
use crate::domain::{PackageKey, TransactionId};

impl JournalStore {
    /// Enumerates only transactions attributed to one validated package key.
    ///
    /// Corruption after a valid preparation record is isolated to that record's
    /// package. A missing or corrupt preparation record in a published
    /// transaction remains a global error because its owner cannot be proved.
    pub fn list_for_package(&self, key: &PackageKey) -> Result<Vec<Transaction>, JournalError> {
        let mut transactions = Vec::new();
        let artifacts = super::artifact_scan::scan(self)?;
        for transaction_id in artifacts.published {
            let owner = self.prepared_key(&transaction_id)?;
            if &owner == key {
                transactions.push(self.load(&transaction_id)?);
            }
        }
        Ok(transactions)
    }

    /// Discovers attributable transaction owners without parsing later steps.
    pub fn package_names(&self) -> Result<JournalPackageScan, JournalError> {
        let artifacts = super::artifact_scan::scan(self)?;
        let mut packages = Vec::new();
        let mut unattributed_corruption = false;
        for transaction_id in artifacts.published {
            match self.prepared_key(&transaction_id) {
                Ok(key) => packages.push(key.package_name().clone()),
                Err(_) => unattributed_corruption = true,
            }
        }
        Ok(JournalPackageScan::new(packages, unattributed_corruption))
    }

    fn prepared_key(&self, transaction_id: &TransactionId) -> Result<PackageKey, JournalError> {
        let transaction = self.transaction_path(transaction_id)?;
        super::artifact_scan::prepared_key_at(self, &transaction, transaction_id)
    }
}
