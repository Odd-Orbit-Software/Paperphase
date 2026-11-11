//! Experimental connector metadata and settings, independent of the guest ABI.
use crate::{Result, require, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub type SettingsValues = BTreeMap<String, Value>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCatalog {
    pub schema_version: u32,
    pub connectors: Vec<ConnectorDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub min_poll_secs: u32,
    pub default_poll_secs: u32,
    pub settings: Vec<SettingDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingDefinition {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub required: bool,
    pub input: SettingInput,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingInput {
    Text {
        max_bytes: u16,
        #[serde(default)]
        default: Option<String>,
    },
    Secret {
        max_bytes: u16,
    },
    Integer {
        min: i64,
        max: i64,
        #[serde(default)]
        default: Option<i64>,
    },
    Boolean {
        #[serde(default)]
        default: Option<bool>,
    },
    Choice {
        options: Vec<SettingChoice>,
        #[serde(default)]
        default: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingChoice {
    pub value: String,
    pub label: String,
}

impl SettingInput {
    pub fn is_secret(&self) -> bool {
        matches!(self, Self::Secret { .. })
    }
    pub fn default_value(&self) -> Option<Value> {
        match self {
            Self::Text { default, .. } | Self::Choice { default, .. } => {
                default.clone().map(Value::String)
            }
            Self::Integer { default, .. } => default.map(Value::from),
            Self::Boolean { default } => default.map(Value::Bool),
            Self::Secret { .. } => None,
        }
    }
    fn accepts(&self, value: &Value) -> bool {
        match self {
            Self::Text { max_bytes, .. } | Self::Secret { max_bytes } => value
                .as_str()
                .is_some_and(|s| s.len() <= usize::from(*max_bytes)),
            Self::Integer { min, max, .. } => {
                value.as_i64().is_some_and(|n| n >= *min && n <= *max)
            }
            Self::Boolean { .. } => value.is_boolean(),
            Self::Choice { options, .. } => value
                .as_str()
                .is_some_and(|s| options.iter().any(|o| o.value == s)),
        }
    }
}

impl ConnectorCatalog {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1,
            "connectors.schema_version",
            "unsupported schema version",
        )?;
        require(
            self.connectors.len() <= 32,
            "connectors",
            "connector limit exceeded",
        )?;
        let mut ids = BTreeSet::new();
        for (i, connector) in self.connectors.iter().enumerate() {
            let path = format!("connectors[{i}]");
            require(
                valid_id(&connector.id) && ids.insert(&connector.id),
                &path,
                "invalid or duplicate connector id",
            )?;
            require(
                !connector.name.trim().is_empty()
                    && connector.name.len() <= 96
                    && connector.description.len() <= 1024,
                &path,
                "invalid name or description size",
            )?;
            require(
                connector.min_poll_secs >= 1
                    && connector.min_poll_secs <= connector.default_poll_secs
                    && connector.default_poll_secs <= 86400,
                &path,
                "polling bounds must be ordered within 1–86400 seconds",
            )?;
            require(
                connector.settings.len() <= 32,
                &path,
                "settings limit exceeded",
            )?;
            let mut field_ids = BTreeSet::new();
            for (j, field) in connector.settings.iter().enumerate() {
                let path = format!("{path}.settings[{j}]");
                require(
                    valid_id(&field.id) && field_ids.insert(&field.id),
                    &path,
                    "invalid or duplicate setting id",
                )?;
                require(
                    !field.label.trim().is_empty()
                        && field.label.len() <= 96
                        && field.help.len() <= 512,
                    &path,
                    "invalid label or help size",
                )?;
                match &field.input {
                    SettingInput::Text { max_bytes, .. } | SettingInput::Secret { max_bytes } => {
                        require(
                            (1..=4096).contains(max_bytes),
                            &path,
                            "text bound must be 1–4096 bytes",
                        )?
                    }
                    SettingInput::Integer { min, max, .. } => require(
                        min <= max
                            && *min >= -9_007_199_254_740_991
                            && *max <= 9_007_199_254_740_991,
                        &path,
                        "integer bounds must be ordered and browser-safe",
                    )?,
                    SettingInput::Choice { options, .. } => {
                        require(
                            !options.is_empty() && options.len() <= 32,
                            &path,
                            "must provide 1–32 choices",
                        )?;
                        let mut values = BTreeSet::new();
                        for option in options {
                            require(
                                valid_id(&option.value)
                                    && values.insert(&option.value)
                                    && !option.label.trim().is_empty()
                                    && option.label.len() <= 96,
                                &path,
                                "invalid or duplicate choice",
                            )?;
                        }
                    }
                    SettingInput::Boolean { .. } => {}
                }
                if let Some(default) = field.input.default_value() {
                    require(
                        field.input.accepts(&default)
                            && (!field.required
                                || default.as_str().is_none_or(|s| !s.trim().is_empty())),
                        &path,
                        "invalid default value",
                    )?;
                }
            }
        }
        require(
            serde_json::to_vec(self).is_ok_and(|json| json.len() <= 256 * 1024),
            "connectors",
            "catalog size limit exceeded",
        )
    }
    pub fn get(&self, id: &str) -> Option<&ConnectorDefinition> {
        self.connectors
            .iter()
            .find(|definition| definition.id == id)
    }
}

impl ConnectorDefinition {
    /// Full non-secret values; omitted secrets retain their existing value.
    /// Explicit null clears an optional value, including a secret.
    pub fn resolve(
        &self,
        submitted: &SettingsValues,
        previous: Option<&SettingsValues>,
    ) -> Result<SettingsValues> {
        require(
            submitted
                .keys()
                .all(|id| self.settings.iter().any(|field| &field.id == id)),
            "settings",
            "unknown setting id",
        )?;
        let mut values = SettingsValues::new();
        for field in &self.settings {
            let value = submitted
                .get(&field.id)
                .cloned()
                .or_else(|| {
                    field
                        .input
                        .is_secret()
                        .then(|| previous.and_then(|old| old.get(&field.id)).cloned())
                        .flatten()
                })
                .or_else(|| field.input.default_value());
            let path = format!("settings.{}", field.id);
            let Some(value) = value else {
                require(!field.required, &path, "required value is missing")?;
                continue;
            };
            if value.is_null() {
                require(!field.required, &path, "required value is missing")?;
                if !field.input.is_secret() {
                    values.insert(field.id.clone(), Value::Null);
                }
                continue;
            }
            require(
                field.input.accepts(&value),
                &path,
                "value has the wrong type, size, or allowed range",
            )?;
            require(
                !field.required || value.as_str().is_none_or(|s| !s.trim().is_empty()),
                &path,
                "required text is empty",
            )?;
            // An empty optional secret clears it rather than preserving a mask.
            if field.input.is_secret() && value.as_str() == Some("") {
                continue;
            }
            values.insert(field.id.clone(), value);
        }
        require(
            serde_json::to_vec(&values).is_ok_and(|json| json.len() <= 16 * 1024),
            "settings",
            "combined settings size limit exceeded",
        )?;
        Ok(values)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionConfig {
    pub id: String,
    pub connector_id: String,
    pub label: String,
    pub enabled: bool,
    pub poll_interval_secs: u32,
    pub settings: SettingsValues,
}

impl ConnectionConfig {
    pub fn validate(&self, catalog: &ConnectorCatalog) -> Result<()> {
        require(valid_id(&self.id), "connection.id", "invalid identifier")?;
        require(
            !self.label.trim().is_empty() && self.label.len() <= 96,
            "connection.label",
            "label must be 1–96 bytes",
        )?;
        let definition = catalog
            .get(&self.connector_id)
            .ok_or_else(|| crate::ValidationError {
                path: "connection.connector_id".into(),
                message: "unknown connector".into(),
            })?;
        require(
            self.poll_interval_secs >= definition.min_poll_secs && self.poll_interval_secs <= 86400,
            "connection.poll_interval_secs",
            "polling interval is outside the allowed range",
        )?;
        definition.resolve(&self.settings, None)?;
        Ok(())
    }
}
