use super::Rational;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimingPolicy {
    #[default]
    Preserve,
    UniformSpeed {
        speed: Rational,
    },
}
