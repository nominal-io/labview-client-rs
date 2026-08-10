# LabVIEW manual test plan — nominal FFI

Manual smoke tests for the wizard-imported VIs, in the order they should be
run. Status as of 2026-08-08:

| Test | What it proves | Status |
|---|---|---|
| 1. Offline client lifecycle | handle/error/string machinery, no network | PASSED 2026-08-07 |
| 2. Real API list + loop | auth, search, handle lists, getters at scale | PASSED 2026-08-07 |
| 3. Staged create/update | staging-handle VIs, labels/properties, archive | PASSED 2026-08-08 |
| 4. Run lifecycle | run VIs, f64 timestamps, run-number search, asset link | PASSED 2026-08-08 |
| 5. Dataset lifecycle | dataset VIs, channel delimiter, catalog endpoints | PASSED 2026-08-08 |
| 6. Video lifecycle | video VIs, /video/v1 endpoints | PASSED 2026-08-08 |
| 7. Channel metadata | channel VIs, data-type enum, metadata upsert | PASSED 2026-08-08 |
| 8. CSV ingest | file upload, ingest job polling, real data in a dataset | PASSED 2026-08-08 |
| 9. Workbook from template | template get, workbook create/search/archive | PASSED 2026-08-08 |
| 10. Who am I | user VIs, token identity probe | PASSED 2026-08-08 |
| 11. Workspace discovery | workspace VIs, finding your workspace RID | PASSED 2026-08-08 |
| 12. Data-source attach | asset/run attach VIs, series-tag staging, scope-name conflict | NOT YET RUN |
| 13. MCAP ingest | mcap staging VIs, topic filters, non-tabular job flow | NOT YET RUN |
| 14. Video ingest | video upload VI, f64 start time, video-RID job result | NOT YET RUN |

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

## Test 5 — Dataset lifecycle (NOT YET RUN)

Same shape as Test 3, against the dataset VIs. Creates a REAL (empty)
dataset; the archive step hides it. Datasets have no timestamps-in and no
data-source getters — the new ground is the channel delimiter and the
catalog endpoints behind the scenes.

1. `nominal client new.vi` — real token.
2. `nominal dataset create begin.vi` — name `labview-ffi-dataset-1` →
   staging.
3. `nominal dataset create set description.vi` — any text.
4. `nominal dataset create set channel delimiter.vi` — `.` (groups channel
   names like `engine.temp` into a tree in the UI).
5. `add label` `labview`; `set property` key `phase` value `smoke-test`.
6. `nominal dataset create commit.vi` → dataset handle;
   `nominal dataset create free.vi`.
7. Verify: `dataset name`, `description` (is_present true), `label count` =
   1, `property` key/value, `created at` recent, `dataset rid` on a wire,
   `dataset url` — optional browser check (goes to the data-sources page).
8. `nominal dataset search.vi` — search_text `labview-ffi`, other filters
   empty → expect your dataset among the results (loop
   `nominal handle list get` → `dataset rid`, compare).
9. Staged update: `update begin` → `add label` `updated` → `update commit`
   (client, RID, staging) → on the new handle `label count` = 1, label =
   `updated` (replace semantics). `update free`.
10. Cleanup: `nominal dataset archive.vi`, free both dataset handles,
    `nominal client free.vi`.

## Test 6 — Video lifecycle (PASSED 2026-08-08)

Same shape as Test 5, against the video VIs. Creates a REAL video in the
workspace; the archive step hides it. A video created this way is an empty
metadata shell — the media file would arrive via ingest later — so there is
nothing to play, only metadata to verify. Videos have no channel delimiter
and no timestamps beyond created-at.

1. `nominal client new.vi` — real token.
2. `nominal video create begin.vi` — name `labview-ffi-video-1` → staging.
3. `nominal video create set description.vi` — any text.
4. `nominal video create add label.vi` — `labview`.
5. `nominal video create set property.vi` — key `camera`, value `front`.
6. `nominal video create commit.vi` — client + staging → video handle.
7. `nominal video create free.vi` (commit does NOT free).
8. Verify on the video handle (padded buffers as usual):
   - `video name` = `labview-ffi-video-1`
   - `video description` — is_present true, matches
   - `video label count` = 1; `label at 0` = `labview`
   - `video property count` = 1; key `camera`, value `front`
   - `video created at` — recent, sanity-check only
   - `video rid` — keep on a wire, needed below
   - `video url` — optional browser check (goes to the data-sources page)
