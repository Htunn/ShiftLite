//! Reconciles `AppDeployment` CRs into BuildKit build Jobs, Deployments, Services and Ingress
//! objects (ADR-003). One `Controller` instance is run per registered cluster by `kubeweave-bin`.

use futures::StreamExt;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
    Container, EnvVar as K8sEnvVar, EnvVarSource, PodSpec, PodTemplateSpec, SecretKeySelector,
    SecurityContext, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::api::networking::v1::{
    HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
    IngressServiceBackend, IngressSpec, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, Patch, PatchParams};
use kube::runtime::controller::Action;
use kube::runtime::watcher;
use kube::runtime::Controller as KubeController;
use kube::{Client, Resource, ResourceExt};
use kubeweave_core::{AppPhase, EngineProfile, Registry};
use kubeweave_crd::{managed_labels, AppDeployment, AppDeploymentStatus};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("kube api error: {0}")]
    Kube(#[from] kube::Error),
    #[error("app is missing a namespace")]
    MissingNamespace,
}

pub type AppResult<T> = Result<T, Error>;

/// Per-cluster reconcile context (ADR-003: one controller per registered cluster).
pub struct Context {
    pub client: Client,
    pub registry: Registry,
    pub profile: EngineProfile,
    pub cluster_id: String,
}

const FIELD_MANAGER: &str = "kubeweave-controller";

fn build_job_name(app: &AppDeployment) -> String {
    format!("{}-build", app.name_any())
}

fn image_ref(registry: &Registry, app: &AppDeployment) -> String {
    format!(
        "{}/{}-{}:g{}",
        registry.url.trim_end_matches('/'),
        app.namespace().unwrap_or_default(),
        app.name_any(),
        app.meta().generation.unwrap_or(0)
    )
}

fn owned_meta(app: &AppDeployment, name: String) -> ObjectMeta {
    let mut labels: BTreeMap<String, String> =
        managed_labels(&app.name_any()).into_iter().collect();
    labels.insert("app".to_string(), app.name_any());
    ObjectMeta {
        name: Some(name),
        namespace: app.namespace(),
        labels: Some(labels),
        owner_references: app.owner_ref(&()).map(|o| vec![o]),
        ..Default::default()
    }
}

