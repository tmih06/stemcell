SHELL := /usr/bin/env bash
.SHELLFLAGS := -eu -o pipefail -c
MAKEFLAGS += --no-print-directory

CARGO ?= cargo
BIN ?= stemcell
PYTHON ?= python3
TOOL_FEATURES_SCRIPT ?= $(PYTHON) src/scripts/tool_features.py build_toggles.toml
MSRV ?= 1.91
AUDIT_IGNORE ?= RUSTSEC-2024-0437
COVERAGE_FEATURES ?= telegram,whatsapp,discord,slack,trello
ARGS ?=

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show available targets
	@printf "\n\033[1mstemcell\033[0m — make targets\n\n"
	@printf "Usage: \033[36mmake <target>\033[0m [ARGS='...']\n"
	@printf "Example: \033[36mmake run ARGS='chat --onboard'\033[0m\n"
	@awk 'BEGIN {FS = ":.*?## "} \
	     /^## ==/ {h = $$0; sub(/^## ==[[:space:]]*/, "", h); sub(/[[:space:]]*==[[:space:]]*$$/, "", h); \
	               printf "\n\033[1m%s\033[0m\n", h; next} \
	     /^[a-zA-Z0-9_.-]+:.*?## / {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}' \
	     $(MAKEFILE_LIST)
	@printf "\n"

## == Setup ==

.PHONY: setup
setup: ## Install system prerequisites via the repo bootstrap script
	bash src/scripts/setup.sh

## == Development ==

.PHONY: build
build: ## Build the default developer binary (from build_toggles.toml)
	@FEATURES="$$( $(TOOL_FEATURES_SCRIPT) )"; \
	STEMCELL_EXPECTED_FEATURES="$$FEATURES" \
	$(CARGO) build --locked --no-default-features --features "$$FEATURES"

.PHONY: build-ci
build-ci: ## Build all features with the repo's CI profile
	$(CARGO) build --locked --profile ci --all-features

.PHONY: build-release
build-release: ## Build the release binary
	@FEATURES="$$( $(TOOL_FEATURES_SCRIPT) )"; \
	STEMCELL_EXPECTED_FEATURES="$$FEATURES" \
	$(CARGO) build --locked --release --no-default-features --features "$$FEATURES"

.PHONY: build-no-default
build-no-default: ## Build with no default features
	$(CARGO) build --locked --no-default-features

## == Build Profiles ==
## Use build-profiles.toml or ./build.sh for custom configurations

.PHONY: build-minimal
build-minimal: ## Build core tools only, no channels
	./build.sh minimal

.PHONY: build-chatbot
build-chatbot: ## Build with no tools (pure chatbot mode)
	./build.sh chatbot

.PHONY: build-telegram
build-telegram: ## Build core tools with Telegram channel
	./build.sh telegram-agent

.PHONY: build-headless
build-headless: ## Build full tools, no channels (for API/A2A)
	./build.sh headless-agent

.PHONY: build-profiles
build-profiles: ## List available build profiles
	./build.sh --list

.PHONY: check
check: ## Fast type-check across all targets and features
	$(CARGO) check --locked --all-targets --all-features

.PHONY: run
run: ## Run the TUI or pass ARGS='...'
	@FEATURES="$$( $(TOOL_FEATURES_SCRIPT) )"; \
	if [[ -n "$(strip $(ARGS))" ]]; then \
	  STEMCELL_EXPECTED_FEATURES="$$FEATURES" \
	  $(CARGO) run --bin $(BIN) --no-default-features --features "$$FEATURES" -- $(ARGS); \
	else \
	  STEMCELL_EXPECTED_FEATURES="$$FEATURES" \
	  $(CARGO) run --bin $(BIN) --no-default-features --features "$$FEATURES"; \
	fi

.PHONY: run-release
run-release: build-release ## Run the release binary or pass ARGS='...'
	@if [[ -n "$(strip $(ARGS))" ]]; then \
	  ./target/release/$(BIN) $(ARGS); \
	else \
	  ./target/release/$(BIN); \
	fi

.PHONY: install
install: ## cargo install the current repo from source
	@FEATURES="$$( $(TOOL_FEATURES_SCRIPT) )"; \
	STEMCELL_EXPECTED_FEATURES="$$FEATURES" \
	$(CARGO) install --path . --locked --force --no-default-features --features "$$FEATURES"

.PHONY: clean
clean: ## Remove build artifacts
	$(CARGO) clean

## == Quality ==

.PHONY: fmt
fmt: ## Format the Rust codebase
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Check formatting without changing files
	$(CARGO) fmt --all -- --check

.PHONY: lint
lint: ## Run clippy with CI-level warnings as errors
	$(CARGO) clippy --locked --lib --bins --tests --all-features -- -D warnings

.PHONY: test
test: ## Run the full all-features test suite
	$(CARGO) test --locked --all-features --verbose

.PHONY: test-ci
test-ci: ## Run the Linux CI test profile (requires clang + mold)
	CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang \
	CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold" \
	$(CARGO) test --locked --profile ci --all-features --verbose

.PHONY: doc
doc: ## Build private-item docs for all features
	$(CARGO) doc --all-features --no-deps --document-private-items

.PHONY: audit
audit: ## Run cargo-audit with the repo's current ignore list
	$(CARGO) audit --ignore $(AUDIT_IGNORE)

.PHONY: coverage
coverage: ## Generate cobertura.xml via cargo-tarpaulin
	$(CARGO) tarpaulin --locked --out Xml --no-default-features --features "$(COVERAGE_FEATURES)"

.PHONY: deny
deny: ## Run cargo-deny advisories/licenses/sources checks
	$(CARGO) deny check advisories licenses sources

.PHONY: typos
typos: ## Check spelling with typos
	typos

.PHONY: secrets
secrets: ## Scan git history and working tree with gitleaks
	gitleaks git --redact --no-banner .

.PHONY: msrv
msrv: ## Verify the minimum supported Rust version still builds
	$(CARGO) +$(MSRV) build --locked --all-features

.PHONY: docs-coverage
docs-coverage: ## Verify wiki integrity (links, source refs, format)
	bash src/scripts/check-wiki.sh

.PHONY: verify
verify: ## Run the main local verification gates
	$(MAKE) fmt-check
	$(MAKE) lint
	$(MAKE) test
	$(MAKE) doc
	$(MAKE) docs-coverage

.PHONY: ci
ci: ## Run the broader CI-style suite (requires extra audit/coverage tools)
	$(MAKE) verify
	$(MAKE) build-ci
	$(MAKE) audit
	$(MAKE) coverage
	$(MAKE) deny
	$(MAKE) typos
	$(MAKE) secrets
	$(MAKE) msrv
	-$(MAKE) build-no-default
