//! Request/response DTOs mirroring `api/openapi.yaml` exactly (field names, not Rust CRD field
//! names — e.g. `env` here is the wire shape, distinct from `kubeweave_crd::EnvVar`).

use kubeweave_core::{AppPhase, Builder, ClusterStatus, ClusterType, Provider, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub sub: String,
    pub email: String,
    pub provider: Provider,
    pub groups: Vec<String>,
    pub roles: Vec<Role>,
}

#[derive(Debug, Serialize)]
pub struct RegistryInfo {
    pub url: String,
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct ClusterInfoDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub cluster_type: ClusterType,
    pub version: String,
    pub status: ClusterStatus,
    #[serde(rename = "ingressClass")]
    pub ingress_class: String,
    #[serde(rename = "storageClass")]
    pub storage_class: Option<String>,
    #[serde(rename = "allowedBuilders")]
    pub allowed_builders: Vec<Builder>,
    pub registry: RegistryInfo,
}

#[derive(Debug, Deserialize)]
pub struct EnvVarDto {
    pub name: String,
    pub value: Option<String>,
    #[serde(rename = "secretRef")]
    pub secret_ref: Option<SecretRefDto>,
}

#[derive(Debug, Deserialize)]
pub struct SecretRefDto {
    pub name: String,
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAppPayload {
    pub name: String,
    #[serde(rename = "gitRepository")]
    pub git_repository: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub builder: Option<Builder>,
    #[serde(rename = "targetCluster")]
    pub target_cluster: String,
    #[serde(default)]
    pub replicas: Option<i32>,
    #[serde(default, rename = "env")]
    pub env: Vec<EnvVarDto>,
    /// Deprecated alias accepted for compatibility with the original draft contract.
    #[serde(default, rename = "envVariables")]
    pub env_variables: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct AppSummary {
    pub name: String,
    pub namespace: String,
    #[serde(rename = "targetCluster")]
    pub target_cluster: String,
    pub status: AppPhase,
    #[serde(rename = "routeUrl")]
    pub route_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AppDetail {
    #[serde(flatten)]
    pub summary: AppSummary,
    #[serde(rename = "gitRepository")]
    pub git_repository: String,
    pub branch: String,
    pub builder: Builder,
    pub replicas: i32,
    #[serde(rename = "availableReplicas")]
    pub available_replicas: i32,
    pub message: Option<String>,
    #[serde(rename = "buildPodName")]
    pub build_pod_name: Option<String>,
    #[serde(rename = "imageRef")]
    pub image_ref: Option<String>,
}
