# Release Process

| | |
|---|---|
| **Status** | 🟢 Current |
| **Owner** | Platform / Control Plane team |
| **Applies to** | v1.0.0+ |
| **Last reviewed** | 2026-10-18 |
| **Related pages** | [Testing](TESTING.md) · [Deployment](DEPLOYMENT.md) |

> ℹ️ **Summary**
> Checklist for cutting a ShiftLite release. v1.0.0 followed this process; use it for
> every subsequent release too.

## Contents
- [1. Decide the version](#1-decide-the-version)
- [2. Bump the version in every tracked location](#2-bump-the-version-in-every-tracked-location)
- [3. Update the changelog](#3-update-the-changelog)
- [4. Run the full verification suite](#4-run-the-full-verification-suite)
- [5. Tag and push](#5-tag-and-push)
- [6. Publish release notes](#6-publish-release-notes)
- [What's excluded from the release](#whats-excluded-from-the-release)

---

## 1. Decide the version

Follow [Semantic Versioning](https://semver.org/):

| Bump | When |
|---|---|
| **patch** (`1.0.x`) | Bug fixes, no API/CRD/behavior changes |
| **minor** (`1.x.0`) | Backwards-compatible additions (new endpoints, new CRD fields with defaults, new UI pages) |
| **major** (`x.0.0`) | Breaking changes to the REST API, the `AppDeployment` CRD schema, or the CLI/config file shape |

---

## 2. Bump the version in every tracked location

| File | Field |
|---|---|
| `control-plane/Cargo.toml` | `[workspace.package].version` (applies to all crates) |
| `ui/package.json` | `version` |
| `api/openapi.yaml` | `info.version` |

> ⚠️ **Warning — these three must always match**
> There is currently no automated check for this — grep for the old version string across
> all three before tagging.

---

## 3. Update the changelog

Add a new `## [x.y.z] - YYYY-MM-DD` section to [CHANGELOG.md](../CHANGELOG.md) under
`[Unreleased]`, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Update the comparison links at the bottom of the file.

---

## 4. Run the full verification suite

```bash
make verify
```

This runs, across both the Rust workspace and the UI: formatting checks, clippy (deny
warnings), `cargo check`, `cargo test --workspace`, TypeScript typecheck, UI unit tests,
and a production UI build. All of it must pass before tagging.

If you have a cluster available, also sanity-check the CRD against it (non-destructive):

```bash
make crd-dry-run
```

> 💡 **Note**
> For a deeper check, run the end-to-end walkthrough in
> [Testing — End-to-end testing against a real cluster](TESTING.md#end-to-end-testing-against-a-real-cluster)
> before a release that touches auth, the controller, or the cluster registry.

---

## 5. Tag and push

```bash
git add -A
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main --tags
```

---

## 6. Publish release notes

Use the new CHANGELOG section as the GitHub release body for the `vX.Y.Z` tag.

---

## What's excluded from the release

> ℹ️ **Note**
> `docs/adr/` (Architecture Decision Records) and `specs/` (technical specifications) are
> internal planning artifacts and are gitignored — they stay on disk locally but are not
> part of the published repository or release.

## Related Pages
- [Testing](TESTING.md)
- [Deployment](DEPLOYMENT.md)
- [Changelog](../CHANGELOG.md)

