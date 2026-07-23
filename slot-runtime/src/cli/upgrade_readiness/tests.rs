#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "compatibility fixtures fail at their exact malformed frame"
)]

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use super::*;

const V1: &str = include_str!("../../../../protocol-fixtures/upgrade-readiness/v1.jsonl");
const V2: &str = include_str!("../../../../protocol-fixtures/upgrade-readiness/v2.jsonl");

#[test]
fn accepts_v1_golden_frames_and_sends_only_v1_read_requests() {
    let mut exchange = FixtureExchange::from_lines(V1.lines());
    let count = check_with(|| exchange.connect()).expect("v1 readiness");
    assert_eq!(count, 1);
    let requests = exchange.finish();
    assert_request(&requests[0], 1, V1_PROBE_ID, "probe", None);
    assert_request(&requests[1], 1, APPS_ID, "list_managed_apps", None);
}

#[test]
fn accepts_v2_golden_frames_and_reuses_the_deployed_build_identity() {
    let mut exchange = FixtureExchange::from_lines(V2.lines());
    let count = check_with(|| exchange.connect()).expect("v2 readiness");
    assert_eq!(count, 2);
    let requests = exchange.finish();
    assert_request(&requests[0], 1, V1_PROBE_ID, "probe", None);
    assert_request(&requests[1], 2, V2_PROBE_ID, "probe", None);
    assert_request(
        &requests[2],
        2,
        APPS_ID,
        "list_managed_apps",
        Some("installed-build"),
    );
}

#[test]
fn rejects_unknown_schema_non_base_non_normal_and_active_runtime_errors() {
    let unknown = [
        r#"{"schema_version":3,"request_id":"upgrade-probe-v1","status":"error","error_code":"invalid_request"}"#,
    ];
    assert_rejected(&unknown, UpgradeReadinessError::UnsupportedSchema);

    let non_base = v1_with_apps(
        r#"[{"package":"com.example.app","active_slot":"work","lifecycle":"normal"}]"#,
    );
    assert_rejected(&non_base, UpgradeReadinessError::NonBase);

    let non_normal = v1_with_apps(
        r#"[{"package":"com.example.app","active_slot":"base","lifecycle":"recovery_required"}]"#,
    );
    assert_rejected(&non_normal, UpgradeReadinessError::NonNormal);

    let daemon_error = [
        v1_probe().to_owned(),
        r#"{"schema_version":1,"request_id":"upgrade-apps","status":"error","error_code":"busy"}"#
            .to_owned(),
    ];
    assert_rejected(&daemon_error, UpgradeReadinessError::Rejected);
}

#[test]
fn rejects_identity_change_and_malformed_or_oversized_reports() {
    let mismatched = [
        v1_probe().to_owned(),
        r#"{"schema_version":1,"request_id":"another-request","status":"ok","payload":{"kind":"managed_apps","data":{"apps":[]}}}"#
            .to_owned(),
    ];
    assert_rejected(&mismatched, UpgradeReadinessError::InvalidFrame);

    let extra_field = [
        v1_probe().to_owned(),
        r#"{"schema_version":1,"request_id":"upgrade-apps","status":"ok","payload":{"kind":"managed_apps","data":{"apps":[],"unsafe":true}}}"#
            .to_owned(),
    ];
    assert_rejected(&extra_field, UpgradeReadinessError::InvalidFrame);

    let apps = (0..=MAX_MANAGED_APPS)
        .map(|index| {
            format!(
                r#"{{"package":"com.example.p{index}","active_slot":"base","lifecycle":"normal"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    assert_rejected(
        &v1_with_apps(&format!("[{apps}]")),
        UpgradeReadinessError::TooManyApps,
    );
}

fn v1_probe() -> &'static str {
    r#"{"schema_version":1,"request_id":"upgrade-probe-v1","status":"ok","payload":{"kind":"probe_report","data":{"ready":true,"user_unlocked":true,"ce_de_supported":true}}}"#
}

fn v1_with_apps(apps: &str) -> Vec<String> {
    vec![
        v1_probe().to_owned(),
        format!(
            r#"{{"schema_version":1,"request_id":"upgrade-apps","status":"ok","payload":{{"kind":"managed_apps","data":{{"apps":{apps}}}}}}}"#
        ),
    ]
}

fn assert_rejected(lines: &[impl AsRef<str>], expected: UpgradeReadinessError) {
    let mut exchange = FixtureExchange::from_lines(lines.iter().map(AsRef::as_ref));
    let error = check_with(|| exchange.connect()).expect_err("must reject");
    assert_eq!(
        std::mem::discriminant(&error),
        std::mem::discriminant(&expected)
    );
}

fn assert_request(
    frame: &[u8],
    schema: u32,
    request_id: &str,
    command: &str,
    build_id: Option<&str>,
) {
    let value: serde_json::Value =
        serde_json::from_slice(frame.strip_suffix(b"\n").expect("newline")).expect("request json");
    assert_eq!(value["schema_version"], schema);
    assert_eq!(value["request_id"], request_id);
    assert_eq!(value["command"], command);
    assert_eq!(value.get("build_id").and_then(|v| v.as_str()), build_id);
}

struct FixtureExchange {
    clients: VecDeque<UnixStream>,
    servers: Vec<UnixStream>,
}

impl FixtureExchange {
    fn from_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Self {
        let mut clients = VecDeque::new();
        let mut servers = Vec::new();
        for line in lines {
            let (client, mut server) = UnixStream::pair().expect("socket pair");
            server.write_all(line.as_bytes()).expect("fixture response");
            server.write_all(b"\n").expect("fixture delimiter");
            clients.push_back(client);
            servers.push(server);
        }
        Self { clients, servers }
    }

    fn connect(&mut self) -> io::Result<UnixStream> {
        self.clients
            .pop_front()
            .ok_or_else(|| io::Error::other("missing fixture stream"))
    }

    fn finish(mut self) -> Vec<Vec<u8>> {
        self.servers
            .iter_mut()
            .map(|stream| {
                let mut frame = Vec::new();
                stream.read_to_end(&mut frame).expect("request frame");
                frame
            })
            .collect()
    }
}