9. `nominal video search.vi` — search_text `labview-ffi`, other filters
   empty → expect your video among the results (loop
   `nominal handle list get` → `video rid`, compare). Free the video
   handles and the list.
10. Staged update: `nominal video update begin.vi` → staging;
    `nominal video update add label.vi` — `updated`;
    `nominal video update commit.vi` — client, RID, staging → new handle.
    `nominal video update free.vi`. On the NEW handle: `label count` = 1,
    `label at 0` = `updated` — the original label is gone (replace
    semantics, same as every other type).
11. Cleanup: `nominal video archive.vi` — client + RID;
    `nominal video free.vi` on both video handles;
    `nominal client free.vi`.

## Test 7 — Channel metadata (PASSED 2026-08-08)

Channels have no create/archive lifecycle — they exist on data sources, and
their metadata can be seeded via the upsert even before data arrives. This
test uses a fresh empty dataset so no real channel data is touched.

The `data_type` parameter is a `NominalChannelDataType` I32:
0=Double 1=Int 2=Uint 3=String 4=Log 5=DoubleArray 6=StringArray 7=Struct
8=Video 9=Spatial 10=Unknown (output only — never valid as input). In
`nominal channel search.vi`, -1 means "no data-type filter".

1. `nominal client new.vi` — real token.
2. `nominal dataset create.vi` — name `labview-ffi-channel-test` (flat is
   fine) → dataset handle; `nominal dataset rid.vi` → RID on a wire.
