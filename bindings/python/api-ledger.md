# Compiler-resolved API ledger

## Evidence

`api-ledger.tsv` is the per-public-path inventory, extracted from rustdoc JSON
on 2026-11-11 using Rust 1.99.1 from the `feat/python-bindings` working tree
(based on `7ebdc5ad9efe9e2fbf0eb3adc563ca8ae05672ec`). The current snapshot
contains 7,489 public paths, including the ownership and fallibility changes.
It is not a declaration-regex count or a generated binding implementation.
Rustdoc resolves signatures without checking every function body; inventory
generation is not evidence that the core or the bindings compile.

Command: `RUSTC_BOOTSTRAP=1 cargo rustdoc --lib -- -Z unstable-options --output-format json`.
The environment override enables rustdoc's unstable JSON output only; ordinary
package builds do not use it. Full compiler output stays under `target/doc`.

Public module edges and `use` targets are followed by compiler item IDs.
Inherent impls are followed from each reachable type, including expanded macro
methods and methods implemented in private source files. Public fields, enum
variants, constants and explicit trait implementations are included. Synthetic
and blanket standard trait impls are not individual Python operations.
Identical `identity` values identify re-exports: register one Python class and
alias it, rather than constructing duplicate types. IDs are snapshot-local.

## Columns and implementation contracts

- `rust_path`: externally reachable Rust path (not private implementation path).
- `python_target`: proposed public Python path, not evidence of implementation.
  Trait impl rows map to the Python construction/equality/iteration/conversion
  operation, not a literal method named `<impl ...>`.
- `signature`: compiler-resolved signature; unusual type forms and generic
  predicates retain structural rustdoc JSON to avoid lossy guesses. Variant
  field IDs resolve to the accompanying variant field rows.
- `dev`: responsible design development item, not worker identity.
- `worker`: current implementation assignment. `unassigned` explicitly identifies
  the clone, display/printing and presentation/diagnostic/help surfaces not
  assigned to this six-worker batch. Shared conversion/owner support does not transfer
  ownership of a domain wrapper to its foundation worker.
- `status`: `pending` for every domain symbol in this baseline inventory.
- `ownership`, `mutation`, `callback_error`: conservative triage derived from
  resolved signatures, **not completed per-operation safety decisions**.
  `borrow-review` requires the domain worker to decide owned snapshot versus
  retained owner/scoped loan. `live-mutable` requires actual owner mutation;
  field assignment cannot silently edit a detached copy. `native-signature`
  requires checking the exact receiver and explicit mutable arguments.
  Every callback contract remains subject to DEV-07 scoped/GC integration;
  preserve native errors and callback exceptions without fallback values.

Boundary defaults: immutable bytes are `bytes`, mutable bytes use `MutableBytes`,
mutable bool uses `Bool`. Strict integer helpers reject bool and overflow;
domain errors use `cs705.Error` with `domain`, `kind` and variant fields.
Owned values transfer independently; lifetime-bearing operations need explicit
owners. Unit enums require distinct Enum classes, data enums named variants,
and proof-bearing native outputs cannot be publicly forged.

## Known limits / remaining work

This is the host's compiler-resolved public surface. Windows-only private
implementation details are not fabricated into public modules; public NLS
entry points remain in the inventory. Ongoing concurrent API changes require
another refresh before treating the inventory as a final snapshot.

The worker/status columns and development-item judgments above describe the
extraction-time baseline, not the current initializer. See `src/lib.rs` for
actual native registrations and `python/cs705` for facades and type annotations.
Do not infer completion or missing implementations from these historical rows.

The proposed `python_target` is not always a literal attribute path:

- Domain error values use `ErrorData`; `cs705.Error` is the raised exception.
- Rust `new` maps to Python construction, and data-variant fields belong to the
  resulting value, not its variant constructor.
- Enum metadata named `name` uses `native_name()` to preserve Python Enum's
  own `name` property. Keyword variants can require `getattr`, such as `None`.
- Rust `csv.dispatch.Dispatch.import` is exposed as `import_kind`, matching
  its Python constructor. No legacy `import` attribute is registered.
- `NormalMemory.native_*` operations are functions in `icf.rows.native` with
  an explicit model argument; `WorkingUr.native_text*` live in `icf.rows.ur`.
- Nested mutable fields can use explicit `*_snapshot()` and `set_*()` methods
  or live owner-backed projections. A matching name alone proves neither
  ownership semantics nor mutation behavior.
- NLS producers `ascii_map`, `platform_map` and `platform_uppercase` accept
  owned `MutableBytes` or writable callback-scoped `csv.nls.Bytes`. Accepting
  both does not extend a loan's lifetime or grant write access to a readonly loan.

Check constructors, fields, variants, constants, aliases and explicit
operations against both the public `.pyi` contract and the registered native
object. An import or an attribute inventory is not a behavioral coverage test.
