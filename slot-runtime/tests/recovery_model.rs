#![doc = "Minimal model-based Journal/Registry recovery properties."]
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    reason = "the property model uses validated temporary-store fixtures"
)]

#[path = "support/mod.rs"]
mod support;

use proptest::prelude::{Just, ProptestConfig, Strategy};
use proptest::{prop_assert, prop_assert_eq, prop_oneof, proptest};
use tempfile::TempDir;
use uclone_slot_runtime::domain::{AppIdentity, CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::journal::{
    JournalError, JournalEvent, JournalStore, Transaction, TransactionSpec,
};
use uclone_slot_runtime::recovery::{RecoveryDecision, decide_recovery};
use uclone_slot_runtime::registry::{PackageRevision, RegistryStore};

#[derive(Debug, Clone, Copy)]
enum LegalPrefix {
    Prepared,
    GateHeld,
    ProcessesQuiesced,
    Applying,
    ViewVerified,
    Committing,
    RegistryCommitted,
    GateReleased,
    CompletedTarget,
    RollingBack,
    RolledBack,
    CompletedPrevious,
    RecoveryBeforeCommit,
    RecoveryAfterCommit,
}

impl LegalPrefix {
    fn events(self) -> Vec<JournalEvent> {
        let nonce = || CommitNonce::parse("nonce-recovery-model").unwrap();
        match self {
            Self::Prepared => Vec::new(),
            Self::GateHeld => vec![JournalEvent::GateHeld],
            Self::ProcessesQuiesced => {
                vec![JournalEvent::GateHeld, JournalEvent::ProcessesQuiesced]
            }
            Self::Applying => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
            ],
            Self::ViewVerified => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::ViewVerified,
            ],
            Self::Committing => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::ViewVerified,
                JournalEvent::Committing { nonce: nonce() },
            ],
            Self::RegistryCommitted => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::ViewVerified,
                JournalEvent::Committing { nonce: nonce() },
                JournalEvent::RegistryCommitted { nonce: nonce() },
            ],
            Self::GateReleased => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::ViewVerified,
                JournalEvent::Committing { nonce: nonce() },
                JournalEvent::RegistryCommitted { nonce: nonce() },
                JournalEvent::GateReleased,
            ],
            Self::CompletedTarget => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::ViewVerified,
                JournalEvent::Committing { nonce: nonce() },
                JournalEvent::RegistryCommitted { nonce: nonce() },
                JournalEvent::GateReleased,
                JournalEvent::Completed,
            ],
            Self::RollingBack => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::RollingBack,
            ],
            Self::RolledBack => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::RollingBack,
                JournalEvent::RolledBack,
            ],
            Self::CompletedPrevious => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::RollingBack,
                JournalEvent::RolledBack,
                JournalEvent::GateReleased,
                JournalEvent::Completed,
            ],
            Self::RecoveryBeforeCommit => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::RecoveryRequired {
                    reason: "platform_failure".to_owned(),
                },
            ],
            Self::RecoveryAfterCommit => vec![
                JournalEvent::GateHeld,
                JournalEvent::ProcessesQuiesced,
                JournalEvent::Applying,
                JournalEvent::ViewVerified,
                JournalEvent::Committing { nonce: nonce() },
                JournalEvent::RecoveryRequired {
                    reason: "platform_failure".to_owned(),
                },
            ],
        }
    }

    const fn has_target_commit(self) -> bool {
        matches!(
            self,
            Self::Committing | Self::RegistryCommitted | Self::GateReleased | Self::CompletedTarget
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum RegistryEvidence {
    Absent,
    Previous,
    Target,
    ConflictingNonce,
    ForeignIdentity,
}

#[derive(Debug, Clone, Copy)]
enum ModelShape {
    BaseExtension,
    SlotExtension,
}

fn legal_prefix_strategy() -> impl Strategy<Value = LegalPrefix> {
    prop_oneof![
        Just(LegalPrefix::Prepared),
        Just(LegalPrefix::GateHeld),
        Just(LegalPrefix::ProcessesQuiesced),
        Just(LegalPrefix::Applying),
        Just(LegalPrefix::ViewVerified),
        Just(LegalPrefix::Committing),
        Just(LegalPrefix::RegistryCommitted),
        Just(LegalPrefix::GateReleased),
        Just(LegalPrefix::CompletedTarget),
        Just(LegalPrefix::RollingBack),
        Just(LegalPrefix::RolledBack),
        Just(LegalPrefix::CompletedPrevious),
        Just(LegalPrefix::RecoveryBeforeCommit),
        Just(LegalPrefix::RecoveryAfterCommit),
    ]
}

fn registry_evidence_strategy() -> impl Strategy<Value = RegistryEvidence> {
    prop_oneof![
        Just(RegistryEvidence::Absent),
        Just(RegistryEvidence::Previous),
        Just(RegistryEvidence::Target),
        Just(RegistryEvidence::ConflictingNonce),
        Just(RegistryEvidence::ForeignIdentity),
    ]
}

fn model_shape_strategy() -> impl Strategy<Value = ModelShape> {
    prop_oneof![
        Just(ModelShape::BaseExtension),
        Just(ModelShape::SlotExtension),
    ]
}

#[derive(Debug)]
struct ModelStores {
    _root: TempDir,
    journal: JournalStore,
    registry: RegistryStore,
    spec: TransactionSpec,
    transaction: Transaction,
}

fn model_stores(shape: ModelShape, prefix: LegalPrefix, evidence: RegistryEvidence) -> ModelStores {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let spec = model_spec("tx-recovery-model", shape);
    journal.create(&spec).unwrap();
    for event in prefix.events() {
        journal.append(spec.transaction_id(), event).unwrap();
    }
    append_evidence(&registry, &spec, shape, evidence);
    let transaction = journal.load(spec.transaction_id()).unwrap();
    ModelStores {
        _root: root,
        journal,
        registry,
        spec,
        transaction,
    }
}

fn model_spec(transaction_id: &str, shape: ModelShape) -> TransactionSpec {
    let base = DataInodes::new(101, 201).unwrap();
    let (previous, target) = match shape {
        ModelShape::BaseExtension => (
            SlotView::new(SlotId::base(), base),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(301, 401).unwrap(),
            ),
        ),
        ModelShape::SlotExtension => (
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(301, 401).unwrap(),
            ),
            SlotView::new(
                SlotId::parse("personal").unwrap(),
                DataInodes::new(501, 601).unwrap(),
            ),
        ),
    };
    support::transaction_spec(support::TransactionFixture::new(
        transaction_id,
        support::TransactionViews::new(base, previous, target),
        "boot-recovery-model",
    ))
}

