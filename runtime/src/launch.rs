//! Authenticated, boot-local launch coordination. Account state remains in PackageAggregate.
//! This transport is private to system_server, the Manager's Root helper, and Zygisk's
//! Root companion; it is not an extension of the public account command wire.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::net::Shutdown;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::BINARY_BUILD_ID;
use crate::adapters::FilePackageStore;
use crate::model::{AccountIoKind, AccountIoPhase, AccountIoScope, PackageAggregate, PackageName};
use crate::ports::PackageStore as _;

pub const SOCKET_NAME: &str = "uclone.slices.launch.v1";
pub const PERMIT_EXTRA: &str = "com.uclone.slices.v2.LAUNCH_PERMIT";
const FRAME_LIMIT: u64 = 64 * 1024;
// Transport deadlines, shared with the existing Runtime frame receiver. They do not
// decide when an Android visit ends or when an account should change.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const PROCESS_VIEW_WORKER: &str =
    "/data/adb/modules/uclone-slices-v2/bin/uclone-process-view";

/// The private Runtime view cannot inherit already-installed global account mounts.
/// Rebooting applies the paired module update and provides an untouched init view.
pub fn validate_runtime_namespace() -> io::Result<()> {
    #[cfg(target_os = "android")]
    {
        if fs::read_link("/proc/self/ns/mnt")? == fs::read_link("/proc/1/ns/mnt")?
            || fs::read_to_string("/proc/1/mountinfo")?.contains("/uclone-slices-v2/slots/")
        {
            return Err(refused(
                "a private runtime namespace and completed module reboot are required",
            ));
        }
    }
    Ok(())
}

type Prepare = dyn Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static;

struct ControlPermit(Arc<AtomicUsize>);

impl Drop for ControlPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct Policy {
    package: String,
    uid: u32,
    active: String,
    default: Option<String>,
    ready: bool,
}

#[derive(Debug)]
struct Connection {
    id: u64,
    writer: Arc<Mutex<UnixStream>>,
    ack: u64,
}

#[derive(Debug, Clone, Serialize)]
struct Permit {
    package: String,
    slot: String,
    token: String,
    caller: &'static str,
    #[serde(skip_serializing)]
    ticket: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    connection: Option<Connection>,
    next_connection: u64,
    revision: u64,
    policies: Vec<Policy>,
    permits: BTreeMap<String, Permit>,
    tickets: BTreeMap<String, String>,
    claimed: BTreeSet<String>,
    policies_valid: bool,
    processes: BTreeMap<u32, ProcessRecord>,
    blocked: BTreeSet<String>,
}

/// One daemon owns one instance. All connection/PID/permit state is disposable after boot.
#[derive(Debug)]
pub struct LaunchGate {
    root: PathBuf,
    state: Mutex<State>,
    changed: Condvar,
    workers: Arc<AtomicUsize>,
}

