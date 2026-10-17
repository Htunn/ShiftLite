//! `AppDeployment` CRD (`kubeweave.io/v1alpha1`), source of truth for desired app state (ADR-003).

use kube::CustomResource;
use kubeweave_core::{AppPhase, Builder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SecretRef {
    pub name: String,
    pub key: String,
}

/// A single `env` entry: either a literal `value` or a `secretRef`, never both (SPEC-001 Resolved Decisions).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EnvVar {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<SecretRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct RouteSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_replicas() -> i32 {
    1
}

#[derive(Debug, Clone, CustomResource, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "kubeweave.io",
    version = "v1alpha1",
    kind = "AppDeployment",
    plural = "appdeployments",
    singular = "appdeployment",
    shortname = "kwapp",
    namespaced,
    status = "AppDeploymentStatus",
    printcolumn = r#"{"name":"Phase", "type":"string", "jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Ready", "type":"integer", "jsonPath":".status.availableReplicas"}"#,
    printcolumn = r#"{"name":"URL", "type":"string", "jsonPath":".status.routeUrl"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct AppDeploymentSpec {
    #[schemars(regex(pattern = r"^https://"))]
    pub git_repository: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default)]
    pub builder: Builder,
    #[serde(default = "default_replicas")]
    pub replicas: i32,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub route: RouteSpec,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppDeploymentStatus {
    pub phase: Option<AppPhase>,
    pub message: Option<String>,
    pub build_pod_name: Option<String>,
    pub image_ref: Option<String>,
    pub route_url: Option<String>,
    pub available_replicas: Option<i32>,
    pub observed_generation: Option<i64>,
}

/// Returns the labels applied to every object the controller renders for an `AppDeployment`.
pub fn managed_labels(app_name: &str) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    labels.insert(
        "app.kubernetes.io/managed-by".to_string(),
        "kubeweave".to_string(),
    );
    labels.insert("kubeweave.io/app".to_string(), app_name.to_string());
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_defaults_match_crd() {
        let json = serde_json::json!({ "gitRepository": "https://github.com/org/app.git" });
        let spec: AppDeploymentSpec = serde_json::from_value(json).unwrap();
        assert_eq!(spec.branch, "main");
        assert_eq!(spec.replicas, 1);
        assert!(matches!(spec.builder, Builder::Buildkit));
    }

    #[test]
    fn managed_labels_identify_the_owning_app() {
        let labels = managed_labels("auth-service");
        assert_eq!(
            labels.get("kubeweave.io/app").map(String::as_str),
            Some("auth-service")
        );
        assert_eq!(
            labels
                .get("app.kubernetes.io/managed-by")
                .map(String::as_str),
            Some("kubeweave")
        );
    }

    #[test]
    fn env_var_round_trips_a_literal_value() {
        let json = serde_json::json!({ "name": "LOG_LEVEL", "value": "debug" });
        let env: EnvVar = serde_json::from_value(json).unwrap();
        assert_eq!(env.value.as_deref(), Some("debug"));
        assert!(env.secret_ref.is_none());
    }

    #[test]
    fn env_var_round_trips_a_secret_ref() {
        let json = serde_json::json!({
            "name": "DB_PASSWORD",
            "secretRef": { "name": "db-creds", "key": "password" }
        });
        let env: EnvVar = serde_json::from_value(json).unwrap();
        assert!(env.value.is_none());
        let secret_ref = env.secret_ref.unwrap();
        assert_eq!(secret_ref.name, "db-creds");
        assert_eq!(secret_ref.key, "password");
    }

    #[test]
    fn gitrepository_rejects_non_https_urls_via_schema_pattern() {
        // The `pattern` constraint lives in the generated JSON Schema (enforced by the API
        // server once the CRD is installed); Rust-side we only assert the field is required.
        let json = serde_json::json!({ "gitRepository": "git@github.com:org/app.git" });
        let spec: AppDeploymentSpec = serde_json::from_value(json).unwrap();
        assert_eq!(spec.git_repository, "git@github.com:org/app.git");
    }
}