fn append_evidence(
    registry: &RegistryStore,
    spec: &TransactionSpec,
    shape: ModelShape,
    evidence: RegistryEvidence,
) {
    if matches!(shape, ModelShape::SlotExtension)
        && !matches!(
            evidence,
            RegistryEvidence::Absent | RegistryEvidence::ForeignIdentity
        )
    {
        let previous = model_spec("tx-recovery-previous", ModelShape::BaseExtension);
        let revision = PackageRevision::committed(
            &previous,
            previous.base_inodes(),
            CommitNonce::parse("nonce-recovery-previous").unwrap(),
        )
        .unwrap();
        registry.append(&revision).unwrap();
    }
    if matches!(
        evidence,
        RegistryEvidence::Absent | RegistryEvidence::Previous
    ) {
        return;
    }
    let nonce = match evidence {
        RegistryEvidence::Target | RegistryEvidence::ForeignIdentity => {
            CommitNonce::parse("nonce-recovery-model").unwrap()
        }
        RegistryEvidence::ConflictingNonce => CommitNonce::parse("nonce-recovery-wrong").unwrap(),
        RegistryEvidence::Absent | RegistryEvidence::Previous => return,
    };
    let revision_spec = match evidence {
        RegistryEvidence::ForeignIdentity => support::transaction_spec_with_contract(
            support::TransactionFixture::new(
                "tx-foreign-recovery-model",
                support::TransactionViews::new(
                    spec.base_inodes(),
                    SlotView::new(SlotId::base(), spec.base_inodes()),
                    SlotView::new(
                        SlotId::parse("work").unwrap(),
                        DataInodes::new(301, 401).unwrap(),
                    ),
                ),
                "boot-foreign-model",
            )
            .with_contract(support::PackageContract::new(
                AppIdentity::new(
                    10_321,
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    1,
                    "/data/app/slotprobe/base.apk",
                )
                .unwrap(),
                uclone_slot_runtime::lifecycle::LifecycleState::Normal,
            )),
        ),
        RegistryEvidence::Target | RegistryEvidence::ConflictingNonce => spec.clone(),
        RegistryEvidence::Absent | RegistryEvidence::Previous => return,
    };
    let revision =
        PackageRevision::committed(&revision_spec, revision_spec.base_inodes(), nonce).unwrap();
    registry.append(&revision).unwrap();
}

