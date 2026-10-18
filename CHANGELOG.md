# Changelog

All notable changes to KubeWeave are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0] - 2026-10-17

### Added
- Rust control plane (`control-plane/`) exposing a REST + SSE API per [api/openapi.yaml](api/openapi.yaml):
  - Multi-provider SSO login (GitHub, Microsoft Entra ID, Keycloak, Red Hat SSO) via OIDC/OAuth2 with PKCE, backed by a signed, `HttpOnly` session cookie.
  - Group-to-role mapping (`viewer`, `developer`, `platform-admin`) enforced on every protected endpoint.
  - Cluster registry with health checks across registered standard Kubernetes and k3s clusters.
  - Application lifecycle API (create / list / get / delete) backed by the `AppDeployment` custom resource.
  - Real-time application log streaming over Server-Sent Events.
  - Session revocation backed by a Kubernetes `Lease` so logout works across replicas without an extra datastore.
- `AppDeployment` CRD (`kubeweave.io/v1alpha1`) and a controller that reconciles it into a rootless BuildKit build `Job`, `Deployment`, `Service`, and `Ingress`.
- Engine profiles abstracting standard Kubernetes vs. k3s defaults (ingress class, storage class, allowed builders).
- React + Tailwind UI (`ui/`) with login, cluster overview, application detail, live log viewer, and a source-to-deployment creation wizard, generated against the OpenAPI contract.
- Unit test coverage across every Rust crate (domain types, CRD schema, auth/JWT round trips, cluster registry, controller object rendering, API validation/authorization) and the UI (status mapping, API client error handling), runnable via `make test` / `make verify`.
- `Makefile` with local-testing targets (`cp-*`, `ui-*`, `crd-*`) and `docs/` guides for deployment, testing, and the release process.
- `docker-compose.yml` with a multi-stage `control-plane/Dockerfile` (Rust release build) and `ui/Dockerfile` (nginx), reverse-proxying `/api`, `/healthz`, and `/readyz` to the control plane on one origin so cookies and SSE work without CORS (`make docker-up` / `make docker-down`).

[Unreleased]: https://github.com/htunn/KubeWeave/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/htunn/KubeWeave/releases/tag/v1.0.0
