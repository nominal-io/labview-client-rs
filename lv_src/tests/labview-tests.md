# LabVIEW manual test plan — nominal FFI

Manual smoke tests for the wizard-imported VIs, in the order they should be
run. Status as of 2026-08-08:

| Test | What it proves | Status |
|---|---|---|
| 1. Offline client lifecycle | handle/error/string machinery, no network | PASSED 2026-08-07 |
| 2. Real API list + loop | auth, search, handle lists, getters at scale | PASSED 2026-08-07 |
| 3. Staged create/update | staging-handle VIs, labels/properties, archive | PASSED 2026-08-08 |
| 4. Run lifecycle | run VIs, f64 timestamps, run-number search, asset link | NOT YET RUN |

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

## Test 4 — Run lifecycle: staged create → verify → search → update → archive (NOT YET RUN)

Creates a REAL run in the workspace; the final archive step hides it. Needs a
real token. New ground vs Test 3: f64 millisecond timestamps in both
directions, the run-number getter, search by run number, and linking a run to
an asset.

**Timestamps in LabVIEW.** Every time parameter is a DBL of Unix
milliseconds (UTC). LabVIEW's `Get Date/Time In Seconds` uses the 1904 epoch,
so convert: `(To Double Precision Float(Get Date/Time In Seconds) -
2082844800) * 1000`. Wrap that in a small "NowMs.vi" — every run test needs
it. Send whole milliseconds (round the result) so values compare exactly on
the way back; the DLL rounds fractional input to the nearest ms.

Setup — one asset to link against:
1. `nominal client new.vi` — real token.
2. `nominal asset create.vi` — name `labview-ffi-run-test-asset` (flat
   create is fine) → asset handle; `nominal asset rid.vi` → keep the RID on
   a wire.

Staged create:
3. `nominal run create begin.vi` — name `labview-ffi-run-1`, start_ms =
   NowMs → staging.
4. `nominal run create set description.vi` — any text.
5. `nominal run create set end.vi` — NowMs + 60000 (a one-minute run).
6. For Loop over `["labview", "ffi-test"]` → `nominal run create add
   label.vi`.
7. `nominal run create set property.vi` — key `phase`, value `smoke-test`.
8. `nominal run create add asset.vi` — the asset RID from step 2.
9. `nominal run create commit.vi` — client + staging → run handle.
10. `nominal run create free.vi`.

Verify on the run handle:
- `run name` = `labview-ffi-run-1`; `run description` matches
- `run number` (U32) — real number assigned by the server, keep it on a wire
- `run start` = the start_ms you sent (exact, if you sent whole ms)
- `run end` — is_present true, value = start + 60000
- `run created at` — recent, sanity-check only
- `run label count` = 2; `label at` 0/1
- `run property count` = 1; key `phase`, value `smoke-test`
- `run asset count` = 1; `run asset rid at` 0 = the RID from step 2
- `run rid` — keep on a wire; `run url` — optional browser check (URL uses
  the run number, not the RID)

Search by run number (proves the non-string filters):
11. `nominal run search.vi` — search_text/label/property empty, run_number =
    the number from the verify step, has_start_after/has_end_before false →
    expect count = 1; `nominal handle list get` index 0 → `run rid` matches;
    free the run handle and the list.

Staged update (replace semantics again):
12. `nominal run update begin.vi` → staging; `add label` `updated`;
    `set end` NowMs + 120000.
13. `nominal run update commit.vi` — client, run RID, staging → new handle.
14. `nominal run update free.vi`. On the NEW handle: `label count` = 1
    (labels replaced), `end` = the new value, `start` unchanged.

Cleanup:
15. `nominal run archive.vi` — client + run RID.
16. `nominal asset archive.vi` — client + asset RID (from step 2).
17. `nominal run free.vi` on both run handles, `nominal asset free.vi`,
    `nominal client free.vi`.
