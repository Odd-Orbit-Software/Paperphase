use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Dataset, Result, require};

/// One atomic connector result. Additional datasets share the successful fetch
/// time; observation times can also be declared for additional dataset IDs.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub dataset: Dataset,
    #[serde(default)]
    pub observed_at: Option<u64>,
    #[serde(default)]
    pub min_poll_secs: Option<u32>,
    #[serde(default)]
    pub additional_datasets: Vec<Dataset>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub additional_observed_at: BTreeMap<String, u64>,
}

impl Publication {
    pub fn validate(&self) -> Result<()> {
        require(
            self.additional_datasets.len() <= 7,
            "publication",
            "at most eight datasets per publication",
        )?;
        require(
            self.min_poll_secs
                .is_none_or(|secs| (1..=604800).contains(&secs)),
            "publication.min_poll_secs",
            "polling hint must be 1–604800 seconds",
        )?;
        let mut ids = BTreeSet::new();
        for dataset in std::iter::once(&self.dataset).chain(&self.additional_datasets) {
            dataset.validate()?;
            require(
                ids.insert(&dataset.id),
                "publication",
                "duplicate dataset identifier",
            )?;
        }
        require(
            self.additional_observed_at.len() <= 7
                && self.additional_observed_at.keys().all(|id| {
                    self.additional_datasets
                        .iter()
                        .any(|dataset| &dataset.id == id)
                }),
            "publication.additional_observed_at",
            "observation timestamps must refer to additional datasets",
        )?;
        Ok(())
    }
}
