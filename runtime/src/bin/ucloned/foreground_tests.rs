#![allow(clippy::unwrap_used)]
use super::*;

fn activity(package: &str, task: i32, pid: u32) -> Activity {
    Activity {
        token: format!("token-{task}"),
        component: format!("{package}/{package}.Main"),
        package: package.to_owned(),
        task,
        pid: Some(pid),
        caller: None,
        result_task: None,
        home: false,
        resumed: true,
    }
}
fn snapshot(foreground: Activity) -> Snapshot {
    Snapshot {
        locked: false,
        foreground: Some(foreground.clone()),
        activities: vec![foreground],
    }
}
fn event(activity: &Activity, timestamp: u128) -> Event {
    Event {
        timestamp,
        kind: EventKind::Resumed(activity.component.clone()),
    }
}
fn home() -> Activity {
    Activity {
        home: true,
        ..activity("com.example.home", 1, 100)
    }
}

#[test]
fn system_event_parser_checks_uid_user_tag_and_component() {
    let line = "1792511062.088391 1000 4215 7173 I wm_set_resumed_activity: [0,com.example.app/.Main,resumeTopActivity]";
    let parsed = parse_event(line).unwrap();
    assert_eq!(parsed.timestamp, 1_792_511_062_088_391);
    assert_eq!(
        parsed.kind,
        EventKind::Resumed("com.example.app/com.example.app.Main".to_owned())
    );
    assert!(parse_event(&line.replacen("1000", "10346", 1)).is_none());
    assert!(parse_event(&line.replacen("[0,", "[10,", 1)).is_none());
    assert!(parse_event("not an event").is_none());
}

#[test]
fn snapshot_uses_primary_resumed_record_and_default_display_lock() {
    let dump = "Display #0 (activities from top to bottom):\n\
        topResumedActivity=ActivityRecord{abc u0 com.example.app/.Main t42}\n\
        * Task{a #42 type=standard}\n\
          * Hist #0: ActivityRecord{abc u0 com.example.app/.Main t42}\n\
            launchedFromUid=10000 launchedFromPackage=com.example.camera userId=0\n\
            app=ProcessRecord{def 1234:com.example.app/u0a123}\n\
            resultTo=ActivityRecord{xyz u0 com.example.source/.Main t41}\n\
            state=RESUMED finishing=false\n\
        Display #1 (activities from top to bottom):\n\
          topResumedActivity=ActivityRecord{rear u0 com.example.rear/.Main t99}\n\
          * Hist #0: ActivityRecord{rear u0 com.example.rear/.Main t99}\n\
            app=ProcessRecord{abc 777:com.example.rear/u0a124}\n\
            state=RESUMED finishing=false\n\
        KeyguardController:\n\
          mKeyguardShowing=false\n";
    let value = parse_snapshot(dump).unwrap();
    let top = value.foreground.unwrap();
    assert_eq!(top.package, "com.example.app");
    assert_eq!(top.task, 42);
    assert_eq!(top.pid, Some(1234));
    assert_eq!(top.caller.as_deref(), Some("com.example.camera"));
    assert_eq!(top.result_task, Some(41));
    assert!(!value.locked);
    assert!(parse_snapshot("unknown format").is_err());
}

#[test]
fn external_entry_is_checked_once_per_visit_and_never_for_internal_navigation() {
    let app = activity("com.example.app", 42, 1234);
    let snap = snapshot(app.clone());
    let mut sessions = Sessions::default();
    assert_eq!(
        sessions.observe(&event(&app, 10), &snap),
        Some(app.package.clone())
    );
    assert_eq!(sessions.observe(&event(&app, 11), &snap), None);
    let home = home();
    sessions.observe(&event(&home, 12), &snapshot(home.clone()));
    assert_eq!(sessions.observe(&event(&app, 13), &snap), Some(app.package));
}

#[test]
fn slice_launch_is_allowed_until_home_or_an_unrelated_app() {
    for next in [home(), activity("com.example.other", 99, 5678)] {
        let app = activity("com.example.app", 42, 1234);
        let snap = snapshot(app.clone());
        let mut sessions = Sessions::default();
        sessions.grant(&app.package, 10, Some(&snap));
        assert_eq!(sessions.observe(&event(&app, 11), &snap), None);
        sessions.observe(&event(&next, 12), &snapshot(next.clone()));
        assert_eq!(sessions.observe(&event(&app, 13), &snap), Some(app.package));
    }
}

