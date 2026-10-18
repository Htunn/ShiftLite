//! ShiftLite control plane entrypoint (SPEC-001): loads config, registers clusters, starts the
//! per-cluster reconcile loops (ADR-003) and the HTTP/SSE API (ADR-001).

use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::Client;
use serde::Deserialize;
use shiftlite_auth::{RoleMapping, SigningKeys};
use shiftlite_cluster::{detect_cluster_type, ClusterRegistry, LeaseDenylist};
use shiftlite_core::{ClusterType, EngineProfile, Provider, Registry};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct SigningKeyConfig {
    private_key_path: String,
    public_key_path: String,
}

#[derive(Debug, Deserialize)]
struct OAuthProviderCfg {
    authorize_url: String,
    token_url: String,
    issuer: Option<String>,
    jwks_url: Option<String>,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RegistryCfg {
    url: String,
    username: String,
    password_secret_ref: String,
}

#[derive(Debug, Deserialize)]
struct ClusterCfg {
    id: String,
    name: String,
    /// Path to a kubeconfig file for this cluster; omit to use the in-cluster config (the home
    /// cluster the control plane itself runs in).
    kubeconfig_path: Option<String>,
    ingress_class: Option<String>,
    storage_class: Option<String>,
    registry: RegistryCfg,
}

#[derive(Debug, Deserialize)]
struct Config {
    bind_addr: String,
    ui_origin: String,
    home_namespace: String,
    signing_key: SigningKeyConfig,
    #[serde(default)]
    role_mappings: Vec<RoleMapping>,
    oauth_providers: HashMap<String, OAuthProviderCfg>,
    clusters: Vec<ClusterCfg>,
}

fn parse_provider(s: &str) -> Option<Provider> {
    match s {
        "github" => Some(Provider::Github),
        "entraid" => Some(Provider::Entraid),
        "keycloak" => Some(Provider::Keycloak),
        "rhsso" => Some(Provider::Rhsso),
        _ => None,
    }
}

async fn build_cluster_client(cfg: &ClusterCfg) -> anyhow::Result<Client> {
    match &cfg.kubeconfig_path {
        Some(path) => {
            let kubeconfig = Kubeconfig::read_from(path)?;
            let config =
                kube::Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
                    .await?;
            Ok(Client::try_from(config)?)
        }
        None => Ok(Client::try_default().await?),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path =
        std::env::var("SHIFTLITE_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());
    let config_raw = fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("reading config {config_path}: {e}"))?;
    let config: Config = serde_yaml::from_str(&config_raw)?;

    let private_pem = fs::read(&config.signing_key.private_key_path)?;
    let public_pem = fs::read(&config.signing_key.public_key_path)?;
    let signing_keys = Arc::new(SigningKeys::from_ed25519_pem(&private_pem, &public_pem)?);

    let home_client = Client::try_default().await?;
    let denylist = Arc::new(LeaseDenylist::new(
        home_client,
        config.home_namespace.clone(),
    ));

    let registry_handle = ClusterRegistry::new();

    for cluster_cfg in &config.clusters {
        let client = build_cluster_client(cluster_cfg).await?;
        let version = client.apiserver_version().await?;
        let git_version = format!(
            "v{}.{}{}",
            version.major, version.minor, version.git_version
        );
        let cluster_type = detect_cluster_type(&git_version);

        let mut profile = match cluster_type {
            ClusterType::K3s => EngineProfile::for_k3s(),
            ClusterType::K8s => EngineProfile::for_k8s(
                cluster_cfg
                    .ingress_class
                    .clone()
                    .unwrap_or_else(|| "nginx".to_string()),
            ),
        };
        if let Some(sc) = &cluster_cfg.storage_class {
            profile.storage_class = Some(sc.clone());
        }

        let registry = Registry {
            url: cluster_cfg.registry.url.clone(),
            username: cluster_cfg.registry.username.clone(),
            password_secret_ref: cluster_cfg.registry.password_secret_ref.clone(),
        };

        registry_handle
            .register(
                cluster_cfg.id.clone(),
                cluster_cfg.name.clone(),
                client.clone(),
                cluster_type,
                profile.clone(),
                registry.clone(),
            )
            .await;

        tokio::spawn(shiftlite_controller::run(
            client,
            registry,
            profile,
            cluster_cfg.id.clone(),
        ));
    }

    {
        let registry_handle = registry_handle.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            loop {
                interval.tick().await;
                registry_handle.refresh_health().await;
            }
        });
    }
    {
        let denylist = denylist.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                if let Err(e) = denylist.refresh().await {
                    tracing::warn!(error = %e, "denylist refresh failed");
                }
            }
        });
    }

    let mut oauth_providers = HashMap::new();
    for (key, cfg) in config.oauth_providers {
        let Some(provider) = parse_provider(&key) else {
            tracing::warn!(provider = %key, "unknown oauth provider in config, skipping");
            continue;
        };
        oauth_providers.insert(
            provider,
            shiftlite_api::OAuthProviderConfig {
                authorize_url: cfg.authorize_url,
                token_url: cfg.token_url,
                issuer: cfg.issuer,
                jwks_url: cfg.jwks_url,
                userinfo_url: None,
                client_id: cfg.client_id,
                client_secret: cfg.client_secret,
                redirect_uri: cfg.redirect_uri,
                scopes: cfg.scopes,
            },
        );
    }

    let state = shiftlite_api::AppState {
        clusters: registry_handle,
        signing_keys: signing_keys.clone(),
        role_mappings: Arc::new(config.role_mappings),
        oauth_providers: Arc::new(oauth_providers),
        denylist,
        http: reqwest::Client::new(),
        ui_origin: config.ui_origin,
    };

    let app = shiftlite_api::router(state);
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "shiftlite control plane listening");
    axum::serve(listener, app).await?;

    Ok(())
}
