# Release process

Checklist for cutting a KubeWeave release. v1.0.0 followed this process; use it for
every subsequent release too.

## 1. Decide the version

Follow [Semantic Versioning](https://semver.org/):
- **patch** (`1.0.x`): bug fixes, no API/CRD/behavior changes.
- **minor** (`1.x.0`): backwards-compatible additions (new endpoints, new CRD fields with
  defaults, new UI pages).
- **major** (`x.0.0`): breaking changes to the REST API, the `AppDeployment` CRD schema,
  or the CLI/config file shape.

## 2. Bump the version in every tracked location

| File | Field |
|---|---|
| `control-plane/Cargo.toml` | `[workspace.package].version` (applies to all crates) |
| `ui/package.json` | `version` |
| `api/openapi.yaml` | `info.version` |

These three must always match. There is currently no automated check for this —
grep for the old version string across all three before tagging.

## 3. Update the changelog

Add a new `## [x.y.z] - YYYY-MM-DD` section to [CHANGELOG.md](../CHANGELOG.md) under
`[Unreleased]`, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Update the comparison links at the bottom of the file.

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

## 5. Tag and push

```bash
git add -A
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main --tags
```

## 6. Publish release notes

Use the new CHANGELOG section as the GitHub release body for the `vX.Y.Z` tag.

## What's excluded from the release

`docs/adr/` (Architecture Decision Records) and `specs/` (technical specifications) are
internal planning artifacts and are gitignored — they stay on disk locally but are not
part of the published repository or release.
