# Testing

| | |
|---|---|
| **Status** | 🟢 Current |
| **Owner** | Platform / Control Plane team |
| **Applies to** | v1.0.0+ |
| **Last reviewed** | 2026-10-18 |
| **Related pages** | [Deployment](DEPLOYMENT.md) · [Release Process](RELEASE.md) |

> ℹ️ **Summary**
> How ShiftLite is tested, what's covered where, and how to run everything locally before
> opening a PR or cutting a release. Two independent suites: Rust (`control-plane/`) and
> TypeScript (`ui/`), both wired into the root [`Makefile`](../Makefile).

## Contents
- [Quick reference](#quick-reference)
- [Control plane (Rust)](#control-plane-rust)
- [Coverage by crate](#coverage-by-crate)
- [A real bug these tests caught](#a-real-bug-these-tests-caught)
- [UI (React/TypeScript)](#ui-reacttypescript)
- [End-to-end testing against a real cluster](#end-to-end-testing-against-a-real-cluster)
- [Running everything before a release](#running-everything-before-a-release)

---

## Quick reference

| I want to... | Run |
|---|---|
| Run Rust unit tests | `make cp-test` |
| Run UI unit tests | `make ui-test` |
| Type-check the UI | `make ui-typecheck` |
| Run everything (fmt, lint, tests, build) | `make verify` |

---

## Control plane (Rust)

```bash
make cp-test        # cargo test --workspace
# or, from control-plane/:
cargo test --workspace
```

### Coverage by crate

| Crate | What's tested |
|---|---|
| `shiftlite-core` | `ApiError` → HTTP status/code mapping, `Role` ordering (`viewer < developer < platform-admin`), `EngineProfile` defaults for k8s/k3s, `Builder` default |
| `shiftlite-crd` | `AppDeploymentSpec` field defaults, `managed_labels`, `EnvVar` (de)serialization for both the literal-`value` and `secretRef` shapes |
| `shiftlite-auth` | PKCE challenge derivation, group→role mapping (dedup, no-match), session JWT encode/decode round trip, tamper detection, OAuth-cookie JWT round trip, `new_id` uniqueness |
| `shiftlite-cluster` | k3s/k8s detection from `gitVersion`, `ClusterRegistry` register/list/lookup behavior against an offline mock `kube::Client` |
| `shiftlite-controller` | BuildKit Job rendering (rootless, registry credentials), Deployment rendering (literal + `secretRef` env vars, replica/selector), Service port mapping, Ingress class selection, owner references |
| `shiftlite-api` | Input validation (DNS-1123 labels, `https://` Git URLs), error → HTTP status mapping, `AuthSession::require_role` authorization logic, OAuth provider parsing, URL-encoding helper |

> 💡 **Note**
> Rendering functions in `shiftlite-controller` (`render_build_job`, `render_deployment`,
> `render_service`, `render_ingress`) are pure — given an `AppDeployment` value they return
> a typed Kubernetes object with no cluster I/O, which is what makes them unit-testable
> without a live cluster. `shiftlite-cluster`'s registry tests use an offline mock
> `kube::Client` (via `tower::service_fn`) for the same reason — none of the tested
> methods make a network call.

### A real bug these tests caught

> ⚠️ **Warning — auth code needs round-trip tests, not just logic tests**
> Adding the JWT round-trip tests in `shiftlite-auth` surfaced a genuine runtime problem:
> the `jsonwebtoken` crate (v11) requires an explicit crypto provider feature
> (`rust_crypto` or `aws_lc_rs`); without one, **any** sign or verify call panics at
> runtime with `Could not automatically determine the process-level CryptoProvider`. This
> had no test coverage before, so it would only have surfaced the first time a real user
> logged in or an ID token was validated. Fixed by adding `features = ["rust_crypto"]` to
> the `jsonwebtoken` dependency in both `shiftlite-auth` and `shiftlite-api`.

---

## UI (React/TypeScript)

```bash
make ui-test         # npm run test (vitest)
make ui-typecheck     # npx tsc -b
make ui-build         # npm run build
```

Tests live next to the code they cover (`*.test.ts(x)`), using
[Vitest](https://vitest.dev/) + [Testing Library](https://testing-library.com/):

| Test file | What it checks |
|---|---|
| `src/components/common/StatusBadge.test.tsx` | Status→color mapping required by SPEC-001 (`Running` = green, `Pending`/`Building`/`Deploying` = yellow, `Degraded` = orange, `Failed` = red) |
| `src/api/generated-client.test.ts` | `unwrap()`/`ApiRequestError` behavior: data pass-through, error-code propagation, and the `internal` fallback when the server omits a `code` |

---

## End-to-end testing against a real cluster

> ℹ️ **Note**
> This is the heaviest level of testing — it exercises the real HTTP API against a real
> Kubernetes/k3s cluster. Use it before a release or after touching auth, the controller,
> or the cluster registry.

Since there's no real IdP wired up for local testing, use the dev-only token minter to
simulate a logged-in session instead of going through a real OIDC/OAuth2 provider:

```bash
cd control-plane
cargo run -p shiftlite-auth --example mint_dev_session -- \
  ./dev-keys/private.pem ./dev-keys/public.pem platform-admin
# prints a signed JWT — use it as the sl_session cookie value
```

Minimal walkthrough (see [docs/DEPLOYMENT.md](DEPLOYMENT.md) for full setup):

1. `make keys && make config` — generate signing keys and a `config.yaml`, then register
   at least one real cluster under `clusters:`.
2. `make crd-apply` — install the CRD on that cluster.
3. `make cp-run` — start the control plane against it.
4. Mint a session token (above) and exercise the API:

   ```bash
   TOKEN=$(cargo run -q -p shiftlite-auth --example mint_dev_session -- \
     ./dev-keys/private.pem ./dev-keys/public.pem platform-admin)
   curl -s -b "sl_session=$TOKEN" localhost:8080/api/v1/auth/me
   curl -s -b "sl_session=$TOKEN" localhost:8080/api/v1/clusters
   curl -s -b "sl_session=$TOKEN" localhost:8080/api/v1/namespaces/default/apps
   ```

   Expect: `/auth/me` echoes the minted claims, `/clusters` shows your cluster as `Ready`
   with the correct engine profile (`k3s` → `traefik`/`local-path`, `k8s` → cluster
   defaults), and `/apps` returns `[]` for a namespace with nothing deployed yet.

> ⚠️ **Warning — creating a real `AppDeployment` has side effects**
> `POST /api/v1/namespaces/{ns}/apps` creates a real custom resource, and the controller
> will immediately try to schedule a BuildKit build `Job` for it on the target cluster.
> Only do this against a cluster you're allowed to run workloads on, with a real
> registry credential configured — otherwise the build will fail at the push step (which
> is an acceptable, expected outcome if you're only testing the create → reconcile wiring,
> not a full build). Delete the `AppDeployment` afterward to clean up
> (`kubectl delete appdeployment <name> -n <namespace>`); owner references cascade-delete
> the Job/Deployment/Service/Ingress it created.

---

## Running everything before a release

```bash
make verify
```

Runs, in order: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo check`,
`cargo test --workspace`, UI typecheck, UI unit tests, and the UI production build.

## Related Pages
- [Deployment](DEPLOYMENT.md)
- [Release Process](RELEASE.md)
- [OpenAPI contract](../api/openapi.yaml)