3. `nominal channel set metadata.vi` — data_source_rid = the dataset RID,
   name `labview.test.channel`, data_type 0 (Double), description
   `smoke test channel`, unit `degC`, clear_unit false → channel handle.
   This seeds the metadata record (the channel has no data — that's fine).
4. Verify on the returned handle: `channel name`, `channel data source
   rid`, `channel description` (is_present true), `channel unit` = `degC`
   (is_present true), `channel data type` = 0.
5. `nominal channel get.vi` — same RID + name → fresh handle; verify the
   same values came back from the server. Free both channel handles.
6. Optional, against real data: `nominal channel list.vi` with the RID of
   any dataset that has ingested data → loop `nominal handle list get` →
   `channel name` array. (On the empty test dataset the seeded metadata may
   or may not appear in search results — don't treat that as a failure.)
7. Cleanup: `nominal dataset archive.vi`, `nominal dataset free.vi`,
   `nominal client free.vi`.

## Test 8 — CSV ingest (NOT YET RUN)

The payoff test: real data lands in a dataset you can open in the Nominal
app. Prepare a small CSV on disk first, e.g. C:\temp\labview-ffi-test.csv:

    time,temperature,pressure
    1723200000,20.5,101.2
    1723200001,21.0,101.3
    1723200002,21.5,101.1

(times are epoch-seconds — any recent values work)

The `time_unit` parameter is a `NominalTimeUnit` I32: 0=Nanoseconds
1=Microseconds 2=Milliseconds 3=Seconds 4=Minutes 5=Hours 6=Days.
Job status (`NominalIngestJobStatus` I32): 0=Submitted 1=Queued
2=InProgress 3=Completed 4=Failed 5=Cancelled 6=Unknown.

1. `nominal client new.vi` — real token.
2. `nominal dataset create.vi` — name `labview-ffi-ingest-test` → RID on a
   wire (keep the handle too).
3. `nominal ingest tabular begin.vi` → staging.
4. `nominal ingest tabular set timestamp epoch.vi` — staging, column
   `time`, time_unit 3 (Seconds).
5. Optional: `nominal ingest tabular add file tag.vi` — tag `source`,
   value `labview`.
6. `nominal ingest csv.vi` — client, staging, the CSV path, the dataset
   RID → job handle. NOTE: blocks for the upload duration (small file ≈
   a second or two).
7. On the job handle: `ingest job rid` (keep on a wire), `ingest job
   status` (likely 0-2), `ingest job result rid` — is_present true, equals
   the dataset RID.
8. `nominal ingest job wait.vi` — client, job RID, poll_interval_ms 0
   (default 2s) → new handle. BLOCKS until the server finishes ingesting
   (typically well under a minute for three rows). `ingest job status` on
   the new handle = 3 (Completed). A failed job is status 4 here, not an
   FFI error.
9. Verify the payoff: `nominal channel list.vi` with the dataset RID →
   expect channels `temperature` and `pressure` (the `time` column is
   consumed as the timestamp). Or open the dataset URL in the app and see
   the data.
10. Cleanup: free the job handles, staging, dataset handle; DON'T archive
    the dataset if you want to look at the data first. `nominal client
    free.vi`.

## Test 9 — Workbook from template (NOT YET RUN)

Requires an existing TEMPLATE in the workspace (create one in the Nominal
app: Workbooks -> Templates, or use any you already have) — grab its RID
from the app URL. Also reuse a real asset RID (e.g. from Test 3's asset
before archiving, or any real asset).

1. `nominal client new.vi` — real token.
2. `nominal template get.vi` — template RID → template handle. Verify
   `template title` / `template commit id` are non-empty.
3. `nominal workbook create begin.vi` → staging.
4. `nominal workbook create set title.vi` — `labview-ffi-workbook-1`
   (optional — defaults to the template's title).
5. `nominal workbook create add scope asset.vi` — a real asset RID.
   (Assets and runs are mutually exclusive — adding a scope run now would
   return error 5.)
6. `nominal workbook create commit.vi` — client, template handle, staging
   → workbook handle. `nominal workbook create free.vi`.
7. Verify: `workbook name` = your title; `workbook scope type` = 0
   (Assets); `workbook scope rid count` = 1; `workbook scope rid at` 0 =
   the asset RID; `workbook url` — open it in the browser and see the
   workbook rendered from the template.
8. `nominal workbook search.vi` — asset_rid = the same RID, other filters
   empty → your workbook among the results.
9. Cleanup: `nominal workbook archive.vi` (client + workbook RID), free
   workbook + template handles, `nominal client free.vi`.

## Test 10 — Who am I (NOT YET RUN)

The smallest test — and the recommended first call in any real LabVIEW
application, as a cheap "is my token valid" probe before doing real work.

1. `nominal client new.vi` — real token.
2. `nominal user me.vi` — client → user handle.
3. Verify: `user email` = your login email; `user display name` non-empty;
   `user rid` starts with `ri.security.` and contains `.user.`;
   `user org rid` contains `.org.`.
4. Sanity: run `nominal user me.vi` with a garbage token (new client with
   token `bad-token-123`) → expect error 8 (ApiError, HTTP 401) — proving
   the probe actually detects bad credentials.
5. Cleanup: free both user handles (if step 4 produced one — it should
   not), both clients.

## Test 11 — Workspace discovery (NOT YET RUN)

Answers "what do I pass as workspace_rid to nominal client new.vi?" from
inside LabVIEW instead of copying it out of the web app.

1. `nominal client new.vi` — real token, workspace_rid EMPTY.
2. `nominal workspace list.vi` — client → list handle + count (expect >= 1).
3. For Loop over count: `nominal handle list get.vi` → per workspace read
   `workspace display name` (is_present may be false — show the RID then)
   and `workspace rid` → build a table.
4. Pick your workspace''s RID from the table; `nominal client free.vi`, then
   `nominal client new.vi` again passing that RID as workspace_rid.
5. `nominal client workspace rid.vi` on the new client — is_present true,
   equals what you passed.
6. Cleanup: free the workspace handles inside the loop, the list, and the
   client.

## Test 12 — Data-source attach (NOT YET RUN)

Wires ingested data to runs and assets — the step that makes Test 8's dataset
show up on an asset or run in the app. Creates a REAL asset, run, and empty
dataset; the archive steps hide them. Needs a real token.

Attaches are one-source-per-call. Scope names (assets) / ref names (runs)
must be unique per asset/run — the server rejects a duplicate (proven in
step 8). Repeated calls accumulate sources; the end state is the same as a
batched attach.

Setup:
1. `nominal client new.vi` — real token.
2. `nominal asset create.vi` — name `labview-ffi-attach-asset` → keep handle
   + `asset rid` on a wire.
3. `nominal dataset create.vi` — name `labview-ffi-attach-dataset` → keep
   handle + `dataset rid` on a wire.
4. `nominal run create.vi` — name `labview-ffi-attach-run`, start_ms NowMs,
   has_end false → keep handle + `run rid` on a wire.

Flat asset attach:
5. `nominal asset add dataset.vi` — client, asset RID, scope_name
   `flight-data`, dataset RID → NEW asset handle (the old one is a stale
   snapshot). Verify on the new handle:
   - `asset data source count` = 1
   - `asset data source name at` 0 = `flight-data`
   - `asset data source rid at` 0 = the dataset RID
   - `asset data source type at` 0 = 0 (Dataset)

Staged attach with series tags (tags filter which series from the dataset
are included in the scope — there is no getter for them; verify visually in
the app if desired):
6. `nominal asset attach dataset begin.vi` — scope_name
   `flight-data-tagged`, dataset RID → staging.
7. `nominal asset attach dataset add tag.vi` — key `vehicle`, value
   `test-rig`. Then `nominal asset attach dataset commit.vi` — client,
   asset RID, staging → NEW asset handle; `asset data source count` = 2.
   `nominal asset attach dataset free.vi` (commit does NOT free).

Duplicate scope name is a server error, not a crash:
8. Re-run step 5 exactly (same scope_name `flight-data`) → expect error 8
   (ApiError) in the error cluster; Clear Errors → `nominal last error.vi`
   → a conflict message naming the scope.

Run attach:
9. `nominal run add dataset.vi` — client, run RID, ref_name `flight-data`,
   dataset RID → NEW run handle. Verify: `run data source count` = 1,
   `run data source name at` 0 = `flight-data`, `run data source rid at` 0
   = the dataset RID, `run data source type at` 0 = 0.
   (`nominal asset add video.vi` / `add connection.vi` and the run
   equivalents are the same shape — covered by the automated suite; spot
   check them here only if you have a real video/connection RID handy.)

Cleanup:
10. `nominal run archive.vi`, `nominal asset archive.vi`,
    `nominal dataset archive.vi` — client + each RID.
11. Free every handle: both extra asset handles from steps 5/7, the run
    handle from step 9, the originals from setup, then
    `nominal client free.vi`.

## Test 13 — MCAP ingest (NOT YET RUN)

Same job flow as Test 8, new staging surface. Requires a real `.mcap` file
containing protobuf timeseries topics (any small robotics log works — note
one or two of its topic names before starting). journald JSON
(`nominal ingest journal json.vi`), Avro-stream
(`nominal ingest avro stream.vi`), and DataFlash
(`nominal ingest dataflash begin/add file tag/free.vi` +
`nominal ingest dataflash.vi`) reuse this exact job flow with smaller option
surfaces — the automated suite covers their wire shapes; spot check them in
LabVIEW only if you have real files of those formats handy.

1. `nominal client new.vi` — real token.
2. `nominal dataset create.vi` — name `labview-ffi-mcap-test` → RID on a
   wire.
3. `nominal ingest mcap begin.vi` → staging.
4. `nominal ingest mcap include topic.vi` — one real topic name from your
   file. (Include and exclude are mutually exclusive — staging both makes
   the ingest call return error 5 before uploading anything.)
5. Optional: `nominal ingest mcap add file tag.vi` — tag `source`, value
   `labview`; `nominal ingest mcap set ignore invalid topics.vi` — true.
6. `nominal ingest mcap.vi` — client, staging, the MCAP path, the dataset
   RID → job handle. Blocks for the upload (MCAPs are bigger than test
   CSVs — expect seconds to minutes).
7. `nominal ingest job wait.vi` — job RID, poll_interval_ms 0 → status 3
   (Completed). MCAP processing takes longer server-side than CSV.
8. Verify: `nominal channel list.vi` with the dataset RID → channels from
   the included topic only (the filter is the point of this test).
9. Cleanup: free job handles + staging (`nominal ingest mcap free.vi`),
   dataset handle; `nominal client free.vi`.

## Test 14 — Video ingest (NOT YET RUN)

The video upload lands in a VIDEO resource, not a dataset — the payoff is a
playable clip in the app. Any short `.mp4` on disk works.

1. `nominal client new.vi` — real token.
2. `nominal video create.vi` — name `labview-ffi-video-ingest` → video RID
   on a wire (keep the handle).
3. `nominal ingest video.vi` — client, the mp4 path, the video RID,
   start_ms = NowMs (or any recent timestamp — it anchors the first frame
   on the timeline) → job handle. Blocks for the upload.
4. On the job handle: `ingest job result rid` — is_present true, equals the
   video RID (video jobs report the VIDEO rid here, not a dataset).
5. `nominal ingest job wait.vi` — poll to status 3 (Completed).
6. Payoff: `nominal video url.vi` on a fresh `nominal video get.vi` handle
   → open in the browser → the clip plays, positioned at start_ms.
7. (`nominal ingest video mcap.vi` is the same shape with a topic string
   instead of start_ms — needs an MCAP with a video stream; skip unless you
   have one.)
8. Cleanup: free job + both video handles; archive the video only after
   you've looked at it. `nominal client free.vi`.
