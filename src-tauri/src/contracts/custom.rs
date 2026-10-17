use super::{AppError, BatchSettings, ErrorCode, StartJobRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomVariationSettings {
    pub schema_version: u8,
    pub foreground_scale: u8,
    pub background_blur: u8,
    pub dynamic_offset: u8,
    pub crop_scale: u8,
    pub hue: u8,
    pub saturation: u8,
    pub tone: u8,
    pub noise: u8,
    pub softness: u8,
    pub pitch: u8,
    pub timbre: u8,
    pub volume: u8,
    pub speed: u8,
    pub encoding: u8,
}

impl CustomVariationSettings {
    pub fn recommended() -> Self {
        Self {
            schema_version: 1,
            foreground_scale: 1,
            background_blur: 1,
            dynamic_offset: 3,
            crop_scale: 0,
            hue: 5,
            saturation: 5,
            tone: 5,
            noise: 5,
            softness: 0,
            pitch: 3,
            timbre: 3,
            volume: 3,
            speed: 0,
            encoding: 5,
        }
    }

    pub fn validate(&self) -> Result<(), AppError> {
        let levels = [
            self.foreground_scale,
            self.background_blur,
            self.dynamic_offset,
            self.crop_scale,
            self.hue,
            self.saturation,
            self.tone,
            self.noise,
            self.softness,
            self.pitch,
            self.timbre,
            self.volume,
            self.speed,
            self.encoding,
        ];
        if self.schema_version != 1 || levels.iter().any(|&level| level > 9) {
            return Err(invalid_settings());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CustomSettingsSource {
    Default,
    Saved,
    Recovered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomSettingsStatus {
    pub settings: CustomVariationSettings,
    pub recommended: CustomVariationSettings,
    pub source: CustomSettingsSource,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveProcessingSettings {
    pub preset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_settings: Option<CustomVariationSettings>,
}

pub fn invalid_settings() -> AppError {
    AppError::new(
        ErrorCode::InvalidSettings,
        "处理设置无效，请恢复推荐设置后重试。",
    )
}

pub fn validate_custom_selection(
    preset_id: &str,
    settings: Option<&CustomVariationSettings>,
) -> Result<(), AppError> {
    match (preset_id == "custom-variation-v1", settings) {
        (true, Some(settings)) => settings.validate(),
        (false, None) => Ok(()),
        _ => Err(invalid_settings()),
    }
}

pub fn decode_custom_settings(value: Value) -> Result<CustomVariationSettings, AppError> {
    let settings: CustomVariationSettings =
        serde_json::from_value(value).map_err(|_| invalid_settings())?;
    settings.validate()?;
    Ok(settings)
}

pub fn decode_batch_settings(value: Value) -> Result<BatchSettings, AppError> {
    let settings: BatchSettings = serde_json::from_value(value).map_err(|_| invalid_settings())?;
    validate_custom_selection(&settings.preset_id, settings.custom_settings.as_ref())?;
    Ok(settings)
}

pub fn decode_start_request(value: Value) -> Result<StartJobRequest, AppError> {
    let request: StartJobRequest = serde_json::from_value(value).map_err(|_| invalid_settings())?;
    validate_custom_selection(&request.preset_id, request.custom_settings.as_ref())?;
    Ok(request)
}
