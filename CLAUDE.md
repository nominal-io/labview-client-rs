# nominal-labview

Rust FFI bridge exposing the `nominal` crate (Nominal's canonical Rust SDK) to
LabVIEW as a C-ABI shared library, importable via LabVIEW's Import Shared Library
wizard with no manual signature patching.

## Goal

Wrap `nominal` in a thin, highly-readable C-ABI layer, compiled to shared
libraries LabVIEW can consume. The FFI layer does field access and type
flattening only — zero business logic, zero JSON parsing (that stays owned by
the `nominal` crate and, transitively, Nominal's API team).

Success criteria:
- LabVIEW's Import Shared Library wizard can auto-generate VIs from the emitted
  header with no manual signature patching.
- When Nominal publishes a new `nominal` version, CI rebuilds and tests
  automatically — breakage shows up as a failed scheduled CI run, not a runtime
  surprise.
- A non-professional Rust developer (the maintainer) can read any function
  top-to-bottom and understand it without prior Rust FFI experience.

## Non-Goals (explicit — do not build these)

- **No LabVIEW VI generation.** CI rebuilds the DLL/SO + header only. Running
  Import Shared Library inside LabVIEW itself stays a manual step, done by the
  maintainer, not automated.
- **No macOS investment.** If `cargo build --target x86_64-apple-darwin` works
  trivially in CI, keep it. Do not build `.framework` bundles, code-signing, or
  spend debugging time here.
- **No compound structs crossing the FFI boundary.** See Golden Rule below.
- **No single mega-macro that generates whole functions from a DSL.** See
  Macro Policy below.
- **No business logic in the FFI crate.** If it's not "convert a type" or
  "call one `nominal` method," it doesn't belong here.

## The Golden Rule

Every `extern "C"` function's parameters and return value must be one of:

| Category | Representation |
|---|---|
| Boolean | `bool` (C `_Bool`) |
| Integer / float | `i32`, `u32`, `f64` — **never `i64`/`u64`**, see below |
| String (in) | `*const c_char` (null-terminated UTF-8) |
| String (out) | caller-supplied `*mut c_char` buffer + `u32` capacity + `*mut u32` needed-bytes out-param |
| Array (in) | `*const T` + `u32` length, `T` itself primitive |
| Array (out) | not permitted — return a handle list instead (see Type Conversion Cookbook) |
| Opaque object | `i32` handle (see Handle Registries) |
| Nothing else | not permitted — flatten it |

**No 64-bit integers cross the FFI boundary, ever.** LabVIEW's Import Shared
Library wizard cannot parse any 64-bit integer type — not `long long`, not
`__int64`, not typedefs or `#define`s of either (empirically verified; each
such parameter imports as an unusable empty cluster, breaking the
no-manual-patching success criterion). Consequences:
- Handles are `i32` (two billion per process is ample).
- Sizes, capacities, counts, and indices are `u32`/`i32` — also never
  `usize`, whose width differs between the 32- and 64-bit DLLs.
- Timestamps are `f64` Unix milliseconds (exact for whole milliseconds up to
  2^53 — about 285,000 years).
- Anything genuinely 64-bit (IDs, nanosecond timestamps) must cross as a
  string or be split — prefer strings.

No `#[repr(C)]` structs with embedded pointers, no unions, no nested arrays. If
a `nominal` type has a field that isn't a primitive, it gets its own getter
function, not a field in a returned struct.

## Repository Layout

```
nominal-labview/
├── CLAUDE.md                   # this file
├── Cargo.toml                  # workspace root
├── justfile                    # all build entry points — no raw cargo commands
├── crates/
│   └── ffi/
│       ├── Cargo.toml          # depends on `nominal = "0.6"` (cbindgen is NOT
│       │                       #   a build-dependency — see Build System)
│       ├── cbindgen.toml
│       └── src/
│           ├── lib.rs          # module declarations only
│           ├── runtime.rs      # global Tokio runtime (see below)
│           ├── error.rs        # error code enum + last-error-message
│           ├── strings.rs      # shared buffer-writing helpers
│           ├── handles.rs      # handle_registry! macro definition
│           ├── client.rs       # nominal_client_new / free
│           ├── asset.rs        # nominal_asset_*
│           ├── run.rs          # nominal_run_*
│           ├── dataset.rs      # nominal_dataset_*  (mirrors nominal::core::catalog)
│           ├── connection.rs
│           ├── channel.rs
│           ├── workbook.rs
│           └── user.rs
├── lv_src/
│   ├── bin/                     # ALL shared objects land here, flat — see Output Naming
│   │   ├── nominalClient_64.dll
│   │   ├── nominalClient_32.dll
│   │   ├── nominalClient_64.so       # Ubuntu (and NI Linux RT, if validated as shareable)
│   │   ├── nominal_ffi.h             # convenience copy of the cbindgen header (canonical
│   │   │                             #   copy lives in crates/ffi/include/)
│   │   └── nilrt/                    # ONLY exists if RT needs a separate build — see Contingency
│   │       ├── nominalClient_64.so
│   │       └── README.md
│   └── <lvproj, typedefs, wrapper VIs — maintained manually, not by Claude Code>
└── .github/workflows/
    ├── ci.yml
    └── nominal-bump-check.yml
```

One source file per `nominal` module — mirrors the crate you're wrapping, so
"where's the FFI for X" always has an obvious answer.

## Type Conversion Cookbook

| `nominal` type | FFI representation |
|---|---|
| `String`, `&str` | buffer + capacity (see String Convention) |
| `HashMap<String, String>` (properties/labels) | `_count(handle, *mut u32)` + `_key_at(handle, i, buf, cap, *mut u32)` + `_value_at(handle, i, buf, cap, *mut u32)` |
| `Vec<String>` (labels) | `_count(handle, *mut u32)` + `_label_at(handle, i, buf, cap, *mut u32)` |
| `Vec<Asset>` / `Vec<Run>` / etc. (list, search results) | out-params: an `i32` handle-list handle + `u32` count. Caller reads entries with `nominal_handle_list_get(list, i, *mut i32)` and frees the list with `nominal_handle_list_free` (the resource handles inside outlive the list). Never return a raw array or double pointer — the wizard can't express them. |
| `DateTime<Utc>` | `f64`, Unix **milliseconds** (pick one unit, document it once in `error.rs`, never mix; `i64` is banned — see Golden Rule) |
| Enums (`IngestJobStatus`, `ChannelDataType`, etc.) | `i32` discriminant; keep a matching hand-written LabVIEW enum typedef in sync manually |
| `Option<T>` | for strings: empty buffer + a separate `bool`/`i32` "is_present" out-param. For handles: `0` means absent (reserve handle `0` as never-valid) |
| Builders (`AssetCreate`, `RunQuery`, etc.) | never exposed directly. Scalar-only requests: flat primitive args, one function body. Requests carrying string collections (labels, properties): a **staging handle** — see below |
| `Vec<String>` / `HashMap<String,String>` (in — labels/properties on create/update) | staging handle: `_create_begin(...)` returns an `i32` staging handle; `_create_add_label(h, s)` / `_create_set_property(h, k, v)` accumulate onto it one string at a time; `_create_commit(client, h, out)` fires the API call; `_create_free(h)` releases it (commit does **not** free — every handle is freed explicitly, uniformly). Same shape for `_update_*`. This exists because a C string array (`char**`) has no LabVIEW representation — the wizard imports it as an unusable empty cluster. The staging object is a plain parameter struct owned by the FFI crate, never the upstream builder type. On update, `_add_label`/`_set_property` mean **replace**: touching labels at all replaces the asset's entire label set with exactly the accumulated ones (upstream semantics) |

## String Convention (pick one, use everywhere)

Caller-supplied buffer, Win32-style, status code returned like every other
function:

```rust
/// Writes `value` (plus a null terminator) into `buf` of capacity `cap`
/// bytes, and stores the byte count needed (excluding the terminator) in
/// `out_needed`. Returns Ok when written, or when `buf` is null (the
/// supported size-query call). Returns BufferTooSmall — writing nothing —
/// when `buf` is non-null but `cap` < needed + 1; the caller retries with a
/// buffer of at least (*out_needed + 1) bytes.
fn write_str_field(value: &str, buf: *mut c_char, cap: u32, out_needed: *mut u32) -> i32 { ... }
```

This avoids the ownership/`_free` mismatch class of bugs entirely — no
allocation crosses the boundary in the string case. Use this exact helper from
`strings.rs` everywhere a string leaves Rust. (`nominal_last_error` uses the
message-silent `write_str_out` variant underneath, so a too-small fetch buffer
can't clobber the message being fetched.)

## Error Handling Convention

**Every FFI function — including every getter — returns `i32`**: `0` =
success, non-zero = an error-code enum value. All results (handles, strings,
counts, timestamps) come back through out-parameters; no function ever
returns data in its return value. This uniformity is load-bearing: it lets
LabVIEW's Import Shared Library wizard apply its "Function Returns Error
Code/Status" mode to every function in one pass, auto-wiring the return value
into the error cluster with no per-function configuration.

On failure, call a global `set_last_error(String)` before returning; expose:

```rust
#[no_mangle]
pub extern "C" fn nominal_last_error(buf: *mut c_char, cap: u32, out_needed: *mut u32) -> i32 { ... }
```

so LabVIEW can fetch the message after a non-zero return — same pattern as
`GetLastError`/`errno` + `strerror`.

**Every exported function body is wrapped in `catch_unwind`.** A Rust panic
unwinding across `extern "C"` is undefined behavior — this is non-negotiable,
not a nice-to-have. Put the wrapping in one small helper in `error.rs` so each
function calls it the same way rather than repeating `catch_unwind`
boilerplate everywhere.

## Async / Runtime

One global multi-threaded Tokio runtime, created once:

```rust
static RUNTIME: once_cell::sync::Lazy<tokio::runtime::Runtime> = once_cell::sync::Lazy::new(|| {
    tokio::runtime::Runtime::new().expect("failed to start Tokio runtime")
});
```

Every FFI function that calls an `async fn` on `nominal` does
`RUNTIME.block_on(async { ... })` internally. LabVIEW never sees async.

## Handle Registries

One registry per resource type, generated by a single small macro — this is
the right place for a macro, because the pattern is 100% mechanical:

```rust
handle_registry!(ClientHandle, nominal::core::NominalClient);
handle_registry!(AssetHandle, nominal::core::Asset);
handle_registry!(RunHandle, nominal::core::Run);
// ... one line per resource type
```

Each invocation expands to a `Lazy<Mutex<HashMap<i32, Arc<T>>>>`, a shared
`AtomicI32` counter, and `insert`/`get`/`remove` functions scoped to that type.
Handle `0` is reserved and never issued, so it can double as "null"/"not
found" in `Option<T>` cases above. Multiple simultaneous `ClientHandle`s must
be supported (multiple independent connections, potentially different
workspaces) — do not collapse this to a single global client.

## Macro Policy

**Use macros for:** the handle registry (above), and the repeated "look up
handle or return error code" prologue that starts almost every function. Keep
each macro small enough that `cargo expand` output is still readable
line-by-line.

**Do NOT use macros for:** the actual per-function body (arg parsing → build
request → `block_on` → convert result). Every function's logic differs enough
(different builder fields, different result shapes) that a generating macro
would need as many special cases as hand-written code has functions — at that
point it's strictly worse for readability. Write these by hand, accept the
repetition. This directly serves the "not a professional Rust developer"
requirement: a set of near-identical, fully-visible functions is easier to
audit than a macro that hides what's actually happening.

## Naming Convention

`nominal_<resource>_<verb>`, e.g. `nominal_client_new`, `nominal_client_free`,
`nominal_asset_create`, `nominal_asset_get`, `nominal_asset_name`,
`nominal_asset_property_count`, `nominal_asset_property_key_at`. Getters and
mutators are separate functions — never overload behavior on argument
count/type (C has no overloading, and it'd read as inconsistent).

## Output Naming Convention

**All built shared objects land flat in `lv_src/bin/` — no per-platform
subfolders.** LabVIEW resolves the correct file at runtime via wildcard match
on this directory, so the naming pattern is load-bearing and must be exact:

```
nominalClient_64.dll     # Windows x64
nominalClient_32.dll     # Windows x86
nominalClient_64.so      # Linux x64 — covers both Ubuntu and NI Linux RT,
                          # built from the gnu target on an old glibc
                          # baseline (see Platform Notes)
```

Cargo's own output name (`nominal_ffi.dll` / `libnominal_ffi.so`, from the
crate name) does **not** match this convention — every `just` recipe must
explicitly rename on copy, not just move the file. Get this wrong and the
wildcard match in LabVIEW silently fails to find the library.

The `cbindgen`-generated header lives canonically in
`crates/ffi/include/nominal_ffi.h`, and the `just header` recipe (a
prerequisite of every build recipe) also drops a convenience copy into
`lv_src/bin/nominal_ffi.h` so the folder LabVIEW points at contains
everything the Import Shared Library wizard needs. The header is the only
non-shared-object allowed in `lv_src/bin/` — it doesn't participate in the
wildcard match.

### Contingency: if Ubuntu and NI Linux RT can't share one `.so`

The plan above assumes one gnu-built `nominalClient_64.so` works for both
Ubuntu and NI Linux RT (validate this — see Platform Notes). **If validation
shows they need separate builds:**

- The Ubuntu build stays as `lv_src/bin/nominalClient_64.so` (flat,
  wildcard-matched, unchanged).
- The NI Linux RT build goes in its own subfolder — **not** in `lv_src/bin/`,
  since a second same-named file there would break the wildcard match:
  `lv_src/bin/nilrt/nominalClient_64.so`.
- That subfolder gets a `README.md` (content below) explaining manual
  deployment, since — unlike Windows/Ubuntu — there is no automated way to get
  a file onto an RT target from CI; this is an accepted manual step, not a gap
  to engineer around.

`lv_src/bin/nilrt/README.md` (only create this if the contingency above is
actually triggered):

```markdown
# NI Linux RT Deployment

This file only exists if the Ubuntu and NI Linux RT builds have diverged and
can no longer share the single `nominalClient_64.so` in `lv_src/bin/`. This
binary must be manually transferred to each RT target — nothing in CI does
this for you.

NI's own recommended transfer method is WebDAV (see "Using WebDAV to Transfer
Files to Real-Time Target" in NI's knowledge base). SCP/SSH below is a
supported alternative if SSH is enabled on your target.

## Transfer via SCP

1. Confirm SSH is enabled on the target: NI MAX → target → Software, or the
   target's web-based configuration → enable "Secure Shell Server (sshd)".
2. Copy the file:

       scp nominalClient_64.so admin@<target-ip>:/home/lvuser/natinst/bin/

   (default RT username is typically `admin` — adjust if yours has been changed)
3. If the directory doesn't exist yet:

       ssh admin@<target-ip> "mkdir -p /home/lvuser/natinst/bin"

## Point LabVIEW at it

In the wrapper VI's Call Library Function Node, set **library name or path** to:

    /home/lvuser/natinst/bin/nominalClient_64.so

This is manual, per target, per deployment — it is not covered by the
`lv_src/bin/` wildcard matching used on Windows/Ubuntu.

Source: NI Knowledge Base, "Calling a Shared Library in a NI Linux Real-Time
Target and LabVIEW Real-Time Module" (id kA03q000001DqeSCAS).
```

## Build System

`justfile` at the repo root — nobody runs raw `cargo` commands:

```just
# Windows 64-bit
build-win64:
    cargo build --release --target x86_64-pc-windows-msvc -p ffi
    cp target/x86_64-pc-windows-msvc/release/nominal_ffi.dll lv_src/bin/nominalClient_64.dll

# Windows 32-bit
build-win32:
    cargo build --release --target i686-pc-windows-msvc -p ffi
    cp target/i686-pc-windows-msvc/release/nominal_ffi.dll lv_src/bin/nominalClient_32.dll

# Linux (Ubuntu + NI Linux RT — see platform notes)
build-linux:
    cargo build --release --target x86_64-unknown-linux-gnu -p ffi
    cp target/x86_64-unknown-linux-gnu/release/libnominal_ffi.so lv_src/bin/nominalClient_64.so

# macOS — best-effort only, do not chase failures here
build-macos:
    cargo build --release --target x86_64-apple-darwin -p ffi || echo "macOS build skipped/failed — not required"

build-all: build-win64 build-win32 build-linux

test:
    cargo test --workspace

lint:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
```

The header is generated by the **cbindgen CLI** (`cargo install cbindgen`),
invoked by the `just header` recipe that every build recipe depends on — it
emits `crates/ffi/include/nominal_ffi.h` and copies it to
`lv_src/bin/nominal_ffi.h`. Feed either copy to LabVIEW's Import Shared
Library wizard — they are identical. cbindgen must NEVER appear in
`Cargo.toml`/`Cargo.lock` (not even as a build-dependency): it is licensed
MPL-2.0, which org policy prohibits in the dependency tree. As an external
tool it ships nothing into the binaries and stays out of the lockfile. CI
fails the lint job if the committed header doesn't match what the sources
generate.

## Platform Notes

- **Windows 64/32-bit:** straightforward — `rustup target add i686-pc-windows-msvc`
  alongside the default 64-bit target. No special tooling.
- **Ubuntu + NI Linux RT — use `x86_64-unknown-linux-gnu`.** The original
  plan called for musl (static, no glibc dependency), but that is unbuildable
  and conceptually wrong for this project: rustc cannot produce a `cdylib`
  for the musl target at all (it drops the crate type with a warning and the
  build emits no `.so`), and a shared object loaded by LabVIEW — a
  glibc-linked host process — must use the system's dynamic linker regardless,
  so "static musl" never delivered its promised independence. glibc
  compatibility is handled by building on an old baseline instead: CI pins
  the `ubuntu-22.04` runner (glibc 2.35), and the `.so` runs on any distro
  with glibc >= the build machine's. **Validation against a real NI Linux RT
  target is still a required checkpoint** — if the RT image's glibc is older
  than the build baseline, rebuild with an older-glibc toolchain (e.g.
  `cargo-zigbuild --target x86_64-unknown-linux-gnu.2.28`), and see the
  Contingency above if Ubuntu and RT can't share one `.so` at all.
- **macOS:** attempt only if free (see justfile — allowed to fail silently).

## Testing Strategy

Three tiers:

1. **Marshaling tests** (bulk of the suite) — construct a `nominal` domain
   type directly from a hand-written JSON fixture via `serde_json::from_str`,
   no network, call the FFI conversion function, assert the output
   buffer/handle is correct.
2. **Mocked HTTP tests** — `wiremock` server on `127.0.0.1`, point
   `NominalClient::builder(...).base_url(mock_server.uri())` at it, exercise
   real request-building + response-parsing + FFI conversion together.
   Include at least one test per resource type that returns a 4xx/malformed
   body and asserts the FFI function returns a non-zero code (not a panic).
3. **Integration tests** (small, scheduled only, not per-PR) — against a real
   staging workspace, RAII cleanup (`Drop` impl that archives/deletes test
   resources), distinctive naming prefix (e.g. `ffi-test-<uuid>`).

`cargo test --workspace` in CI runs tiers 1+2 on every push. Tier 3 runs on a
separate scheduled workflow with a staging token in GitHub Secrets — do not
wire tier 3 into the default `test` recipe.

## CI/CD

**`ci.yml`** — on push/PR:
- `just lint` (fmt + clippy, deny warnings)
- `just test` (tiers 1+2)
- `just build-all`

**`nominal-bump-check.yml`** — scheduled (daily cron):
- `cargo update -p nominal`
- `just test`
- If green: open a PR bumping `Cargo.lock`.
- If red: fail the workflow visibly (this *is* the "catch API drift fast"
  signal the whole project exists to provide).

## Suggested Build Order

1. Scaffold the workspace, `justfile`, `cbindgen.toml`, CI skeleton.
2. Implement `runtime.rs`, `error.rs`, `strings.rs`, `handles.rs` (the shared
   infrastructure every resource module depends on).
3. Implement **one full type end-to-end** — `client.rs` (connect/disconnect,
   via `NominalClient::builder(token).workspace_rid(...).base_url(...).build()`
   as the primary path; `from_profile`/`from_profile_env` wrappers are a
   sanctioned later addition — maintainer decision 2026-08-08 — as secondary
   constructors, never the default) + `asset.rs` (create/get/list/search/update/
   archive/unarchive + all field getters) — with full tier-1+2 test coverage
   and a green CI run, including a real Import Shared Library test against
   the generated header.
4. Only after step 3 is validated: repeat the pattern for the remaining types
   (Run, Dataset, Video, Connection, Channel, Drive/Files, Ingest, Workbook,
   User, Workspace — see Reference: Current Crate Surface below).
5. Validate the `x86_64-unknown-linux-gnu` build against a real NI Linux RT
   target; apply the Contingency section above if needed.

Do not parallelize step 4 across types before step 3 is fully proven — the
whole point of doing Asset first is to catch pattern-level mistakes once, not
twelve times.

## Reference: Current Crate Surface (`nominal` v0.6.0)

Ground truth as of this writing, pulled from docs.rs — check
`https://docs.rs/nominal/latest/nominal/all.html` for drift before starting,
since this crate is under active development (was 0.4.1 a few weeks prior to
this brief).

**Structs (grouped by resource):**
- `core::NominalClient`, `core::NominalClientBuilder`
- `core::Asset`, `core::AssetCreate`, `core::AssetUpdate`, `core::AssetsClient`
- `core::Run`, `core::RunCreate`, `core::RunUpdate`, `core::RunsClient`
- `core::Dataset`, `core::DatasetCreate`, `core::DatasetUpdate`
- `core::Video`, `core::VideoCreate`, `core::VideoUpdate`
- `core::Connection`, `core::ConnectionUpdate`
- `core::Channel`, `core::ChannelQuery`, `core::ChannelUpdate`
- `core::CatalogClient` (owns Dataset/Video/Connection/Channel operations)
- `core::Workbook`, `core::WorkbookCreate`, `core::WorkbooksClient`
- `core::Template`, `core::TemplatesClient`
- `core::User`, `core::UsersClient`
- `core::Workspace`, `core::WorkspacesClient`
- `core::IngestClient`, `core::IngestJob`, `core::UploadOptions`
- Ingest format structs: `CsvIngest`, `ParquetIngest`, `McapIngest`,
  `JournalJsonIngest`, `AvroStreamIngest`, `DataflashIngest`, `VideoIngest`
- `core::Timestamp`
- `config::Config`, `config::Profile` (profile-file path — secondary client
  constructors only, see Suggested Build Order step 3)

**Enums:**
- `core::AssetQuery`, `core::RunQuery`, `core::DatasetQuery`, `core::VideoQuery`,
  `core::WorkbookQuery` (query/filter DSLs — flatten to primitive filter args
  in each FFI `_search` function, do not expose the DSL itself)
- `core::ChannelDataType`, `core::DataSource`, `core::DatasetTarget`,
  `core::VideoTarget`, `core::FileType`, `core::IngestJobStatus`,
  `core::IngestType`, `core::TimeUnit`, `core::UploadEvent`,
  `core::WorkbookDataScope`
- `error::Error` (map this to the `i32` error codes in Error Handling
  Convention above — do not expose Rust's `Error` enum shape directly)

**Not present in this crate (confirm before assuming missing/needed):** no
dedicated `Checklist` type was found in the surface pulled above — verify
whether checklists are out of scope for v1 or live under a different module
name before scoping them into the build order.
