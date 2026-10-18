//! `GET /api/v1/clusters` (SPEC-001 Acceptance Criteria: Clusters).

use crate::error::ApiResult;
use crate::session::AuthSession;
use crate::state::AppState;
use crate::types::{ClusterInfoDto, RegistryInfo};
use axum::extract::State;
use axum::Json;
use shiftlite_core::Role;

pub async fn list_clusters(
    State(state): State<AppState>,
    session: AuthSession,
) -> ApiResult<Json<Vec<ClusterInfoDto>>> {
    session.require_role(Role::Viewer)?;

    let mut out = Vec::new();
    for info in state.clusters.list().await {
        let profile = state.clusters.get_profile(&info.id).await;
        let registry = state.clusters.get_registry(&info.id).await;
        let (ingress_class, storage_class, allowed_builders) = profile
            .map(|p| (p.ingress_class, p.storage_class, p.allowed_builders))
            .unwrap_or_default();
        out.push(ClusterInfoDto {
            id: info.id,
            name: info.name,
            cluster_type: info.cluster_type,
            version: info.version,
            status: info.status,
            ingress_class,
            storage_class,
            allowed_builders,
            registry: registry
                .map(|r| RegistryInfo {
                    url: r.url,
                    username: r.username,
                })
                .unwrap_or(RegistryInfo {
                    url: String::new(),
                    username: String::new(),
                }),
        });
    }
    Ok(Json(out))
}
