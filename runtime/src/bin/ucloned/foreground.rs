//! System task observations are inputs, never a second account store. Account mutation stays
//! behind the daemon's transaction lock and the Runtime's existing activation use case.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uclone_slices_runtime::{production, validation::is_package_name};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Activity {
    token: String,
    component: String,
    package: String,
    task: i32,
    pid: Option<u32>,
    caller: Option<String>,
    result_task: Option<i32>,
    home: bool,
    resumed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Snapshot {
    locked: bool,
    foreground: Option<Activity>,
    #[serde(skip)]
    activities: Vec<Activity>,
}

#[derive(Debug, Clone)]
struct Session {
    since: u128,
    identities: BTreeSet<(i32, u32)>,
    tasks: BTreeSet<i32>,
    packages: BTreeSet<String>,
    continuation: bool,
}

/// Exceptions exist only for successful Slice activations and only for their current visit.
#[derive(Debug, Default)]
pub(super) struct Sessions {
    manual: BTreeMap<String, Session>,
    last_entry: Option<(String, i32, u32)>,
    latest_event: u128,
    event_locked: bool,
    failed_visits: BTreeMap<String, u128>,
}

impl Sessions {
    pub(super) fn completed_request(&mut self, line: &str, response: &str) {
        let Ok(request) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        let Ok(response) = serde_json::from_str::<serde_json::Value>(response) else {
            return;
        };
        let Some(package) = response["ok"]["package"]["package"].as_str() else {
            return;
        };
        if request["op"] != "activate_slot"
            || request["package"].as_str() != Some(package)
            || request["slot"] != response["ok"]["package"]["active_slot"]
        {
            return;
        }
        let snapshot = read_snapshot().ok();
        self.grant(package, now_micros(), snapshot.as_ref());
    }

    fn grant(&mut self, package: &str, since: u128, snapshot: Option<&Snapshot>) {
        self.failed_visits.remove(package);
        let anchor = snapshot
            .and_then(|snapshot| snapshot.foreground.as_ref())
            .filter(|activity| activity.package == package)
            .and_then(|activity| activity.pid.map(|pid| (activity.task, pid)));
        self.manual.insert(
            package.to_owned(),
            Session {
                since,
                identities: anchor.into_iter().collect(),
                tasks: anchor.into_iter().map(|(task, _pid)| task).collect(),
                packages: BTreeSet::from([package.to_owned()]),
                continuation: snapshot.is_some_and(|snapshot| snapshot.locked),
            },
        );
        // A launch while the screen is locked must not inherit an earlier lock event forever.
        if let Some(snapshot) = snapshot {
            self.event_locked = snapshot.locked;
        }
    }

    fn visit(&mut self, activity: &Activity, timestamp: u128) {
        self.manual.retain(|package, session| {
            // The launch generates system events before the synchronous RPC completes. A
            // buffered Home event from before the grant must not cancel the new exception.
            if timestamp < session.since {
                return true;
            }
            if activity.package == *package {
                let Some(pid) = activity.pid else { return true };
                let identity = (activity.task, pid);
                // Internal Activities can use other processes or tasks. A temporary
                // external flow or screen lock can also outlive the original process.
                let known_origin = activity
                    .caller
                    .as_ref()
                    .is_some_and(|caller| session.packages.contains(caller))
                    || activity
                        .result_task
                        .is_some_and(|task| session.tasks.contains(&task));
                if session.identities.is_empty()
                    || session.identities.contains(&identity)
                    || known_origin
                    || session.continuation
                {
                    session.identities.insert(identity);
                    session.tasks.insert(activity.task);
                    session.continuation = false;
                    true
                } else {
                    false
                }
            } else if activity.home {
                false
            } else if session.tasks.contains(&activity.task)
                || activity
                    .result_task
                    .is_some_and(|task| session.tasks.contains(&task))
                || activity
                    .caller
                    .as_ref()
                    .is_some_and(|caller| session.packages.contains(caller))
            {
                // Covers same-task camera/picker activities and new-task payment chains.
                // A later Home or unrelated task removes the whole chain.
                session.tasks.insert(activity.task);
                session.packages.insert(activity.package.clone());
                session.continuation = true;
                true
            } else {
                session.identities.is_empty() && activity.package == "com.uclone.slices.v2"
            }
        });
    }

