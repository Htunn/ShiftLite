//! `/api/v1/namespaces/{namespace}/apps*` handlers (SPEC-001 Acceptance Criteria: Applications).

use crate::error::{ApiResult, AppError};
use crate::session::AuthSession;
use crate::state::AppState;
use crate::types::{AppDetail, AppSummary, CreateAppPayload};
use crate::validate::{is_dns1123_label, is_https_git_url};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use kube::api::{Api, DeleteParams, PostParams};
use kube::ResourceExt;
use serde::Deserialize;
use shiftlite_core::{ApiError, ClusterStatus, FieldError, Role};
use shiftlite_crd::{
    AppDeployment, AppDeploymentSpec, EnvVar as CrdEnvVar, RouteSpec, SecretRef as CrdSecretRef,
};

#[derive(Debug, Deserialize)]
pub struct ClusterQuery {
    pub cluster: Option<String>,
}

fn to_summary(app: &AppDeployment, cluster_id: &str) -> AppSummary {
    let status = app.status.as_ref();
    AppSummary {
        name: app.name_any(),
        namespace: app.namespace().unwrap_or_default(),
        target_cluster: cluster_id.to_string(),
        status: status
            .and_then(|s| s.phase)
            .unwrap_or(shiftlite_core::AppPhase::Pending),
        route_url: status.and_then(|s| s.route_url.clone()),
    }
}

fn to_detail(app: &AppDeployment, cluster_id: &str) -> AppDetail {
    let status = app.status.clone().unwrap_or_default();
    AppDetail {
        summary: to_summary(app, cluster_id),
        git_repository: app.spec.git_repository.clone(),
        branch: app.spec.branch.clone(),
        builder: app.spec.builder,
        replicas: app.spec.replicas,
        available_replicas: status.available_replicas.unwrap_or(0),
        message: status.message,
        build_pod_name: status.build_pod_name,
        image_ref: status.image_ref,
    }
}

pub async fn list_apps(
    State(state): State<AppState>,
    session: AuthSession,
    Path(namespace): Path<String>,
    Query(q): Query<ClusterQuery>,
) -> ApiResult<Json<Vec<AppSummary>>> {
    session.require_role(Role::Viewer)?;

    let cluster_ids = match q.cluster {
        Some(id) => vec![id],
        None => state
            .clusters
            .list()
            .await
            .into_iter()
            .map(|c| c.id)
            .collect(),
    };

    let mut out = Vec::new();
    for id in cluster_ids {
        let Some(client) = state.clusters.get_client(&id).await else {
            continue;
        };
        let api: Api<AppDeployment> = Api::namespaced(client, &namespace);
        if let Ok(list) = api.list(&Default::default()).await {
            out.extend(list.items.iter().map(|a| to_summary(a, &id)));
        }
    }
    Ok(Json(out))
}

