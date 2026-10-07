use video_editor::contracts::{JobSnapshot, QueueSnapshot};
#[test]
fn wire_contract_roundtrip() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/job-snapshot.json")).unwrap();
    let snapshot: JobSnapshot = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    assert_eq!(value["state"], "running");
    assert_eq!(value["version"], 3);
}

#[test]
fn queue_wire_contract_roundtrip() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/queue-snapshot.json")).unwrap();
    let snapshot: QueueSnapshot = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    assert!(value["items"][0]["snapshot"].is_null());
    assert_eq!(value["items"][1]["state"], "canceled");
    assert_eq!(value["items"][2]["jobId"], "test-job");
}