    fn observe(&mut self, event: &Event, snapshot: &Snapshot) -> Option<String> {
        let outdated = event.timestamp < self.latest_event;
        self.latest_event = self.latest_event.max(event.timestamp);
        if let EventKind::Keyguard(locked) = event.kind
            && !outdated
        {
            self.event_locked = locked;
        }
        if snapshot.locked || self.event_locked {
            for session in self.manual.values_mut() {
                if snapshot.locked || event.timestamp >= session.since {
                    session.continuation = true;
                }
            }
            return None;
        }
        let confirms_entry = !outdated
            && snapshot
                .foreground
                .as_ref()
                .is_some_and(|activity| match &event.kind {
                    EventKind::Resumed(component) => activity.component == *component,
                    EventKind::TaskFront(task) => activity.task == *task,
                    EventKind::Keyguard(_) => false,
                });
        // Bringing a task forward can precede its resume, or be cancelled by a gesture.
        if matches!(event.kind, EventKind::TaskFront(_)) && !confirms_entry {
            return None;
        }
        // A recorded Home resume ends the visit even if dumpsys still shows the app.
        // It never proves that the app has been entered again: wait for that app's event.
        let historical = match &event.kind {
            EventKind::Resumed(component) => snapshot
                .activities
                .iter()
                .find(|activity| activity.component == *component),
            EventKind::TaskFront(task) => snapshot
                .activities
                .iter()
                .find(|activity| activity.task == *task),
            EventKind::Keyguard(_) => None,
        };
        if let Some(activity) = historical {
            self.failed_visits
                .retain(|package, since| event.timestamp <= *since || activity.package == *package);
            self.visit(activity, event.timestamp);
            if self
                .last_entry
                .as_ref()
                .is_some_and(|(package, task, _pid)| {
                    package != &activity.package || *task != activity.task
                })
            {
                self.last_entry = None;
            }
        }
        if !confirms_entry {
            return None;
        }
        let activity = snapshot.foreground.as_ref()?;
        let pid = activity.pid?;
        self.visit(activity, now_micros());
        let identity = (activity.package.clone(), activity.task, pid);
        let new_entry = self.last_entry.as_ref() != Some(&identity);
        self.last_entry = Some(identity);
        (new_entry
            && !activity.home
            && !self.manual.contains_key(&activity.package)
            && !self.failed_visits.contains_key(&activity.package))
        .then(|| activity.package.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Event {
    timestamp: u128,
    kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EventKind {
    Resumed(String),
    TaskFront(i32),
    Keyguard(bool),
}

fn parse_event(line: &str) -> Option<Event> {
    let mut fields = line.split_whitespace();
    let timestamp = fields.next()?;
    let (seconds, micros) = timestamp.split_once('.')?;
    let micros = micros.get(..6)?;
    let timestamp = seconds.parse::<u128>().ok()? * 1_000_000 + micros.parse::<u128>().ok()?;
    // These tags are emitted by system_server. Untrusted apps can also write event tags;
    // their messages must not expire someone else's manual foreground session.
    if fields.next()? != "1000" {
        return None;
    }
    let _pid = fields.next()?.parse::<u32>().ok()?;
    let _tid = fields.next()?.parse::<u32>().ok()?;
    let _level = fields.next()?;
    let tag = fields.next()?.trim_end_matches(':');
    let body = line.split_once('[')?.1.split_once(']')?.0;
    let parts: Vec<_> = body.split(',').map(str::trim).collect();
    if parts.first().copied() != Some("0") {
        return None;
    }
    let value = *parts.get(1)?;
    let kind = match tag {
        "wm_set_resumed_activity" => EventKind::Resumed(normalize_component(value)?),
        "wm_task_to_front" if parts.get(2).copied() == Some("0") => {
            EventKind::TaskFront(value.parse().ok()?)
        }
        "wm_set_keyguard_shown" => match value {
            "0" => EventKind::Keyguard(false),
            "1" => EventKind::Keyguard(true),
            _ => return None,
        },
        _ => return None,
    };
    Some(Event { timestamp, kind })
}

fn now_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_micros())
}

fn normalize_component(component: &str) -> Option<String> {
    let (package, class) = component.split_once('/')?;
    if !is_package_name(package) || class.is_empty() {
        return None;
    }
    Some(if class.starts_with('.') {
        format!("{package}/{package}{class}")
    } else {
        component.to_owned()
    })
}

fn activity_header(line: &str, home: bool) -> Option<Activity> {
    let record = line.split_once("ActivityRecord{")?.1.split_once('}')?.0;
    let mut fields = record.split_whitespace();
    let token = fields.next()?.to_owned();
    if fields.next()? != "u0" {
        return None;
    }
    let component = normalize_component(fields.next()?)?;
    let package = component.split_once('/')?.0.to_owned();
    let task = fields.find_map(|field| field.strip_prefix('t')?.parse::<i32>().ok())?;
    Some(Activity {
        token,
        component,
        package,
        task,
        pid: None,
        caller: None,
        result_task: None,
        home,
        resumed: false,
    })
}

fn field_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_once(key)?.1.split_whitespace().next()
}

fn parse_snapshot(dump: &str) -> Result<Snapshot, std::io::Error> {
    let locked = dump
        .lines()
        .find_map(|line| match field_value(line, "mKeyguardShowing=")? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
        .ok_or_else(|| std::io::Error::other("missing default-display keyguard state"))?;
    let mut activities = Vec::new();
    let mut current: Option<Activity> = None;
    let mut primary_display = false;
    let mut home = false;
    let mut resumed_token = None;
    for raw in dump.lines() {
        let line = raw.trim();
        if line.starts_with("Display #") {
            if let Some(activity) = current.take() {
                activities.push(activity);
            }
            primary_display = line.starts_with("Display #0 ");
            home = false;
        }
        if !primary_display {
            continue;
        }
        if line.starts_with("topResumedActivity=") || line.starts_with("mResumedActivity:") {
            resumed_token = activity_header(line, false).map(|activity| activity.token);
        }
        if line.starts_with('*') && line.contains("Task{") && !line.contains("ActivityRecord{") {
            if let Some(activity) = current.take() {
                activities.push(activity);
            }
            home = line.contains("type=home");
        } else if line.starts_with('*') && line.contains("ActivityRecord{") {
            if let Some(activity) = current.take() {
                activities.push(activity);
            }
            current = activity_header(line, home);
        } else if let Some(activity) = current.as_mut() {
            if line.starts_with("app=ProcessRecord{") {
                activity.pid = line.split_once('{').and_then(|(_, rest)| {
                    rest.split_whitespace()
                        .nth(1)?
                        .split_once(':')?
                        .0
                        .parse()
                        .ok()
                });
            } else if line.starts_with("launchedFromUid=") {
                activity.caller = field_value(line, "launchedFromPackage=")
                    .filter(|package| is_package_name(package))
                    .map(str::to_owned);
            } else if line.starts_with("resultTo=ActivityRecord{") {
                activity.result_task = activity_header(line, false).map(|target| target.task);
            } else if line.starts_with("state=") {
                activity.resumed = field_value(line, "state=") == Some("RESUMED");
            }
        }
    }
    if let Some(activity) = current {
        activities.push(activity);
    }
    let foreground = activities
        .iter()
        .find(|activity| {
            activity.resumed
                && resumed_token
                    .as_ref()
                    .is_none_or(|token| token == &activity.token)
        })
        .cloned();
    Ok(Snapshot {
        locked,
        foreground,
        activities,
    })
}

pub(super) fn read_snapshot() -> Result<Snapshot, std::io::Error> {
    let output = Command::new("/system/bin/dumpsys")
        .args(["activity", "activities"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("activity observation failed"));
    }
    parse_snapshot(&String::from_utf8_lossy(&output.stdout))
}

pub(super) fn observe_once() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&read_snapshot()?)?);
    Ok(())
}

