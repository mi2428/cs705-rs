SHELL                   := /bin/bash
.SHELLFLAGS             := -eu -o pipefail -c
.DEFAULT_GOAL           := help

CARGO                   ?= cargo
CARGO_FLAGS             ?= --offline --locked
TARGET_DIR              ?= target

HELP_NAME_WIDTH         := 15

##@ Development

.PHONY: build test check fmt fmt-check lint doc quality clean
build: ## Build the CLI executable
	"$(CARGO)" build $(CARGO_FLAGS) --target-dir "$(TARGET_DIR)"

test: ## Run CLI unit and integration tests
	"$(CARGO)" test $(CARGO_FLAGS) --target-dir "$(TARGET_DIR)"

check: ## Check the CLI and its tests without code generation
	"$(CARGO)" check $(CARGO_FLAGS) --all-targets --target-dir "$(TARGET_DIR)"

fmt: ## Format this package's Rust sources
	"$(CARGO)" fmt --package cs705-cli

fmt-check: ## Check formatting without rewriting sources
	"$(CARGO)" fmt --package cs705-cli --check

lint: ## Reject Clippy warnings in the CLI and its tests
	"$(CARGO)" clippy $(CARGO_FLAGS) --all-targets --no-deps --target-dir "$(TARGET_DIR)" -- -D warnings

doc: ## Build CLI documentation and reject rustdoc warnings
	RUSTDOCFLAGS="$(RUSTDOCFLAGS) -D warnings" "$(CARGO)" doc $(CARGO_FLAGS) --no-deps --target-dir "$(TARGET_DIR)"

quality: fmt-check check lint test doc ## Check formatting, code, lints, tests, and documentation

# Explicit target directories keep inherited CARGO_TARGET_DIR out of clean.
clean: ## Remove only this package's selected build output
	"$(CARGO)" clean $(CARGO_FLAGS) --target-dir "$(TARGET_DIR)"

##@ Help

.PHONY: help
help: ## Show this help message
	@awk -v width="$(HELP_NAME_WIDTH)" 'BEGIN { FS = ":.*##" } \
		/^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } \
		/^[a-zA-Z0-9_.-]+:.*##/ { \
			sub(/^[[:space:]]+/, "", $$2); \
			printf "  \033[36m%-*s\033[0m%s\n", width, $$1, $$2 \
		}' $(MAKEFILE_LIST)
	@printf "\n\033[1mVariables:\033[0m\n"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "CARGO" "Cargo executable (default: cargo)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "CARGO_FLAGS" "Cargo policy flags (default: --offline --locked)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "TARGET_DIR" "Package-local output directory (default: target); overrides CARGO_TARGET_DIR"
