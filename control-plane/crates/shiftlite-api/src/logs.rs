//! `GET /api/v1/namespaces/{ns}/apps/{appName}/logs/stream` — SSE log viewer (SPEC-001).

use crate::error::{ApiResult, AppError};
use crate::session::AuthSession;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use futures::stream;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, ListParams, LogParams};
use serde::{Deserialize, Serialize};
use shiftlite_core::{ApiError, Role};
use std::convert::Infallible;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub cluster: Option<String>,
    pub container: Option<String>,
    #[serde(default = "default_tail")]
    pub tail: i64,
    #[serde(default = "default_follow")]
    pub follow: bool,
}

fn default_tail() -> i64 {
    100
}

fn default_follow() -> bool {
    true
}

#[derive(Serialize)]
struct LogEvent {
    ts: String,
    pod: String,
    container: String,
    stream: &'static str,
    line: String,
}

pub async fn stream_logs(
    State(state): State<AppState>,
    session: AuthSession,
    Path((namespace, app_name)): Path<(String, String)>,
    Query(q): Query<LogsQuery>,
) -> ApiResult<impl IntoResponse> {
    session.require_role(Role::Viewer)?;

    let cluster_ids = match q.cluster.clone() {
        Some(id) => vec![id],
        None => state
            .clusters
            .list()
            .await
            .into_iter()
            .map(|c| c.id)
            .collect(),
    };
    let mut client = None;
    for id in cluster_ids {
        if let Some(c) = state.clusters.get_client(&id).await {
            client = Some(c);
            break;
        }
    }
    let client = client.ok_or(AppError(ApiError::AppNotFound))?;

    let pods: Api<Pod> = Api::namespaced(client.clone(), &namespace);
    let list = pods
        .list(&ListParams::default().labels(&format!("shiftlite.io/app={app_name}")))
        .await
        .map_err(AppError::from)?;
    let pod_name = list
        .items
        .first()
        .and_then(|p| p.metadata.name.clone())
        .ok_or(AppError(ApiError::AppNotFound))?;

    let container = q.container.clone();
    let tail_lines = q.tail;
    let follow = q.follow;

    let log_stream = stream::unfold(
        (pods, pod_name.clone(), container, tail_lines, follow, false),
        move |(pods, pod_name, container, tail_lines, follow, sent_end)| async move {
            if sent_end {
                return None;
            }
            let params = LogParams {
                container: container.clone(),
                follow: false,
                tail_lines: Some(tail_lines),
                timestamps: true,
                ..Default::default()
            };
            match pods.logs(&pod_name, &params).await {
                Ok(text) => {
                    let events: Vec<Result<Event, Infallible>> = text
                        .lines()
                        .map(|line| {
                            let (ts, rest) = line.split_once(' ').unwrap_or(("", line));
                            let ev = LogEvent {
                                ts: ts.to_string(),
                                pod: pod_name.clone(),
                                container: "app".to_string(),
                                stream: "stdout",
                                line: rest.to_string(),
                            };
                            Ok(Event::default()
                                .id(ts)
                                .event("log")
                                .json_data(&ev)
                                .unwrap_or_else(|_| Event::default().event("log")))
                        })
                        .collect();
                    let next_state = (pods, pod_name, container, tail_lines, follow, !follow);
                    Some((stream::iter(events), next_state))
                }
                Err(_) => None,
            }
        },
    )
    .flatten();

    let sse = Sse::new(log_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    );
    Ok(sse)
}
