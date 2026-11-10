use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasmtime::{Result, ensure, format_err};

/// A deliberately small experimental contract, not the final plugin schema.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    pub schema_version: u32,
    pub id: String,
    pub fields: Vec<Field>,
    pub rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub kind: FieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Number,
    Boolean,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}

impl Dataset {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported dataset schema version"
        );
        ensure!(valid_id(&self.id), "invalid dataset identifier");
        ensure!(
            !self.fields.is_empty() && self.fields.len() <= 32,
            "a dataset must have 1–32 fields"
        );
        ensure!(self.rows.len() <= 512, "dataset row limit exceeded");
        let mut ids = BTreeSet::new();
        for field in &self.fields {
            ensure!(valid_id(&field.id), "invalid field identifier");
            ensure!(ids.insert(&field.id), "duplicate field identifier");
            ensure!(
                field.unit.as_ref().is_none_or(|unit| unit.len() <= 64),
                "field unit is too long"
            );
        }
        for row in &self.rows {
            ensure!(
                row.len() == self.fields.len(),
                "row fields do not match schema"
            );
            for field in &self.fields {
                let value = row
                    .get(&field.id)
                    .ok_or_else(|| format_err!("row is missing a declared field"))?;
                let matches = value.is_null()
                    || match field.kind {
                        FieldKind::Text => value.as_str().is_some_and(|s| s.len() <= 4096),
                        FieldKind::Number => value.is_number(),
                        FieldKind::Boolean => value.is_boolean(),
                    };
                ensure!(matches, "row value does not match field type or size limit");
            }
        }
        Ok(())
    }
}
