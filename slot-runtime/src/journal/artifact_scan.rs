use std::fs;
use std::path::Path;

use super::{JournalError, JournalEvent, JournalStore};
use crate::domain::{PackageKey, TransactionId};

pub(super) struct JournalArtifactScan {
    pub(super) published: Vec<TransactionId>,
}

pub(super) fn scan(store: &JournalStore) -> Result<JournalArtifactScan, JournalError> {
    super::storage::validate_directory(store.root(), store.owner_uid())?;
    super::storage::validate_directory(store.transactions_root(), store.owner_uid())?;
    let entries = fs::read_dir(store.transactions_root()).map_err(|source| {
        JournalError::io(
            "read transactions directory",
            store.transactions_root(),
            source,
        )
    })?;
    let mut published = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| {
            JournalError::io(
                "read transaction artifact",
                store.transactions_root(),
                source,
            )
        })?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| JournalError::Corrupt("unexpected transaction artifact".to_owned()))?;
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            return Err(unexpected(&name));
        }
        if let Ok(transaction_id) = TransactionId::parse(&name) {
            published.push(transaction_id);
            continue;
        }
        name.strip_prefix(".new-")
            .ok_or_else(|| unexpected(&name))
            .and_then(|value| TransactionId::parse(value).map_err(|_| unexpected(&name)))?;
    }
    published.sort();
    Ok(JournalArtifactScan { published })
}

pub(super) fn prepared_key_at(
    store: &JournalStore,
    transaction: &Path,
    transaction_id: &TransactionId,
) -> Result<PackageKey, JournalError> {
    super::storage::validate_directory(transaction, store.owner_uid())?;
    let first = super::storage::read_first_step(
        &transaction.join("steps"),
        store.owner_uid(),
        transaction_id,
    )?;
    let JournalEvent::Prepared { spec } = first.event() else {
        return Err(JournalError::Corrupt(
            "first event is not prepared".to_owned(),
        ));
    };
    spec.validate()?;
    if spec.transaction_id() != transaction_id {
        return Err(JournalError::Corrupt(
            "prepared transaction id does not match its directory".to_owned(),
        ));
    }
    Ok(PackageKey::new(spec.package_name().clone(), spec.user_id()))
}

fn unexpected(name: &str) -> JournalError {
    JournalError::Corrupt(format!("unexpected transaction artifact {name}"))
}
