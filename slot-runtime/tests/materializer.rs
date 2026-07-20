#![doc = "Fail-closed data-slot materialization tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

mod materializer {
    mod failures;
    mod happy_path;
    mod restart;
    mod support;
}
