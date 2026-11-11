use serde::{Deserialize, Serialize};

use crate::{Result, require};

/// A request to the host controller, never permission to bypass panel policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefreshHint {
    #[default]
    Automatic,
    Partial,
    Full,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshPolicy {
    pub min_interval_ms: u64,
    pub full_after_partials: u32,
    pub full_after_secs: u64,
    pub max_partial_area_percent: u32,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            min_interval_ms: 250,
            full_after_partials: 10,
            full_after_secs: 3600,
            max_partial_area_percent: 40,
        }
    }
}

impl RefreshPolicy {
    pub fn validate(&self) -> Result<()> {
        require(
            self.min_interval_ms <= 3_600_000,
            "refresh_policy.min_interval_ms",
            "interval must be at most one hour",
        )?;
        require(
            (1..=1000).contains(&self.full_after_partials),
            "refresh_policy.full_after_partials",
            "partial count must be 1–1000",
        )?;
        require(
            (1..=604800).contains(&self.full_after_secs),
            "refresh_policy.full_after_secs",
            "maintenance interval must be 1–604800 seconds",
        )?;
        require(
            (1..=100).contains(&self.max_partial_area_percent),
            "refresh_policy.max_partial_area_percent",
            "partial area must be 1–100 percent",
        )
    }
}
