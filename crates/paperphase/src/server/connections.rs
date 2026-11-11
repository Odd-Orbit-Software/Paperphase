//! Settings records only. Connector execution and provider scheduling are separate work.
use std::collections::BTreeSet;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use paperphase_core::{ConnectionConfig, ConnectorCatalog, SettingsValues, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ApiError, ApiResult, Shared, internal, invalid, persist};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConnectionStore {
    schema_version: u32,
    revision: u64,
    connections: Vec<ConnectionConfig>,
}
impl Default for ConnectionStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            connections: vec![],
        }
    }
}
pub(super) fn validate_store(
    store: &ConnectionStore,
    catalog: &ConnectorCatalog,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        store.schema_version == 1,
        "unsupported connections schema version"
    );
    anyhow::ensure!(store.connections.len() <= 32, "connection limit exceeded");
    let mut ids = BTreeSet::new();
    for connection in &store.connections {
        anyhow::ensure!(ids.insert(&connection.id), "duplicate connection id");
        connection.validate(catalog)?;
    }
    Ok(())
}

pub(super) async fn catalog(State(app): State<Shared>) -> Json<ConnectorCatalog> {
    Json(app.connector_catalog.clone())
}

fn public_connection(connection: &ConnectionConfig, catalog: &ConnectorCatalog) -> Value {
    // Build this response explicitly: ConnectionConfig must never be returned directly.
    let definition = catalog
        .get(&connection.connector_id)
        .expect("store validated against catalog");
    let mut visible = SettingsValues::new();
    let mut secret_present = Vec::new();
    for field in &definition.settings {
        if field.input.is_secret() {
            if connection
                .settings
                .get(&field.id)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
            {
                secret_present.push(&field.id);
            }
        } else if let Some(value) = connection.settings.get(&field.id) {
            visible.insert(field.id.clone(), value.clone());
        }
    }
    json!({"id":connection.id,"connector_id":connection.connector_id,"label":connection.label,
        "enabled":connection.enabled,"poll_interval_secs":connection.poll_interval_secs,
        "settings":visible,"secret_present":secret_present})
}
pub(super) async fn list(State(app): State<Shared>) -> Json<Value> {
    let store = app.connections.lock().await;
    Json(
        json!({"revision":store.revision,"connections":store.connections.iter().map(|connection| public_connection(connection, &app.connector_catalog)).collect::<Vec<_>>(),"execution":"settings_only"}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SaveConnection {
    base_revision: u64,
    connector_id: String,
    label: String,
    enabled: bool,
    poll_interval_secs: u32,
    settings: SettingsValues,
}

fn conflict() -> ApiError {
    ApiError(StatusCode::CONFLICT, "Connection settings changed in another tab. Your form is retained; discard it to load the latest connections.".into())
}

pub(super) async fn save(
    State(app): State<Shared>,
    Path(id): Path<String>,
    Json(request): Json<SaveConnection>,
) -> ApiResult<Json<Value>> {
    if !valid_id(&id) {
        return Err(invalid("invalid connection id"));
    }
    // The task owns the complete transaction even if its HTTP client disconnects.
    tokio::spawn(async move {
        let mut store = app.connections.lock().await;
        if request.base_revision != store.revision { return Err(conflict()); }
        let previous = store.connections.iter().find(|connection| connection.id == id);
        if previous.is_some_and(|old| old.connector_id != request.connector_id) { return Err(invalid("an existing connection cannot change connector type")); }
        let definition = app.connector_catalog.get(&request.connector_id).ok_or_else(|| invalid("unknown connector"))?;
        let settings = definition.resolve(&request.settings, previous.map(|connection| &connection.settings)).map_err(invalid)?;
        let connection = ConnectionConfig { id: id.clone(), connector_id: request.connector_id, label: request.label.trim().into(), enabled: request.enabled, poll_interval_secs: request.poll_interval_secs, settings };
        connection.validate(&app.connector_catalog).map_err(invalid)?;
        let mut updated = store.clone();
        if let Some(index) = updated.connections.iter().position(|old| old.id == id) { updated.connections[index] = connection; }
        else {
            if updated.connections.len() >= 32 { return Err(invalid("connection limit exceeded")); }
            updated.connections.push(connection);
        }
        updated.revision = store.revision.checked_add(1).ok_or_else(|| internal("connection revision exhausted"))?;
        let file = app.connections_file.clone(); let to_write = updated.clone();
        tokio::task::spawn_blocking(move || persist(&file, &to_write)).await.map_err(internal)?.map_err(internal)?;
        let response = json!({"revision":updated.revision,"connection":public_connection(updated.connections.iter().find(|connection| connection.id == id).expect("inserted above"), &app.connector_catalog)});
        *store = updated;
        Ok(Json(response))
    }).await.map_err(internal)?
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RemoveConnection {
    base_revision: u64,
}

pub(super) async fn remove(
    State(app): State<Shared>,
    Path(id): Path<String>,
    Json(request): Json<RemoveConnection>,
) -> ApiResult<Json<Value>> {
    tokio::spawn(async move {
        let mut store = app.connections.lock().await;
        if request.base_revision != store.revision {
            return Err(conflict());
        }
        if !store
            .connections
            .iter()
            .any(|connection| connection.id == id)
        {
            return Err(ApiError(
                StatusCode::NOT_FOUND,
                "connection not found".into(),
            ));
        }
        let mut updated = store.clone();
        updated.connections.retain(|connection| connection.id != id);
        updated.revision = store
            .revision
            .checked_add(1)
            .ok_or_else(|| internal("connection revision exhausted"))?;
        let file = app.connections_file.clone();
        let to_write = updated.clone();
        tokio::task::spawn_blocking(move || persist(&file, &to_write))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        let revision = updated.revision;
        *store = updated;
        Ok(Json(json!({"revision":revision})))
    })
    .await
    .map_err(internal)?
}
