# Slots Preview package-state authority

This note describes the current read authority for one user-zero package. It is a
characterization of the implementation in `slot-runtime/src/production/state.rs`,
not a proposal to broaden what the runtime may accept. Unknown, incomplete, or
ambiguous state must remain fail-closed.

## Authority boundary

`ProductionPlatform::package_state` is the public read entry point. Before calling
the store-backed loader it applies the package-specific recovery override and the
offline-rescue status. If neither overlay is active, `production::state::load`
reconstructs the package state from these durable and live authorities:

| Order | Authority | What must be true |
| --- | --- | --- |
| 1 | Enrollment attempt | No attempt exists. Any valid pending or recovery attempt is a recovery fence. |
| 2 | Enrollment | A base-only `ManagedPackage` exists and is valid for the requested package/user. |
| 3 | Catalog | The complete catalog has one matching base entry and only matching extension entries. |
| 4 | Package state | A hash-linked lifecycle stream exists and its latest revision names the requested package. |
| 5 | Registry | The latest active slot, when present, matches the enrolled identity/base anchors and catalog view. |
| 6 | Journal | Every transaction for this package is complete; a completed target has a matching Registry revision and commit nonce. |
| 7 | Compatibility policy | The enrolled identity and observed support class are accepted. |
| 8 | Package probe | The live identity, PackageManager base inodes, canonical view, and active-process view agree with the persisted contract. |
| 9 | Lifecycle guard | The persisted lifecycle and live observation permit the resulting base/slot view. |
| 10 | Gate probe | The exact current enabled/suspended facts are readable. |

Any store read or live observation error is mapped to `ServiceError::RecoveryRequired`.
The loader returns `PackageState::RecoveryRequired` for a readable but incomplete or
incoherent set of records. It returns `PackageState::Quarantined` when the installed
identity no longer belongs to the accepted enrollment, and `PackageState::Absent`
only for a clean package-free control plane.

## Characterization matrix

The executable rows live in
`slot-runtime/src/production/tests/package_state_matrix.rs`. The matrix uses the
existing `ProductionStores`, `ready_package`, `NativeBaseProbe`, package-state
store, journal, catalog, Registry, and slot-metadata fixtures.

| Scenario | Durable/live setup | Current result | Safety interpretation |
| --- | --- | --- | --- |
| Absent | All package stores empty and no journal transaction | `Absent` | The only absence proof. An orphan artifact is not absent. |
| Ready base | Enrollment, accepted policy, base catalog, normal package-state revision, no unfinished journal, coherent base probe | `Ready` | Base is the normal anchor; Registry may be empty when no completed target exists. |
| Ready extension | Ready base plus extension catalog, `Ready` metadata, completed target journal, matching Registry revision/nonce, coherent extension probe | `Ready` | Extension is usable only after metadata and commit proof agree. |
| Enrollment attempt | Any readable attempt remains | `RecoveryRequired` | The attempt fences publication/cleanup ambiguity. |
| Orphan | Policy (or another package artifact) exists without enrollment | `RecoveryRequired` | Never reinterpret partial enrollment as a new package. |
| Missing package state | Enrollment, policy, and base catalog exist without a lifecycle stream | `RecoveryRequired` | No lifecycle authority, no serving view. |
| Unfinished journal | A published transaction ends before `Completed` | `RecoveryRequired` | Do not infer previous/target from an interrupted transaction. |
| Missing catalog | Enrollment, policy, package state, but no base catalog | `RecoveryRequired` | The immutable data-view map is unavailable. |
| Missing Registry after target commit | Extension journal is `CompletedTarget`, but its Registry revision is absent | `RecoveryRequired` | Journal and Registry commit point must agree. |
| Missing/unready active metadata | Active extension metadata is absent, `Creating`, `Quarantined`, or `Deleted` | `RecoveryRequired` | Do not expose an extension whose user-facing record is not `Ready`. |
| Observation probe failure | `observe_package` returns `ProbeError` | `Err(RecoveryRequired)` | No live identity/view proof, no serving view. |
| Gate probe failure | `gate_snapshot` returns `ProbeError` | `Err(RecoveryRequired)` | Exact gate facts are part of the read contract. |
| Persisted recovery lifecycle | Latest package-state revision is `RecoveryRequired` | `RecoveryRequired` | A durable recovery fence cannot be bypassed by a healthy probe. |
| Persisted transitional lifecycle | Latest revision is `LifecycleDrifted`, `RepairWaiting`, `UpdatePreparing`, `UpdateWindowOpen`, or `UpdateVerifying` without an exact update proof | `RecoveryRequired` | Transitional state is not an ordinary serveable state. |
| Identity mismatch | UID or signing identity differs from enrollment | `Quarantined` | Old slots are not automatically attached to a new identity. |
| Blocked support class | Live probe classifies a system/shared-UID package as blocked | `Quarantined` | The Preview allowlist does not manage this package class. |
| Persisted quarantine lifecycle | Latest package-state revision is `Quarantined` | `Quarantined` | Quarantine is terminal until an explicit repair path exists. |