impl LaunchGate {
    pub fn new(root: impl Into<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            root: root.into(),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            workers: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Starts the private listener. The callback shares the public account transaction
    /// lock, but process specialization and cancellation are served independently.
    pub fn start(self: &Arc<Self>, prepare: Arc<Prepare>) -> io::Result<()> {
        let listener = listen_control()?;
        let gate = Arc::clone(self);
        reset_process_registry(
            &self.root,
            fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim(),
        )?;
        let connections = Arc::clone(&self.workers);
        thread::spawn(move || {
            for incoming in listener.incoming() {
                match incoming {
                    Ok(stream) => {
                        // Reject ordinary UIDs before allocating a thread or reading a frame.
                        // Keep the system subscription independent of bounded Root workers.
                        let peer = match authenticate(&stream) {
                            Ok(peer) => peer,
                            Err(_) => continue,
                        };
                        let permit = if peer == Peer::Root {
                            if connections
                                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                                    (count < 32).then_some(count + 1)
                                })
                                .is_err()
                            {
                                continue;
                            }
                            Some(ControlPermit(Arc::clone(&connections)))
                        } else {
                            None
                        };
                        let gate = Arc::clone(&gate);
                        let prepare = Arc::clone(&prepare);
                        thread::spawn(move || {
                            let _permit = permit;
                            if let Err(error) = gate.serve(stream, peer, &prepare) {
                                eprintln!("launch control rejected: {error}");
                            }
                        });
                    }
                    Err(error) => eprintln!("launch control accept failed: {error}"),
                }
            }
        });
        Ok(())
    }

    pub fn refresh(&self) -> io::Result<()> {
        // A fresh daemon has no previous policies yet; the exported discovery still names
        // every enrolled package, so an unreadable one stays enrolled across restarts.
        let mut known = exported_enrollment_uids();
        known.extend(
            self.lock()?
                .policies
                .iter()
                .map(|policy| (policy.package.clone(), policy.uid)),
        );
        let policies = load_policies(&self.root, &known).and_then(|policies| {
            export_policy_discovery(&policies)?;
            Ok(policies)
        });
        let mut state = self.lock()?;
        let old_process_count = state.processes.len();
        state.processes.retain(|_, record| {
            ProcessIdentity::read(record.pid).is_ok_and(|actual| {
                actual.uid == record.uid
                    && actual.started == record.started
                    && actual.namespace == record.namespace
            })
        });
        let processes_changed = old_process_count != state.processes.len();
        match policies {
            Ok(mut policies) => {
                for policy in &mut policies {
                    policy.ready &= !state.blocked.contains(&policy.package);
                }
                if state.policies_valid
                    && state.policies == policies
                    && !processes_changed
                    && state
                        .connection
                        .as_ref()
                        .is_none_or(|connection| connection.ack != 0)
                {
                    return Ok(());
                }
                state.permits.retain(|_, permit| {
                    policies.iter().any(|policy| {
                        policy.package == permit.package
                            && policy.ready
                            && policy.active == permit.slot
                    })
                });
                state.policies = policies;
                state.policies_valid = true;
            }
            Err(error) => {
                state.policies_valid = false;
                state.permits.clear();
                drop(state);
                self.publish()?;
                return Err(error);
            }
        }
        drop(state);
        self.publish()
    }

    /// Root's explicit switch/retry/boot launch must obtain a single-use permit before
    /// calling ActivityManager. The Hook acknowledges it outside WindowManager's lock.
    pub(crate) fn permit_explicit(&self, package: &str, expected_slot: &str) -> io::Result<String> {
        self.issue_permit(package, None, Some(expected_slot))
    }

    /// Called before releasing the account transaction lock, so an intervening switch
    /// cannot turn a completed default preparation into a permit for another account.
    pub fn permit_prepared(&self, package: &str, ticket: &str) -> io::Result<String> {
        self.issue_permit(package, Some(ticket), None)
    }

    pub(crate) fn block_new_processes(&self, package: &str) -> io::Result<()> {
        self.lock()?.blocked.insert(package.to_owned());
        self.refresh()?;
        let (connection, revision) = {
            let state = self.lock()?;
            (
                state
                    .connection
                    .as_ref()
                    .ok_or_else(|| refused("system support disconnected"))?
                    .id,
                state.revision,
            )
        };
        self.await_ack(connection, revision)
    }

    /// The daemon calls this while still holding its account transaction lock. Until
    /// then even an App-Zygote child cannot bind after quiescence was confirmed.
    pub fn finish_transaction(&self) -> io::Result<()> {
        self.lock()?.blocked.clear();
        self.refresh()
    }

    pub fn ticket_live(&self, ticket: &str) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.connection.is_some() && state.tickets.contains_key(ticket))
    }

    pub fn native_ready(&self) -> bool {
        let expected = fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map(|boot| format!("{} {}\n", BINARY_BUILD_ID, boot.trim()));
        expected.is_ok_and(|expected| {
            fs::read_to_string(self.root.join("zygisk-ready"))
                .is_ok_and(|actual| actual == expected)
        })
    }

    pub(crate) fn readiness(&self) -> (bool, bool) {
        let system = self.state.lock().is_ok_and(|state| {
            state.policies_valid
                && state
                    .connection
                    .as_ref()
                    .is_some_and(|connection| connection.ack >= state.revision)
        });
        let process = self.native_ready()
            && Command::new("/data/adb/modules/uclone-slices-v2/bin/uclone-namespace-anchor")
                .arg("--check")
                .output()
                .is_ok_and(|result| result.status.success());
        (system, process)
    }

    fn lock(&self) -> io::Result<std::sync::MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| refused("launch state lock failed"))
    }

    fn publish(&self) -> io::Result<()> {
        let (writer, frame) = {
            let mut state = self.lock()?;
            state.revision = state
                .revision
                .checked_add(1)
                .ok_or_else(|| refused("launch revision exhausted"))?;
            let Some(connection) = &state.connection else {
                return Ok(());
            };
            let frame = state_frame(&state);
            (Arc::clone(&connection.writer), frame)
        };
        let mut writer = writer
            .lock()
            .map_err(|_| refused("launch writer lock failed"))?;
        write_frame(&mut writer, &frame)
    }

    fn issue_permit(
        &self,
        package: &str,
        ticket: Option<&str>,
        expected_slot: Option<&str>,
    ) -> io::Result<String> {
        let aggregate =
            validate_account_for_start(&self.root, &PackageName::new(package).map_err(invalid)?)?;
        if expected_slot.is_some_and(|expected| expected != aggregate.active_slot().as_str()) {
            return Err(refused("launch does not match committed account"));
        }
        self.lock()?.blocked.remove(package);
        self.refresh()?;
        if !self.native_ready() {
            return Err(refused("process support is not active"));
        }
        let token = random_token()?;
        let (connection_id, revision) = {
            let mut state = self.lock()?;
            if !state.policies_valid
                || !state
                    .policies
                    .iter()
                    .any(|policy| policy.package == package && policy.ready)
            {
                return Err(refused("account is not ready"));
            }
            let slot = state
                .policies
                .iter()
                .find(|policy| policy.package == package)
                .ok_or_else(|| refused("account was removed"))?
                .active
                .clone();
            if expected_slot.is_some_and(|expected| expected != slot) {
                return Err(refused("launch does not match committed account"));
            }
            let connection_id = state
                .connection
                .as_ref()
                .ok_or_else(|| refused("system support is not connected"))?
                .id;
            if let Some(ticket) = ticket
                && state.tickets.get(ticket).map(String::as_str) != Some(package)
            {
                return Err(refused("launch was cancelled"));
            }
            let caller = if ticket.is_some() { "manager" } else { "root" };
            state
                .permits
                .retain(|_, permit| permit.package != package || permit.caller != caller);
            state.permits.insert(
                token.clone(),
                Permit {
                    package: package.to_owned(),
                    slot,
                    token: token.clone(),
                    caller,
                    ticket: ticket.map(str::to_owned),
                },
            );
            (connection_id, state.revision + 1)
        };
        if let Err(error) = self
            .publish()
            .and_then(|()| self.await_ack(connection_id, revision))
        {
            self.lock()?.permits.remove(&token);
            let _ = self.publish();
            return Err(error);
        }
        if ticket.is_some_and(|ticket| !self.ticket_live(ticket)) {
            self.lock()?.permits.remove(&token);
            self.publish()?;
            return Err(refused("launch was cancelled"));
        }
        Ok(token)
    }

    fn await_ack(&self, connection_id: u64, revision: u64) -> io::Result<()> {
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        let mut state = self.lock()?;
        loop {
            let connection = state
                .connection
                .as_ref()
                .filter(|connection| connection.id == connection_id)
                .ok_or_else(|| refused("system support disconnected"))?;
            if connection.ack >= revision {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(refused("system support did not acknowledge"));
            }
            state = self
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| refused("launch acknowledgement lock failed"))?
                .0;
        }
    }

    fn cancel(&self, ticket: &str) -> io::Result<()> {
        let mut state = self.lock()?;
        state.tickets.remove(ticket);
        state.claimed.remove(ticket);
        state
            .permits
            .retain(|_, permit| permit.ticket.as_deref() != Some(ticket));
        drop(state);
        self.changed.notify_all();
        self.publish()
    }

    fn serve(
        self: &Arc<Self>,
        mut stream: UnixStream,
        peer: Peer,
        prepare: &Arc<Prepare>,
    ) -> io::Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let first = read_frame(&mut reader)?;
        let command: Control = serde_json::from_str(&first).map_err(invalid)?;
        if peer == Peer::System {
            if !matches!(&command, Control::Subscribe { build_id } if build_id == BINARY_BUILD_ID) {
                return Err(refused("system build mismatch"));
            }
            return self.subscribe(reader);
        }
        let result: io::Result<Value> = match command {
            Control::Status {} => {
                let state = self.lock()?;
                Ok(json!({"ok": {
                    "build_id": BINARY_BUILD_ID,
                    "system_ready": state.policies_valid && state.connection.as_ref()
                        .is_some_and(|connection| connection.ack >= state.revision),
                    "process_ready": self.native_ready(),
                }}))
            }
            Control::Cancel { ticket } => self.cancel(&ticket).map(|()| json!({"ok": {}})),
            Control::Prepare { ticket } => {
                let package = {
                    let mut state = self.lock()?;
                    let package = state.tickets.get(&ticket).cloned();
                    package.filter(|_| state.claimed.insert(ticket.clone()))
                };
                match package {
                    Some(package) => prepare(&package, &ticket)
                        .map_err(refused)
                        .map(|permit| prepared_response(&package, &permit)),
                    None => Err(refused("launch ticket is not live")),
                }
            }
            Control::Specialize {
                pid,
                uid,
                started,
                package,
            } => self
                .specialize(pid, uid, started, &package)
                .map(|()| json!({"ok": {}})),
            _ => Err(refused("command is not available to this peer")),
        };
        match result {
            Ok(response) => write_frame(&mut stream, &response),
            Err(error) => {
                eprintln!("launch operation failed: {error}");
                write_frame(
                    &mut stream,
                    &json!({"error": {"code": "launch_unavailable"}}),
                )
            }
        }
    }

    fn subscribe(self: &Arc<Self>, mut reader: BufReader<UnixStream>) -> io::Result<()> {
        reader.get_ref().set_read_timeout(None)?;
        let id = {
            let mut state = self.lock()?;
            if let Some(previous) = state.connection.take()
                && let Ok(writer) = previous.writer.lock()
            {
                let _ = writer.shutdown(Shutdown::Both);
            }
            state.tickets.clear();
            state.claimed.clear();
            state.permits.clear();
            state.next_connection += 1;
            let id = state.next_connection;
            state.connection = Some(Connection {
                id,
                writer: Arc::new(Mutex::new(reader.get_ref().try_clone()?)),
                ack: 0,
            });
            id
        };
        let result = (|| {
            self.refresh()?;
            loop {
                let frame = read_subscription_frame(&mut reader)?;
                let event: SystemEvent = serde_json::from_str(&frame).map_err(invalid)?;
                if let SystemEvent::Verify {
                    pid,
                    uid,
                    started,
                    ref package,
                } = event
                {
                    if self
                        .workers
                        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                            (count < 32).then_some(count + 1)
                        })
                        .is_ok()
                    {
                        let permit = ControlPermit(Arc::clone(&self.workers));
                        let gate = Arc::clone(self);
                        let package = package.clone();
                        // The subscription reader must remain free to receive the Ack
                        // awaited by specialization. No Android service lock is held.
                        thread::spawn(move || {
                            let _permit = permit;
                            if let Err(error) = gate.specialize(pid, uid, started, &package) {
                                eprintln!("deferred process verification rejected: {error}");
                            }
                        });
                    }
                    continue;
                }
                let mut state = self.lock()?;
                if state
                    .connection
                    .as_ref()
                    .is_none_or(|connection| connection.id != id)
                {
                    return Err(refused("system connection was replaced"));
                }
                match event {
                    SystemEvent::Ack { revision } => {
                        if revision > state.revision {
                            return Err(refused("invalid revision"));
                        }
                        if let Some(connection) = state.connection.as_mut() {
                            connection.ack = connection.ack.max(revision);
                        }
                    }
                    SystemEvent::Begin { ticket, package } => {
                        validate_token(&ticket)?;
                        // A Begin can race a switch that just published ready=false, or an
                        // unenrollment the system has not applied yet. Refusing only this
                        // ticket keeps every other permit, ticket and pending verification.
                        // The preparation step then finds no live ticket and fails alone.
                        if !state.policies_valid
                            || state.tickets.contains_key(&ticket)
                            || !state
                                .policies
                                .iter()
                                .any(|policy| policy.package == package && policy.ready)
                        {
                            eprintln!("op=launch_begin package={package} refused=not_ready");
                            continue;
                        }
                        state.tickets.insert(ticket, package);
                    }
                    SystemEvent::Cancel { ticket } => {
                        state.tickets.remove(&ticket);
                        state.claimed.remove(&ticket);
                        state
                            .permits
                            .retain(|_, permit| permit.ticket.as_deref() != Some(&ticket));
                    }
                    SystemEvent::Consumed { token } => {
                        if let Some(permit) = state.permits.remove(&token)
                            && let Some(ticket) = permit.ticket
                        {
                            state.tickets.remove(&ticket);
                            state.claimed.remove(&ticket);
                        }
                    }
                    SystemEvent::Verify { .. } => unreachable!(),
                }
                drop(state);
                self.changed.notify_all();
            }
        })();
        let mut state = self.lock()?;
        if state
            .connection
            .as_ref()
            .is_some_and(|connection| connection.id == id)
        {
            state.connection = None;
            state.tickets.clear();
            state.claimed.clear();
            state.permits.clear();
        }
        drop(state);
        self.changed.notify_all();
        result
    }

    fn specialize(&self, pid: u32, uid: u32, started: u64, package: &str) -> io::Result<()> {
        let support_ready = {
            let state = self.lock()?;
            state.policies_valid && state.connection.is_some() && !state.blocked.contains(package)
        };
        if !self.native_ready() || !support_ready {
            return Err(refused("launch support is not ready"));
        }
        let package = PackageName::new(package).map_err(invalid)?;
        let store = FilePackageStore::open(&self.root).map_err(invalid)?;
        let aggregate = validate_account_for_start(&self.root, &package)?;
        let (apk_path, apk_device, apk_inode) = aggregate.identity().apk_stamp();
        let apk = fs::metadata(apk_path)?;
        if apk.dev() != apk_device || apk.ino() != apk_inode {
            return Err(refused("installed APK identity changed"));
        }
        let slot = aggregate.active_slot().as_str();
        let process = ProcessIdentity::read(pid)?;
        if process.started != started || process.uid != uid {
            return Err(refused("process changed"));
        }
        let inherited = validate_process_family(&self.root, &aggregate, &process)?;
        let mut record = ProcessRecord {
            pid,
            uid,
            started,
            package: package.as_str().to_owned(),
            package_uid: aggregate.identity().uid(),
            slot: slot.to_owned(),
            namespace: process.namespace.clone(),
            verified: false,
        };
        // Includes an isolated UID in stop confirmation while its Root mount worker
        // is in progress; it cannot authorize any parent/child view yet.
        save_process_record(&self.root, &record)?;
        let uid_text = uid.to_string();
        let pid_text = pid.to_string();
        let started_text = started.to_string();
        let mut command = Command::new(PROCESS_VIEW_WORKER);
        command.args([
            &pid_text,
            &uid_text,
            &started_text,
            package.as_str(),
            slot,
            "--install",
        ]);
        if inherited {
            command.arg("--verify-inherited");
        }
        if uid != aggregate.identity().uid() {
            command.arg("--isolated");
        }
        let output = command.output()?;
        if !output.status.success() {
            // The packaged worker emits only a fixed stage and errno. Preserve that
            // cause in the Runtime log instead of hiding it behind the public error.
            let detail = String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(256)
                .collect::<String>();
            eprintln!("process view worker rejected: {}", detail.trim());
            return Err(refused("process CE/DE view was not installed"));
        }
        // A native request is already under the App UID. A transaction either sees and
        // kills it, or its persisted intent makes this second check reject it.
        let current = store
            .load(&package)
            .map_err(invalid)?
            .ok_or_else(|| refused("package was removed"))?;
        if current != aggregate
            || !account_ready_to_execute(&store, &current)?
            || ProcessIdentity::read(pid)? != process
            || self.lock()?.blocked.contains(package.as_str())
        {
            return Err(refused("account or process changed during specialization"));
        }
        record.verified = true;
        save_process_record(&self.root, &record)?;
        self.refresh()?;
        let (connection, revision) = {
            let mut state = self.lock()?;
            let connection = state
                .connection
                .as_ref()
                .ok_or_else(|| refused("system support disconnected"))?
                .id;
            state.processes.insert(pid, record);
            (connection, state.revision + 1)
        };
        self.publish()?;
        self.await_ack(connection, revision)
    }
}

