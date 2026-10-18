//! Shared application state injected into every axum handler.

use shiftlite_auth::{RoleMapping, SigningKeys};
use shiftlite_cluster::{ClusterRegistry, LeaseDenylist};
use shiftlite_core::Provider;
use std::collections::HashMap;
use std::sync::Arc;

/// Static OAuth2/OIDC configuration for one identity provider (ADR-002).
#[derive(Debug, Clone)]
pub struct OAuthProviderConfig {
    pub authorize_url: String,
    pub token_url: String,
    /// OIDC discovery issuer (empty for GitHub, which is OAuth2-only).
    pub issuer: Option<String>,
    pub jwks_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub clusters: ClusterRegistry,
    pub signing_keys: Arc<SigningKeys>,
    pub role_mappings: Arc<Vec<RoleMapping>>,
    pub oauth_providers: Arc<HashMap<Provider, OAuthProviderConfig>>,
    pub denylist: Arc<LeaseDenylist>,
    pub http: reqwest::Client,
    /// Origin the UI is served from, used to build absolute login redirect URLs.
    pub ui_origin: String,
}
