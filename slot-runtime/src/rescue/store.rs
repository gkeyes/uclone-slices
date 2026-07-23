use std::path::{Path, PathBuf};

use super::storage::{self, StorePaths};
use super::{
    FIXED_RESCUE_JOURNAL_ROOT, RescueError, RescueEvent, RescueId, RescueSpec, RescueStep,
    RescueTransaction,
};
use crate::domain::PackageName;

#[doc = "Filesystem-backed single-epoch emergency rescue journal."]
#[derive(Debug, Clone)]
pub struct RescueJournalStore {
    paths: StorePaths,
    package: PackageName,
}

impl RescueJournalStore {
    #[doc = "Creates or opens a bounded rescue journal below a supplied host/runtime root."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RescueError> {
        let package = PackageName::parse(crate::protocol::ALLOWED_PACKAGE)?;
        Self::for_package(root, &package)
    }

    #[doc = "Opens one package-scoped rescue journal below a supplied root."]
    pub fn for_package(root: impl AsRef<Path>, package: &PackageName) -> Result<Self, RescueError> {
        Ok(Self {
            paths: storage::initialize(root.as_ref(), package)?,
            package: package.clone(),
        })
    }

    #[doc = "Opens the compiled root-only Preview rescue-journal path."]
    pub fn fixed() -> Result<Self, RescueError> {
        Self::new(FIXED_RESCUE_JOURNAL_ROOT)
    }

    #[doc = "Opens one package-scoped journal at the production rescue root."]
    pub fn fixed_for(package: &PackageName) -> Result<Self, RescueError> {
        Self::for_package(FIXED_RESCUE_JOURNAL_ROOT, package)
    }

    #[doc = "Durably prepares a new rescue or resumes only the exact same specification."]
    pub fn begin(&self, spec: &RescueSpec) -> Result<RescueTransaction, RescueError> {
        spec.validate()?;
        if spec.package_key().package_name() != &self.package {
            return Err(RescueError::SpecMismatch);
        }
        if let Some(existing) = self.load()? {
            if existing.spec() != spec {
                return Err(RescueError::SpecMismatch);
            }
            return Ok(existing);
        }
        let first = RescueStep::new(
            spec.rescue_id().clone(),
            1,
            None,
            RescueEvent::Prepared {
                spec: Box::new(spec.clone()),
            },
        )?;
        storage::publish_first(&self.paths, &first)?;
        self.load()?.ok_or_else(|| {
            RescueError::Corrupt("prepared rescue disappeared after publication".to_owned())
        })
    }

    #[doc = "Appends one legal hash-linked event and makes it durable."]
    pub fn append(
        &self,
        rescue_id: &RescueId,
        event: RescueEvent,
    ) -> Result<RescueStep, RescueError> {
        let transaction = self
            .load()?
            .ok_or_else(|| RescueError::Corrupt("missing prepared rescue".to_owned()))?;
        if transaction.spec().rescue_id() != rescue_id {
            return Err(RescueError::SpecMismatch);
        }
        let phase = transaction.phase();
        event.validate_after(phase, transaction.spec())?;
        let previous = transaction
            .steps()
            .last()
            .ok_or_else(|| RescueError::Corrupt("rescue has no steps".to_owned()))?;
        let generation = previous
            .generation()
            .checked_add(1)
            .ok_or(RescueError::BoundExceeded("step generation"))?;
        let generation_index = usize::try_from(generation)
            .map_err(|_| RescueError::BoundExceeded("step generation"))?;
        if generation_index > storage::MAX_STEPS {
            return Err(RescueError::BoundExceeded("rescue steps"));
        }
        let step = RescueStep::new(
            rescue_id.clone(),
            generation,
            Some(previous.sha256().to_owned()),
            event,
        )?;
        storage::write_step(&self.paths, &step)?;
        Ok(step)
    }

    #[doc = "Loads and verifies the sole allowlisted rescue epoch when present."]
    pub fn load(&self) -> Result<Option<RescueTransaction>, RescueError> {
        let steps = storage::read_steps(&self.paths, None)?;
        if steps.is_empty() {
            return Ok(None);
        }
        let first = steps
            .first()
            .ok_or_else(|| RescueError::Corrupt("rescue has no first step".to_owned()))?;
        let RescueEvent::Prepared { spec } = first.event() else {
            return Err(RescueError::Corrupt(
                "first rescue event is not prepared".to_owned(),
            ));
        };
        spec.validate()?;
        if spec.package_key().package_name() != &self.package {
            return Err(RescueError::Corrupt(
                "rescue journal path identity mismatch".to_owned(),
            ));
        }
        verify_step_identity(spec.rescue_id(), &steps)?;
        Ok(Some(RescueTransaction::new(spec.as_ref().clone(), steps)?))
    }

    #[doc = "Returns the immutable path for one event generation."]
    pub fn step_path(&self, generation: u64) -> PathBuf {
        self.paths.steps.join(storage::step_file_name(generation))
    }

    #[doc = "Returns the rescue-journal root."]
    pub fn root(&self) -> &Path {
        &self.paths.root
    }
}

fn verify_step_identity(id: &RescueId, steps: &[RescueStep]) -> Result<(), RescueError> {
    for step in steps {
        if step.rescue_id() != id {
            return Err(RescueError::Corrupt(
                "rescue step belongs to another specification".to_owned(),
            ));
        }
    }
    Ok(())
}
