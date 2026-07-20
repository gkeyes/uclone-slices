#![doc = "Recovery-only native base restoration coverage for the Android backend."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "private integration-test modules share validated fixture helpers"
)]

#[path = "android_base_rescue/happy.rs"]
mod happy;
#[path = "android_base_rescue/rejections.rs"]
mod rejections;
#[path = "android_base_rescue/support.rs"]
mod support;
