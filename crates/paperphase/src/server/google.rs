//! Host-owned Google account linking. No OAuth credentials cross the Wasm ABI.
use super::{ApiError, ApiResult, Shared, internal, invalid, persist};
use anyhow::{Context, Result, bail, ensure};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::blocking::Client;
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const SCOPES: [&str; 2] = [
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
    "https://www.googleapis.com/auth/calendar.events.readonly",
];
pub(super) const CALLBACK: &str = "/api/google/callback";
pub(super) const DEFAULT_REDIRECT: &str =
    "https://auth.paperphase.oddorbit.software/oauth/google/callback";
const MAX_ACCOUNTS: usize = 8;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random() -> Result<String> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("Could not create an authorisation session"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()).as_ref())
}
fn clean(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) client_id: String,
    pub(super) client_secret: String,
    pub(super) redirect_uri: String,
}
impl Config {
    fn validate(&self) -> Result<()> {
        ensure!(
            clean(&self.client_id, 1024) && self.client_id.ends_with(".apps.googleusercontent.com"),
            "Enter a Google Web application OAuth client ID"
        );
        ensure!(
            clean(&self.client_secret, 1024),
            "Enter the OAuth client secret"
        );
        let uri =
            url::Url::parse(&self.redirect_uri).context("Enter a valid HTTPS callback URL")?;
        ensure!(
            self.redirect_uri.len() <= 2048
                && uri.scheme() == "https"
                && uri.domain().is_some()
                && uri.username().is_empty()
                && uri.password().is_none()
                && uri.query().is_none()
                && uri.fragment().is_none(),
            "The hosted callback must be an HTTPS URL without credentials, query or fragment"
        );
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Calendar {
    id: String,
    label: String,
    primary: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Account {
    id: String,
    label: String,
    primary_id: String,
    access_token: String,
    refresh_token: String,
    expires_at: u64,
    #[serde(default)]
    requires_reauth: bool,
    #[serde(default)]
    calendars: Vec<Calendar>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    schema_version: u32,
    config: Option<Config>,
    accounts: BTreeMap<String, Account>,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            schema_version: 1,
            config: None,
            accounts: BTreeMap::new(),
        }
    }
}
struct Pending {
    verifier: String,
    label: String,
    return_to: String,
    expires_at: u64,
}
struct Inner {
    saved: Saved,
    pending: BTreeMap<String, Pending>,
    last_link_error: Option<LinkFailure>,
}

// Only these fixed messages may reach a browser or the service log. Never
// forward provider response bodies, URLs, OAuth state, codes or token values.
#[derive(Clone, Serialize)]
struct LinkFailure {
    code: &'static str,
    message: &'static str,
}
impl LinkFailure {
    fn from_error(error: &anyhow::Error) -> Self {
        let (code, message) = match error.to_string().as_str() {
            "Account-linking session is missing or already used" => (
                "missing_session",
                "This login attempt is missing or already used. Start a new link from Connections.",
            ),
            "Account-linking session expired" => (
                "expired_session",
                "This login attempt expired. Start a new link and finish within ten minutes.",
            ),
            "Account-linking address changed; start again from this editor" => (
                "address_changed",
                "The editor address changed during linking. Start again using the same Pi address.",
            ),
            "Google account linking was declined" => (
                "access_denied",
                "Google account linking was declined. Start again and approve both Calendar permissions.",
            ),
            "Google returned no valid authorisation code" => (
                "missing_code",
                "Google returned no valid authorisation code. Start a new link from Connections.",
            ),
            "Google rejected the OAuth client credentials" => (
                "client_rejected",
                "Google rejected the OAuth client. Check the Web application client ID and secret.",
            ),
            "Google authorisation expired or was revoked; reconnect this account" => (
                "invalid_grant",
                "Google rejected the authorisation code or grant. Start a fresh link from the same editor.",
            ),
            "Could not contact Google; try again" => (
                "google_unreachable",
                "The Pi could not contact Google's token service. Check its network and clock, then try again.",
            ),
            "Both read-only Calendar permissions are required"
            | "Google did not grant both required Calendar permissions" => (
                "permissions_missing",
                "Google did not grant both Calendar permissions. Reconnect and approve calendar listing and event reading.",
            ),
            "Google Calendar API is disabled for this OAuth project" => (
                "calendar_api_disabled",
                "Enable Google Calendar API in the Google Cloud project that owns this OAuth client, then try linking again.",
            ),
            "Google calendar access token was rejected" => (
                "calendar_auth",
                "Google rejected access to the calendar list. Start a fresh account link.",
            ),
            "Google calendar list access was denied" => (
                "calendar_permissions",
                "Google denied calendar-list access. Check Calendar permissions and any Workspace administrator restrictions.",
            ),
            "Could not read Google calendars" => (
                "calendar_unreachable",
                "The Pi could not read Google's calendar list. Check its network and try again.",
            ),
            "No primary calendar is available for this account" => (
                "no_primary_calendar",
                "Google returned no primary calendar for this account. Check that Google Calendar is available for the selected account.",
            ),
            "Google did not grant offline access; reconnect and consent again" => (
                "offline_access_missing",
                "Google did not grant offline access. Reconnect and approve the requested permissions.",
            ),
            "Calendar list exceeds the supported 128-calendar limit" => (
                "calendar_limit",
                "This account has more calendars than Paperphase currently supports.",
            ),
            "Could not save Google account" => (
                "storage_failed",
                "The Pi could not save this Google account. Check its available storage and file permissions.",
            ),
            "OAuth configuration changed" | "OAuth configuration changed during authorisation" => (
                "configuration_changed",
                "Google setup changed during linking. Start a new attempt from Connections.",
            ),
            _ => (
                "link_failed",
                "Google account linking could not be completed. Check Calendar API access and try again.",
            ),
        };
        Self { code, message }
    }
}

#[derive(Clone)]
struct Endpoints {
    auth: String,
    token: String,
    calendars: String,
}
impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            calendars: "https://www.googleapis.com/calendar/v3/users/me/calendarList".into(),
        }
    }
}
pub(super) struct Service {
    file: PathBuf,
    client: Client,
    inner: Mutex<Inner>,
    endpoints: Endpoints,
}