### Gate combinations

For each of the five Android enabled states (`Default`, `Enabled`, `Disabled`,
`DisabledUser`, `DisabledUntilUsed`) and both suspended values, a coherent base
package remains `Ready`. The resulting `ObservedGateState.enabled` is true only
for `Default` and `Enabled`; `ObservedGateState.suspended` preserves the probe's
value. A gate read error is a recovery error, as above. This is observation, not
permission to mutate or silently restore a gate.

## Switch crash-boundary matrix

`slot-runtime/tests/runtime_coordinator/crash_matrix.rs` executes the real
`SwitchCoordinator` once at every typed `FaultPoint`. Each row checks the durable
Journal view, Registry proof, applied view, gate, retained lease, and
`decide_recovery` result.

| Crash point | Durable Journal view | Registry/current-view proof | Gate/lease fact | Current recovery decision |
| --- | --- | --- | --- | --- |
| `Prepared` | `PreCommit` | Previous view; no target Registry revision | Gate not yet held; lease retained | `RollbackToPrevious` |
| `GateHeld` | `PreCommit` | Previous view; no target Registry revision | Gate held; lease retained | `RollbackToPrevious` |
| `ProcessesQuiesced` | `PreCommit` | Previous view; no target Registry revision | Gate held; lease retained | `RollbackToPrevious` |
| `Applying` | `PreCommit` | Previous view; no target Registry revision | Gate held; lease retained | `RollbackToPrevious` |
| `TargetApplied` | `PreCommit` | Target is physically applied; no target Registry revision | Gate held; lease retained | `RollbackToPrevious` |
| `ViewVerified` | `PreCommit` | Target is verified; no target Registry revision | Gate held; lease retained | `RollbackToPrevious` |
| `Committing` | `CommitPending` | Target is applied; Registry still previous/absent | Gate held; lease retained | `RollbackToPrevious` |
| `RegistryPublished` | `CommitPending` | Matching target Registry revision exists | Gate held; lease retained | `RollForwardToTarget` |
| `RegistryCommitted` | `PostCommit` | Matching target Registry revision exists | Gate held; lease retained | `RollForwardToTarget` |
| `GateRestored` | `PostCommit` | Matching target Registry revision exists | Exact gate restored; lease retained | `RollForwardToTarget` |
| `GateReleased` | `CompletedTarget` | Matching target Registry revision exists | Gate restored; lease retained | `NoAction` |
| `Completed` | `CompletedTarget` | Matching target Registry revision exists | Gate restored; lease retained | `NoAction` |
| `GateLeaseRetired` | `CompletedTarget` | Matching target Registry revision exists | Gate restored; lease retired | `NoAction` |

The accompanying proptest in `slot-runtime/tests/recovery_model.rs` varies legal
Journal prefixes across Base-to-extension and extension-to-extension shapes, with
absent, previous, matching-target, conflicting-nonce, and foreign-identity
Registry evidence. It also proves that an illegal next Journal event is rejected
without changing the persisted transaction.

## Lifecycle fail-closed rule

The package-state matrix now covers every persisted `LifecycleState`. Only
`Normal` may produce an ordinary `PackageState::Ready` result. A coherent live
probe does not override a persisted `LifecycleDrifted`, `RepairWaiting`,
`UpdatePreparing`, `UpdateWindowOpen`, or `UpdateVerifying` state; these resolve to
`RecoveryRequired` unless a future update use case supplies an exact verification
proof. `Quarantined` remains an isolated terminal result.

The lifecycle guard retains one narrow update exception for the future managed
update path: `UpdateVerifying` may return `AllowUpdateVerification` only when the
installed version/path changed, the active view is Base, and PackageManager,
canonical, and active-process inodes all prove the enrolled Base. This exception
does not make the ordinary package-state loader serve a transitional snapshot.

## Non-goals and boundaries

This matrix does not alter production code, delete or repair durable artifacts, or
exercise the real Android mount/gate backend. It does not claim that a passing
host fixture proves a device view. Device reconciliation, rescue overlays, and the
`recovery_overrides` set remain separate authorities and require their own runtime
evidence.
