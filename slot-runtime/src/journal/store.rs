use std::fs;
use std::path::{Path, PathBuf};

use super::storage::{
    ensure_directory, initialize_root, read_steps, step_file_name, sync_directory,
    validate_directory, validate_transaction_directory, write_step,
};
use super::{JournalError, JournalEvent, JournalStep, Transaction, TransactionSpec};
use crate::domain::TransactionId;
use crate::recovery::{RecoveryDecision, decide_recovery};
use crate::registry::PackageRevision;

#[doc = "Filesystem-backed append-only transaction journal."]
#[derive(Debug, Clone)]
pub struct JournalStore {
    root: PathBuf,
    transactions: PathBuf,
    owner_uid: u32,
}

impl JournalStore {
    #[doc = "Creates or opens a journal root with root-only directory permissions."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, JournalError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid = initialize_root(&root)?;
        let transactions = root.join("transactions");
        ensure_directory(&transactions, owner_uid)?;
        Ok(Self {
            root,
            transactions,
            owner_uid,
        })
    }

    #[doc = "Atomically publishes the prepared transaction before any side effect."]
    pub fn create(&self, spec: &TransactionSpec) -> Result<(), JournalError> {
        self.validate_store()?;
        spec.validate()?;
        let final_path = self.transaction_path(spec.transaction_id())?;
        if fs::symlink_metadata(&final_path).is_ok() {
            return Err(JournalError::AlreadyExists(spec.transaction_id().clone()));
        }
        let staging = self
            .transactions
            .join(format!(".new-{}", spec.transaction_id().as_str()));
        if fs::symlink_metadata(&staging).is_ok() {
            return Err(JournalError::StagingExists(staging));
        }
        ensure_directory(&staging, self.owner_uid)?;
        let steps = staging.join("steps");
        ensure_directory(&steps, self.owner_uid)?;
        let first = JournalStep::new(
            spec.transaction_id().clone(),
            1,
            None,
            JournalEvent::Prepared {
                spec: Box::new(spec.clone()),
            },
        )?;
        write_step(&steps, self.owner_uid, &first)?;
        sync_directory(&staging, self.owner_uid)?;
        fs::rename(&staging, &final_path)
            .map_err(|source| JournalError::io("publish transaction", &final_path, source))?;
        validate_transaction_directory(&final_path, self.owner_uid)?;
        sync_directory(&self.transactions, self.owner_uid)
    }

    #[doc = "Appends one legal, hash-linked event and makes it durable."]
    pub fn append(
        &self,
        transaction_id: &TransactionId,
        event: JournalEvent,
    ) -> Result<JournalStep, JournalError> {
        let transaction = self.load(transaction_id)?;
        self.append_after(&transaction, event, false)
    }

    pub(crate) fn append_proven_rollback(
        &self,
        transaction_id: &TransactionId,
        latest: Option<&PackageRevision>,
    ) -> Result<JournalStep, JournalError> {
        let transaction = self.load(transaction_id)?;
        if decide_recovery(&transaction, latest) != RecoveryDecision::RollbackToPrevious {
            return Err(JournalError::IllegalTransition {
                previous: "committing",
                next: "rolling_back",
            });
        }
        self.append_after(&transaction, JournalEvent::RollingBack, true)
    }

    fn append_after(
        &self,
        transaction: &Transaction,
        event: JournalEvent,
        proven_rollback: bool,
    ) -> Result<JournalStep, JournalError> {
        let previous = transaction
            .steps()
            .last()
            .ok_or_else(|| JournalError::Corrupt("transaction has no steps".to_owned()))?;
        if !proven_rollback
            && matches!(event, JournalEvent::RollingBack)
            && matches!(previous.event(), JournalEvent::Committing { .. })
        {
            return Err(JournalError::IllegalTransition {
                previous: "committing",
                next: "rolling_back",
            });
        }
        event.validate_after(previous.event())?;
        let generation = previous
            .generation()
            .checked_add(1)
            .ok_or_else(|| JournalError::Corrupt("generation overflow".to_owned()))?;
        let step = JournalStep::new(
            transaction.spec().transaction_id().clone(),
            generation,
            Some(previous.sha256().to_owned()),
            event,
        )?;
        let steps = self
            .transaction_path(transaction.spec().transaction_id())?
            .join("steps");
        write_step(&steps, self.owner_uid, &step)?;
        Ok(step)
    }

    #[doc = "Loads and verifies every published step and transition."]
    pub fn load(&self, transaction_id: &TransactionId) -> Result<Transaction, JournalError> {
        self.validate_store()?;
        let transaction_path = self.transaction_path(transaction_id)?;
        validate_transaction_directory(&transaction_path, self.owner_uid)?;
        let steps_path = transaction_path.join("steps");
        let steps = read_steps(&steps_path, self.owner_uid, transaction_id)?;
        let first = steps.first().ok_or_else(|| {
            JournalError::Corrupt("transaction has no committed steps".to_owned())
        })?;
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
        for pair in steps.windows(2) {
            let [previous, next] = pair else {
                return Err(JournalError::Corrupt(
                    "invalid transition window".to_owned(),
                ));
            };
            next.event().validate_after(previous.event())?;
        }
        Ok(Transaction::new(spec.as_ref().clone(), steps))
    }

    #[doc = "Enumerates and verifies every transaction directory in identifier order."]
    pub fn list(&self) -> Result<Vec<Transaction>, JournalError> {
        self.transaction_ids()?
            .into_iter()
            .map(|transaction_id| self.load(&transaction_id))
            .collect()
    }

    pub(super) fn transaction_ids(&self) -> Result<Vec<TransactionId>, JournalError> {
        Ok(super::artifact_scan::scan(self)?.published)
    }

    #[doc = "Returns the validated transaction directory path."]
    pub fn transaction_path(
        &self,
        transaction_id: &TransactionId,
    ) -> Result<PathBuf, JournalError> {
        Ok(self.transactions.join(transaction_id.as_str()))
    }

    #[doc = "Returns the immutable final path for one generation."]
    pub fn step_path(
        &self,
        transaction_id: &TransactionId,
        generation: u64,
    ) -> Result<PathBuf, JournalError> {
        Ok(self
            .transaction_path(transaction_id)?
            .join("steps")
            .join(step_file_name(generation)))
    }

    #[doc = "Returns the journal root."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(super) const fn owner_uid(&self) -> u32 {
        self.owner_uid
    }

    pub(super) fn transactions_root(&self) -> &Path {
        &self.transactions
    }

    fn validate_store(&self) -> Result<(), JournalError> {
        validate_directory(&self.root, self.owner_uid)?;
        validate_directory(&self.transactions, self.owner_uid)
    }
}
