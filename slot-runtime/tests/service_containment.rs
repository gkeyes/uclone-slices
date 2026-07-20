#![allow(missing_docs)]

mod service_support;

use service_support::{Call, FakePlatform, allowed, request};
use uclone_slot_runtime::daemon::RequestHandler;
use uclone_slot_runtime::protocol::{Command, ErrorCode};
use uclone_slot_runtime::service::{PreviewService, RescueExecution};

#[test]
fn base_rescue_failure_is_reported_without_opening_ordinary_state() {
    let platform = FakePlatform::default().with_rescue_execution(RescueExecution::RecoveryRequired);
    let mut service = PreviewService::new(platform);
    let response = service.handle(&request(Command::RescueToBase { package: allowed() }));
    assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    assert_eq!(service.platform().calls(), vec![Call::Rescue]);
}