/// Renders the BuildKit (rootless) build Job: clones `spec.gitRepository`, builds the image and
/// pushes it to the cluster's configured registry (SPEC-001 Resolved Decisions, ADR-004).
fn render_build_job(app: &AppDeployment, registry: &Registry) -> Job {
    let image = image_ref(registry, app);
    let args = vec![
        "build".to_string(),
        "--frontend=dockerfile.v0".to_string(),
        format!("--local=context=git#{}", app.spec.branch),
        "--opt=filename=Dockerfile".to_string(),
        format!("--output=type=image,name={image},push=true"),
    ];
    let mut env = vec![K8sEnvVar {
        name: "BUILDKITD_FLAGS".to_string(),
        value: Some("--oci-worker-no-process-sandbox".to_string()),
        ..Default::default()
    }];
    env.push(K8sEnvVar {
        name: "SOURCE_GIT_REPOSITORY".to_string(),
        value: Some(app.spec.git_repository.clone()),
        ..Default::default()
    });
    env.push(K8sEnvVar {
        name: "REGISTRY_USERNAME".to_string(),
        value: Some(registry.username.clone()),
        ..Default::default()
    });
    env.push(K8sEnvVar {
        name: "REGISTRY_PASSWORD".to_string(),
        value_from: Some(EnvVarSource {
            secret_key_ref: Some(SecretKeySelector {
                name: registry.password_secret_ref.clone(),
                key: "password".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    });

    Job {
        metadata: owned_meta(app, build_job_name(app)),
        spec: Some(JobSpec {
            backoff_limit: Some(2),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(managed_labels(&app.name_any()).into_iter().collect()),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    restart_policy: Some("Never".to_string()),
                    containers: vec![Container {
                        name: "buildkit".to_string(),
                        image: Some("moby/buildkit:rootless".to_string()),
                        args: Some(args),
                        env: Some(env),
                        security_context: Some(SecurityContext {
                            run_as_non_root: Some(true),
                            privileged: Some(false),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        status: None,
    }
}

fn render_deployment(app: &AppDeployment, image: &str) -> Deployment {
    let name = app.name_any();
    let labels: BTreeMap<String, String> = managed_labels(&name)
        .into_iter()
        .chain(std::iter::once(("app".to_string(), name.clone())))
        .collect();

    let env: Vec<K8sEnvVar> = app
        .spec
        .env
        .iter()
        .map(|e| K8sEnvVar {
            name: e.name.clone(),
            value: e.value.clone(),
            value_from: e.secret_ref.as_ref().map(|s| EnvVarSource {
                secret_key_ref: Some(SecretKeySelector {
                    name: s.name.clone(),
                    key: s.key.clone(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        })
        .collect();

    Deployment {
        metadata: owned_meta(app, name.clone()),
        spec: Some(DeploymentSpec {
            replicas: Some(app.spec.replicas),
            selector: LabelSelector {
                match_labels: Some([("app".to_string(), name.clone())].into()),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "app".to_string(),
                        image: Some(image.to_string()),
                        env: Some(env),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        status: None,
    }
}

fn render_service(app: &AppDeployment) -> Service {
    let name = app.name_any();
    Service {
        metadata: owned_meta(app, name.clone()),
        spec: Some(ServiceSpec {
            selector: Some([("app".to_string(), name.clone())].into()),
            ports: Some(vec![ServicePort {
                port: 80,
                target_port: Some(IntOrString::Int(8080)),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        status: None,
    }
}

/// Renders a standard `networking.k8s.io/v1` Ingress for both engines. Traefik (k3s) understands
/// core Ingress objects directly, so a Traefik-specific `IngressRoute` CRD is not required for v1
/// (documented deviation from ADR-004's original `IngressRenderer` sketch — see Implementation
/// Notes in SPEC-001).
fn render_ingress(app: &AppDeployment, profile: &EngineProfile, hostname: &str) -> Ingress {
    let name = app.name_any();
    Ingress {
        metadata: owned_meta(app, name.clone()),
        spec: Some(IngressSpec {
            ingress_class_name: Some(profile.ingress_class.clone()),
            rules: Some(vec![IngressRule {
                host: Some(hostname.to_string()),
                http: Some(HTTPIngressRuleValue {
                    paths: vec![HTTPIngressPath {
                        path: Some("/".to_string()),
                        path_type: "Prefix".to_string(),
                        backend: IngressBackend {
                            service: Some(IngressServiceBackend {
                                name: name.clone(),
                                port: Some(ServiceBackendPort {
                                    number: Some(80),
                                    ..Default::default()
                                }),
                            }),
                            ..Default::default()
                        },
                    }],
                }),
            }]),
            ..Default::default()
        }),
        status: None,
    }
}

async fn apply<K>(api: &Api<K>, name: &str, obj: &K) -> AppResult<()>
where
    K: kube::Resource + serde::Serialize + serde::de::DeserializeOwned + Clone + std::fmt::Debug,
    K::DynamicType: Default,
{
    api.patch(
        name,
        &PatchParams::apply(FIELD_MANAGER).force(),
        &Patch::Apply(obj),
    )
    .await?;
    Ok(())
}

async fn patch_status(
    api: &Api<AppDeployment>,
    name: &str,
    status: AppDeploymentStatus,
) -> AppResult<()> {
    let patch = serde_json::json!({ "status": status });
    api.patch_status(
        name,
        &PatchParams::apply(FIELD_MANAGER).force(),
        &Patch::Merge(patch),
    )
    .await?;
    Ok(())
}

async fn reconcile(app: Arc<AppDeployment>, ctx: Arc<Context>) -> AppResult<Action> {
    let ns = app.namespace().ok_or(Error::MissingNamespace)?;
    let name = app.name_any();
    let apps: Api<AppDeployment> = Api::namespaced(ctx.client.clone(), &ns);
    let jobs: Api<Job> = Api::namespaced(ctx.client.clone(), &ns);
    let deployments: Api<Deployment> = Api::namespaced(ctx.client.clone(), &ns);
    let services: Api<Service> = Api::namespaced(ctx.client.clone(), &ns);
    let ingresses: Api<Ingress> = Api::namespaced(ctx.client.clone(), &ns);

    if !ctx.profile.allows(app.spec.builder) {
        warn!(app = %name, cluster = %ctx.cluster_id, "builder not supported by engine profile");
        patch_status(
            &apps,
            &name,
            AppDeploymentStatus {
                phase: Some(AppPhase::Failed),
                message: Some("builder not supported on this engine".to_string()),
                ..Default::default()
            },
        )
        .await?;
        return Ok(Action::await_change());
    }

    let phase = app
        .status
        .as_ref()
        .and_then(|s| s.phase)
        .unwrap_or(AppPhase::Pending);
    let image = image_ref(&ctx.registry, &app);

    match phase {
        AppPhase::Pending => {
            let job = render_build_job(&app, &ctx.registry);
            apply(&jobs, &build_job_name(&app), &job).await?;
            patch_status(
                &apps,
                &name,
                AppDeploymentStatus {
                    phase: Some(AppPhase::Building),
                    build_pod_name: Some(build_job_name(&app)),
                    observed_generation: app.meta().generation,
                    ..Default::default()
                },
            )
            .await?;
            Ok(Action::requeue(Duration::from_secs(5)))
        }
        AppPhase::Building => match jobs.get_opt(&build_job_name(&app)).await? {
            Some(job) => {
                let status = job.status.unwrap_or_default();
                if status.succeeded.unwrap_or(0) > 0 {
                    let deployment = render_deployment(&app, &image);
                    apply(&deployments, &name, &deployment).await?;
                    apply(&services, &name, &render_service(&app)).await?;
                    if app.spec.route.enabled {
                        let hostname = app
                            .spec
                            .route
                            .hostname
                            .clone()
                            .unwrap_or_else(|| format!("{name}.{ns}.apps.kubeweave.local"));
                        apply(
                            &ingresses,
                            &name,
                            &render_ingress(&app, &ctx.profile, &hostname),
                        )
                        .await?;
                    }
                    patch_status(
                        &apps,
                        &name,
                        AppDeploymentStatus {
                            phase: Some(AppPhase::Deploying),
                            image_ref: Some(image),
                            ..Default::default()
                        },
                    )
                    .await?;
                    Ok(Action::requeue(Duration::from_secs(5)))
                } else if status.failed.unwrap_or(0) > 0 {
                    patch_status(
                        &apps,
                        &name,
                        AppDeploymentStatus {
                            phase: Some(AppPhase::Failed),
                            message: Some("build job failed".to_string()),
                            build_pod_name: Some(build_job_name(&app)),
                            ..Default::default()
                        },
                    )
                    .await?;
                    Ok(Action::await_change())
                } else {
                    Ok(Action::requeue(Duration::from_secs(5)))
                }
            }
            None => Ok(Action::requeue(Duration::from_secs(5))),
        },
        AppPhase::Deploying | AppPhase::Running | AppPhase::Degraded => {
            match deployments.get_opt(&name).await? {
                Some(d) => {
                    let available = d.status.and_then(|s| s.available_replicas).unwrap_or(0);
                    let desired = app.spec.replicas;
                    let new_phase = if desired == 0 || available >= desired {
                        AppPhase::Running
                    } else {
                        AppPhase::Degraded
                    };
                    patch_status(
                        &apps,
                        &name,
                        AppDeploymentStatus {
                            phase: Some(new_phase),
                            available_replicas: Some(available),
                            image_ref: Some(image),
                            observed_generation: app.meta().generation,
                            ..Default::default()
                        },
                    )
                    .await?;
                    Ok(Action::requeue(Duration::from_secs(15)))
                }
                None => Ok(Action::requeue(Duration::from_secs(5))),
            }
        }
        AppPhase::Failed => Ok(Action::await_change()),
    }
}

fn error_policy(_app: Arc<AppDeployment>, err: &Error, _ctx: Arc<Context>) -> Action {
    error!(error = %err, "reconcile failed");
    Action::requeue(Duration::from_secs(30))
}

/// Runs the reconcile loop for one registered cluster until the process exits. Intended to be
/// spawned as its own task per cluster by `kubeweave-bin` (ADR-003).
pub async fn run(client: Client, registry: Registry, profile: EngineProfile, cluster_id: String) {
    let apps: Api<AppDeployment> = Api::all(client.clone());
    let ctx = Arc::new(Context {
        client,
        registry,
        profile,
        cluster_id,
    });

    KubeController::new(apps, watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            if let Err(e) = res {
                error!(error = %e, "controller reconcile error");
            } else {
                info!("reconciled app");
            }
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubeweave_core::Builder;
    use kubeweave_crd::{AppDeploymentSpec, EnvVar, RouteSpec, SecretRef};

    fn test_app(generation: i64) -> AppDeployment {
        let mut app = AppDeployment::new(
            "auth-service",
            AppDeploymentSpec {
                git_repository: "https://github.com/org/auth-service.git".to_string(),
                branch: "main".to_string(),
                builder: Builder::Buildkit,
                replicas: 2,
                env: vec![
                    EnvVar {
                        name: "PLAIN".to_string(),
                        value: Some("1".to_string()),
                        secret_ref: None,
                    },
                    EnvVar {
                        name: "FROM_SECRET".to_string(),
                        value: None,
                        secret_ref: Some(SecretRef {
                            name: "db-creds".to_string(),
                            key: "password".to_string(),
                        }),
                    },
                ],
                route: RouteSpec {
                    enabled: true,
                    hostname: None,
                },
            },
        );
        app.meta_mut().namespace = Some("team-a".to_string());
        app.meta_mut().generation = Some(generation);
        // owner_ref() requires a uid, which the API server sets; real reconciled objects always have one.
        app.meta_mut().uid = Some("11111111-1111-1111-1111-111111111111".to_string());
        app
    }

    fn test_registry() -> Registry {
        Registry {
            url: "ghcr.io/example-org".to_string(),
            username: "bot".to_string(),
            password_secret_ref: "ghcr-push-credential".to_string(),
        }
    }

    #[test]
    fn image_ref_is_deterministic_per_generation() {
        let app = test_app(3);
        let image = image_ref(&test_registry(), &app);
        assert_eq!(image, "ghcr.io/example-org/team-a-auth-service:g3");
    }

    #[test]
    fn build_job_runs_rootless_and_carries_registry_credentials() {
        let app = test_app(1);
        let job = render_build_job(&app, &test_registry());
        let pod_spec = job.spec.unwrap().template.spec.unwrap();
        let container = &pod_spec.containers[0];

        assert_eq!(container.image.as_deref(), Some("moby/buildkit:rootless"));
        let sec_ctx = container.security_context.as_ref().unwrap();
        assert_eq!(sec_ctx.run_as_non_root, Some(true));
        assert_eq!(sec_ctx.privileged, Some(false));

        let env = container.env.as_ref().unwrap();
        assert!(env
            .iter()
            .any(|e| e.name == "REGISTRY_USERNAME" && e.value.as_deref() == Some("bot")));
        let password_env = env.iter().find(|e| e.name == "REGISTRY_PASSWORD").unwrap();
        let secret_ref = password_env
            .value_from
            .as_ref()
            .unwrap()
            .secret_key_ref
            .as_ref()
            .unwrap();
        assert_eq!(secret_ref.name, "ghcr-push-credential");
    }

    #[test]
    fn deployment_renders_literal_and_secret_env_vars() {
        let app = test_app(1);
        let deployment = render_deployment(&app, "ghcr.io/example-org/team-a-auth-service:g1");
        let pod_spec = deployment.spec.unwrap().template.spec.unwrap();
        let env = pod_spec.containers[0].env.as_ref().unwrap();

        let plain = env.iter().find(|e| e.name == "PLAIN").unwrap();
        assert_eq!(plain.value.as_deref(), Some("1"));

        let from_secret = env.iter().find(|e| e.name == "FROM_SECRET").unwrap();
        assert!(from_secret.value.is_none());
        let secret_ref = from_secret
            .value_from
            .as_ref()
            .unwrap()
            .secret_key_ref
            .as_ref()
            .unwrap();
        assert_eq!(secret_ref.name, "db-creds");
        assert_eq!(secret_ref.key, "password");
    }

    #[test]
    fn deployment_replica_count_and_selector_match_app_name() {
        let app = test_app(1);
        let deployment = render_deployment(&app, "img:latest");
        let spec = deployment.spec.unwrap();
        assert_eq!(spec.replicas, Some(2));
        assert_eq!(
            spec.selector
                .match_labels
                .unwrap()
                .get("app")
                .map(String::as_str),
            Some("auth-service")
        );
    }

    #[test]
    fn service_exposes_port_80_targeting_8080() {
        let app = test_app(1);
        let service = render_service(&app);
        let port = &service.spec.unwrap().ports.unwrap()[0];
        assert_eq!(port.port, 80);
        assert_eq!(port.target_port, Some(IntOrString::Int(8080)));
    }

    #[test]
    fn ingress_uses_the_cluster_profiles_ingress_class() {
        let app = test_app(1);
        let profile = EngineProfile::for_k3s();
        let ingress = render_ingress(&app, &profile, "auth-service.team-a.apps.kubeweave.local");
        assert_eq!(
            ingress.spec.as_ref().unwrap().ingress_class_name.as_deref(),
            Some("traefik")
        );
        let rule = &ingress.spec.unwrap().rules.unwrap()[0];
        assert_eq!(
            rule.host.as_deref(),
            Some("auth-service.team-a.apps.kubeweave.local")
        );
    }

    #[test]
    fn rendered_objects_carry_an_owner_reference_back_to_the_app() {
        let app = test_app(1);
        let job = render_build_job(&app, &test_registry());
        let owners = job.metadata.owner_references.expect("owner reference set");
        assert_eq!(owners[0].name, "auth-service");
    }
}
