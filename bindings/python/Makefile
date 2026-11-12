SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help

CARGO ?= cargo
CARGO_FLAGS ?= --offline --locked
PYTHON ?= python3.15
TARGET_DIR ?= target
export PYO3_PYTHON ?= $(PYTHON)

.PHONY: help license-check fmt-check check lint test doc quality wheel test-python
help:
	@printf '%s\n' 'Targets: license-check fmt-check check lint test doc quality wheel test-python' \
	  'Variables: CARGO CARGO_FLAGS PYTHON PYO3_PYTHON TARGET_DIR' \
	  'wheel requires maturin 1.16.0 in PYTHON; test-python requires the installed wheel.'

license-check:
	cmp ../../LICENSE-MIT LICENSE-MIT
	cmp ../../LICENSE-APACHE LICENSE-APACHE

fmt-check:
	"$(CARGO)" fmt --check

check:
	"$(CARGO)" check $(CARGO_FLAGS) --all-targets --target-dir "$(TARGET_DIR)"

lint:
	"$(CARGO)" clippy $(CARGO_FLAGS) --all-targets --target-dir "$(TARGET_DIR)" -- -D warnings

test:
	"$(CARGO)" test $(CARGO_FLAGS) --all-targets --target-dir "$(TARGET_DIR)"

doc:
	RUSTDOCFLAGS="$(RUSTDOCFLAGS) -D warnings" "$(CARGO)" doc $(CARGO_FLAGS) --no-deps --target-dir "$(TARGET_DIR)"

quality: license-check fmt-check check lint test doc

wheel: license-check
	"$(PYTHON)" -m maturin build $(CARGO_FLAGS) --interpreter "$(PYTHON)" --target-dir "$(TARGET_DIR)" --out "$(TARGET_DIR)/wheels"

test-python:
	@for test in tests/*.py; do "$(PYTHON)" "$$test"; done
	@for module in cs705.csv._smoke cs705.icf._clipboard_check cs705.icf._normalizer_check \
	  cs705.icf.settings._digital_check cs705.icf.settings._network_check; do "$(PYTHON)" -m "$$module"; done
