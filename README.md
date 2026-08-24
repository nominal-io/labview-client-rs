# nominal-labview

A thin Rust FFI bridge that exposes [Nominal](https://nominal.io)'s canonical
Rust SDK (the `nominal` crate) to LabVIEW.

## What this is

LabVIEW talks to native code through the **Import Shared Library** wizard,
which auto-generates VIs from a C header. That wizard only understands a
narrow, old-school subset of C: primitive numeric types, `bool`, null-terminated
strings, and flat one-dimensional arrays of primitives — no 64-bit integers,
no structs containing pointers, no nested/multi-dimensional arrays, no unions.
Anything outside that subset either fails to import or comes in as an
unusable empty cluster, and has to be patched by hand.

This repo is a compiled-once solution to that problem: instead of hand-patching
VIs every time the `nominal` crate changes, the FFI layer is written so the
wizard's auto-generated bindings are already correct and never need touching.
Every exported function is restricted to that C-ABI-safe subset (see the
Golden Rule in `CLAUDE.md`) — `i32`/`u32`/`f64`/`bool`, buffer+capacity strings,
and `i32` opaque handles standing in for anything richer (clients, assets,
runs, datasets, etc.).

## How it works

- **Thin wrapper, not a reimplementation.** Every FFI function does field
  access and type flattening only — it calls straight into `nominal` and
  converts the result. No business logic, no JSON parsing, no request
  building lives in this repo; that all stays owned by the `nominal` crate
  and Nominal's API team. This keeps the LabVIEW API's behavior aligned with
  Nominal's other SDKs and means this crate shares maintenance burden with
  the team that owns `nominal`, rather than drifting into its own thing.
- **Handles, not structs.** Rust objects (`NominalClient`, `Asset`, `Run`, ...)
  never cross the FFI boundary directly. Each is stored in a per-type registry
  and referenced from LabVIEW by an opaque `i32` handle, freed explicitly by
  the caller.
- **Uniform error handling.** Every function returns an `i32` status code (`0`
  = success); actual results come back through out-parameters. This lets the
  Import Shared Library wizard wire every function into LabVIEW's error
  cluster the same way, with zero per-function configuration. On failure,
  call `nominal_last_error` to fetch the message.
- **One Tokio runtime.** `nominal`'s async API is driven from a single global
  runtime via `block_on`; LabVIEW only ever sees synchronous calls.
- **catch_unwind everywhere.** No Rust panic is allowed to unwind across the
  FFI boundary.
- **Minimal dependency footprint.** Keeping the crate graph small keeps the
  compiled shared library small and fast to load — LabVIEW loads it fresh
  each time a VI calls into it, so startup cost matters more here than in a
  typical Rust binary.

## Other design decisions

- **`nominal_<resource>_<verb>` naming** (`nominal_asset_create`,
  `nominal_asset_property_count`, ...) — getters and mutators are always
  separate functions, never overloaded on argument count/type, because C (and
  the Import Shared Library wizard) has no concept of overloading.
- **Flat `lv_src/bin/` output with exact filenames**
  (`nominalClient_64.dll`/`_32.dll`/`_64.so`) — LabVIEW resolves the correct
  library at runtime by wildcard-matching that folder, so the naming pattern
  is load-bearing; Cargo's own output names don't match it, so every build
  recipe explicitly renames on copy rather than just moving the file.
- **Staging handles for builders that carry string collections** (labels,
  properties on create/update) — a C `char**` array has no LabVIEW
  representation and imports as an unusable empty cluster, so those fields
  are accumulated one string at a time onto a staging handle
  (`_create_add_label`, `_create_set_property`, ...) before being committed.
- **Timestamps as `f64` Unix milliseconds** — 64-bit integers can't cross the
  FFI boundary at all (see the Golden Rule above), and `f64` milliseconds
  stays exact for about 285,000 years, which is plenty of headroom.
- **Macros only for the handle registry and the handle-lookup prologue** —
  that pattern is 100% mechanical, so a macro pays for itself there;
  per-function bodies are hand-written because a generating macro would need
  as many special cases as there are functions, which would be strictly
  harder for a non-professional Rust developer to audit than the repetition
  it'd save.
- **Three-tier test strategy** — marshaling tests against hand-written JSON
  fixtures (no network), mocked-HTTP tests via `wiremock`, and small
  scheduled-only integration tests against a real staging workspace — keeps
  per-PR CI fast and network-independent while still exercising real API
  behavior on a schedule.

## Layout

- `crates/ffi/` — the Rust FFI crate, one source file per `nominal` module
  (`client.rs`, `asset.rs`, `run.rs`, `dataset.rs`, ...).
- `lv_src/bin/` — compiled shared libraries (`nominalClient_64.dll`, `_32.dll`,
  `_64.so`) plus the generated header, laid out flat so LabVIEW's wildcard
  library resolution finds them.
- `lv_src/` — the LabVIEW project, typedefs, and wrapper VIs (maintained by
  hand, not generated).
- `justfile` — the only supported way to build; wraps `cargo build` per
  target and renames outputs to the LabVIEW-facing naming convention.

## Building

```
just build-all   # Windows x64/x86 + Linux x64
just test        # marshaling + mocked-HTTP tests
just lint        # fmt + clippy
```

## GitHub Workflows

- **`ci.yml`** (every push to `main`, every PR) — runs lint, tests, and both
  platform builds:
  - `lint` — `just lint` (fmt + clippy), then regenerates the cbindgen header
    and diffs it against the committed copy — catches anyone editing an FFI
    signature without running `just header`, which would otherwise leave the
    committed header (and what LabVIEW imports) silently out of sync.
  - `test` — `just test`, tiers 1+2 only (marshaling + mocked HTTP); tier-3
    integration tests never run per-PR, only on the scheduled workflow below.
  - `build-windows` — builds x64 and x86, the primary LabVIEW deployment
    targets, and uploads them as artifacts. Installs NASM
    (`ilammy/setup-nasm`) first, because `aws-lc-sys` (rustls's crypto
    backend, pulled in transitively via `nominal`) needs it to assemble its
    i686 code — the 32-bit build fails without it.
  - `build-linux` — pinned to `ubuntu-22.04`, not `-latest`, because the
    built `.so` requires the target machine's glibc to be at least the build
    machine's; a newer runner would silently raise that floor and could break
    older targets such as NI Linux RT.
  - Across every job, `cbindgen` is installed as a CLI tool
    (`taiki-e/install-action`) rather than added to `Cargo.toml` — it's
    MPL-2.0 licensed, which org policy prohibits in the dependency tree, so
    it has to stay an external tool that never touches the lockfile.
- **`nominal-bump-check.yml`** (daily cron + manual `workflow_dispatch`) —
  runs `cargo update -p nominal` and the test suite against whatever version
  that resolves to. This is the project's core value proposition: catching
  upstream `nominal` API drift as a scheduled CI failure instead of a runtime
  surprise inside LabVIEW.
  - Tests run even when `Cargo.lock` didn't move — a yanked or newly-broken
    transitive dependency should still fail loudly.
  - If the lockfile changed and tests pass, it opens a PR bumping
    `Cargo.lock`; if tests fail, the workflow itself fails visibly rather
    than opening a PR — a red run *is* the signal this project exists to
    provide.

See `CLAUDE.md` for the full design rationale, the type-conversion cookbook,
and the reasoning behind every convention above.
