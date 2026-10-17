//! HTTP API for the KubeWeave control plane (SPEC-001, api/openapi.yaml).

mod apps;
mod auth;
mod clusters;
mod error;
mod logs;
mod session;
mod state;
mod types;
mod validate;

pub use state::{AppState, OAuthProviderConfig};

use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(|| async { "ok" }))
        .route("/api/v1/auth/login", get(auth::login))
        .route("/api/v1/auth/callback", get(auth::callback))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/api/v1/clusters", get(clusters::list_clusters))
        .route(
            "/api/v1/namespaces/{namespace}/apps",
            get(apps::list_apps).post(apps::create_app),
        )
        .route(
            "/api/v1/namespaces/{namespace}/apps/{app_name}",
            get(apps::get_app).delete(apps::delete_app),
        )
        .route(
            "/api/v1/namespaces/{namespace}/apps/{app_name}/logs/stream",
            get(logs::stream_logs),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
