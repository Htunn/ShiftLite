# Deployment

This covers what's actually implemented in v1.0.0: installing the `AppDeployment` CRD on
a cluster and running the control plane against it. The commands below assume a cluster
you can already reach with `kubectl` (k3s or standard k8s) — the same setup described in
the README's [Getting started](../README.md#getting-started) section, with more context.

## 1. Point `kubectl` at your cluster

```bash
kubectl config current-context
make kube-check   # kubectl cluster-info + kubectl version
```

If you have multiple clusters, repeat the steps below once per cluster and give each one
a distinct `id` in `clusters:` (see step 4).

## 2. Install the CRD

```bash
make crd-dry-run   # server-side dry-run first — safe, makes no changes
make crd-apply
kubectl get crd appdeployments.kubeweave.io
kubectl explain appdeployment.spec
```

## 3. Generate a session-signing keypair

The control plane signs session cookies with Ed25519 (EdDSA). Generate a dev keypair:

```bash
make keys   # writes control-plane/dev-keys/{private,public}.pem
```

In a real deployment, generate this once and store the private key in a Kubernetes
`Secret` rather than on disk — the control plane only needs the PEM bytes, not a
particular file layout.

## 4. Configure and register your cluster(s)

```bash
make config   # copies control-plane/config.example.yaml -> control-plane/config.yaml
```

Edit `control-plane/config.yaml`:
- `home_namespace`: an existing namespace used for the session-revocation `Lease` objects.
- `clusters[].kubeconfig_path`: path to a kubeconfig for this cluster (e.g. your k3s
  cluster's `~/.kube/config`, or `k3s kubectl config view --raw > k3s.kubeconfig` on the
  node itself). Omit it only if the control plane process itself will run inside that
  cluster.
- `clusters[].registry`: a container registry you can push images to (Docker Hub, GHCR,
  or a GitLab Container Registry) — KubeWeave does not host its own registry.
- `oauth_providers`: fill in the identity providers you actually use; delete the rest.

## 5. Run the control plane against the cluster

```bash
make cp-run
```

This runs `kubeweave-bin` locally (not inside the cluster) with `KUBEWEAVE_CONFIG`
pointing at `control-plane/config.yaml`. On startup it detects each registered cluster's
engine type from its reported `gitVersion` (`+k3s` → k3s profile, otherwise k8s profile)
and starts one reconcile loop per cluster.

Verify it's up:

```bash
curl -s localhost:8080/healthz
curl -s localhost:8080/readyz
```

## 6. Run the UI

```bash
make ui-install
make ui-dev
```

Open `http://localhost:5173` — the dev server proxies `/api` to the control plane on
`localhost:8080`.

## Not yet implemented

The control plane does not yet ship as a container image, Helm chart, or Kubernetes
`Deployment` manifest for running **itself** inside a cluster — today it's run as a local
process against whichever clusters you register (steps above). Containerizing the control
plane and packaging it for in-cluster deployment is tracked as a post-v1.0.0 item; see the
"Status" note in the [README](../README.md).