#[test]
fn camera_picker_payment_chain_and_lock_preserve_the_selected_account() {
    let app = activity("com.example.app", 42, 1234);
    let snap = snapshot(app.clone());
    let mut sessions = Sessions::default();
    sessions.grant(&app.package, 10, Some(&snap));
    let camera = Activity {
        result_task: Some(42),
        ..activity("com.example.camera", 43, 2000)
    };
    let picker = Activity {
        caller: Some(camera.package.clone()),
        ..activity("com.example.picker", 44, 3000)
    };
    let payment = Activity {
        caller: Some(app.package.clone()),
        ..activity("com.example.payment", 45, 4000)
    };
    for next in [&camera, &picker, &payment, &app] {
        sessions.observe(&event(next, 11), &snapshot(next.clone()));
        assert!(sessions.manual.contains_key(&app.package));
    }
    let mut locked = snapshot(home());
    locked.locked = true;
    sessions.observe(
        &Event {
            timestamp: 12,
            kind: EventKind::Keyguard(true),
        },
        &locked,
    );
    assert_eq!(
        sessions.observe(
            &Event {
                timestamp: 13,
                kind: EventKind::Keyguard(false)
            },
            &snap
        ),
        None
    );
    assert!(sessions.manual.contains_key(&app.package));
}

#[test]
fn buffered_home_before_manual_launch_cannot_cancel_that_launch() {
    let app = activity("com.example.app", 42, 1234);
    let mut snap = snapshot(app.clone());
    snap.activities.push(home());
    let mut sessions = Sessions::default();
    sessions.grant(&app.package, 20, Some(&snap));
    assert_eq!(sessions.observe(&event(&home(), 19), &snap), None);
    assert!(sessions.manual.contains_key(&app.package));
}

#[test]
fn fast_home_return_is_not_lost_when_the_live_snapshot_is_already_back_in_the_app() {
    let app = activity("com.example.app", 42, 1234);
    let mut snap = snapshot(app.clone());
    snap.activities.push(home());
    let mut sessions = Sessions::default();
    sessions.grant(&app.package, 10, Some(&snap));
    assert_eq!(
        sessions.observe(&event(&home(), 11), &snap),
        Some(app.package)
    );
}

#[test]
fn process_recreation_is_a_new_entry_even_if_the_task_id_is_unchanged() {
    let app = activity("com.example.app", 42, 1234);
    let mut sessions = Sessions::default();
    sessions.grant(&app.package, 10, Some(&snapshot(app.clone())));
    let restarted = Activity {
        pid: Some(9999),
        ..app
    };
    assert_eq!(
        sessions.observe(&event(&restarted, 11), &snapshot(restarted.clone())),
        Some(restarted.package)
    );
}

#[test]
fn failed_or_non_activation_rpc_does_not_grant_a_manual_session() {
    let mut sessions = Sessions::default();
    sessions.completed_request(
        r#"{"op":"activate_slot","package":"com.example.app","slot":"slot-1"}"#,
        r#"{"error":{"code":"operation_failed"}}"#,
    );
    sessions.completed_request(
        r#"{"op":"set_default_slot","package":"com.example.app","slot":"slot-1"}"#,
        r#"{"ok":{"package":{"package":"com.example.app","active_slot":"slot-1"}}}"#,
    );
    assert!(sessions.manual.is_empty());
}

#[test]
fn failed_automatic_switch_does_not_loop_on_its_rollback_relaunch() {
    let app = activity("com.example.app", 42, 1234);
    let mut snap = snapshot(app.clone());
    snap.activities.push(home());
    let mut sessions = Sessions::default();
    sessions.failed_visits.insert(app.package.clone(), 20);
    assert_eq!(sessions.observe(&event(&home(), 19), &snap), None);
    assert!(sessions.failed_visits.contains_key(&app.package));
    sessions.observe(&event(&home(), 21), &snapshot(home()));
    assert_eq!(sessions.observe(&event(&app, 22), &snap), Some(app.package));
}
