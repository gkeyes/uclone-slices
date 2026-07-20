#![doc = "Persistence-boundary validation tests for slot runtime domain values."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

#[allow(dead_code)]
mod support;

use serde_json::{Map, Value};
use uclone_slot_runtime::domain::{DataInodes, SlotId, SlotView, UserId};
use uclone_slot_runtime::journal::TransactionSpec;

fn valid_spec() -> TransactionSpec {
    let base = DataInodes::new(100, 200).unwrap();
    support::transaction_spec(support::TransactionFixture::new(
        "tx-domain-0001",
        support::TransactionViews::new(
            base,
            SlotView::new(SlotId::base(), DataInodes::new(100, 200).unwrap()),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(300, 400).unwrap(),
            ),
        ),
        "boot-domain-0001",
    ))
}

#[test]
fn rejects_non_primary_user_from_persisted_json() {
    let decoded = serde_json::from_str::<UserId>("10");

    assert!(decoded.is_err());
}

#[test]
fn rejects_same_slot_transaction_at_deserialization_boundary() {
    let mut value = serde_json::to_value(valid_spec()).unwrap();
    let object = value.as_object_mut().unwrap();
    let managed = object.get("managed_package").unwrap().as_object().unwrap();
    let mut previous = Map::new();
    previous.insert(
        "slot_id".to_owned(),
        managed.get("active_slot").cloned().unwrap_or(Value::Null),
    );
    previous.insert(
        "inodes".to_owned(),
        managed.get("active_inodes").cloned().unwrap_or(Value::Null),
    );
    object.insert("target_view".to_owned(), Value::Object(previous));

    let decoded = serde_json::from_value::<TransactionSpec>(value);

    assert!(decoded.is_err());
}
