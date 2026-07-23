# UClone Slots Preview — Architecture Stabilization Roadmap

Status: implementation roadmap for `slices-preview`.

Implementation status in this roadmap is updated through commit `b47efe1`
(`Add durable emergency containment manifest`).
The roadmap is intentionally incremental: each phase preserves the current fail-closed
behavior and can be reverted independently. It does not authorize a device reboot or a
change to persistent device data.

## Objective

Reduce state and protocol drift without a rewrite. The target is one explicit package-state
interpretation boundary, typed application/use-case seams, a versioned Runtime/APK/Bridge
contract, and thin Boot/Emergency adapters. The existing CE/DE transaction, Gate, Journal,
Registry, inode, and `RecoveryRequired` safety rules remain the protected behavioral baseline.

## Current status at the baseline

| Area | Status | Evidence / boundary |
| --- | --- | --- |
| State-authority documentation and characterization matrix | **Complete** | [`SLOTS_STATE_AUTHORITY.md`](SLOTS_STATE_AUTHORITY.md), `slot-runtime/src/production/tests/package_state_matrix.rs` |
| Switch/Recovery crash-point matrix and minimal model tests | **Complete** | `slot-runtime/tests/runtime_coordinator/crash_matrix.rs`, `slot-runtime/tests/recovery_model.rs` |
| Rust/Kotlin protocol-v2 golden fixtures | **Complete** | `protocol-fixtures/`, Rust and manager RuntimeProtocol tests |
| Rust request correlation and internal diagnostics | **Complete foundation** | `slot-runtime/src/service/diagnostics.rs`, commit `2ac1cd2`; semantic cause taxonomy remains incomplete |
| PackageAggregate as normal Runtime read authority | **Complete** | `slot-runtime/src/production/package_aggregate.rs`, `facts.rs`, `resolver.rs`, `state.rs` |
| Paired APK/Bridge/Runtime build identity | **Complete** | `slot-bridge/`, manager Runtime protocol adapter; device artifact smoke remains separate |
| Emergency Manifest | **Storage foundation complete; integration in progress** | Durable storage/validation and tests are committed; explicit healthy Runtime ownership, producer, typed query, and Shell consumer are not yet integrated |
| Immutable manager state/coordinator | **Complete foundation** | Immutable `SlotsUiState`, verification revocation, serialized coordinator, and focused tests are committed; stale-intent deduplication remains a follow-up |
| Managed update lifecycle | **Not started** | Requires owner/artifact vocabulary and PackageState schema plan before commands |
| Boot/Shell convergence | **Not started** | Shell remains an independent fail-closed fallback until a typed manifest is consumed |
| Legacy/Preview CI and product separation | **Documented, not complete** | [`PRODUCT_BOUNDARIES.md`](PRODUCT_BOUNDARIES.md); independent gates still need completion |

An Emergency Manifest must not describe a healthy managed package as `NotManaged` or
`BaseRetired`. Before producer or Shell integration, it needs an explicit Runtime-ownership
state plus a live, boot-scoped handoff proof. A stale persisted ownership value alone must
never cause an emergency watcher to stop containment.

## Non-negotiable safety invariants

1. The native Base CE/DE directories and their enrolled inodes are never moved, exchanged,
   overwritten, or reused as an extension slot.
2. CE and DE for an active extension always belong to the same slot and are verified in the
   canonical, mirror, Zygote, and App-process views.
3. Journal intent is durable before platform mutation; Registry publication is the commit
   point and must carry the matching transaction nonce.
4. A view, identity, Gate, Journal, Registry, or inode that cannot be proved is not served:
   the App remains contained and the result is `RecoveryRequired` or `Quarantined`.
5. Gate release restores the exact pre-operation enabled/suspended state. Unknown client
   results never launch the target App.
6. Runtime, APK, Bridge, Boot, and rescue paths must agree on the same package facts; a
   fallback may strengthen containment but must not claim completion.
7. `user0` and the current conditional Direct Boot boundary remain in force. No system
   partition writes, OverlayFS, permissive SELinux policy, or Launcher data operations.

## PR sequence and phase gates

Each PR is deliberately narrow. A PR may be merged only when its entry gate is satisfied,
its exit artifacts are recorded, and its rollback path has been checked.

### Phase 0 — Freeze and baseline

**Goal:** make current behavior, dependencies, schemas, artifacts, and safety boundaries
reviewable before further behavior changes.

**PRs:**

- `docs: record slices architecture seams and invariants`
- `build: make Preview validation environment reproducible`

**Scope:** architecture/state-source docs, protocol/store manifest, current behavior fixtures,
build identity and environment parameters. No Runtime, Shell, Store, Gate, Journal, Registry,
mount, or UI behavior changes.

**Entry gate:** clean source diff is understood; the pre-existing
`tools/test-post-fs-setup-migration.sh` worktree change is explicitly excluded.

**Exit gate:** clean checkout runs the declared host checks; Preview and Legacy artifacts are
named and identified separately; `PRODUCT_BOUNDARIES.md` and this roadmap match the tree.