#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
    #[serde(default)]
    scope: Option<String>,
}
impl Tokens {
    fn validate(&self) -> Result<()> {
        ensure!(
            clean(&self.access_token, 8192)
                && self.token_type.eq_ignore_ascii_case("bearer")
                && (1..=86400).contains(&self.expires_in),
            "Google returned invalid token metadata"
        );
        ensure!(
            self.refresh_token.as_ref().is_none_or(|v| clean(v, 8192)),
            "Google returned invalid refresh metadata"
        );
        if let Some(scope) = &self.scope {
            ensure!(
                SCOPES
                    .iter()
                    .all(|required| scope.split_whitespace().any(|s| s == *required)),
                "Both read-only Calendar permissions are required"
            );
        }
        Ok(())
    }
}

impl Service {
    pub(super) fn open(file: PathBuf, client: Client) -> Result<Self> {
        let saved: Saved = if file.try_exists()? {
            super::read_json(&file)?
        } else {
            Saved::default()
        };
        ensure!(
            saved.schema_version == 1 && saved.accounts.len() <= MAX_ACCOUNTS,
            "Invalid Google account store"
        );
        if let Some(config) = &saved.config {
            config.validate()?;
        }
        for (id, account) in &saved.accounts {
            ensure!(
                id == &account.id
                    && paperphase_core::valid_id(id)
                    && clean(&account.label, 128)
                    && clean(&account.access_token, 8192)
                    && clean(&account.refresh_token, 8192)
                    && account.calendars.len() <= 128,
                "Invalid saved Google account"
            );
        }
        Ok(Self {
            file,
            client,
            inner: Mutex::new(Inner {
                saved,
                pending: BTreeMap::new(),
                last_link_error: None,
            }),
            endpoints: Endpoints::default(),
        })
    }
    pub(super) fn public(&self) -> Result<Value> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
        Ok(
            json!({"configured":inner.saved.config.is_some(),"client_id":inner.saved.config.as_ref().map(|c|&c.client_id),"client_secret_present":inner.saved.config.is_some(),"redirect_uri":inner.saved.config.as_ref().map_or(DEFAULT_REDIRECT,|c|c.redirect_uri.as_str()),"scopes":SCOPES,"last_link_error":inner.last_link_error,"accounts":inner.saved.accounts.values().map(|a|json!({"id":a.id,"label":a.label,"requires_reauth":a.requires_reauth,"calendars":a.calendars})).collect::<Vec<_>>()}),
        )
    }
    fn configure(&self, mut config: Config) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
        if config.client_secret.is_empty()
            && let Some(old) = &inner.saved.config
            && old.client_id == config.client_id
        {
            config.client_secret = old.client_secret.clone();
        }
        config.validate()?;
        ensure!(
            inner.saved.accounts.is_empty()
                || inner
                    .saved
                    .config
                    .as_ref()
                    .is_some_and(|old| old.client_id == config.client_id),
            "Disconnect existing accounts before changing the OAuth client"
        );
        let mut saved = inner.saved.clone();
        saved.config = Some(config);
        persist(&self.file, &saved)?;
        inner.saved = saved;
        inner.pending.clear();
        inner.last_link_error = None;
        Ok(())
    }
    fn begin(&self, label: &str, return_to: &str) -> Result<String> {
        let label = label.trim();
        ensure!(clean(label, 128), "Enter a label for this Google account");
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
        let config = inner
            .saved
            .config
            .clone()
            .context("Configure Google OAuth first")?;
        inner.pending.retain(|_, p| p.expires_at > now());
        ensure!(
            inner.pending.len() < 8,
            "Too many account-linking windows are open; wait or finish an existing one"
        );
        let nonce = random()?;
        let verifier = random()?;
        let state = URL_SAFE_NO_PAD.encode(serde_json::to_vec(
            &json!({"nonce":nonce,"return_to":return_to}),
        )?);
        let mut url = url::Url::parse(&self.endpoints.auth)?;
        url.query_pairs_mut()
            .append_pair("client_id", &config.client_id)
            .append_pair("redirect_uri", &config.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &SCOPES.join(" "))
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent select_account")
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge(&verifier))
            .append_pair("code_challenge_method", "S256");
        inner.pending.insert(
            state,
            Pending {
                verifier,
                label: label.into(),
                return_to: return_to.into(),
                expires_at: now() + 600,
            },
        );
        inner.last_link_error = None;
        Ok(url.into())
    }
    fn token_request(&self, config: &Config, grant: &[(&str, &str)]) -> Result<Tokens> {
        let mut body = url::form_urlencoded::Serializer::new(String::new());
        body.append_pair("client_id", &config.client_id)
            .append_pair("client_secret", &config.client_secret);
        for (key, value) in grant {
            body.append_pair(key, value);
        }
        let response = self
            .client
            .post(&self.endpoints.token)
            .timeout(Duration::from_secs(10))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body.finish())
            .send()
            .map_err(|_| anyhow::anyhow!("Could not contact Google; try again"))?;
        let status = response.status();
        let mut bytes = Vec::new();
        response.take(16385).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16384, "Google token response exceeds budget");
        if !status.is_success() {
            let reply: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            if reply["error"] == "invalid_client" || reply["error"] == "unauthorized_client" {
                bail!("Google rejected the OAuth client credentials");
            }
            if reply["error"] == "invalid_grant" {
                bail!("Google authorisation expired or was revoked; reconnect this account");
            }
            bail!("Google token request failed; check the OAuth client and reconnect");
        }
        let tokens: Tokens = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Invalid Google token response"))?;
        tokens.validate()?;
        Ok(tokens)
    }
    fn calendars_for(&self, token: &str) -> Result<Vec<Calendar>> {
        let mut url = url::Url::parse(&self.endpoints.calendars)?;
        url.query_pairs_mut()
            .append_pair("minAccessRole", "reader")
            .append_pair("maxResults", "250")
            .append_pair("fields", "nextPageToken,items(id,summary,primary)");
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(10))
            .bearer_auth(token)
            .send()
            .map_err(|_| anyhow::anyhow!("Could not read Google calendars"))?;
        let status = response.status();
        let mut bytes = Vec::new();
        response.take(256 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 256 * 1024,
            "Google calendar list exceeds budget"
        );
        let value: Value =
            serde_json::from_slice(&bytes).context("Invalid Google calendar list")?;
        if !status.is_success() {
            // Google's API errors carry diagnostic reasons separately from the
            // human message. Inspect reasons, but never expose the raw payload.
            let reasons = value["error"]["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(value["error"]["details"].as_array().into_iter().flatten())
                .filter_map(|item| item["reason"].as_str())
                .collect::<Vec<_>>();
            if reasons.iter().any(|r| {
                matches!(
                    *r,
                    "accessNotConfigured" | "SERVICE_DISABLED" | "API_DISABLED"
                )
            }) {
                bail!("Google Calendar API is disabled for this OAuth project");
            }
            if reasons.iter().any(|r| {
                matches!(
                    *r,
                    "insufficientPermissions" | "ACCESS_TOKEN_SCOPE_INSUFFICIENT"
                )
            }) {
                bail!("Google did not grant both required Calendar permissions");
            }
            if status.as_u16() == 401 {
                bail!("Google calendar access token was rejected");
            }
            if status.as_u16() == 403 {
                bail!("Google calendar list access was denied");
            }
            bail!("Google calendar list request failed; check Calendar API access and permissions");
        }
        let items = value["items"]
            .as_array()
            .context("Invalid Google calendar collection")?;
        ensure!(
            items.len() <= 128
                && value
                    .get("nextPageToken")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty),
            "Calendar list exceeds the supported 128-calendar limit"
        );
        items
            .iter()
            .map(|item| {
                let id = item["id"]
                    .as_str()
                    .filter(|s| clean(s, 1024))
                    .context("Invalid Google calendar identifier")?;
                let label = item["summary"]
                    .as_str()
                    .unwrap_or(id)
                    .chars()
                    .take(128)
                    .collect();
                Ok(Calendar {
                    id: id.into(),
                    label,
                    primary: item["primary"].as_bool().unwrap_or(false),
                })
            })
            .collect()
    }
    fn finish(
        &self,
        state: &str,
        code: Option<&str>,
        error: Option<&str>,
        host: &str,
    ) -> Result<()> {
        let mut known = false;
        let result = self.finish_inner(state, code, error, host, &mut known);
        // Unsolicited/replayed callbacks must not overwrite a real attempt's
        // diagnostic. A known session is consumed by finish_inner on every path.
        if known {
            let failure = result.as_ref().err().map(LinkFailure::from_error);
            if let Some(failure) = &failure {
                eprintln!("Google account linking failed ({})", failure.code);
            }
            self.inner
                .lock()
                .map_err(|_| anyhow::anyhow!("Account store unavailable"))?
                .last_link_error = failure;
        }
        result
    }
    fn finish_inner(
        &self,
        state: &str,
        code: Option<&str>,
        error: Option<&str>,
        host: &str,
        known: &mut bool,
    ) -> Result<()> {
        let (pending, config) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
            let pending = inner
                .pending
                .remove(state)
                .context("Account-linking session is missing or already used")?;
            *known = true;
            ensure!(
                pending.expires_at > now(),
                "Account-linking session expired"
            );
            ensure!(
                pending.return_to == format!("http://{host}{CALLBACK}"),
                "Account-linking address changed; start again from this editor"
            );
            let config = inner
                .saved
                .config
                .clone()
                .context("OAuth configuration changed")?;
            (pending, config)
        };
        ensure!(error.is_none(), "Google account linking was declined");
        let code = code
            .filter(|s| clean(s, 4096))
            .context("Google returned no valid authorisation code")?;
        let tokens = self.token_request(
            &config,
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("code_verifier", &pending.verifier),
                ("redirect_uri", &config.redirect_uri),
            ],
        )?;
        let calendars = self.calendars_for(&tokens.access_token)?;
        let primary = calendars
            .iter()
            .find(|c| c.primary)
            .map(|c| c.id.clone())
            .context("No primary calendar is available for this account")?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
        ensure!(
            inner.saved.config.as_ref().is_some_and(
                |c| c.client_id == config.client_id && c.redirect_uri == config.redirect_uri
            ),
            "OAuth configuration changed during authorisation"
        );
        let existing = inner
            .saved
            .accounts
            .values()
            .find(|a| a.primary_id == primary);
        ensure!(
            existing.is_some() || inner.saved.accounts.len() < MAX_ACCOUNTS,
            "Google account limit reached"
        );
        let id = existing
            .map(|a| a.id.clone())
            .unwrap_or(format!("google.{}", &random()?[..16]));
        let refresh = tokens
            .refresh_token
            .or_else(|| existing.map(|a| a.refresh_token.clone()))
            .context("Google did not grant offline access; reconnect and consent again")?;
        let account = Account {
            id: id.clone(),
            label: pending.label,
            primary_id: primary,
            access_token: tokens.access_token,
            refresh_token: refresh,
            expires_at: now() + tokens.expires_in,
            requires_reauth: false,
            calendars,
        };
        let mut saved = inner.saved.clone();
        saved.accounts.insert(id, account);
        persist(&self.file, &saved).context("Could not save Google account")?;
        inner.saved = saved;
        Ok(())
    }
    pub(super) fn access_token(&self, id: &str) -> Result<String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
        let account = inner
            .saved
            .accounts
            .get(id)
            .context("Link the selected Google account first")?;
        ensure!(!account.requires_reauth, "Reconnect this Google account");
        if account.expires_at > now() + 60 {
            return Ok(account.access_token.clone());
        }
        let config = inner
            .saved
            .config
            .clone()
            .context("Configure Google OAuth first")?;
        let refresh = account.refresh_token.clone();
        let tokens = self.token_request(
            &config,
            &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
        );
        let tokens = match tokens {
            Ok(tokens) => tokens,
            Err(error) => {
                if error.to_string().contains("expired or was revoked") {
                    let mut saved = inner.saved.clone();
                    saved
                        .accounts
                        .get_mut(id)
                        .expect("account exists")
                        .requires_reauth = true;
                    persist(&self.file, &saved)?;
                    inner.saved = saved;
                }
                return Err(error);
            }
        };
        let mut saved = inner.saved.clone();
        let account = saved.accounts.get_mut(id).expect("account exists");
        account.access_token = tokens.access_token;
        account.expires_at = now() + tokens.expires_in;
        if let Some(refresh) = tokens.refresh_token {
            account.refresh_token = refresh;
        }
        let token = account.access_token.clone();
        persist(&self.file, &saved)?;
        inner.saved = saved;
        Ok(token)
    }
    pub(super) fn validate_selection(&self, id: &str, calendar: &str) -> Result<()> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
        let account = inner
            .saved
            .accounts
            .get(id)
            .context("Link the selected Google account first")?;
        ensure!(
            calendar == "primary" || account.calendars.iter().any(|c| c.id == calendar),
            "Choose a calendar belonging to this linked account"
        );
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Account store unavailable"))?;
        let mut saved = inner.saved.clone();
        ensure!(
            saved.accounts.remove(id).is_some(),
            "Google account not found"
        );
        persist(&self.file, &saved)?;
        inner.saved = saved;
        Ok(())
    }
}

