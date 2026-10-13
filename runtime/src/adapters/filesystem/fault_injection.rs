//! Test-only fault injection for `FileSlotStorage`.
//!
//! The production build compiles no-op hooks with the same names (see the bottom of
//! `filesystem.rs`), so none of this state exists outside host tests. Every hook is
//! thread-local: each libtest test runs on its own thread, so tests cannot see each other's
//! injected faults.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::{FileSlotStorage, io_error};
use crate::ports::AdapterError;

thread_local! {
    static RENAME_FAILURE: RefCell<Option<RenameFailure>> = const { RefCell::new(None) };
    static COPY_FAILURE_PATHS: RefCell<Option<(PathBuf, PathBuf)>> = const { RefCell::new(None) };
    static AVAILABLE_BYTES_BY_DEVICE: RefCell<BTreeMap<u64, u64>> =
        const { RefCell::new(BTreeMap::new()) };
}

#[derive(Clone)]
struct RenameFailure {
    target_prefixes: Vec<PathBuf>,
    triggered: Rc<Cell<bool>>,
}

pub(super) struct RenameFailureGuard {
    previous: Option<RenameFailure>,
    triggered: Rc<Cell<bool>>,
}

impl RenameFailureGuard {
    pub(super) fn was_triggered(&self) -> bool {
        self.triggered.get()
    }
}

impl Drop for RenameFailureGuard {
    fn drop(&mut self) {
        RENAME_FAILURE.with(|failure| {
            *failure.borrow_mut() = self.previous.take();
        });
    }
}

/// Makes the next rename whose target starts with one of `target_prefixes` fail with EXDEV.
pub(super) fn fail_next_rename_into(target_prefixes: &[&Path]) -> RenameFailureGuard {
    let triggered = Rc::new(Cell::new(false));
    let failure = RenameFailure {
        target_prefixes: target_prefixes
            .iter()
            .map(|path| path.to_path_buf())
            .collect(),
        triggered: Rc::clone(&triggered),
    };
    let previous = RENAME_FAILURE.with(|configured| configured.borrow_mut().replace(failure));
    RenameFailureGuard {
        previous,
        triggered,
    }
}

pub(super) fn take_rename_failure(target: &Path) -> bool {
    RENAME_FAILURE.with(|configured| {
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
    })
}

pub(super) struct CopyFailureGuard {
    previous: Option<(PathBuf, PathBuf)>,
}

impl Drop for CopyFailureGuard {
    fn drop(&mut self) {
        COPY_FAILURE_PATHS.with(|paths| {
            *paths.borrow_mut() = self.previous.take();
        });
    }
}

/// Makes the next whole-tree copy from `source` into `target` fail.
pub(super) fn fail_next_copy(source: &Path, target: &Path) -> CopyFailureGuard {
    let previous = COPY_FAILURE_PATHS.with(|configured| {
        configured
            .borrow_mut()
            .replace((source.to_path_buf(), target.to_path_buf()))
    });
    CopyFailureGuard { previous }
}

pub(super) fn take_copy_failure(source: &Path, target: &Path) -> bool {
    COPY_FAILURE_PATHS.with(|configured| {
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

pub(super) fn available_bytes_override(device: u64) -> Option<u64> {
    AVAILABLE_BYTES_BY_DEVICE.with(|configured| configured.borrow().get(&device).copied())
}

impl FileSlotStorage {
    /// Reports `available` free bytes for the filesystem that holds `path`.
    pub(super) fn set_available_bytes(
        &mut self,
        path: &Path,
        available: u64,
    ) -> Result<(), AdapterError> {
        let device = fs::metadata(path).map_err(io_error)?.dev();
        AVAILABLE_BYTES_BY_DEVICE.with(|configured| {
            configured.borrow_mut().insert(device, available);
        });
        Ok(())
    }
}
