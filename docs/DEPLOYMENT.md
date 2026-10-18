# Deployment

| | |
|---|---|
| **Status** | 🟢 Current |
| **Owner** | Platform / Control Plane team |
| **Applies to** | v1.0.0+ |
| **Last reviewed** | 2026-10-18 |
| **Related pages** | [Testing](TESTING.md) · [Release Process](RELEASE.md) |

> ℹ️ **Summary**
> How to install the `AppDeployment` CRD and run the control plane against a cluster you
> control — as a local process, or packaged with `docker-compose.yml`. This covers what's
> actually implemented in v1.0.0; see [Not yet implemented](#not-yet-implemented) for
> what's still missing.

## Contents
- [Prerequisites](#prerequisites)
- [1. Point kubectl at your cluster](#1-point-kubectl-at-your-cluster)
- [2. Install the CRD](#2-install-the-crd)
- [3. Generate a session-signing keypair](#3-generate-a-session-signing-keypair)
- [4. Configure and register your cluster(s)](#4-configure-and-register-your-clusters)
- [5. Run the control plane against the cluster](#5-run-the-control-plane-against-the-cluster)
- [6. Run the UI](#6-run-the-ui)
- [Docker Compose (production-style)](#docker-compose-production-style)
- [Not yet implemented](#not-yet-implemented)

---

## Prerequisites

The commands below assume a cluster you can already reach with `kubectl` (k3s or standard
k8s) — the same setup described in the README's
[Getting started](../README.md#getting-started) section, with more context here.

---

## 1. Point kubectl at your cluster

```bash
kubectl config current-context
make kube-check   # kubectl cluster-info + kubectl version
```

> 💡 **Note**
> If you have multiple clusters, repeat the steps below once per cluster and give each
> one a distinct `id` in `clusters:` (see [step 4](#4-configure-and-register-your-clusters)).

---

## 2. Install the CRD

```bash
make crd-dry-run   # server-side dry-run first — safe, makes no changes
make crd-apply
kubectl get crd appdeployments.shiftlite.io
kubectl explain appdeployment.spec
```

---

## 3. Generate a session-signing keypair

The control plane signs session cookies with Ed25519 (EdDSA). Generate a dev keypair:

```bash
make keys   # writes control-plane/dev-keys/{private,public}.pem
```

> ⚠️ **Warning — don't ship dev keys to production**
> In a real deployment, generate this once and store the private key in a Kubernetes
> `Secret` rather than on disk — the control plane only needs the PEM bytes, not a
> particular file layout.

---

## 4. Configure and register your cluster(s)

```bash
make config   # copies control-plane/config.example.yaml -> control-plane/config.yaml
```

Edit `control-plane/config.yaml`:

| Field | What to set it to |
|---|---|
| `home_namespace` | A namespace that exists in your cluster (used for the session-denylist `Lease` objects) |
| `clusters[].kubeconfig_path` | Path to a kubeconfig for this cluster, or omit it to use the in-cluster config when the control plane itself runs inside the cluster |
| `clusters[].registry` | A container registry you can push to (Docker Hub, GHCR, GitLab Container Registry) — ShiftLite does not host its own registry |
| `oauth_providers` | Client ID/secret for the identity providers you actually use; delete the rest |

> ⚠️ **Warning — never commit real credentials**
> `config.yaml` and `dev-keys/` are gitignored — never commit real credentials.

---

## 5. Run the control plane against the cluster

```bash
make cp-run
```

This runs `shiftlite-bin` locally (not inside the cluster) with `SHIFTLITE_CONFIG`
pointing at `control-plane/config.yaml`. On startup it detects each registered cluster's
engine type from its reported `gitVersion` (`+k3s` → k3s profile, otherwise k8s profile)
and starts one reconcile loop per cluster.

Verify it's up:

```bash
curl -s localhost:8080/healthz
curl -s localhost:8080/readyz
```

---

## 6. Run the UI

```bash
make ui-install
make ui-dev
```

Open `http://localhost:5173` — the dev server proxies `/api` to the control plane on
`localhost:8080`.

---

## Docker Compose (production-style)

`docker-compose.yml` at the repo root builds the control plane (`control-plane/Dockerfile`)
and the UI (`ui/Dockerfile`, served by nginx) as separate images, with nginx reverse-proxying
`/api`, `/healthz`, and `/readyz` to the control plane on the internal compose network —
keeping everything on one origin so cookies and SSE work without CORS, per SPEC-001.

```bash
make keys     # generates control-plane/dev-keys/{private,public}.pem
make config   # copies control-plane/config.example.yaml -> config.yaml
```

Edit `control-plane/config.yaml` for the container paths used by `docker-compose.yml`:

| Field | Container path |
|---|---|
| `signing_key.private_key_path` / `public_key_path` | `/etc/shiftlite/keys/private.pem` / `public.pem` (bind-mounted from `control-plane/dev-keys/`) |
| `clusters[].kubeconfig_path` | `/etc/shiftlite/kubeconfigs/<file>` — copy the matching kubeconfig into `control-plane/kubeconfigs/` (gitignored, bind-mounted read-only) |
| *(home cluster)* | Also place a kubeconfig for the control plane's own "home" cluster (used for the session-revocation `Lease`) at `control-plane/kubeconfigs/home.kubeconfig` — this is what `KUBECONFIG` points at inside the container |

> ⚠️ **Warning — containers can't reach the host's loopback interface**
> If a registered cluster's API server is only reachable via `127.0.0.1` on the Docker
> host (a local k3s, for example), point that cluster's kubeconfig at
> `https://host.docker.internal:6443` instead. The same applies to a `proxy-url` (e.g. a
> SOCKS5 SSH tunnel) bound to `127.0.0.1` on the host — it needs to be reachable at
> `host.docker.internal` from inside the container, which may require binding the tunnel
> to `0.0.0.0` rather than `127.0.0.1` on the host side.

Then build and start both services:

```bash
make docker-up       # docker compose up -d --build
make docker-logs     # follow both services' logs
```

The UI is served at `http://localhost:8080`; the control plane itself is not published to
the host by default (only `ui`'s nginx talks to it, over the internal compose network) —
see the commented-out `ports:` block in `docker-compose.yml` to expose it directly for
debugging.

```bash
make docker-down     # stop and remove the stack
```

> 💡 **Note**
> This packages the control plane and UI as container images and runs them together, but
> it is still a single-host deployment (`docker compose`), not a Kubernetes-native one.

---

## Not yet implemented

> ⚠️ **Warning — no in-cluster deployment yet**
> Running the control plane **as a workload inside a Kubernetes cluster** (a Helm chart or
> plain `Deployment`/`Service`/`Ingress` manifests for the control plane itself, using a
> `ServiceAccount` instead of mounted kubeconfigs) is not yet implemented — today it runs
> either as a local process or via `docker-compose.yml` outside the clusters it manages.
> This is tracked as a post-v1.0.0 item; see the "Status" note in the [README](../README.md).

## Related Pages
- [Testing](TESTING.md)
- [Release Process](RELEASE.md)
- [README](../README.md)