pub async fn create_app(
    State(state): State<AppState>,
    session: AuthSession,
    Path(namespace): Path<String>,
    Json(payload): Json<CreateAppPayload>,
) -> ApiResult<(StatusCode, Json<AppSummary>)> {
    session.require_role(Role::Developer)?;

    let mut fields = Vec::new();
    if !is_dns1123_label(&payload.name) {
        fields.push(FieldError {
            field: "name".to_string(),
            reason: "invalid_format".to_string(),
        });
    }
    if !is_https_git_url(&payload.git_repository) {
        fields.push(FieldError {
            field: "gitRepository".to_string(),
            reason: "invalid_format".to_string(),
        });
    }
    for e in &payload.env {
        if e.value.is_some() && e.secret_ref.is_some() {
            fields.push(FieldError {
                field: format!("env.{}", e.name),
                reason: "mutually_exclusive".to_string(),
            });
        }
    }
    if !fields.is_empty() {
        return Err(AppError(ApiError::ValidationFailed { fields }));
    }

    let status = state
        .clusters
        .status(&payload.target_cluster)
        .await
        .ok_or(AppError(ApiError::ClusterNotFound))?;
    if status == ClusterStatus::Unreachable {
        return Err(AppError(ApiError::ClusterUnreachable));
    }
    let profile = state
        .clusters
        .get_profile(&payload.target_cluster)
        .await
        .ok_or(AppError(ApiError::ClusterNotFound))?;

    let builder = payload.builder.unwrap_or(profile.default_builder);
    if !profile.allows(builder) {
        return Err(AppError(ApiError::BuilderNotSupported));
    }

    let client = state
        .clusters
        .get_client(&payload.target_cluster)
        .await
        .ok_or(AppError(ApiError::ClusterNotFound))?;
    let api: Api<AppDeployment> = Api::namespaced(client, &namespace);

    if api
        .get_opt(&payload.name)
        .await
        .map_err(AppError::from)?
        .is_some()
    {
        return Err(AppError(ApiError::AlreadyExists));
    }

    let env = payload
        .env
        .into_iter()
        .map(|e| CrdEnvVar {
            name: e.name,
            value: e.value,
            secret_ref: e.secret_ref.map(|s| CrdSecretRef {
                name: s.name,
                key: s.key,
            }),
        })
        .chain(
            payload
                .env_variables
                .into_iter()
                .flatten()
                .map(|(k, v)| CrdEnvVar {
                    name: k,
                    value: Some(v),
                    secret_ref: None,
                }),
        )
        .collect();

    let app = AppDeployment::new(
        &payload.name,
        AppDeploymentSpec {
            git_repository: payload.git_repository,
            branch: payload.branch.unwrap_or_else(|| "main".to_string()),
            builder,
            replicas: payload.replicas.unwrap_or(1),
            env,
            route: RouteSpec {
                enabled: true,
                hostname: None,
            },
        },
    );

    let created = api
        .create(&PostParams::default(), &app)
        .await
        .map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(to_summary(&created, &payload.target_cluster)),
    ))
}

async fn find_app(
    state: &AppState,
    namespace: &str,
    app_name: &str,
    cluster: Option<String>,
) -> ApiResult<(AppDeployment, String)> {
    let cluster_ids = match cluster {
        Some(id) => vec![id],
        None => state
            .clusters
            .list()
            .await
            .into_iter()
            .map(|c| c.id)
            .collect(),
    };
    for id in cluster_ids {
        let Some(client) = state.clusters.get_client(&id).await else {
            continue;
        };
        let api: Api<AppDeployment> = Api::namespaced(client, namespace);
        if let Ok(Some(app)) = api.get_opt(app_name).await {
            return Ok((app, id));
        }
    }
    Err(AppError(ApiError::AppNotFound))
}

pub async fn get_app(
    State(state): State<AppState>,
    session: AuthSession,
    Path((namespace, app_name)): Path<(String, String)>,
    Query(q): Query<ClusterQuery>,
) -> ApiResult<Json<AppDetail>> {
    session.require_role(Role::Viewer)?;
    let (app, cluster_id) = find_app(&state, &namespace, &app_name, q.cluster).await?;
    Ok(Json(to_detail(&app, &cluster_id)))
}

pub async fn delete_app(
    State(state): State<AppState>,
    session: AuthSession,
    Path((namespace, app_name)): Path<(String, String)>,
    Query(q): Query<ClusterQuery>,
) -> ApiResult<StatusCode> {
    session.require_role(Role::Developer)?;
    let (_, cluster_id) = find_app(&state, &namespace, &app_name, q.cluster).await?;
    let client = state
        .clusters
        .get_client(&cluster_id)
        .await
        .ok_or(AppError(ApiError::ClusterNotFound))?;
    let api: Api<AppDeployment> = Api::namespaced(client, &namespace);
    api.delete(&app_name, &DeleteParams::default())
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::ACCEPTED)
}
