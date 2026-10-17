# Testing

KubeWeave has two independent test suites: Rust unit tests for the control plane
(`control-plane/`) and TypeScript unit tests for the UI (`ui/`). Both are wired into the
root [`Makefile`](../Makefile) — `make test` runs everything; `make verify` additionally
runs formatting and lint checks and is what should pass before cutting a release.

## Control plane (Rust)

```bash
make cp-test        # cargo test --workspace
# or, from control-plane/:
cargo test --workspace
```

Coverage per crate, as of v1.0.0:

| Crate | What's tested |
|---|---|
| `kubeweave-core` | `ApiError` → HTTP status/code mapping, `Role` ordering (`viewer < developer < platform-admin`), `EngineProfile` defaults for k8s/k3s, `Builder` default |
| `kubeweave-crd` | `AppDeploymentSpec` field defaults, `managed_labels`, `EnvVar` (de)serialization for both the literal-`value` and `secretRef` shapes |
| `kubeweave-auth` | PKCE challenge derivation, group→role mapping (dedup, no-match), session JWT encode/decode round trip, tamper detection, OAuth-cookie JWT round trip, `new_id` uniqueness |
| `kubeweave-cluster` | k3s/k8s detection from `gitVersion`, `ClusterRegistry` register/list/lookup behavior against an offline mock `kube::Client` |
| `kubeweave-controller` | BuildKit Job rendering (rootless, registry credentials), Deployment rendering (literal + `secretRef` env vars, replica/selector), Service port mapping, Ingress class selection, owner references |
| `kubeweave-api` | Input validation (DNS-1123 labels, `https://` Git URLs), error → HTTP status mapping, `AuthSession::require_role` authorization logic, OAuth provider parsing, URL-encoding helper |

Rendering functions in `kubeweave-controller` (`render_build_job`, `render_deployment`,
`render_service`, `render_ingress`) are pure — given an `AppDeployment` value they return a
typed Kubernetes object with no cluster I/O, which is what makes them unit-testable without
a live cluster. `kubeweave-cluster`'s registry tests use an offline mock `kube::Client`
(via `tower::service_fn`) for the same reason — none of the tested methods make a network
call.

### A real bug these tests caught

Adding the JWT round-trip tests in `kubeweave-auth` surfaced a genuine runtime problem: the
`jsonwebtoken` crate (v11) requires an explicit crypto provider feature (`rust_crypto` or
`aws_lc_rs`); without one, **any** sign or verify call panics at runtime with
`Could not automatically determine the process-level CryptoProvider`. This had no test
coverage before, so it would only have surfaced the first time a real user logged in or an
ID token was validated. Fixed by adding `features = ["rust_crypto"]` to the `jsonwebtoken`
dependency in both `kubeweave-auth` and `kubeweave-api`. This is a good example of why
auth-path code in particular needs an encode/decode round-trip test, not just logic tests.

## UI (React/TypeScript)

```bash
make ui-test         # npm run test (vitest)
make ui-typecheck     # npx tsc -b
make ui-build         # npm run build
```

Tests live next to the code they cover (`*.test.ts(x)`), using
[Vitest](https://vitest.dev/) + [Testing Library](https://testing-library.com/):

- `src/components/common/StatusBadge.test.tsx` — asserts the status→color mapping required
  by SPEC-001 (`Running` = green, `Pending`/`Building`/`Deploying` = yellow, `Degraded` =
  orange, `Failed` = red).
- `src/api/generated-client.test.ts` — `unwrap()`/`ApiRequestError` behavior: data pass-through,
  error-code propagation, and the `internal` fallback when the server omits a `code`.

## Running everything before a release

```bash
make verify
```

Runs, in order: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo check`,
`cargo test --workspace`, UI typecheck, UI unit tests, and the UI production build.
