use video_editor::contracts::JobSnapshot;
#[test]
fn wire_contract_roundtrip() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/job-snapshot.json")).unwrap();
    let snapshot: JobSnapshot = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    assert_eq!(value["state"], "running");
    assert_eq!(value["version"], 3);
}
