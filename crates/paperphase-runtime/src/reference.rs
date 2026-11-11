//! Immutable plugin reference tables shared by all invocations of a binding.
use paperphase_core::valid_id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wasmtime::{Result, ensure};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ROWS: usize = 200_000;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceData {
    tables: BTreeMap<String, BTreeMap<String, Box<str>>>,
    #[serde(skip)]
    bytes: usize,
    #[serde(skip)]
    rows: usize,
}
impl ReferenceData {
    pub fn insert(&mut self, table: &str, key: &str, values: &[String]) -> Result<()> {
        ensure!(
            valid_id(table)
                && key.len() <= 192
                && !key.is_empty()
                && !key.chars().any(char::is_control),
            "invalid reference table or key"
        );
        let value = serde_json::to_string(values)?;
        ensure!(
            values.len() <= 64 && value.len() <= 4096,
            "reference value exceeds budget"
        );
        if let Some(old) = self.tables.get(table).and_then(|rows| rows.get(key)) {
            ensure!(old.as_ref() == value, "conflicting reference key");
            return Ok(());
        }
        ensure!(
            self.rows < MAX_ROWS
                && self.bytes + table.len() + key.len() + value.len() <= MAX_BYTES
                && (self.tables.contains_key(table) || self.tables.len() < 8),
            "reference table budget exceeded"
        );
        self.rows += 1;
        self.bytes += table.len() + key.len() + value.len();
        self.tables
            .entry(table.into())
            .or_default()
            .insert(key.into(), value.into_boxed_str());
        Ok(())
    }
    pub fn append(&mut self, table: &str, key: &str, values: &[String]) -> Result<()> {
        let item = serde_json::to_string(values)?;
        let Some(old) = self.get(table, key) else {
            return self.insert(table, key, &[item]);
        };
        let mut grouped: Vec<String> = serde_json::from_str(old)?;
        if grouped.contains(&item) {
            return Ok(());
        }
        ensure!(grouped.len() < 64, "grouped reference row budget exceeded");
        grouped.push(item);
        let replacement = serde_json::to_string(&grouped)?;
        ensure!(
            replacement.len() <= 4096 && self.bytes - old.len() + replacement.len() <= MAX_BYTES,
            "grouped reference value budget exceeded"
        );
        self.bytes = self.bytes - old.len() + replacement.len();
        self.tables
            .get_mut(table)
            .expect("existing table")
            .insert(key.into(), replacement.into_boxed_str());
        Ok(())
    }
    pub fn validate(&mut self) -> Result<()> {
        ensure!(self.tables.len() <= 8, "too many reference tables");
        self.bytes = 0;
        self.rows = 0;
        for (table, rows) in &self.tables {
            ensure!(valid_id(table), "invalid reference table");
            for (key, value) in rows {
                ensure!(
                    !key.is_empty()
                        && key.len() <= 192
                        && !key.chars().any(char::is_control)
                        && value.len() <= 4096,
                    "invalid cached reference row"
                );
                let values: Vec<String> = serde_json::from_str(value)?;
                ensure!(values.len() <= 64, "invalid reference columns");
                self.rows += 1;
                self.bytes += table.len() + key.len() + value.len();
                ensure!(
                    self.rows <= MAX_ROWS && self.bytes <= MAX_BYTES,
                    "cached reference budget exceeded"
                );
            }
        }
        Ok(())
    }
    pub fn get(&self, table: &str, key: &str) -> Option<&str> {
        self.tables.get(table)?.get(key).map(AsRef::as_ref)
    }
    pub fn memory_cost(&self) -> usize {
        self.bytes.saturating_add(self.rows.saturating_mul(96))
    }
    pub fn rows(&self) -> usize {
        self.rows
    }
}