**Rollback:** revert documentation/build-validation commits only; no device data is touched.

### Phase 1 — Characterization, invariants, and contract tests

**Goal:** protect current behavior before moving ownership.

**PRs:**

- `test(runtime): add package-state characterization matrix`
- `test(runtime): add switch and recovery crash-point matrix`
- `test(protocol): add Rust/Kotlin v2 golden corpus`
- `test(runtime): enable minimal model/property testing`

**Scope:** state combinations, legal Journal prefixes, crash decisions, schema corruption,
request/response compatibility, and a small model that generates legal/illegal event prefixes.
Tests must record existing defects as characterization expectations rather than changing
production behavior.

**Entry gate:** Phase 0 artifacts exist and all existing fast host tests pass.

**Exit gate:** every currently reachable package state has an explicit row; each persistent
crash boundary has an allowed terminal decision; both Rust and Kotlin consume the same golden
frames; property tests execute in CI (long stress tests remain a separate profile).

**Rollback:** delete/revert test and fixture commits; no production or persistent format change.

### Phase 2 — One online PackageAggregate resolver

**Goal:** remove duplicate normal Runtime state interpretation while retaining all current
Store files and schemas.

**PRs:**

- `refactor(runtime): introduce pure PackageFacts and PackageAggregate`
- `refactor(runtime): shadow-compare aggregate with existing loader`
- `refactor(runtime): migrate read use cases to aggregate decisions`

**Scope:** `slot-runtime/src/domain/`, `production/state.rs`, and an application resolver.
The aggregate is a derived decision object, not a new persistent Store. Boot/Emergency remain
outside this phase.

**Entry gate:** Phase 1 matrix is green.

**Exit gate:** all online use cases resolve the same aggregate; shadow comparison has no
unexplained differences; locked, unlocked, and recovery-only evidence scopes are tested.

**Rollback:** keep the old loader behind a temporary selection point and revert call-site
migration if any fixture differs.

### Phase 3 — Separate safety disposition from diagnostic cause

**Goal:** retain fail-closed handling while preserving actionable root causes.

**PRs:**

- `refactor(runtime): add internal safety and diagnostic axes`
- `diag(runtime): add structured operation correlation`
- `protocol: expose compatible diagnostic extensions`

**Scope:** internal `SafetyDisposition`, `DiagnosticCause`, operation/transaction/package
correlation, and adapter-level logging. Existing public error codes remain compatible until
the protocol migration.

**Entry gate:** all old error-code mappings have golden coverage.

**Exit gate:** every failure records request/operation/package/transaction/phase; safe action
is unchanged; sink failure cannot block or weaken containment; causes are not collapsed into
`Internal`/`RecoveryRequired` without a recorded reason.

**Rollback:** disable diagnostic extensions and keep the compatibility error mapping.

### Phase 4 — Type the Runtime/APK/Bridge protocol boundary

**Goal:** make command, payload, IDs, deadlines, build pairing, and error codes one contract.

**PRs:**

- `protocol: centralize v2 constants and golden fixtures`
- `protocol: validate request/operation correlation and deadlines`
- `protocol: add compatibility adapter for the next version`

**Scope:** `slot-runtime/src/protocol.rs`, CLI/RPC adapters, manager `RuntimeProtocol` and
`RootRpcClient`, Bridge pairing. Keep the current v2 wire fields and a compatibility adapter.

**Entry gate:** Phase 1 golden corpus and Phase 3 diagnostic mapping are green.

**Exit gate:** Rust, Kotlin, Bridge, and Shell fixtures consume identical frames; response
request IDs are checked; outer deadlines leave transport cleanup time; mixed build identities
return a typed rejection; no transaction/store schema changes.

**Rollback:** retain the v2 adapter and switch clients back to it.

### Phase 5 — Converge Boot and Emergency through a typed manifest

**Goal:** stop Shell from reconstructing Journal/Registry/Retired state while preserving an
independent emergency containment path.

**PRs:**

- `runtime: publish versioned Emergency Manifest`
- `cli: add read-only emergency status and recovery query`
- `boot: migrate upgrade/reconcile consumers to typed manifest`
- `boot: define daemon/watcher ownership handoff`

**Scope:** Runtime producer, fixed-path atomic manifest, read-only query, then Shell adapters.
Shell may only strengthen containment when Runtime is unavailable. It must not parse internal
Journal JSON or infer Completed/Retired from globs.

**Entry gate:** Emergency Manifest storage/validation tests pass, including same-boot package
addition, epoch fencing, parent-directory sync, and concurrent writer serialization.

**Exit gate:** daemon, Boot, upgrade, emergency, rescue, and offline paths have explicit
ownership; manifest publication is fail-closed; Shell fixtures cover malformed/old/missing
manifest and daemon-offline cases; no boot hook blocks startup.

The ownership handoff is a mandatory pre-integration gate: a matching typed live proof may
put the watcher in standby, while daemon death, boot/generation/epoch mismatch, or query
failure must immediately resume containment. The watcher remains available; it does not
permanently exit after handoff.

