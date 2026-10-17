//! Cluster registry, health checks, and the Lease-backed session denylist (ADR-002, ADR-003).

use k8s_openapi::api::coordination::v1::Lease;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use kube::api::{Api, ObjectMeta, Patch, PatchParams, PostParams};
use kube::{Client, ResourceExt};
use kubeweave_auth::Denylist;
use kubeweave_core::{ClusterStatus, ClusterType, EngineProfile, Registry};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct ClusterInfo {
    pub id: String,
    pub name: String,
    pub cluster_type: ClusterType,
    pub version: String,
    pub status: ClusterStatus,
}

struct ClusterEntry {
    info: ClusterInfo,
    client: Client,
    profile: EngineProfile,
    registry: Registry,
}

/// In-memory registry of target clusters (SPEC-001 `/api/v1/clusters`). The control plane runs in
/// a home cluster and reaches each registered cluster through a stored ServiceAccount credential
/// (SPEC-001 Resolved Decisions: own ServiceAccount, not impersonation, for v1).
#[derive(Default, Clone)]
pub struct ClusterRegistry {
    entries: Arc<RwLock<HashMap<String, ClusterEntry>>>,
}

impl ClusterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
        client: Client,
        cluster_type: ClusterType,
        profile: EngineProfile,
        registry: Registry,
    ) {
        let id = id.into();
        let entry = ClusterEntry {
            info: ClusterInfo {
                id: id.clone(),
                name: name.into(),
                cluster_type,
                version: String::new(),
                status: ClusterStatus::Unreachable,
            },
            client,
            profile,
            registry,
        };
        self.entries.write().await.insert(id, entry);
    }

    pub async fn list(&self) -> Vec<ClusterInfo> {
        self.entries
            .read()
            .await
            .values()
            .map(|e| e.info.clone())
            .collect()
    }

    pub async fn get_client(&self, id: &str) -> Option<Client> {
        self.entries.read().await.get(id).map(|e| e.client.clone())
    }

    pub async fn get_profile(&self, id: &str) -> Option<EngineProfile> {
        self.entries.read().await.get(id).map(|e| e.profile.clone())
    }

    pub async fn get_registry(&self, id: &str) -> Option<Registry> {
        self.entries
            .read()
            .await
            .get(id)
            .map(|e| e.registry.clone())
    }

    pub async fn status(&self, id: &str) -> Option<ClusterStatus> {
        self.entries.read().await.get(id).map(|e| e.info.status)
    }

    /// Probes every registered cluster's API server (timeout per SPEC-001: `Unreachable` after
    /// 15s) and updates its status/version. Intended to run on a periodic background task.
    pub async fn refresh_health(&self) {
        let ids: Vec<String> = self.entries.read().await.keys().cloned().collect();
        for id in ids {
            let client = match self.get_client(&id).await {
                Some(c) => c,
                None => continue,
            };
            let probe =
                tokio::time::timeout(Duration::from_secs(15), client.apiserver_version()).await;
            let mut entries = self.entries.write().await;
            if let Some(entry) = entries.get_mut(&id) {
                match probe {
                    Ok(Ok(version)) => {
                        entry.info.status = ClusterStatus::Ready;
                        entry.info.version = format!("{}.{}", version.major, version.minor);
                    }
                    _ => {
                        warn!(cluster = %id, "cluster health check failed or timed out");
                        entry.info.status = ClusterStatus::Unreachable;
                    }
                }
            }
        }
    }
}

/// Session revocation backed by a `coordination.k8s.io/v1` `Lease` per `jti` (ADR-002), with an
/// in-memory cache so hot-path `is_denylisted` checks do not hit the API server synchronously.
pub struct LeaseDenylist {
    client: Client,
    namespace: String,
    cache: Arc<RwLock<HashSet<String>>>,
}

