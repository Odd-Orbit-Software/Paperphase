use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Result, require, valid_id};

/// Versioned data schema, independent of the experimental host-call ABI.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    pub schema_version: u32,
    pub id: String,
    pub fields: Vec<Field>,
    pub rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub kind: FieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Number,
    Boolean,
}

impl Dataset {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1,
            "dataset.schema_version",
            "unsupported schema version",
        )?;
        require(valid_id(&self.id), "dataset.id", "invalid identifier")?;
        require(
            !self.fields.is_empty() && self.fields.len() <= 32,
            "dataset.fields",
            "must have 1–32 fields",
        )?;
        require(self.rows.len() <= 512, "dataset.rows", "row limit exceeded")?;
        let mut ids = BTreeSet::new();
        for (i, field) in self.fields.iter().enumerate() {
            let path = format!("dataset.fields[{i}]");
            require(
                valid_id(&field.id),
                format!("{path}.id"),
                "invalid identifier",
            )?;
            require(
                ids.insert(&field.id),
                format!("{path}.id"),
                "duplicate identifier",
            )?;
            require(
                field.unit.as_ref().is_none_or(|s| s.len() <= 64),
                format!("{path}.unit"),
                "unit is too long",
            )?;
        }
        for (i, row) in self.rows.iter().enumerate() {
            let path = format!("dataset.rows[{i}]");
            require(
                row.len() == self.fields.len(),
                &path,
                "fields do not match schema",
            )?;
            for field in &self.fields {
                let value = row.get(&field.id);
                require(value.is_some(), &path, "missing declared field")?;
                let value = value.expect("presence checked above");
                let matches = value.is_null()
                    || match field.kind {
                        FieldKind::Text => value.as_str().is_some_and(|s| s.len() <= 4096),
                        FieldKind::Number => value.is_number(),
                        FieldKind::Boolean => value.is_boolean(),
                    };
                require(
                    matches,
                    format!("{path}.{}", field.id),
                    "value does not match field type or size limit",
                )?;
            }
        }
        Ok(())
    }
}