**Rollback:** keep the prior Shell fallback and disable manifest consumers; use independent
rescue-to-Base before any authorized device rollback.

### Phase 6 — Android UI state and operation coordination

**Goal:** ensure UI state is immutable and never treats stale/unknown data as safety proof.

**PRs:**

- `refactor(manager): add immutable UiState and verification state`
- `refactor(manager): isolate RuntimeGateway and operation coordinator`
- `test(manager): cover timeout, cancellation, stale intent, and launch gating`

**Scope:** `slot-manager-app` state, reducer/coordinator, typed repository outcomes, and
screen wiring. Preserve visible wording and the mutation → fresh status → launch order.

**Entry gate:** protocol adapter tests and existing manager suite are green.

**Exit gate:** unknown/timeout revokes proof and reconciles without launch; only a fresh
verified `Committed` snapshot can launch; operations are serialized/deduplicated; background
and stale responses cannot overwrite newer state.

**Rollback:** retain the old ViewModel entry point and revert screen/coordinator wiring.

### Phase 7 — Managed lifecycle, Legacy separation, and Boot release gates

This phase is intentionally split into independently reviewable tracks.

#### 7A — Managed Package update lifecycle

**PRs:**

- `model(runtime): separate immutable owner identity from mutable installed artifact`
- `runtime: add schema-compatible managed update window`
- `runtime: reconcile update, clear-data, uninstall, and reinstall drift`

Only start after Phase 2–4. Enrollment keeps owner identity/Base anchors; PackageState owns
the accepted artifact/update context. An update outside an exact Gate/token window fails
closed; UID/signature changes quarantine old slots; no automatic acceptance of a new identity.

**Exit gate:** update, downgrade, clear-data, uninstall, reinstall, TOCTOU, and crash-point
tests prove Base/slot/Gate behavior. Schema migration and corruption rollback are mandatory.

#### 7B — Legacy/Preview product and CI separation

**PRs:**

- `docs/ci: separate Legacy and Preview build contracts`
- `build: add independent Legacy and Preview gates`
- `cleanup: remove only confirmed-unused compatibility paths`

Keep `app/` and `launcher-module/` behavior unchanged. Launcher integration remains optional
and cannot perform Root, mount, Journal, Gate, or data operations.

#### 7C — Boot extraction and release validation

Extract host-testable startup coordination from `bin/ucloned.rs` only after 5A–5B and lifecycle
tests are green. Keep Shell hooks unchanged within the extraction PR. A real-device install,
reboot, unlock, and rescue run requires separate explicit user authorization.

**Exit gate for Preview release:** host gates, artifact pairing, managed lifecycle, online
multi-App smoke, one authorized reboot/unlock run, independent rescue, and final evidence all
pass. Until then the product remains Preview.

## Cross-phase acceptance checklist

- `cargo fmt --check`, strict Clippy, rustdoc, Rust tests, Android unit/lint, protocol golden,
  Shell fixtures, and CI artifact checks are green for the affected phase.
- All builds use repository-external build/cache directories where supported; no generated
  build artifacts are committed.
- Every PR identifies changed invariants, preserved behavior, exact evidence artifacts, and a
  reversible rollback point.
- No PR silently absorbs unrelated dirty worktree changes.
- No host test is presented as evidence of HyperOS mount, Boot, Direct Boot, or reboot safety.

## Do Not Touch during stabilization

- Base CE/DE paths, inode ownership, fscrypt/SELinux labels, or canonical/mirror/Zygote view
  proof.
- Journal-before-side-effect ordering, Registry nonce commit semantics, Gate lease snapshots,
  exact Gate restoration, and `RecoveryRequired`/`Quarantined` fail-closed behavior.
- Existing persistent paths, file modes, schemas, hash chains, rescue tombstones, and slot
  metadata unless a separate migration PR includes old/new/corrupt fixtures and rollback.
- KernelSU startup, containment, rescue, or upgrade Shell behavior outside the dedicated Phase
  5 PRs; never replace independent fallback with a shared failure point without device proof.
- User-provided paths/Shell, non-user0 packages, OverlayFS, system partition writes, permissive
  SELinux, and automatic device reboot.
- Legacy Restore data behavior and Legacy Launcher protocol until Phase 7B has independent
  build/test evidence.

## Recommended next PRs

1. **Add the explicit Runtime-ownership state and transition tests** to the committed Emergency
   Manifest storage foundation. Do not connect a producer until stale ownership and new-boot
   behavior are proven fail-closed.
2. **Add the typed `EmergencyStatus` query, live owner proof, and golden frames.** This enables
   Shell migration without asking Shell to understand internal files.
3. **Integrate the Runtime producer, then migrate the watcher as a separate PR.** The watcher
   must remain in standby and resume containment if ownership proof disappears.
4. **Continue the managed-update vocabulary/schema sequence.** Owner/artifact vocabulary may
   land behavior-neutrally; commands wait for a migration-tested PackageState schema.

The first three PRs are intentionally reversible and do not require a device reboot. Any
unexpected state must remain contained and be investigated as a new repair PR, not papered over
with another special case.
