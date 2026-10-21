#![allow(clippy::unwrap_used)]
use super::*;

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