fn service(app: &Shared) -> ApiResult<Arc<Service>> {
    app.google
        .clone()
        .ok_or_else(|| invalid("Google account support is unavailable"))
}
pub(super) async fn status(State(app): State<Shared>) -> ApiResult<Json<Value>> {
    let google = service(&app)?;
    Ok(Json(
        tokio::task::spawn_blocking(move || google.public())
            .await
            .map_err(internal)?
            .map_err(internal)?,
    ))
}
pub(super) async fn configure(
    State(app): State<Shared>,
    Json(config): Json<Config>,
) -> ApiResult<Json<Value>> {
    let google = service(&app)?;
    tokio::task::spawn_blocking(move || google.configure(config))
        .await
        .map_err(internal)?
        .map_err(invalid)?;
    status(State(app)).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Begin {
    label: String,
}
pub(super) async fn begin(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(request): Json<Begin>,
) -> ApiResult<Json<Value>> {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| invalid("Open the editor directly"))?;
    let return_to = format!("http://{host}{CALLBACK}");
    let google = service(&app)?;
    let url = tokio::task::spawn_blocking(move || google.begin(&request.label, &return_to))
        .await
        .map_err(internal)?
        .map_err(invalid)?;
    Ok(Json(json!({"url":url})))
}
#[derive(Deserialize)]
pub(super) struct Callback {
    state: String,
    code: Option<String>,
    error: Option<String>,
}
pub(super) async fn callback(
    State(app): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<Callback>,
) -> Response {
    let Ok(google) = service(&app) else {
        return Redirect::to("/?google=failed").into_response();
    };
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let result = tokio::task::spawn_blocking(move || {
        google.finish(
            &query.state,
            query.code.as_deref(),
            query.error.as_deref(),
            &host,
        )
    })
    .await;
    Redirect::to(if result.is_ok_and(|r| r.is_ok()) {
        "/?google=connected"
    } else {
        "/?google=failed"
    })
    .into_response()
}
pub(super) async fn remove(
    State(app): State<Shared>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let store = app.connections.lock().await;
    if store.connections.iter().any(|c| {
        c.connector_id == "calendar.google"
            && c.settings.get("account_id").and_then(Value::as_str) == Some(&id)
    }) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Remove this account's calendar connections before disconnecting it".into(),
        ));
    }
    let google = service(&app)?;
    tokio::task::spawn_blocking(move || google.remove(&id))
        .await
        .map_err(internal)?
        .map_err(invalid)?;
    drop(store);
    status(State(app)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener, thread};

    fn config() -> Config {
        Config {
            client_id: "fixture.apps.googleusercontent.com".into(),
            client_secret: "fixture-client-secret".into(),
            redirect_uri: DEFAULT_REDIRECT.into(),
        }
    }
    fn service() -> (tempfile::TempDir, Service) {
        let dir = tempfile::tempdir().unwrap();
        let client = paperphase_runtime::shared_http_client(Duration::from_secs(2)).unwrap();
        let service = Service::open(dir.path().join("google.json"), client).unwrap();
        service.configure(config()).unwrap();
        (dir, service)
    }
    fn state(url: &str) -> String {
        url::Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned()
    }
    fn mock(replies: Vec<(u16, Value)>) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let mut requests = vec![];
            for (status, reply) in replies {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = vec![];
                let mut buffer = [0; 4096];
                loop {
                    let count = socket.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(boundary) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..boundary]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|s| s.strip_prefix("content-length:"))
                            .map(|s| s.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if bytes.len() >= boundary + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                let body = reply.to_string();
                write!(socket,"HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        (address, handle)
    }
    fn tokens(access: &str, refresh: Option<&str>) -> Value {
        let mut value = json!({"access_token":access,"token_type":"Bearer","expires_in":3600,"scope":SCOPES.join(" ")});
        if let Some(refresh) = refresh {
            value["refresh_token"] = json!(refresh);
        }
        value
    }
    fn calendars() -> Value {
        json!({"items":[{"id":"person@example.org","summary":"Personal","primary":true},{"id":"shared@example.org","summary":"Shared"}]})
    }

    #[test]
    fn sessions_route_custom_hosts_and_are_isolated_between_pis() {
        let (_dir, first) = service();
        let (_other, second) = service();
        let return_to = "http://kitchen-pi.local:8080/api/google/callback";
        let auth = first.begin("Personal", return_to).unwrap();
        let state = state(&auth);
        let payload: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&state).unwrap()).unwrap();
        assert_eq!(payload["return_to"], return_to);
        assert_eq!(payload["nonce"].as_str().unwrap().len(), 43);
        let verifier = first.inner.lock().unwrap().pending[&state].verifier.clone();
        let url = url::Url::parse(&auth).unwrap();
        let params: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params["code_challenge"], challenge(&verifier));
        assert_eq!(params["redirect_uri"], DEFAULT_REDIRECT);
        assert!(!auth.contains(&verifier) && !auth.contains("fixture-client-secret"));
        assert!(
            second
                .finish(&state, Some("fixture-code"), None, "kitchen-pi.local:8080")
                .is_err()
        );
        assert!(first.inner.lock().unwrap().pending.contains_key(&state));
        assert!(
            first
                .finish(&state, Some("fixture-code"), None, "another-pi.local:8080")
                .is_err()
        );
        assert!(
            first
                .finish(&state, Some("fixture-code"), None, "kitchen-pi.local:8080")
                .is_err()
        );
        let state = super::tests::state(&first.begin("Personal", return_to).unwrap());
        first
            .inner
            .lock()
            .unwrap()
            .pending
            .get_mut(&state)
            .unwrap()
            .expires_at = 0;
        assert!(
            first
                .finish(&state, Some("fixture-code"), None, "kitchen-pi.local:8080")
                .is_err()
        );
    }

    #[test]
    fn linking_refresh_rotation_redaction_and_private_persistence() {
        let (dir, mut service) = service();
        let (address, server) = mock(vec![
            (200, tokens("fixture-access", Some("fixture-refresh"))),
            (200, calendars()),
            (
                200,
                tokens("fixture-new-access", Some("fixture-rotated-refresh")),
            ),
        ]);
        service.endpoints.token = format!("{address}/token");
        service.endpoints.calendars = format!("{address}/calendars");
        let state = state(
            &service
                .begin("Work", "http://192.168.1.53:8080/api/google/callback")
                .unwrap(),
        );
        service
            .finish(&state, Some("fixture-code"), None, "192.168.1.53:8080")
            .unwrap();
        assert!(
            service
                .finish(&state, Some("fixture-code"), None, "192.168.1.53:8080")
                .is_err()
        );
        let public = service.public().unwrap();
        let id = public["accounts"][0]["id"].as_str().unwrap();
        assert!(paperphase_core::valid_id(id));
        assert_eq!(
            public["accounts"][0]["calendars"].as_array().unwrap().len(),
            2
        );
        service
            .validate_selection(id, "shared@example.org")
            .unwrap();
        assert!(
            service
                .validate_selection(id, "unlinked@example.org")
                .is_err()
        );
        for value in ["fixture-access", "fixture-refresh", "fixture-client-secret"] {
            assert!(!public.to_string().contains(value));
        }
        assert_eq!(service.access_token(id).unwrap(), "fixture-access");
        service
            .inner
            .lock()
            .unwrap()
            .saved
            .accounts
            .get_mut(id)
            .unwrap()
            .expires_at = 0;
        assert_eq!(service.access_token(id).unwrap(), "fixture-new-access");
        let saved = std::fs::read_to_string(dir.path().join("google.json")).unwrap();
        assert!(
            saved.contains("fixture-rotated-refresh") && !saved.contains("\"fixture-refresh\"")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join("google.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let requests = server.join().unwrap();
        let form: BTreeMap<_, _> =
            url::form_urlencoded::parse(requests[0].split("\r\n\r\n").nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        assert_eq!(form["code_verifier"].len(), 43);
        assert_eq!(form["redirect_uri"], DEFAULT_REDIRECT);
        assert!(
            requests[1]
                .to_lowercase()
                .contains("authorization: bearer fixture-access")
        );
        assert!(requests[2].contains("refresh_token=fixture-refresh"));
        service.remove(id).unwrap();
        assert!(
            service.public().unwrap()["accounts"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn revoked_refresh_requires_reauthorisation_and_does_not_retry_forever() {
        let (_dir, mut service) = service();
        let (address, server) = mock(vec![
            (200, tokens("fixture-access", Some("fixture-refresh"))),
            (200, calendars()),
            (
                400,
                json!({"error":"invalid_grant","error_description":"private-provider-detail"}),
            ),
        ]);
        service.endpoints.token = format!("{address}/token");
        service.endpoints.calendars = format!("{address}/calendars");
        let state = state(
            &service
                .begin("Personal", "http://lounge-pi:8080/api/google/callback")
                .unwrap(),
        );
        service
            .finish(&state, Some("fixture-code"), None, "lounge-pi:8080")
            .unwrap();
        let id = service.public().unwrap()["accounts"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        service
            .inner
            .lock()
            .unwrap()
            .saved
            .accounts
            .get_mut(&id)
            .unwrap()
            .expires_at = 0;
        let error = service.access_token(&id).unwrap_err().to_string();
        assert!(!error.contains("private-provider-detail"));
        assert!(
            service.public().unwrap()["accounts"][0]["requires_reauth"]
                .as_bool()
                .unwrap()
        );
        assert!(service.access_token(&id).is_err());
        assert_eq!(server.join().unwrap().len(), 3);
    }

    #[test]
    fn calendar_api_failures_report_fixed_guidance_without_provider_details() {
        for (reason, modern, expected) in [
            ("accessNotConfigured", false, "calendar_api_disabled"),
            ("SERVICE_DISABLED", true, "calendar_api_disabled"),
            (
                "ACCESS_TOKEN_SCOPE_INSUFFICIENT",
                true,
                "permissions_missing",
            ),
        ] {
            let (_dir, mut service) = service();
            let mut error = json!({"error":{"message":"private-provider-detail with fixture-access and fixture-code"}});
            error["error"][if modern { "details" } else { "errors" }] =
                json!([{"reason":reason,"metadata":{"private":"private-project-detail"}}]);
            let (address, server) = mock(vec![
                (200, tokens("fixture-access", Some("fixture-refresh"))),
                (403, error),
            ]);
            service.endpoints.token = format!("{address}/token");
            service.endpoints.calendars = format!("{address}/calendars");
            let state = state(
                &service
                    .begin(
                        "Personal",
                        "http://paperphase.local:8080/api/google/callback",
                    )
                    .unwrap(),
            );
            assert!(
                service
                    .finish(&state, Some("fixture-code"), None, "paperphase.local:8080")
                    .is_err()
            );
            let public = service.public().unwrap();
            assert_eq!(public["last_link_error"]["code"], expected);
            assert!(public["accounts"].as_array().unwrap().is_empty());
            let encoded = public.to_string();
            for private in [
                "private-provider-detail",
                "private-project-detail",
                "fixture-access",
                "fixture-refresh",
                "fixture-client-secret",
                "fixture-code",
                &state,
            ] {
                assert!(!encoded.contains(private));
            }
            assert!(
                service
                    .finish(&state, Some("fixture-code"), None, "paperphase.local:8080")
                    .is_err()
            );
            assert_eq!(
                service.public().unwrap()["last_link_error"],
                public["last_link_error"],
                "A replay cannot overwrite the useful error"
            );
            assert_eq!(server.join().unwrap().len(), 2);
            let _ = service
                .begin(
                    "Personal",
                    "http://paperphase.local:8080/api/google/callback",
                )
                .unwrap();
            assert!(service.public().unwrap()["last_link_error"].is_null());
        }
    }

    #[test]
    fn token_client_errors_and_unknown_failures_do_not_reflect_private_values() {
        let (_dir, mut service) = service();
        let (address, server) = mock(vec![(
            401,
            json!({"error":"invalid_client","error_description":"private-provider-detail fixture-client-secret"}),
        )]);
        service.endpoints.token = format!("{address}/token");
        let state = state(
            &service
                .begin("Personal", "http://paperphase:8080/api/google/callback")
                .unwrap(),
        );
        assert!(
            service
                .finish(&state, Some("fixture-code"), None, "paperphase:8080")
                .is_err()
        );
        let public = service.public().unwrap();
        assert_eq!(public["last_link_error"]["code"], "client_rejected");
        assert!(!public.to_string().contains("private-provider-detail"));
        assert!(!public.to_string().contains("fixture-client-secret"));
        let safe = serde_json::to_string(&LinkFailure::from_error(&anyhow::anyhow!(
            "private-provider-detail fixture-client-secret"
        )))
        .unwrap();
        assert!(
            !safe.contains("private-provider-detail") && !safe.contains("fixture-client-secret")
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }
}
