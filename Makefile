### KubeWeave local testing Makefile
### Run `make help` for a list of targets.

SHELL := /bin/bash
.DEFAULT_GOAL := help

CONTROL_PLANE := control-plane
UI := ui
CRD := deploy/crds/appdeployments.kubeweave.io.yaml
KUBEWEAVE_CONFIG ?= $(CONTROL_PLANE)/config.yaml

.PHONY: help
help: ## Show this help
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

## --- Setup ---

.PHONY: keys
keys: ## Generate a dev Ed25519 session-signing keypair under control-plane/dev-keys/
	mkdir -p $(CONTROL_PLANE)/dev-keys
	openssl genpkey -algorithm ed25519 -out $(CONTROL_PLANE)/dev-keys/private.pem
	openssl pkey -in $(CONTROL_PLANE)/dev-keys/private.pem -pubout -out $(CONTROL_PLANE)/dev-keys/public.pem

.PHONY: config
config: ## Copy config.example.yaml to config.yaml if it doesn't exist yet
	test -f $(KUBEWEAVE_CONFIG) || cp $(CONTROL_PLANE)/config.example.yaml $(KUBEWEAVE_CONFIG)

## --- Kubernetes (k3s/k8s) ---

.PHONY: kube-check
kube-check: ## Verify kubectl can reach the cluster (k3s or standard k8s)
	kubectl cluster-info
	kubectl version

.PHONY: crd-dry-run
crd-dry-run: ## Server-side dry-run of the AppDeployment CRD against the live cluster
	kubectl apply --dry-run=server -f $(CRD)

.PHONY: crd-apply
crd-apply: ## Install the AppDeployment CRD into the current kubectl context
	kubectl apply -f $(CRD)
	kubectl get crd appdeployments.kubeweave.io

## --- Rust control plane ---

.PHONY: cp-fmt
cp-fmt: ## Check Rust formatting
	cd $(CONTROL_PLANE) && cargo fmt --all -- --check

.PHONY: cp-lint
cp-lint: ## Run clippy across the workspace
	cd $(CONTROL_PLANE) && cargo clippy --workspace --all-targets -- -D warnings

.PHONY: cp-check
cp-check: ## cargo check the whole workspace
	cd $(CONTROL_PLANE) && cargo check --workspace

.PHONY: cp-test
cp-test: ## Run all Rust unit/integration tests
	cd $(CONTROL_PLANE) && cargo test --workspace

.PHONY: cp-build
cp-build: ## Release build of kubeweave-bin
	cd $(CONTROL_PLANE) && cargo build --release -p kubeweave-bin

.PHONY: cp-run
cp-run: config ## Run the control plane locally (KUBEWEAVE_CONFIG=$(KUBEWEAVE_CONFIG))
	cd $(CONTROL_PLANE) && KUBEWEAVE_CONFIG=$(CURDIR)/$(KUBEWEAVE_CONFIG) cargo run -p kubeweave-bin

## --- React UI ---

.PHONY: ui-install
ui-install: ## Install UI dependencies
	cd $(UI) && npm install

.PHONY: ui-generate-client
ui-generate-client: ## Regenerate src/api/schema.ts from api/openapi.yaml
	cd $(UI) && npm run generate-client

.PHONY: ui-typecheck
ui-typecheck: ## Type-check the UI
	cd $(UI) && npx tsc -b

.PHONY: ui-test
ui-test: ## Run UI unit tests (vitest)
	cd $(UI) && npm run test

.PHONY: ui-build
ui-build: ## Production build of the UI (ui/dist/)
	cd $(UI) && npm run build

.PHONY: ui-dev
ui-dev: ## Run the UI dev server (proxies /api to localhost:8080)
	cd $(UI) && npm run dev

## --- Aggregate targets ---

.PHONY: build
build: cp-build ui-build ## Build the control plane (release) and the UI

.PHONY: test
test: cp-test ui-typecheck ui-test ui-build ## Run the full local test suite (Rust + UI tests, typecheck, build)

.PHONY: verify
verify: cp-fmt cp-lint cp-check cp-test ui-typecheck ui-test ui-build ## Everything CI would check, before a release

.PHONY: clean
clean: ## Remove build artifacts (Rust target/, UI node_modules/dist)
	cd $(CONTROL_PLANE) && cargo clean
	rm -rf $(UI)/dist $(UI)/node_modules $(UI)/.vite
