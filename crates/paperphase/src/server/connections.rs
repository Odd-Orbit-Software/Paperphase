//! Private connection records, cached datasets, and redacted configuration responses.
use std::collections::{BTreeMap, BTreeSet};

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
    pub(super) schema_version: u32,
    pub(super) revision: u64,
    pub(super) connections: Vec<ConnectionConfig>,
    #[serde(default)]
    pub(super) data: BTreeMap<String, super::fetching::ConnectionData>,
    #[serde(default)]
    pub(super) origin_deadlines: BTreeMap<String, u64>,
    #[serde(skip)]
    pub(super) dirty: bool,
}
impl Default for ConnectionStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            connections: vec![],
            data: BTreeMap::new(),
            origin_deadlines: BTreeMap::new(),
            dirty: false,
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
    anyhow::ensure!(
        store.data.len() <= 32
            && store.origin_deadlines.len() <= 256
            && store
                .origin_deadlines
                .keys()
                .all(|origin| origin.len() <= 8192),
        "invalid cache or origin budget count"
    );
    let mut cached_bytes = 0;
    for (id, data) in &store.data {
        anyhow::ensure!(
            ids.contains(id) && data.generation <= store.revision,
            "orphaned or invalid connection cache"
        );
        anyhow::ensure!(
            data.last_error
                .as_ref()
                .is_none_or(|message| message.len() <= 1024),
            "oversized connector error"
        );
        if let Some(snapshot) = &data.snapshot {
            anyhow::ensure!(
                &snapshot.connection_id == id,
                "cache connection id mismatch"
            );
            paperphase_core::SnapshotSet {
                schema_version: 1,
                snapshots: vec![snapshot.clone()],
            }
            .validate()?;
            cached_bytes += serde_json::to_vec(snapshot)?.len();
        }
    }
    anyhow::ensure!(
        cached_bytes <= 512 * 1024,
        "connection data cache limit exceeded"
    );
    anyhow::ensure!(
        serde_json::to_vec_pretty(store)?.len() < crate::simulator::MAX_JSON_BYTES,
        "connection store size limit exceeded"
    );
    Ok(())
}

pub(super) fn merge_snapshots(
    store: &ConnectionStore,
    fixtures: &paperphase_core::SnapshotSet,
) -> paperphase_core::SnapshotSet {
    let mut data = fixtures.clone();
    data.snapshots.extend(
        store
            .data
            .values()
            .filter_map(|state| state.snapshot.clone()),
    );
    data
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
fn public_with_status(
    connection: &ConnectionConfig,
    app: &super::App,
    store: &ConnectionStore,
) -> Value {
    let mut value = public_connection(connection, &app.connector_catalog);
    value["fetch"] = super::fetching::public_status(app, store, connection);
    value
}
pub(super) async fn list(State(app): State<Shared>) -> Json<Value> {
    let store = app.connections.lock().await;
    Json(
        json!({"revision":store.revision,"connections":store.connections.iter().map(|connection| public_with_status(connection, &app, &store)).collect::<Vec<_>>(),"execution":if app.executors.is_some() {"compiled"} else {"settings_only"}}),
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
        if app.snapshots.snapshots.iter().any(|snapshot| snapshot.connection_id == id) { return Err(invalid("connection id conflicts with a fixture source")); }
        if app.snapshots.snapshots.len() + store.connections.len() + usize::from(previous.is_none()) > 256 { return Err(invalid("snapshot source limit exceeded")); }
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
        updated.data.entry(id.clone()).or_default().reconfigure(updated.revision, (app.clock)(), request.enabled);
        validate_store(&updated, &app.connector_catalog).map_err(invalid)?;
        let file = app.connections_file.clone(); let to_write = updated.clone();
        tokio::task::spawn_blocking(move || persist(&file, &to_write)).await.map_err(internal)?.map_err(internal)?;
        let response = json!({"revision":updated.revision,"connection":public_connection(updated.connections.iter().find(|connection| connection.id == id).expect("inserted above"), &app.connector_catalog)});
        updated.dirty = false; *store = updated;
        super::fetching::bump_epoch(&app.data_epoch); drop(store); super::fetching::changed(&app).await;
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
        updated.data.remove(&id);
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
        updated.dirty = false;
        *store = updated;
        super::fetching::bump_epoch(&app.data_epoch);
        drop(store);
        super::fetching::changed(&app).await;
        Ok(Json(json!({"revision":revision})))
    })
    .await
    .map_err(internal)?
}
