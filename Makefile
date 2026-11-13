SHELL                   := /bin/bash
.SHELLFLAGS             := -eu -o pipefail -c
.DEFAULT_GOAL           := help

CARGO                   ?= cargo
CARGO_FLAGS             ?= --offline --locked
TARGET_DIR              ?= target
PYTHON                  ?= python3.15
PYO3_PYTHON             ?= $(PYTHON)
PYTHON_TARGET_DIR       ?= bindings/python/target
TARGETS                 ?= rust
TESTS                   ?= native
CHECK                   ?= 0
comma := ,

ifneq ($(CHECK),0)
ifneq ($(CHECK),1)
$(error CHECK must be 0 or 1)
endif
endif

ifneq ($(words $(TARGETS)),1)
$(error TARGETS must be rust, python, or rust,python)
endif
ifeq ($(filter rust python rust$(comma)python python$(comma)rust,$(TARGETS)),)
$(error TARGETS must be rust, python, or rust,python)
endif
ifneq ($(words $(TESTS)),1)
$(error TESTS must be native, wheel, or native,wheel)
endif
ifeq ($(filter native wheel native$(comma)wheel wheel$(comma)native,$(TESTS)),)
$(error TESTS must be native, wheel, or native,wheel)
endif

selected_targets := $(subst $(comma), ,$(TARGETS))
selected_tests := $(subst $(comma), ,$(TESTS))
ifneq ($(filter wheel,$(selected_tests)),)
ifeq ($(filter python,$(selected_targets)),)
$(error Wheel tests require TARGETS to include python)
endif
endif

HELP_NAME_WIDTH         := 20

##@ Development

.PHONY: build test check fmt lint doc quality clean
# Double-colon rules run each selected component without overriding recipes.
ifneq ($(filter rust,$(selected_targets)),)
build:: ## Build selected components (Python: wheel)
	"$(CARGO)" build $(CARGO_FLAGS) --lib --target-dir "$(TARGET_DIR)"

ifneq ($(filter native,$(selected_tests)),)
test:: ## Run selected test suites
	"$(CARGO)" test $(CARGO_FLAGS) --target-dir "$(TARGET_DIR)"
endif

check:: ## Check selected components without code generation
	"$(CARGO)" check $(CARGO_FLAGS) --all-targets --target-dir "$(TARGET_DIR)"

fmt:: ## Format Rust sources; CHECK=1 checks without writing
	"$(CARGO)" fmt --package cs705 $(if $(filter 1,$(CHECK)),--check)

lint:: ## Reject Clippy warnings in selected components
	"$(CARGO)" clippy $(CARGO_FLAGS) --all-targets --target-dir "$(TARGET_DIR)" -- -D warnings

doc:: ## Build documentation and reject rustdoc warnings
	RUSTDOCFLAGS="$(RUSTDOCFLAGS) -D warnings" "$(CARGO)" doc $(CARGO_FLAGS) --no-deps --target-dir "$(TARGET_DIR)"

# Explicit target directories keep inherited CARGO_TARGET_DIR out of clean.
clean:: ## Remove selected components' build output
	"$(CARGO)" clean $(CARGO_FLAGS) --target-dir "$(TARGET_DIR)"
endif

ifneq ($(filter python,$(selected_targets)),)
build::
	cmp LICENSE-MIT bindings/python/LICENSE-MIT
	cmp LICENSE-APACHE bindings/python/LICENSE-APACHE
	PYO3_PYTHON="$(PYO3_PYTHON)" "$(PYTHON)" -m maturin build $(CARGO_FLAGS) --manifest-path bindings/python/Cargo.toml --interpreter "$(PYTHON)" --target-dir "$(PYTHON_TARGET_DIR)" --out "$(PYTHON_TARGET_DIR)/wheels"

fmt::
	"$(CARGO)" fmt --manifest-path bindings/python/Cargo.toml $(if $(filter 1,$(CHECK)),--check)

check::
	cmp LICENSE-MIT bindings/python/LICENSE-MIT
	cmp LICENSE-APACHE bindings/python/LICENSE-APACHE
	PYO3_PYTHON="$(PYO3_PYTHON)" "$(CARGO)" check $(CARGO_FLAGS) --manifest-path bindings/python/Cargo.toml --all-targets --target-dir "$(PYTHON_TARGET_DIR)"

lint::
	PYO3_PYTHON="$(PYO3_PYTHON)" "$(CARGO)" clippy $(CARGO_FLAGS) --manifest-path bindings/python/Cargo.toml --all-targets --target-dir "$(PYTHON_TARGET_DIR)" -- -D warnings

ifneq ($(filter native,$(selected_tests)),)
test::
	PYO3_PYTHON="$(PYO3_PYTHON)" "$(CARGO)" test $(CARGO_FLAGS) --manifest-path bindings/python/Cargo.toml --all-targets --target-dir "$(PYTHON_TARGET_DIR)"
endif

doc::
	PYO3_PYTHON="$(PYO3_PYTHON)" RUSTDOCFLAGS="$(RUSTDOCFLAGS) -D warnings" "$(CARGO)" doc $(CARGO_FLAGS) --manifest-path bindings/python/Cargo.toml --no-deps --target-dir "$(PYTHON_TARGET_DIR)"

clean::
	"$(CARGO)" clean $(CARGO_FLAGS) --manifest-path bindings/python/Cargo.toml --target-dir "$(PYTHON_TARGET_DIR)"

ifneq ($(filter wheel,$(selected_tests)),)
test::
	@for test in bindings/python/tests/*.py; do "$(PYTHON)" "$$test"; done
	@for module in cs705.csv._smoke cs705.icf._clipboard_check cs705.icf._normalizer_check \
	  cs705.icf.settings._digital_check cs705.icf.settings._network_check; do "$(PYTHON)" -m "$$module"; done
endif
endif

quality: override CHECK = 1
quality: fmt check lint test doc ## Check formatting, code, lints, tests, and documentation

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
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "CHECK" "Formatting mode: 0 writes, 1 checks (default: 0; quality always checks)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "TARGETS" "Comma-separated components: rust,python (default: rust)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "TESTS" "Comma-separated suites: native,wheel (default: native); wheel requires an installed Python wheel"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "CARGO" "Cargo executable (default: cargo)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "CARGO_FLAGS" "Cargo policy flags (default: --offline --locked)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "TARGET_DIR" "Package-local output directory (default: target); overrides CARGO_TARGET_DIR"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "PYTHON" "Python interpreter with maturin or the installed wheel (default: python3.15)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "PYO3_PYTHON" "Interpreter used by PyO3 (default: PYTHON)"
	@printf "  \033[36m%-*s\033[0m%s\n" "$(HELP_NAME_WIDTH)" "PYTHON_TARGET_DIR" "Python build output relative to the repository root (default: bindings/python/target)"
