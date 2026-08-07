# LabVIEW manual test plan — nominal FFI

Manual smoke tests for the wizard-imported VIs, in the order they should be
run. Status as of 2026-08-07:

| Test | What it proves | Status |
|---|---|---|
| 1. Offline client lifecycle | handle/error/string machinery, no network | PASSED 2026-08-07 |
| 2. Real API list + loop | auth, search, handle lists, getters at scale | PASSED 2026-08-07 |
| 3. Staged create/update | staging-handle VIs, labels/properties, archive | NOT YET RUN |

Re-run all three after any re-import, and after any DLL change that touches
signatures.

---

## Import setup (once per import — get these right first)

- Import against `lv_src\bin\nominal_ffi.h` + `lv_src\bin\nominalClient_64.dll`
  (64-bit LabVIEW). The `bin` folder is the build output — the source of
  truth. The wizard makes its own DLL copy next to the generated VIs; after
  every Rust rebuild that copy is stale until the wizard is re-run (symptom:
  "cannot find function in the dll" for newly added functions).
- **Restart LabVIEW and delete the wizard output folder before re-running the
  wizard.** LabVIEW binds to stale in-memory custom controls by name — this
  once kept broken empty-cluster controls alive through three imports.
- Error Handling Mode: **"Function Returns Error Code/Status"** for every
  function.
- Calling convention: **C** (not stdcall). Thread: **"Run in any thread"**
  (wizard defaults to UI thread — that freezes the whole LabVIEW UI for the
  duration of every network call). Make the wrapper VIs **reentrant**.
- Leave Include Paths and Preprocessor Definitions empty — the header is
  self-contained by design.
- The `aws_lc_*` functions listed by the wizard are rustls internals — leave
  them unchecked, always.

## Conventions the tests rely on

- **String getters** (`buf`, `cap`, `out_needed`): wire a pre-padded string
  into `buf` — Initialize Array (U8, 1024) → Byte Array To String — and wire
  `cap` = 1024. NEVER wire an empty string with a large `cap`: the DLL trusts
  `cap` and would write past the real buffer. `buf out` comes back clean
  (LabVIEW cuts at the null terminator). If a call errors with code 10
  (BufferTooSmall), `out_needed` says how many bytes to allocate (+1).
- **Error messages**: after any non-zero return, run Clear Errors →
  `nominal last error.vi` (nodes skip execution when error-in is set, so the
  Clear Errors is required). Read it immediately — the message slot is global
  per process.
- **Handles**: I32, never 0, never reused within a process, reset on LabVIEW
  restart. Never persist a handle — persist the RID string and re-`get`.
  Everything obtained from a `_new`/`_begin`/`_get`/`_commit`/`_list` call is
  freed with its own `_free` function, explicitly, once.

---

## Test 1 — Offline client lifecycle (no token, no network)

Chain, error wire straight through:

1. `nominal client new.vi` — token `test-token`, workspace_rid `""`,
   base_url `""` → client handle (expect >= 1).
2. `nominal client base url.vi` — padded buffer → expect
   `https://api.gov.nominal.io/api`.
3. `nominal client free.vi` → expect return 0 / no error.
4. `nominal client free.vi` again, same handle → expect error code 4 in the
   error cluster; Clear Errors → `nominal last error.vi` → message
   `invalid ClientHandle handle: <n>`.

## Test 2 — Real API list + loop (needs a real API token)

1. `nominal client new.vi` — real token, workspace RID or empty, base_url
   empty (production).
2. `nominal asset search.vi` — all four filters empty (matches everything)
   → list handle + count. NOTE: paginated network fetch; large workspaces
   take tens of seconds. The VI blocks but the UI must stay responsive — if
   the UI freezes, a CLFN is still set to "Run in UI thread".
3. For Loop wired to count: `nominal handle list get.vi` (list, i) →
   `nominal asset name.vi` (padded buffer) → build names array →
   `nominal asset free.vi` per handle.
4. `nominal handle list free.vi`, then `nominal client free.vi`.

Expect: array of your workspace's asset names, no errors. Gotcha found in the
first run: make sure only asset handles reach `nominal asset free.vi` — the
list handle goes to `nominal handle list free.vi` only (a mixed-up wire shows
up as one invalid-handle error, typically on the highest handle number).

## Test 3 — Staged create → verify → staged update → archive (NOT YET RUN)

Creates a REAL asset in the workspace — use a distinctive name; the final
archive step hides it. Needs a real token.

Create:
1. `nominal client new.vi` — real token.
2. `nominal asset create begin.vi` — name `labview-ffi-test-1` → staging.
3. `nominal asset create set description.vi` — any text.
4. For Loop over `["labview", "ffi-test"]` → `nominal asset create add
   label.vi` (staging + error wire through shift registers).
5. `nominal asset create set property.vi` — key `vehicle`, value `test-rig`.
6. `nominal asset create commit.vi` — client + staging → asset handle.
7. `nominal asset create free.vi` — staging (commit does NOT free).

Verify on the asset handle (padded buffers):
- `asset name` = `labview-ffi-test-1`
- `asset description` present, matches
- `asset label count` = 2; `label at` 0/1 = `ffi-test`, `labview` (sorted)
- `asset property count` = 1; key `vehicle`, value `test-rig`
- `asset rid` — keep on a wire, needed below
- `asset url` — optional: open in a browser, see the asset in the web app

Update (also demonstrates REPLACE semantics — deliberate):
8. `nominal asset update begin.vi` → staging.
9. `nominal asset update add label.vi` — `updated`.
10. `nominal asset update commit.vi` — client, RID, staging → new asset
    handle.
11. `nominal asset update free.vi`.
12. On the NEW handle: `label count` = 1, `label at 0` = `updated` — the
    original labels are gone, because touching labels in an update replaces
    the entire set. (The old handle still reads 2 — handles are snapshots.)

Cleanup:
13. `nominal asset archive.vi` — client + RID.
14. `nominal asset free.vi` on both asset handles; `nominal client free.vi`.
