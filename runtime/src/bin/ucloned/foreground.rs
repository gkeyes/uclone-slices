//! Read-only task diagnostics. Launch decisions are made by the system bridge.
use serde::Serialize;
use std::process::Command;
use uclone_slices_runtime::validation::is_package_name;

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
    Ok(Snapshot { locked, foreground })
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

#[cfg(test)]
#[path = "foreground_tests.rs"]
mod tests;
