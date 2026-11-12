SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help

PYTHON ?= python3.15
RUFF ?= ruff
PYRIGHT ?= pyright

.PHONY: help check test quality
help:
	@printf '%s\n' 'Targets: check test quality' \
	  'Variables: PYTHON (with cs705 installed), RUFF, PYRIGHT'

check:
	"$(RUFF)" check --target-version py315 --select E,F,I,UP,B,ANN,D --ignore D203,D213 memory_diff.py test_memory_diff.py
	"$(RUFF)" format --check --target-version py315 memory_diff.py test_memory_diff.py
	"$(PYRIGHT)" --pythonversion 3.15 --pythonpath "$(PYTHON)" memory_diff.py test_memory_diff.py

test:
	"$(PYTHON)" -B -m unittest -v test_memory_diff

quality: check test
