//! Shared domain types for the ShiftLite control plane (SPEC-001).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ClusterType {
    K8s,
    K3s,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ClusterStatus {
    Ready,
    Unreachable,
}

/// Resolved decision (SPEC-001): BuildKit rootless is the sole v1 builder; `Dockerfile` is reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Builder {
    #[default]
    Buildkit,
    Dockerfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AppPhase {
    Pending,
    Building,
    Deploying,
    Running,
    Degraded,
    Failed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Viewer,
    Developer,
    PlatformAdmin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Entraid,
    Keycloak,
    Rhsso,
}

/// Push target for built images (SPEC-001 Resolved Decisions): external, user-supplied registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Registry {
    pub url: String,
    pub username: String,
    /// Name of the Secret (in the control plane's home namespace) holding the push password/token.
    pub password_secret_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum IngressKind {
    Nginx,
    Contour,
    Traefik,
}

/// Engine differences between standard k8s and k3s (ADR-004), resolved per registered cluster.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EngineProfile {
    pub cluster_type: ClusterType,
    pub ingress: IngressKind,
    pub ingress_class: String,
    /// `None` means use the cluster's default StorageClass.
    pub storage_class: Option<String>,
    pub allowed_builders: Vec<Builder>,
    pub default_builder: Builder,
}

impl EngineProfile {
    pub fn for_k8s(ingress_class: impl Into<String>) -> Self {
        Self {
            cluster_type: ClusterType::K8s,
            ingress: IngressKind::Nginx,
            ingress_class: ingress_class.into(),
            storage_class: None,
            allowed_builders: vec![Builder::Buildkit, Builder::Dockerfile],
            default_builder: Builder::Buildkit,
        }
    }

    pub fn for_k3s() -> Self {
        Self {
            cluster_type: ClusterType::K3s,
            ingress: IngressKind::Traefik,
            ingress_class: "traefik".to_string(),
            storage_class: Some("local-path".to_string()),
            allowed_builders: vec![Builder::Buildkit, Builder::Dockerfile],
            default_builder: Builder::Buildkit,
        }
    }

    pub fn allows(&self, builder: Builder) -> bool {
        self.allowed_builders.contains(&builder)
    }
}

/// Machine-readable error codes returned in the `code` field of every non-2xx response (api/openapi.yaml).
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ApiError {
    #[error("unsupported provider")]
    InvalidProvider,
    #[error("invalid or expired oauth state")]
    InvalidState,
    #[error("identity provider unavailable")]
    IdpUnavailable,
    #[error("identity provider token validation failed")]
    IdpValidationFailed,
    #[error("identity provider code exchange failed")]
    IdpExchangeFailed,
    #[error("authentication required")]
    Unauthenticated,
    #[error("forbidden")]
    Forbidden,
    #[error("namespace not found")]
    NamespaceNotFound,
    #[error("cluster not found")]
    ClusterNotFound,
    #[error("application not found")]
    AppNotFound,
    #[error("already exists")]
    AlreadyExists,
    #[error("validation failed")]
    ValidationFailed { fields: Vec<FieldError> },
    #[error("builder not supported on this engine")]
    BuilderNotSupported,
    #[error("target cluster unreachable")]
    ClusterUnreachable,
    #[error("internal error")]
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldError {
    pub field: String,
    pub reason: String,
}

impl ApiError {
    /// HTTP status code for this error, per the documented error table in SPEC-001 / api/openapi.yaml.
    pub fn status_code(&self) -> u16 {
        match self {
            ApiError::InvalidProvider => 400,
            ApiError::InvalidState => 400,
            ApiError::IdpUnavailable => 503,
            ApiError::IdpValidationFailed => 401,
            ApiError::IdpExchangeFailed => 502,
            ApiError::Unauthenticated => 401,
            ApiError::Forbidden => 403,
            ApiError::NamespaceNotFound => 404,
            ApiError::ClusterNotFound => 404,
            ApiError::AppNotFound => 404,
            ApiError::AlreadyExists => 409,
            ApiError::ValidationFailed { .. } => 422,
            ApiError::BuilderNotSupported => 422,
            ApiError::ClusterUnreachable => 503,
            ApiError::Internal => 500,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            ApiError::InvalidProvider => "invalid_provider",
            ApiError::InvalidState => "invalid_state",
            ApiError::IdpUnavailable => "idp_unavailable",
            ApiError::IdpValidationFailed => "idp_validation_failed",
            ApiError::IdpExchangeFailed => "idp_exchange_failed",
            ApiError::Unauthenticated => "unauthenticated",
            ApiError::Forbidden => "forbidden",
            ApiError::NamespaceNotFound => "namespace_not_found",
            ApiError::ClusterNotFound => "cluster_not_found",
            ApiError::AppNotFound => "app_not_found",
            ApiError::AlreadyExists => "already_exists",
            ApiError::ValidationFailed { .. } => "validation_failed",
            ApiError::BuilderNotSupported => "builder_not_supported",
            ApiError::ClusterUnreachable => "cluster_unreachable",
            ApiError::Internal => "internal",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Role::Viewer => "viewer",
            Role::Developer => "developer",
            Role::PlatformAdmin => "platform-admin",
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_profile_defaults_match_spec() {
        let k3s = EngineProfile::for_k3s();
        assert!(k3s.allows(Builder::Buildkit));
        assert_eq!(k3s.storage_class.as_deref(), Some("local-path"));
    }

    #[test]
    fn error_status_codes_match_contract() {
        assert_eq!(ApiError::BuilderNotSupported.status_code(), 422);
        assert_eq!(ApiError::ClusterUnreachable.status_code(), 503);
    }

    #[test]
    fn error_codes_match_the_openapi_contract() {
        assert_eq!(ApiError::Unauthenticated.code(), "unauthenticated");
        assert_eq!(ApiError::AlreadyExists.code(), "already_exists");
        assert_eq!(
            ApiError::ValidationFailed { fields: vec![] }.code(),
            "validation_failed"
        );
    }

    #[test]
    fn roles_are_ordered_from_least_to_most_privileged() {
        assert!(Role::Viewer < Role::Developer);
        assert!(Role::Developer < Role::PlatformAdmin);
    }

    #[test]
    fn builder_defaults_to_buildkit() {
        assert!(matches!(Builder::default(), Builder::Buildkit));
    }

    #[test]
    fn k8s_engine_profile_uses_the_given_ingress_class_and_cluster_default_storage() {
        let k8s = EngineProfile::for_k8s("nginx");
        assert_eq!(k8s.ingress_class, "nginx");
        assert_eq!(k8s.storage_class, None);
        assert_eq!(k8s.default_builder, Builder::Buildkit);
    }
}