fn state_frame(state: &State) -> Value {
    json!({
        "type": "state", "build_id": BINARY_BUILD_ID,
        "revision": state.revision, "valid": state.policies_valid,
        "packages": state.policies,
        "permits": state.permits.values().collect::<Vec<_>>(),
        "processes": state.processes.values().collect::<Vec<_>>(),
    })
}

fn prepared_response(package: &str, permit: &str) -> Value {
    json!({"ok": {"package": package, "permit": permit, "permit_extra": PERMIT_EXTRA}})
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Subscribe {
        build_id: String,
    },
    Status {},
    Prepare {
        ticket: String,
    },
    Cancel {
        ticket: String,
    },
    Specialize {
        pid: u32,
        uid: u32,
        started: u64,
        package: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum SystemEvent {
    Ack {
        revision: u64,
    },
    Begin {
        ticket: String,
        package: String,
    },
    Cancel {
        ticket: String,
    },
    Consumed {
        token: String,
    },
    Verify {
        pid: u32,
        uid: u32,
        started: u64,
        package: String,
    },
}

/// Only an unreadable store invalidates every policy. One package whose records cannot be
/// read stays enrolled but not ready under its last known UID, so it keeps failing closed
/// while every other package can still launch. Without a known UID it is left out and logged.
fn load_policies(root: &Path, known: &BTreeMap<String, u32>) -> io::Result<Vec<Policy>> {
    let store = FilePackageStore::open(root).map_err(invalid)?;
    let mut policies = Vec::new();
    for package in store.list().map_err(invalid)? {
        match load_policy(&store, &package) {
            Ok(policy) => policies.push(policy),
            Err(error) => {
                eprintln!("op=load_policies package={package} error={error}");
                if let Some(&uid) = known.get(package.as_str()) {
                    policies.push(Policy {
                        package: package.as_str().to_owned(),
                        uid,
                        active: String::new(),
                        default: None,
                        ready: false,
                    });
                }
            }
        }
    }
    Ok(policies)
}

fn load_policy(store: &FilePackageStore, package: &PackageName) -> io::Result<Policy> {
    let aggregate = store
        .load(package)
        .map_err(invalid)?
        .ok_or_else(|| refused("package changed while reading policy"))?;
    aggregate.validate().map_err(invalid)?;
    Ok(Policy {
        package: package.as_str().to_owned(),
        uid: aggregate.identity().uid(),
        active: aggregate.active_slot().as_str().to_owned(),
        default: aggregate
            .default_slot()
            .map(|slot| slot.as_str().to_owned()),
        ready: account_ready_to_execute(store, &aggregate)?,
    })
}

pub(crate) fn validate_account_for_start(
    root: &Path,
    package: &PackageName,
) -> io::Result<PackageAggregate> {
    let store = FilePackageStore::open(root).map_err(invalid)?;
    let aggregate = store
        .load(package)
        .map_err(invalid)?
        .ok_or_else(|| refused("package is not enrolled"))?;
    if !account_ready_to_execute(&store, &aggregate)? {
        return Err(refused("account transaction prevents process start"));
    }
    Ok(aggregate)
}

fn account_ready_to_execute(
    store: &FilePackageStore,
    aggregate: &PackageAggregate,
) -> io::Result<bool> {
    let package = aggregate.package();
    if !aggregate.is_ready()
        || store
            .load_rebind_intent(package)
            .map_err(invalid)?
            .is_some()
        || store.load_trash_intent(package).map_err(invalid)?.is_some()
    {
        return Ok(false);
    }
    let Some(intent) = store.load_account_io_intent(package).map_err(invalid)? else {
        return Ok(true);
    };
    intent.validate().map_err(invalid)?;
    if intent.maintenance_active {
        return Ok(false);
    }
    match intent.kind {
        AccountIoKind::Backup => Ok(matches!(
            intent.phase,
            AccountIoPhase::Preparing | AccountIoPhase::Ready
        ) && matches!(intent.scope, Some(AccountIoScope::Account { ref slot }) if
                !slot.is_base() && slot != aggregate.active_slot())),
        AccountIoKind::Restore => Ok(intent.phase == AccountIoPhase::Ready),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessIdentity {
    pub(crate) pid: u32,
    pub(crate) uid: u32,
    pub(crate) started: u64,
    parent: u32,
    namespace: String,
}

impl ProcessIdentity {
    fn read(pid: u32) -> io::Result<Self> {
        Self::read_from(&PathBuf::from(format!("/proc/{pid}")))
    }

    pub(crate) fn read_from(root: &Path) -> io::Result<Self> {
        let pid = root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| refused("invalid process"))?
            .parse::<u32>()
            .map_err(invalid)?;
        if pid <= 1 {
            return Err(refused("invalid process"));
        }
        let stat = fs::read_to_string(root.join("stat"))?;
        let fields = stat
            .rsplit_once(')')
            .ok_or_else(|| refused("invalid proc stat"))?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        let parent = fields
            .get(1)
            .ok_or_else(|| refused("missing parent"))?
            .parse()
            .map_err(invalid)?;
        let started = fields
            .get(19)
            .ok_or_else(|| refused("missing start time"))?
            .parse()
            .map_err(invalid)?;
        let status = fs::read_to_string(root.join("status"))?;
        let uids = status
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .ok_or_else(|| refused("missing UID"))?
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if uids.len() != 4 || uids.iter().any(|uid| uid != &uids[0]) {
            return Err(refused("process UID is not specialized"));
        }
        let namespace = fs::read_link(root.join("ns/mnt"))?
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            pid,
            uid: uids[0],
            started,
            parent,
            namespace,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessRecord {
    pid: u32,
    uid: u32,
    started: u64,
    package: String,
    package_uid: u32,
    slot: String,
    namespace: String,
    verified: bool,
}

fn validate_process_family(
    root: &Path,
    aggregate: &PackageAggregate,
    process: &ProcessIdentity,
) -> io::Result<bool> {
    let init = fs::read_link("/proc/1/ns/mnt")?
        .to_string_lossy()
        .into_owned();
    let current = fs::read_link("/proc/self/ns/mnt")?
        .to_string_lossy()
        .into_owned();
    if process.namespace == init || process.namespace == current {
        return Err(refused("process does not own a data namespace"));
    }
    let parent = ProcessIdentity::read(process.parent)?;
    let inherited = parent.namespace == process.namespace;
    if process.uid == aggregate.identity().uid() && !inherited {
        return Ok(false);
    }
    // Only the documented user0 isolated/App-Zygote UID range can inherit an App's
    // already verified view. Never grant a system/shared/profile UID access to it.
    if process.uid != aggregate.identity().uid() && !(90000..=99999).contains(&process.uid) {
        return Err(refused("unexpected process UID"));
    }
    if !inherited && parent.uid == 0 && (90000..=99999).contains(&process.uid) {
        let proc = PathBuf::from(format!("/proc/{}", parent.pid));
        let domain = fs::read_to_string(proc.join("attr/current"))?;
        let command = fs::read(proc.join("cmdline"))?;
        if domain.trim_end_matches(['\0', '\n']) == "u:r:zygote:s0"
            && command
                .split(|byte| *byte == 0)
                .next()
                .is_some_and(|name| name == b"zygote" || name == b"zygote64")
        {
            return Ok(false);
        }
    }
    let record: ProcessRecord = serde_json::from_str(&fs::read_to_string(
        root.join("processes")
            .join(format!("{}-{}.json", process.parent, parent.started)),
    )?)
    .map_err(invalid)?;
    if !record.verified
        || record.started != parent.started
        || record.uid != parent.uid
        || record.package != aggregate.package().as_str()
        || record.package_uid != aggregate.identity().uid()
        || record.slot != aggregate.active_slot().as_str()
        || record.namespace != parent.namespace
    {
        return Err(refused("process parent is not an active account process"));
    }
    Ok(inherited)
}

fn save_process_record(root: &Path, record: &ProcessRecord) -> io::Result<()> {
    let directory = root.join("processes");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let target = directory.join(format!("{}-{}.json", record.pid, record.started));
    let temporary = directory.join(format!("{}-{}.pending", record.pid, record.started));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(serde_json::to_string(record).map_err(invalid)?.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn reset_process_registry(root: &Path, boot: &str) -> io::Result<()> {
    let marker = root.join("processes-boot");
    match fs::read_to_string(&marker) {
        Ok(previous) if previous == boot => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match fs::remove_dir_all(root.join("processes")) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let temporary = root.join("processes-boot.pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(boot.as_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, marker)
}

/// Adds verified isolated descendants to the ordinary UID scan. PID reuse never
/// makes a saved record authorize or keep alive an unrelated process.
pub(crate) fn tracked_processes(root: &Path, uid: u32) -> io::Result<Vec<PathBuf>> {
    let directory = root.join("processes");
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut processes = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_none_or(|extension| extension != "json")
        {
            continue;
        }
        let contents = match fs::read_to_string(entry.path()) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let record: ProcessRecord = serde_json::from_str(&contents).map_err(invalid)?;
        match ProcessIdentity::read(record.pid) {
            Ok(actual)
                if actual.started == record.started
                    && actual.uid == record.uid
                    && actual.namespace == record.namespace =>
            {
                if record.package_uid == uid {
                    processes.push(PathBuf::from(format!("/proc/{}", record.pid)));
                }
            }
            Ok(_) => {
                remove_process_record(&entry.path())?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                remove_process_record(&entry.path())?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(processes)
}

fn remove_process_record(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Peer {
    Root,
    System,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn authenticate(stream: &UnixStream) -> io::Result<Peer> {
    let credentials = rustix::net::sockopt::socket_peercred(stream)?;
    if credentials.uid.as_raw() == 0 {
        return Ok(Peer::Root);
    }
    if credentials.uid.as_raw() != 1000 {
        return Err(refused("unauthorized launch peer"));
    }
    let pid = credentials.pid.as_raw_nonzero().get();
    let domain = fs::read_to_string(format!("/proc/{pid}/attr/current"))?;
    let command = fs::read(format!("/proc/{pid}/cmdline"))?;
    if credentials.uid.as_raw() == 1000
        && domain.trim_end_matches(['\0', '\n']) == "u:r:system_server:s0"
        && command.split(|byte| *byte == 0).next() == Some(b"system_server".as_slice())
    {
        Ok(Peer::System)
    } else {
        Err(refused("unauthorized launch peer"))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn authenticate(_stream: &UnixStream) -> io::Result<Peer> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Android launch peer required",
    ))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn listen_control() -> io::Result<UnixListener> {
    use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
    let socket = rustix::net::socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::CLOEXEC,
        None,
    )?;
    rustix::net::bind(
        &socket,
        &SocketAddrUnix::new_abstract_name(SOCKET_NAME.as_bytes())?,
    )?;
    rustix::net::listen(&socket, 32)?;
    Ok(UnixListener::from(socket))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn listen_control() -> io::Result<UnixListener> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Android launch socket required",
    ))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn connect_control() -> io::Result<UnixStream> {
    use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
    let socket = rustix::net::socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::CLOEXEC,
        None,
    )?;
    rustix::net::connect(
        &socket,
        &SocketAddrUnix::new_abstract_name(SOCKET_NAME.as_bytes())?,
    )?;
    Ok(UnixStream::from(socket))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub fn connect_control() -> io::Result<UnixStream> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Android launch socket required",
    ))
}

fn read_subscription_frame(reader: &mut impl io::BufRead) -> io::Result<String> {
    let mut frame = String::new();
    let count = reader.take(FRAME_LIMIT + 1).read_line(&mut frame)?;
    if count == 0 || count as u64 > FRAME_LIMIT || !frame.ends_with('\n') {
        return Err(refused("invalid launch frame"));
    }
    Ok(frame)
}

fn read_frame(reader: &mut BufReader<UnixStream>) -> io::Result<String> {
    let deadline = Instant::now() + CONTROL_TIMEOUT;
    let mut frame = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "launch frame deadline",
            ));
        }
        reader.get_ref().set_read_timeout(Some(remaining))?;
        let buffered = reader.fill_buf()?;
        if buffered.is_empty() {
            return Err(refused("launch frame ended early"));
        }
        let count = buffered
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffered.len(), |index| index + 1);
        if frame.len() as u64 + count as u64 > FRAME_LIMIT {
            return Err(refused("launch frame is too large"));
        }
        let complete = buffered[count - 1] == b'\n';
        frame.extend_from_slice(&buffered[..count]);
        reader.consume(count);
        if complete {
            return String::from_utf8(frame).map_err(invalid);
        }
    }
}

fn write_frame(stream: &mut UnixStream, value: &Value) -> io::Result<()> {
    stream.set_write_timeout(Some(CONTROL_TIMEOUT))?;
    let mut frame = serde_json::to_vec(value).map_err(invalid)?;
    frame.push(b'\n');
    if frame.len() as u64 > FRAME_LIMIT {
        return Err(refused("launch state exceeds frame limit"));
    }
    stream.write_all(&frame)
}

fn random_token() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn validate_token(token: &str) -> io::Result<()> {
    (token.len() == 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    .then_some(())
    .ok_or_else(|| refused("invalid launch token"))
}

/// Early-boot discovery contains enrollment only, never an active/default account or
/// authority to launch. A missing live connection still rejects instrumented starts.
pub fn export_enrollment(root: &Path) -> io::Result<()> {
    export_policy_discovery(&load_policies(root, &exported_enrollment_uids())?)
}

/// UIDs from the previous discovery manifest, so a package whose records became unreadable
/// stays enrolled across a reboot instead of silently starting unmanaged.
#[cfg(target_os = "android")]
fn exported_enrollment_uids() -> BTreeMap<String, u32> {
    let Ok(contents) = fs::read("/data/system/uclone-slices-v2/enrollment.json") else {
        return BTreeMap::new();
    };
    let Ok(manifest) = serde_json::from_slice::<Value>(&contents) else {
        return BTreeMap::new();
    };
    manifest["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let package = row["package"].as_str()?;
            let uid = u32::try_from(row["uid"].as_u64()?).ok()?;
            Some((package.to_owned(), uid))
        })
        .collect()
}

#[cfg(not(target_os = "android"))]
fn exported_enrollment_uids() -> BTreeMap<String, u32> {
    BTreeMap::new()
}

#[cfg(target_os = "android")]
fn export_policy_discovery(policies: &[Policy]) -> io::Result<()> {
    let directory = Path::new("/data/system/uclone-slices-v2");
    match fs::symlink_metadata(directory) {
        Ok(metadata)
            if metadata.is_dir() && !metadata.file_type().is_symlink() && metadata.uid() == 0 => {}
        Ok(_) => return Err(refused("enrollment discovery directory is unsafe")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(directory)?,
        Err(error) => return Err(error),
    }
    std::os::unix::fs::chown(directory, Some(0), Some(1000))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o750))?;
    let enrollments = policies
        .iter()
        .map(|policy| json!({"package": policy.package, "uid": policy.uid}))
        .collect::<Vec<_>>();
    // system_server trusts discovery only from this boot, so a module that was disabled or
    // removed (and therefore never re-exported) cannot keep blocking its former Apps.
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let manifest = serde_json::to_vec(&json!({
        "build_id": BINARY_BUILD_ID,
        "boot_id": boot_id.trim(),
        "packages": enrollments,
    }))
    .map_err(invalid)?;
    let path = directory.join("enrollment.json");
    if fs::read(&path).ok().as_deref() != Some(manifest.as_slice()) {
        let temporary = directory.join("enrollment.pending");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o640)
            .open(&temporary)?;
        file.write_all(&manifest)?;
        file.sync_all()?;
        std::os::unix::fs::chown(&temporary, Some(0), Some(1000))?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o640))?;
        fs::rename(temporary, &path)?;
        File::open(directory)?.sync_all()?;
    }
    let module_manifest = format!(
        "{}\n{}",
        BINARY_BUILD_ID,
        policies
            .iter()
            .map(|policy| format!("{} {}\n", policy.package, policy.uid))
            .collect::<String>()
    );
    let module = Path::new("/data/adb/modules/uclone-slices-v2");
    let module_path = module.join("enrollment.list");
    if fs::read_to_string(&module_path).ok().as_deref() != Some(module_manifest.as_str()) {
        let temporary = module.join("enrollment.pending");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(module_manifest.as_bytes())?;
        file.sync_all()?;
        fs::rename(temporary, module_path)?;
        File::open(module)?.sync_all()?;
    }
    for path in [directory.to_path_buf(), path] {
        if !Command::new("/system/bin/chcon")
            .args(["u:object_r:system_data_file:s0"])
            .arg(path)
            .status()?
            .success()
        {
            return Err(refused("enrollment discovery context failed"));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn export_policy_discovery(_policies: &[Policy]) -> io::Result<()> {
    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
fn refused(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message.into())
}

#[cfg(test)]
mod tests;