impl LeaseDenylist {
    pub fn new(client: Client, namespace: impl Into<String>) -> Self {
        Self {
            client,
            namespace: namespace.into(),
            cache: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    fn lease_name(jti: &str) -> String {
        format!("kw-session-deny-{:x}", md5_like_hash(jti))
    }

    pub async fn revoke(
        &self,
        jti: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), kube::Error> {
        let api: Api<Lease> = Api::namespaced(self.client.clone(), &self.namespace);
        let name = Self::lease_name(jti);
        let lease = Lease {
            metadata: ObjectMeta {
                name: Some(name.clone()),
                labels: Some([("kubeweave.io/jti-hash".to_string(), name.clone())].into()),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::coordination::v1::LeaseSpec {
                renew_time: Some(MicroTime(k8s_openapi::jiff::Timestamp::now())),
                ..Default::default()
            }),
        };
        match api.create(&PostParams::default(), &lease).await {
            Ok(_) => {}
            Err(kube::Error::Api(err)) if err.code == 409 => {
                api.patch(
                    &name,
                    &PatchParams::default(),
                    &Patch::Merge(serde_json::json!({
                        "spec": { "renewTime": chrono::Utc::now().to_rfc3339() }
                    })),
                )
                .await?;
            }
            Err(e) => return Err(e),
        }
        self.cache.write().await.insert(jti.to_string());
        let _ = expires_at; // sweeping expired leases is a follow-up (SPEC-001 Implementation Notes)
        Ok(())
    }

    /// Refreshes the in-memory cache from the cluster; call periodically from a background task.
    pub async fn refresh(&self) -> Result<(), kube::Error> {
        let api: Api<Lease> = Api::namespaced(self.client.clone(), &self.namespace);
        let leases = api.list(&Default::default()).await?;
        let names: HashSet<String> = leases.items.iter().map(|l| l.name_any()).collect();
        *self.cache.write().await = names;
        Ok(())
    }
}

impl Denylist for LeaseDenylist {
    fn is_denylisted(&self, jti: &str) -> bool {
        let name = Self::lease_name(jti);
        match self.cache.try_read() {
            Ok(cache) => cache.contains(&name),
            Err(_) => false,
        }
    }
}

/// Cheap, non-cryptographic hash used only to derive a stable Kubernetes object name from a
/// session `jti` (names must be DNS-1123 labels; `jti` itself may not be).
fn md5_like_hash(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

/// Determines the engine type from the reported Kubernetes `gitVersion` (ADR-004): clusters
/// whose version string contains `+k3s` are treated as k3s, everything else as standard k8s.
pub fn detect_cluster_type(git_version: &str) -> ClusterType {
    if git_version.contains("+k3s") {
        ClusterType::K3s
    } else {
        ClusterType::K8s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http_body_util::Full;
    use kubeweave_core::{Builder, IngressKind};

    #[test]
    fn detects_k3s_from_git_version() {
        assert_eq!(detect_cluster_type("v1.30.2+k3s1"), ClusterType::K3s);
        assert_eq!(detect_cluster_type("v1.30.2"), ClusterType::K8s);
    }

    /// A `kube::Client` that never makes a real connection — used to exercise the registry's
    /// in-memory bookkeeping without a live cluster.
    fn offline_client() -> Client {
        let service = tower::service_fn(|_req: http::Request<kube::client::Body>| async {
            Ok::<_, std::convert::Infallible>(http::Response::new(Full::new(Bytes::new())))
        });
        Client::new(service, "default")
    }

    fn test_registry_entry() -> Registry {
        Registry {
            url: "ghcr.io/example-org".to_string(),
            username: "bot".to_string(),
            password_secret_ref: "ghcr-push-credential".to_string(),
        }
    }

    #[tokio::test]
    async fn register_makes_a_cluster_visible_via_list_and_lookups() {
        let registry = ClusterRegistry::new();
        let profile = EngineProfile::for_k3s();
        registry
            .register(
                "cluster-1",
                "Edge k3s",
                offline_client(),
                ClusterType::K3s,
                profile,
                test_registry_entry(),
            )
            .await;

        let clusters = registry.list().await;
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].id, "cluster-1");
        assert_eq!(clusters[0].status, ClusterStatus::Unreachable); // not probed yet

        assert!(registry.get_client("cluster-1").await.is_some());
        assert!(registry.get_client("unknown").await.is_none());

        let profile = registry.get_profile("cluster-1").await.unwrap();
        assert_eq!(profile.ingress, IngressKind::Traefik);
        assert!(profile.allows(Builder::Buildkit));

        let reg = registry.get_registry("cluster-1").await.unwrap();
        assert_eq!(reg.url, "ghcr.io/example-org");
    }

    #[tokio::test]
    async fn status_reports_none_for_unregistered_clusters() {
        let registry = ClusterRegistry::new();
        assert_eq!(registry.status("missing").await, None);
    }
}
