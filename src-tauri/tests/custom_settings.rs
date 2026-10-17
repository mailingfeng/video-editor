use serde_json::{json, Value};
use video_editor::contracts::*;

#[test]
fn recommended_and_untrusted_settings() {
    let settings = CustomVariationSettings::recommended();
    assert_eq!(
        (settings.foreground_scale, settings.background_blur),
        (1, 1)
    );
    assert_eq!(
        (settings.crop_scale, settings.softness, settings.speed),
        (0, 0, 0)
    );
    assert_eq!(
        (
            settings.pitch,
            settings.timbre,
            settings.volume,
            settings.dynamic_offset
        ),
        (3, 3, 3, 3)
    );
    assert_eq!(settings.encoding, 5);
    let valid: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/custom-settings.json")).unwrap();
    assert_eq!(decode_custom_settings(valid.clone()).unwrap(), settings);
    for key in valid
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.as_str() != "schemaVersion")
    {
        for bad in [json!(-1), json!(1.5), json!(10), Value::Null, json!("5")] {
            let mut value = valid.clone();
            value[key] = bad;
            assert_eq!(
                decode_custom_settings(value).unwrap_err().code,
                ErrorCode::InvalidSettings,
                "{key}"
            );
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert_eq!(
            decode_custom_settings(missing).unwrap_err().code,
            ErrorCode::InvalidSettings
        );
    }
    for value in [json!(2), json!(0), Value::Null, json!("1")] {
        let mut bad = valid.clone();
        bad["schemaVersion"] = value;
        assert_eq!(
            decode_custom_settings(bad).unwrap_err().code,
            ErrorCode::InvalidSettings
        );
    }
    let mut extra = valid;
    extra["seed"] = json!(123);
    assert_eq!(
        decode_custom_settings(extra).unwrap_err().code,
        ErrorCode::InvalidSettings
    );
    let mut typed = settings;
    typed.hue = 10;
    assert_eq!(
        typed.validate().unwrap_err().code,
        ErrorCode::InvalidSettings
    );
}

#[test]
fn custom_mode_requires_settings() {
    let settings = CustomVariationSettings::recommended();
    assert!(validate_custom_selection("custom-variation-v1", None).is_err());
    assert!(validate_custom_selection("content-variation-v1", Some(&settings)).is_err());
    assert!(validate_custom_selection("custom-variation-v1", Some(&settings)).is_ok());
    assert!(validate_custom_selection("content-variation-v1", None).is_ok());
    let mut batch = json!({"outputDirectory":"/out","presetId":"custom-variation-v1","metadata":{"mode":"preserve"}});
    assert_eq!(
        decode_batch_settings(batch.clone()).unwrap_err().code,
        ErrorCode::InvalidSettings
    );
    batch["customSettings"] = serde_json::to_value(&settings).unwrap();
    assert!(decode_batch_settings(batch.clone()).is_ok());
    batch["inputPath"] = json!("/input.mp4");
    assert!(decode_start_request(batch.clone()).is_ok());
    batch["customSettings"]["speed"] = json!(1.5);
    assert_eq!(
        decode_start_request(batch).unwrap_err().code,
        ErrorCode::InvalidSettings
    );
}

#[test]
fn legacy_and_custom_wire_roundtrip() {
    let legacy = json!({"outputDirectory":"/out","presetId":"content-variation-v1","metadata":{"mode":"preserve"}});
    assert_eq!(
        serde_json::to_value(decode_batch_settings(legacy.clone()).unwrap()).unwrap(),
        legacy
    );
    let value: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/queue-custom-snapshot.json"
    ))
    .unwrap();
    let snapshot: QueueSnapshot = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        snapshot.active_settings.as_ref().unwrap().preset_id,
        "custom-variation-v1"
    );
    assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    assert!(!value.to_string().contains("filter"));
}
