use super::matrix_probe::MatrixProbe;
use super::{fixture_cases, fixture_data};
use crate::domain::{PackageKey, SlotView};
use crate::service::{PackageState, ServiceError};

#[derive(Clone, Copy)]
pub(super) enum Row {
    Absent,
    ReadyBase,
    ReadyExtension,
    Attempt,
    Orphan,
    MissingState,
    UnfinishedJournal,
    MissingCatalog,
    MissingRegistry,
    MissingMetadata,
    MissingMetadataCreating,
    MissingMetadataQuarantined,
    MissingMetadataDeleted,
    ObservationFailure,
    GateFailure,
    RecoveryLifecycle,
    IdentityMismatch,
    Blocked,
    PersistedQuarantine,
    UpdatePreparing,
    UpdateWindowOpen,
    UpdateVerifying,
    LifecycleDrift,
    RepairWaiting,
}

pub(super) enum Expected {
    Absent,
    ReadyBase,
    ReadyExtension(SlotView),
    Recovery,
    Quarantined,
    Error,
}

pub(super) struct Fixture {
    pub(super) _root: tempfile::TempDir,
    pub(super) stores: super::super::super::stores::ProductionStores,
    pub(super) key: PackageKey,
    pub(super) probe: MatrixProbe,
    pub(super) expected: Expected,
}

pub(super) const fn rows() -> [Row; 24] {
    [
        Row::Absent,
        Row::ReadyBase,
        Row::ReadyExtension,
        Row::Attempt,
        Row::Orphan,
        Row::MissingState,
        Row::UnfinishedJournal,
        Row::MissingCatalog,
        Row::MissingRegistry,
        Row::MissingMetadata,
        Row::MissingMetadataCreating,
        Row::MissingMetadataQuarantined,
        Row::MissingMetadataDeleted,
        Row::ObservationFailure,
        Row::GateFailure,
        Row::RecoveryLifecycle,
        Row::IdentityMismatch,
        Row::Blocked,
        Row::PersistedQuarantine,
        Row::UpdatePreparing,
        Row::UpdateWindowOpen,
        Row::UpdateVerifying,
        Row::LifecycleDrift,
        Row::RepairWaiting,
    ]
}

pub(super) fn build(row: Row) -> Fixture {
    match row {
        Row::Absent => fixture_data::absent(),
        Row::ReadyBase => fixture_data::base_case(
            Expected::ReadyBase,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |_, _, _| {},
        ),
        Row::ReadyExtension => {
            fixture_data::extension(Some(crate::slot_metadata::SlotRecordState::Ready), true)
        }
        Row::Attempt => fixture_data::base_case(
            Expected::Recovery,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| {
                stores
                    .attempts
                    .create_pending(key, fixture_data::default_gate())
                    .unwrap();
            },
        ),
        Row::Orphan => fixture_cases::orphan(),
        Row::MissingState => fixture_data::enrolled_case(true, false),
        Row::UnfinishedJournal => fixture_cases::unfinished(),
        Row::MissingCatalog => fixture_data::enrolled_case(false, true),
        Row::MissingRegistry => {
            fixture_data::extension(Some(crate::slot_metadata::SlotRecordState::Ready), false)
        }
        Row::MissingMetadata => fixture_data::extension(None, true),
        Row::MissingMetadataCreating => {
            fixture_data::extension(Some(crate::slot_metadata::SlotRecordState::Creating), true)
        }
        Row::MissingMetadataQuarantined => fixture_data::extension(
            Some(crate::slot_metadata::SlotRecordState::Quarantined),
            true,
        ),
        Row::MissingMetadataDeleted => {
            fixture_data::extension(Some(crate::slot_metadata::SlotRecordState::Deleted), true)
        }
        Row::ObservationFailure => fixture_data::base_case(
            Expected::Error,
            MatrixProbe::observation_failure(),
            |_, _, _| {},
        ),
        Row::GateFailure => {
            fixture_data::base_case(Expected::Error, MatrixProbe::gate_failure(), |_, _, _| {})
        }
        Row::RecoveryLifecycle => fixture_data::base_case(
            Expected::Recovery,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| fixture_cases::transition_recovery(stores, key),
        ),
        Row::IdentityMismatch => fixture_cases::identity_mismatch(),
        Row::Blocked => fixture_cases::blocked(),
        Row::PersistedQuarantine => fixture_data::base_case(
            Expected::Quarantined,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| fixture_cases::transition_quarantine(stores, key),
        ),
        Row::UpdatePreparing => fixture_data::base_case(
            Expected::Recovery,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| fixture_cases::transition_update_preparing(stores, key),
        ),
        Row::UpdateWindowOpen => fixture_data::base_case(
            Expected::Recovery,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| fixture_cases::transition_update_window_open(stores, key),
        ),
        Row::UpdateVerifying => fixture_data::base_case(
            Expected::Recovery,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| fixture_cases::transition_update_verifying(stores, key),
        ),
        Row::LifecycleDrift => fixture_data::base_case(
            Expected::Recovery,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| fixture_cases::transition_drift(stores, key),
        ),
        Row::RepairWaiting => fixture_data::base_case(
            Expected::Recovery,
            MatrixProbe::healthy(fixture_data::default_gate()),
            |stores, key, _| fixture_cases::transition_repair_waiting(stores, key),
        ),
    }
}

#[allow(
    clippy::panic,
    reason = "matrix assertions use explicit impossible-state branches"
)]
pub(super) fn assert_expected(actual: Result<PackageState, ServiceError>, expected: Expected) {
    match expected {
        Expected::Error => assert_eq!(actual, Err(ServiceError::RecoveryRequired)),
        Expected::Absent => assert_eq!(actual.unwrap(), PackageState::Absent),
        Expected::Recovery => assert_eq!(actual.unwrap(), PackageState::RecoveryRequired),
        Expected::Quarantined => assert_eq!(actual.unwrap(), PackageState::Quarantined),
        Expected::ReadyBase => match actual {
            Ok(PackageState::Ready(snapshot)) => {
                assert!(snapshot.managed().active_slot().is_base());
                assert!(snapshot.slots().is_empty());
            }
            _ => panic!("expected a ready base"),
        },
        Expected::ReadyExtension(target) => match actual {
            Ok(PackageState::Ready(snapshot)) => {
                assert_eq!(snapshot.managed().active_slot(), target.slot_id());
                assert_eq!(snapshot.managed().active_inodes(), target.inodes());
                assert_eq!(snapshot.slot(target.slot_id()), Some(&target));
            }
            _ => panic!("expected a ready extension"),
        },
    }
}