fn expected_decision(
    shape: ModelShape,
    prefix: LegalPrefix,
    evidence: RegistryEvidence,
    transaction: &Transaction,
) -> RecoveryDecision {
    match evidence {
        RegistryEvidence::Target if prefix.has_target_commit() => {
            if transaction.view() == uclone_slot_runtime::journal::TransactionView::CompletedTarget
            {
                RecoveryDecision::NoAction
            } else {
                RecoveryDecision::RollForwardToTarget
            }
        }
        RegistryEvidence::Absent | RegistryEvidence::Previous
            if matches!(evidence, RegistryEvidence::Previous)
                || matches!(shape, ModelShape::BaseExtension) =>
        {
            match transaction.view() {
                uclone_slot_runtime::journal::TransactionView::PreCommit
                | uclone_slot_runtime::journal::TransactionView::CommitPending
                | uclone_slot_runtime::journal::TransactionView::RolledBack => {
                    RecoveryDecision::RollbackToPrevious
                }
                uclone_slot_runtime::journal::TransactionView::CompletedPrevious => {
                    RecoveryDecision::NoAction
                }
                uclone_slot_runtime::journal::TransactionView::RecoveryRequired
                    if transaction.recoverable_platform_precommit() =>
                {
                    RecoveryDecision::RollbackToPrevious
                }
                _ => RecoveryDecision::RecoveryRequired,
            }
        }
        _ => RecoveryDecision::RecoveryRequired,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn legal_transaction_prefixes_x_registry_evidence_are_fail_closed(
        shape in model_shape_strategy(),
        prefix in legal_prefix_strategy(),
        evidence in registry_evidence_strategy(),
    ) {
        let stores = model_stores(shape, prefix, evidence);
        let latest = stores
            .registry
            .latest(stores.spec.package_name())
            .unwrap();
        let decision = decide_recovery(&stores.transaction, latest.as_ref());
        let expected = expected_decision(shape, prefix, evidence, &stores.transaction);

        prop_assert_eq!(decision, expected);
        match decision {
            RecoveryDecision::RollbackToPrevious => {
                prop_assert!(matches!(evidence, RegistryEvidence::Absent | RegistryEvidence::Previous));
            }
            RecoveryDecision::RollForwardToTarget => {
                prop_assert!(matches!(evidence, RegistryEvidence::Target));
                prop_assert!(prefix.has_target_commit());
            }
            RecoveryDecision::NoAction => {
                prop_assert!(
                    matches!(
                        (prefix, evidence),
                        (
                            LegalPrefix::CompletedPrevious,
                            RegistryEvidence::Absent | RegistryEvidence::Previous,
                        )
                            | (
                                LegalPrefix::GateReleased | LegalPrefix::CompletedTarget,
                                RegistryEvidence::Target,
                            )
                    )
                );
            }
            RecoveryDecision::RecoveryRequired => {}
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn generated_invalid_next_event_is_rejected_without_journal_mutation(
        shape in model_shape_strategy(),
        prefix in legal_prefix_strategy(),
    ) {
        let stores = model_stores(shape, prefix, RegistryEvidence::Absent);
        let error = stores.journal.append(
            stores.spec.transaction_id(),
            JournalEvent::Prepared {
                spec: Box::new(stores.spec.clone()),
            },
        );

        let rejected = matches!(error, Err(JournalError::IllegalTransition { .. }));
        prop_assert!(rejected);
        let after = stores.journal.load(stores.spec.transaction_id()).unwrap();
        prop_assert_eq!(after, stores.transaction);
    }
}