pub(super) fn spawn(
    root: PathBuf,
    build_id: String,
    transaction_lock: Arc<Mutex<()>>,
    sessions: Arc<Mutex<Sessions>>,
) {
    drop(thread::spawn(move || {
        loop {
            if let Err(error) = monitor(
                root.clone(),
                build_id.clone(),
                Arc::clone(&transaction_lock),
                Arc::clone(&sessions),
            ) {
                eprintln!("ucloned foreground observation disconnected: {error}; reconnecting");
            }
            if let Ok(mut sessions) = sessions.lock() {
                *sessions = Sessions::default();
            }
            // Supervisor backoff only, never a deadline for an account or background visit.
            thread::sleep(Duration::from_secs(1));
        }
    }));
}

fn monitor(
    root: PathBuf,
    build_id: String,
    transaction_lock: Arc<Mutex<()>>,
    sessions: Arc<Mutex<Sessions>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new("/system/bin/logcat")
        .args([
            "-b",
            "events",
            "-T",
            "0",
            "-v",
            "epoch,uid,usec",
            "wm_set_resumed_activity:I",
            "wm_task_to_front:I",
            "wm_set_keyguard_shown:I",
            "*:S",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no event stream"))?;
    eprintln!("ucloned foreground observation connected");
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        {
            let _transaction = transaction_lock
                .lock()
                .map_err(|_error| std::io::Error::other("foreground transaction lock poisoned"))?;
            if let Ok(snapshot) = read_snapshot()
                && let Some(activity) = snapshot.foreground.as_ref()
            {
                let event = Event {
                    timestamp: now_micros(),
                    kind: EventKind::Resumed(activity.component.clone()),
                };
                apply_observation(&root, &build_id, &sessions, &event, &snapshot)?;
            }
        }
        for line in BufReader::new(output).lines() {
            let Some(event) = parse_event(&line?) else {
                continue;
            };
            let _transaction = transaction_lock
                .lock()
                .map_err(|_error| std::io::Error::other("foreground transaction lock poisoned"))?;
            let snapshot = match read_snapshot() {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    eprintln!("ucloned foreground observation unavailable: {error}");
                    continue;
                }
            };
            apply_observation(&root, &build_id, &sessions, &event, &snapshot)?;
        }
        Err(std::io::Error::other("system event stream closed").into())
    })();
    let _kill = child.kill();
    let _wait = child.wait();
    result
}

fn apply_observation(
    root: &Path,
    build_id: &str,
    sessions: &Mutex<Sessions>,
    event: &Event,
    snapshot: &Snapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = sessions
        .lock()
        .map_err(|_error| std::io::Error::other("foreground session lock poisoned"))?
        .observe(event, snapshot);
    if let Some(package) = target {
        let Some(expected) = snapshot.foreground.as_ref() else {
            return Ok(());
        };
        let still_foreground = || {
            read_snapshot().is_ok_and(|live| {
                !live.locked
                    && live.foreground.as_ref().is_some_and(|current| {
                        current.component == expected.component
                            && current.task == expected.task
                            && current.pid == expected.pid
                    })
            })
        };
        match production(root, build_id)?.activate_default_account(&package, still_foreground) {
            Ok(true) => eprintln!("op=foreground_default package={package} result=switched"),
            Ok(false) => {}
            Err(error) => {
                // A rollback relaunch must not become an automatic force-stop loop.
                if let Ok(mut sessions) = sessions.lock() {
                    sessions.failed_visits.insert(package.clone(), now_micros());
                }
                eprintln!("op=foreground_default package={package} error={error}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "foreground_tests.rs"]
mod tests;
