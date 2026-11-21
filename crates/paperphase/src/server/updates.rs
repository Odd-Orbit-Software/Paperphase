//! Owner update API and display hand-off. Activation belongs to the independent agent.
use super::{ApiError, ApiResult, Shared, internal, invalid, scheduling};
use anyhow::{Context, Result, ensure};
use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
};
use paperphase::update::{
    agent, package, releases,
    store::{self, Installation, Job, Phase},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub(super) struct Service {
    pub config: Installation,
    pub launch_id: String,
    hold: Option<String>,
    start: Mutex<()>,
}
impl Service {
    pub fn from_environment() -> Result<Option<Arc<Self>>> {
        let Some(path) = std::env::var_os("PAPERPHASE_INSTALLATION") else {
            return Ok(None);
        };
        let path = PathBuf::from(path);
        let config = Installation::load(&path)?;
        config.init()?;
        let hold = std::env::var("PAPERPHASE_UPDATE_HOLD").ok();
        ensure!(
            hold.as_ref().is_none_or(|id| store::job_id(id)),
            "invalid supervisor launch transaction"
        );
        Ok(Some(Arc::new(Self {
            config,
            launch_id: std::env::var("PAPERPHASE_LAUNCH_ID").unwrap_or_default(),
            hold,
            start: Mutex::new(()),
        })))
    }
    pub fn holding_startup(&self) -> bool {
        self.hold.as_ref().is_some_and(|id| {
            self.config
                .job_dir(id)
                .and_then(|dir| store::read::<Job>(&dir.join("job.json")))
                .map_or(true, |job| {
                    job.id != *id
                        || !matches!(
                            job.phase,
                            Phase::Healthy | Phase::RolledBack | Phase::Failed
                        )
                })
        })
    }
    pub fn maintenance_finished(&self, id: &str) -> bool {
        self.config
            .job_dir(id)
            .and_then(|dir| store::read::<Job>(&dir.join("job.json")))
            .is_ok_and(|job| job.id == id && job.phase.terminal())
    }
}
fn service(app: &Shared) -> ApiResult<Arc<Service>> {
    app.updates.as_ref().cloned().ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "This installation needs the update supervisor set up before UI updates can run."
                .into(),
        )
    })
}
use paperphase::update::store::public_job;
pub(super) async fn status(State(app): State<Shared>) -> ApiResult<Json<Value>> {
    let Some(service) = &app.updates else {
        return Ok(Json(
            json!({"managed":false,"version":env!("CARGO_PKG_VERSION"),"targets":[],"job":null}),
        ));
    };
    let active = service.config.active().map_err(internal)?;
    let targets=service.config.targets.iter().map(|(id,target)|{
        let installed=active.slots.get(id).map(|slot|json!({"version":slot.manifest.version,"build":slot.manifest.build,"tag":slot.manifest.release}));
        json!({"id":id,"name":target.label,"repository":target.repository,"installed":installed,"publisher_configured":!target.trusted_keys.is_empty()})
    }).collect::<Vec<_>>();
    Ok(Json(
        json!({"managed":true,"status_port":service.config.status_port,"version":env!("CARGO_PKG_VERSION"),"targets":targets,"job":service.config.job().map_err(internal)?.map(public_job)}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Check {
    target: String,
    #[serde(default)]
    experimental: bool,
    #[serde(default)]
    release_url: Option<String>,
}
pub(super) async fn check(
    State(app): State<Shared>,
    Json(request): Json<Check>,
) -> ApiResult<Json<Value>> {
    let service = service(&app)?;
    if request.target == "all" && request.release_url.is_none() {
        let targets = service.config.targets.clone();
        let result = tokio::task::spawn_blocking(move || {
            releases::check_many(&targets, request.experimental)
        })
        .await
        .map_err(internal)?
        .map_err(invalid)?;
        return Ok(Json(serde_json::to_value(result).map_err(internal)?));
    }
    let target = service
        .config
        .targets
        .get(&request.target)
        .cloned()
        .ok_or_else(|| invalid("Unknown installed update target."))?;
    let tag = request
        .release_url
        .as_deref()
        .map(|url| releases::release_tag(url, &target.repository))
        .transpose()
        .map_err(invalid)?;
    let result = tokio::task::spawn_blocking(move || {
        releases::check(&target, request.experimental, tag.as_deref())
    })
    .await
    .map_err(internal)?
    .map_err(invalid)?;
    Ok(Json(json!({"releases":result})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Install {
    target: String,
    tag: String,
    #[serde(default)]
    experimental: bool,
}
pub(super) async fn install(
    State(app): State<Shared>,
    Json(request): Json<Install>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let service = service(&app)?;
    let _start = service
        .start
        .try_lock()
        .map_err(|_| invalid("Another update is being prepared."))?;
    let target = service
        .config
        .targets
        .get(&request.target)
        .ok_or_else(|| invalid("Unknown installed target."))?;
    if target.trusted_keys.is_empty() {
        return Err(invalid(
            "A trusted publisher key is needed before this target can be updated.",
        ));
    }
    let job = service.config.start_job(&request.target).map_err(invalid)?;
    let response = public_job(job.clone());
    let work = Arc::clone(&service);
    tokio::spawn(async move {
        finish(
            app,
            work,
            job,
            Some((request.tag, request.experimental)),
            None,
        )
        .await;
    });
    Ok((StatusCode::ACCEPTED, Json(response)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Upload {
    target: String,
}
pub(super) async fn upload(
    State(app): State<Shared>,
    Query(request): Query<Upload>,
    bytes: Bytes,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let service = service(&app)?;
    let _start = service
        .start
        .try_lock()
        .map_err(|_| invalid("Another update is being prepared."))?;
    let target = service
        .config
        .targets
        .get(&request.target)
        .ok_or_else(|| invalid("Unknown installed target."))?;
    if target.trusted_keys.is_empty() {
        return Err(invalid(
            "A trusted publisher key is needed before installing a package.",
        ));
    }
    if bytes.is_empty() || bytes.len() as u64 > package::DOWNLOAD_LIMIT {
        return Err(invalid(
            "The release package is empty or exceeds the download limit.",
        ));
    }
    let job = service.config.start_job(&request.target).map_err(invalid)?;
    let response = public_job(job.clone());
    let work = Arc::clone(&service);
    tokio::spawn(async move {
        finish(app, work, job, None, Some(bytes)).await;
    });
    Ok((StatusCode::ACCEPTED, Json(response)))
}
async fn finish(
    app: Shared,
    service: Arc<Service>,
    mut job: Job,
    release: Option<(String, bool)>,
    upload: Option<Bytes>,
) {
    let work = Arc::clone(&service);
    let mut copy = job.clone();
    let result=tokio::task::spawn_blocking(move||->Result<Job>{
        let target=work.config.targets.get(&copy.target).context("unknown update target")?;
        let path=work.config.job_dir(&copy.id)?.join("package.ppkg");
        let mut selected_tag=None;
        if let Some(bytes)=upload {std::fs::write(&path,&bytes)?;}
        else if let Some((tag,experimental))=release {
            let release=releases::check(target,experimental,Some(&tag))?.into_iter().next().context("No compatible package was found in the selected release. Experimental releases require opt-in.")?;
            releases::download(target,&release,&path,|done,total|{copy.message=format!("Downloading update… {}%",done.saturating_mul(100)/total.max(1));work.config.save_job(&copy)})?;
            // Verify the exact selected release, not only its publisher/repository.
            selected_tag=Some(tag);
        }
        copy.phase=Phase::Verifying;copy.message="Checking the publisher, compatibility and package files…".into();work.config.save_job(&copy)?;
        let temp=tempfile::tempdir_in(work.config.job_dir(&copy.id)?)?;
        let manifest=package::extract(&path,temp.path(),&target.trusted_keys,&target.repository,selected_tag.as_deref(),&target.target)?;
        agent::validate_payload(&manifest,temp.path())?;
        copy.manifest=Some(manifest);copy.phase=Phase::WaitingForDisplay;copy.message="Waiting for the Updating screen. Display cooldowns still apply…".into();work.config.save_job(&copy)?;Ok(copy)
    }).await;
    let result = match result {
        Ok(result) => result,
        Err(error) => Err(error.into()),
    };
    let result = match result {
        Ok(prepared) => {
            job = prepared;
            async {
            scheduling::begin_maintenance(Arc::clone(&app),&job.id).await.map_err(|e|anyhow::anyhow!("{}",e.1))?;
            let deadline=tokio::time::Instant::now()+Duration::from_secs(600);
            loop {
                if app.stopping.load(std::sync::atomic::Ordering::SeqCst){anyhow::bail!("Update preparation interrupted; installed version unchanged.");}
                let ack={let ws=app.workspace.lock().await;ws.maintenance_ack.clone()};
                if let Some(ack)=ack {job.display_ack=Some(ack);break;}
                ensure!(tokio::time::Instant::now()<deadline,"The Updating screen was not acknowledged in time. Your installed version is unchanged.");
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            job.phase=Phase::ReadyForActivation;job.message="Updating screen confirmed. Preparing the restart…".into();service.config.save_job(&job)?;Ok(())
        }.await
        }
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("update preparation failed: {error:#}");
        job.phase = Phase::Failed;
        job.message =
            format!("Update preparation failed: {error}. Your installed version is unchanged.");
        if let Err(error) = service.config.save_job(&job) {
            eprintln!("saving update failure: {error:#}");
        }
        let mut ws = app.workspace.lock().await;
        if ws.maintenance.as_deref() == Some(&job.id) {
            ws.maintenance = None;
            ws.maintenance_ack = None;
            ws.pending = true;
            ws.refresh_wake.notify_one();
        }
    }
}
pub(super) async fn health(State(app): State<Shared>) -> Json<Value> {
    Json(
        json!({"ready":true,"pid":std::process::id(),"launch_id":app.updates.as_ref().map(|u|u.launch_id.as_str()).unwrap_or(""),"version":env!("CARGO_PKG_VERSION")}),
    )
}

/// No new provider/reference work starts while output is held for activation or health.
pub(super) async fn network_held(app: &Shared) -> bool {
    app.stopping.load(std::sync::atomic::Ordering::SeqCst)
        || app
            .updates
            .as_ref()
            .is_some_and(|service| service.holding_startup())
        || app.workspace.lock().await.maintenance.is_some()
}
