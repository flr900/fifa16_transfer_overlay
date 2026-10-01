---
baseline_commit: b46ea7f9d7e632ebafe1156c7f9f3bd2047f3123
---

# Story 1.1: Read career state and identify the active save

Status: in-progress

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As Felipe,
I want the Companion to read the career date and transfer budget and to identify which career is loaded,
so that every Scout feature works from the real state of my active save.

## Acceptance Criteria

1. **Given** a career is loaded in FIFA 16, **when** `save_repo::read_current_date()` and `read_transfer_budget()` are called, **then** they return `GJUr.currdate` as a `Date` (YYYYMMDD) and `dqXv.transferbudget` as `i32`, both wrapped in `Result<_, SaveRepoError>`, **and** the values match what the game shows for the same career.
2. **Given** a career is loaded, **when** `save_repo::identify_active_save()` runs, **then** it returns `SHA-256(startdate|firstname|surname|clubteamid)` in lowercase hex, built from `GJUr.startdate`, `mPrV.firstname`, `mPrV.surname` and `mPrV.clubteamid` (AD-11), **and** the hash is a valid Windows file name even when the manager name contains accents or reserved characters.
3. **Given** the same career is loaded in two separate game sessions, and a different career is loaded in a third, **when** `identify_active_save()` runs in each session, **then** the two sessions of the same career produce the same hash, **and** the different career produces a different hash, **and** the result is documented: if `GJUr.startdate` is not stable across sessions, the finding and the fallback chosen are recorded before the story is closed.
4. **Given** no career is loaded, or the process or table cannot be reached, **when** any `save_repo` function is called, **then** it returns `CarreiraNaoCarregada`, `ProcessoInacessivel` or `TabelaNaoEncontrada` as appropriate, **and** it never panics (no `unwrap()`/`expect()`, slices accessed via `.get()`; NFR6), **and** no `scout::*` code calls `memscan`, `fifa_db` or `pointer_scan` directly (AD-2).
5. **Given** the game build is not FIFA 16 `16.0.2904053`, **when** `save_repo` initialises, **then** it logs the mismatch and reports `TabelaNaoEncontrada` rather than reading unverified memory (NFR4).
6. **(Added while creating this story — see Dev Notes "Scope changes")** **Given** locating the career database requires a full-memory scan (~17 s), **when** `save_repo` needs it, **then** the scan runs only through a new generic `AsyncTask<T>` (`src/async_task.rs`, AD-4), never on the render thread, **and** once located, every field read (`read_current_date`, `read_transfer_budget`, `identify_active_save`) is a cheap synchronous read of a few bytes at the cached address, re-validated against the DB signature and re-located if the heap moved.

## Tasks / Subtasks

- [ ] Task 1: Spike — resolve field short names and prove which fields are LIVE in the blob (AC: #1, #3) — **do this first, it gates the design**
  - [ ] 1.1 Resolve short names from `fifa_ng_db-meta.xml` (`D:\Program Files\FIFA 16\data\db\fifa_ng_db-meta.xml`; reader: `fifa16_db_parser.py::load_metadata`): tables `GJUr`, `mPrV`, `dqXv` (short names are the 4-char ids, e.g. CZUM playerid = `ykFq`) and fields `currdate`, `startdate`, `firstname`, `surname`, `clubteamid`, `transferbudget`. Record the table→field short-name map in a `const` block (Portuguese `//!` comment saying where each came from).
  - [ ] 1.2 **Freshness test of the blob.** `PROJECT_MEMORY.md` ("blob ... snapshot somente-lido-uma-vez") says the heap copy of the DB is a disconnected snapshot for player attributes. Check whether `GJUr.currdate` and `dqXv.transferbudget` in the blob change after: (a) advancing the career a few days without saving; (b) spending/receiving transfer budget in game (or after the user's write in the session-4 technique). Record results.
  - [ ] 1.3 **If the blob is stale for date and/or budget**, find the live location instead (session-4 technique: exact-value scan for the value shown on screen, e.g. `memscan::scan_for_i32_value`, stability check per the checklist at `PROJECT_MEMORY.md` "Lição geral sobre metodologia"; for date, scan `YYYYMMDD` as i32). Decide and document a strategy (live-scan per call? pointer/anchor? tolerate snapshot + explicit limitation?). Do NOT proceed to Task 3 without a documented decision; if no acceptable strategy exists, stop and report to Felipe (Stories 1.5, 2.2, 2.3 depend on it).
  - [ ] 1.4 Check whether `mPrV.firstname`/`surname` are plain integers or Huffman-coded strings in the blob (`fifa_db.rs` only decodes `storage_type == 3` ints — see Dev Notes). If strings need Huffman decoding, either port the minimal decoder from `fifa16_db_parser.py` or propose a numeric-only identity (e.g. `startdate|clubteamid|<numeric manager id>`) — a deviation from AD-11 that Felipe must approve before it is used.
  - [ ] 1.5 Append findings to `PROJECT_MEMORY.md` (new section "Sessão 6 — Story 1.1: leitura de estado da carreira") and to this story's Completion Notes.
- [ ] Task 2: `async_task.rs` — generic `AsyncTask<T>` (AC: #6)
  - [ ] 2.1 `enum TaskState<T> { Idle, Running, Done(T), Failed(SaveRepoError) }`; `AsyncTask<T>` wraps `Arc<Mutex<TaskState<T>>>` + `Arc<AtomicBool>`; `poll(&self) -> TaskState<T> where T: Clone` is non-destructive and idempotent; constructor spawns a `std::thread` (same pattern as `spawn_scan_thread` in `lib.rs:73`). `Failed` is a sibling of `Done`, never nested in `T` (AD-4).
  - [ ] 2.2 A second `start` while `Running` must be a no-op (guard via the `AtomicBool`).
  - [ ] 2.3 Unit tests with a fake closure: Idle→Running→Done, Failed path, `poll()` called twice returns the same `Done`, double-start ignored, poisoned mutex does not panic.
- [ ] Task 3: `save_repo.rs` (AC: #1, #2, #4, #5, #6)
  - [ ] 3.1 `SaveRepoError` enum with at least `TabelaNaoEncontrada`, `ProcessoInacessivel`, `CarreiraNaoCarregada` (derive `Debug, Clone, PartialEq`; `impl Display` in Portuguese user-safe text; no extra crate).
  - [ ] 3.2 `Date(pub i32)` newtype (`#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]`), YYYYMMDD raw as read from `GJUr.currdate`. Serde derive comes in Story 1.3 (`#[serde(transparent)]`) — do not add serde here.
  - [ ] 3.3 Locate step (runs in an `AsyncTask`): `memscan::find_databases_in_memory()` returns several blobs (~9 per PROJECT_MEMORY); choose the one that (a) has `GJUr` with `written_record_count >= 1` and (b) `CZUM` with a plausible record count (full DB ≈ 32k records, 38 tables). Cache `region_base + offset_in_region` and the parsed `TableDescriptor`s (not the 64 MB region bytes). No matching DB → `CarreiraNaoCarregada`.
  - [ ] 3.4 Field reads: compute the absolute address with `fifa_db::locate_packed_field` (+ `region_base`), read only `byte_count` bytes via `memscan::read_region_bytes(&Region{..})`, decode with `fifa_db::read_packed_int`, apply the metadata `range_low` offset where the field has one (see the `+ 1` for strength/overall in `lib.rs:113-121`; `currdate` is raw YYYYMMDD per `fifa16_search.py:decode_yyyymmdd`). Before each read re-check the 8-byte `DB\0\x08...` signature at the cached start; on mismatch invalidate the cache and return a recoverable error so the caller can re-locate.
  - [ ] 3.5 Public API: `read_current_date() -> Result<Date, SaveRepoError>`, `read_transfer_budget() -> Result<i32, SaveRepoError>`, `identify_active_save() -> Result<String, SaveRepoError>`, plus the `AsyncTask` entry point that primes the cache (e.g. `locate_career() -> AsyncTask<()>`). Leave `write_transfer_budget`, `read_squad_players`, `read_all_players` to later stories (1.5, 3.2/2.4).
  - [ ] 3.6 Build check (AC #5): verify the FIFA build is `16.0.2904053` (e.g. `GetFileVersionInfo` on the main module, or module size/PE version); mismatch → log via `tracing::warn!("[save_repo] ...")` and return `TabelaNaoEncontrada`. If version info is not readable, treat as "unverified" and log, but do not block — document the choice.
  - [ ] 3.7 `identify_active_save`: concatenate `startdate|firstname|surname|clubteamid` (or the approved numeric variant from Task 1.4) as UTF-8, SHA-256, lowercase hex. Add `sha2 = "0.10"` (**not in the Architecture Stack table — verify the current version with `cargo add sha2` and record it**) — hex formatting by hand, no `hex` crate.
  - [ ] 3.8 All indexing through `.get(a..b)`; all errors via `Result`; logging `tracing::info!/warn!("[save_repo] ...")`; `//!` module comment in Portuguese explaining the why (incl. AD-2 "única porta para memscan/fifa_db").
- [ ] Task 4: Wire into `lib.rs` for manual verification (AC: #1–#3, #6)
  - [ ] 4.1 Add `mod async_task; mod save_repo;`. In the existing debug window add a small section "Carreira (save_repo)": a button "Localizar carreira" that starts the `AsyncTask`, shows Localizando…/erro, and, once located, shows currdate, transferbudget and the save hash each frame-cheaply (or on button "Reler"). This is scaffolding for verification; Story 1.2 replaces it with the real panel. Do not remove the existing test sections.
  - [ ] 4.2 Never call a `save_repo` function that scans from inside `render()` (see `lib.rs` module comment lines 8–13).
- [ ] Task 5: Tests and manual verification (AC: #1–#6)
  - [ ] 5.1 Unit tests (pure logic, no process access): SHA-256 hex of a known vector; hash of a name with accents and reserved chars contains only `[0-9a-f]{64}`; `Date` ordering; packed-int decoding with a synthetic buffer (mirror `fifa16_db_parser.read_packed_int` cases, including a field not byte-aligned); error mapping (no DB found → `CarreiraNaoCarregada`).
  - [ ] 5.2 Optional integration test marked `#[ignore]` that parses a real `DATA` file from `save_backups/` and checks `GJUr`/`dqXv`/`mPrV` reads and the hash (gives a repeatable oracle without the game).
  - [ ] 5.3 Manual, on Windows with the game: (a) values in the debug window equal the game screens (date: career calendar; budget: Transferências screen); (b) AC #3 protocol — career A session 1, restart the game and reload career A (same hash), load career B (different hash); (c) menu with no career loaded → `CarreiraNaoCarregada`; (d) FPS stays smooth while the locate runs. Record every result in Completion Notes.

## Dev Notes

### What exists today (read before touching anything)
- Crate: `fifa_overlay/` (`cdylib`, Rust 2021). Files: `src/lib.rs` (598 lines, hudhook DX11 `ImguiRenderLoop`, a proof-of-concept window with scan/value-scan/pointer-scan/write-test sections), `src/memscan.rs`, `src/fifa_db.rs`, `src/pointer_scan.rs`. Deps: hudhook 0.9 (dx11), imgui 0.12, tracing(+subscriber/appender), memchr 2.7, windows 0.62 with features Foundation/Memory/Threading/Diagnostics_Debug/ProcessStatus. **`save_repo` and `async_task` do not exist yet.**
- `memscan.rs`: `find_databases_in_memory() -> Vec<DbLocation>` (full scan ≈ 17 s; each `DbLocation` clones its whole region bytes), `read_region_bytes(&Region) -> Option<Vec<u8>>` (protected `ReadProcessMemory` on the current process; works for any small range), `write_bytes_at`, `scan_for_i32_value`, `filter_addresses_by_i32_value`. **Never dereference raw pointers** — the game frees pages concurrently (crash history in `PROJECT_MEMORY.md` "Bug 2").
- `fifa_db.rs`: `parse_database_tables`, `TableDescriptor`/`FieldDescriptor` (short names are 4 bytes), `read_packed_int`, `locate_packed_field`, `build_packed_bytes`, `field_by_shortname`, `find_player_record_index`/`read_field_for_record` (CZUM-oriented but generic over any table). Only integer fields (`storage_type == 3` in the existing usage). No metadata XML, no Huffman strings. Note `data.get(pos..pos + 4).map(|s| ...try_into().unwrap())` already exists here — do not copy that `unwrap()` into new code; new code uses `?`/`Option`.
- The existing proof-of-concept already reads `CZUM` fields from the blob and they matched the game (strength=43, overall=82, potential=81 for playerid 74449). **That proves the blob is correct at load time, not that it tracks live changes** — see the freshness risk below.

### Critical risk: the heap DB blob may be a stale snapshot
`PROJECT_MEMORY.md` ("Nova descoberta: a database completa de jogadores (CZUM) existe como blob no heap, mas não é a fonte viva") shows writes to player attributes in the blob never reach the game; it is probably read once at career load. The only *proven live* location is the `dqXv.transferbudget` int32 found by an exact-value scan (session 4: one occurrence, heap address changes each session, edit became visible after a screen change and persisted in the saved DATA). Implications for this story:
- `GJUr.currdate` read from the blob may not advance as the career advances → Missão progress (Story 2.3) and the AD-8 deadline check would be wrong. Test it explicitly (Task 1.2).
- `dqXv.transferbudget` read from the blob may show the value at load, not the current one; the session-4 find-by-value approach is circular for reading (you need the value to find it) unless the blob gives a close-enough seed or a stable anchor/pointer is found. Stories 1.5/2.2 write the budget and must read it back — the strategy chosen here defines them.
- Static fields (`startdate`, manager, `clubteamid`) are fine from the blob.
Whatever Task 1 concludes, update `ARCHITECTURE-SPINE.md` AD-2 notes (and tell Felipe) if the `save_repo` contract (`read_current_date`, `read_transfer_budget`, `write_transfer_budget`) needs to change.

### Scope changes made while creating this story
1. **`AsyncTask<T>` moved from Story 2.4 into Story 1.1.** The epics placed it in 2.4, but locating the career database needs the ~17 s scan from the very first panel (Story 1.2), and AD-4 forbids ad-hoc threads for full scans. `epics.md` (Story 1.1 AC and Story 2.4 first AC) and Story 1.2 (panel shows "Localizando carreira…" while locating) were updated accordingly.
2. **New dependency `sha2`** is required by AD-11 but missing from the Architecture Stack table — add it to the Stack when implementing.
3. **Possible AD-11 deviation** if manager names are Huffman strings (Task 1.4).

### Architecture compliance (non-negotiable)
- AD-2: `save_repo.rs` is the only module that may call `memscan`/`fifa_db`/`pointer_scan`. Screens and `scout::*` never do. (Until Story 1.2 the debug section in `lib.rs` may call `save_repo` only.)
- AD-4: `AsyncTask<T>` with `TaskState { Idle, Running, Done(T), Failed(SaveRepoError) }`; `poll()` non-destructive; single-field reads synchronous; any full-`CZUM`/full-memory scan goes through an `AsyncTask`.
- AD-5: every public `save_repo` function returns `Result<T, SaveRepoError>`; variants at least `TabelaNaoEncontrada`, `ProcessoInacessivel`, `CarreiraNaoCarregada`.
- AD-11: file-name hash = `SHA-256(startdate + "|" + firstname + "|" + surname + "|" + clubteamid)`, lowercase hex, components read from process memory (not from file `mtime` like the Python side). Validating the stability of `GJUr.startdate` is this story's first technical task.
- AD-12 / Consistency: dates are a `Date(i32)` newtype over YYYYMMDD; no `panic!/unwrap()/expect()`; slices via `.get(a..b)`; logging `tracing::info!/warn!` with tag `"[save_repo] ..."`; `//!` module comments in Portuguese explaining why; `snake_case` files/functions, `PascalCase` types; `_state` / `_in_progress` naming for async state already used in `lib.rs`.
- Layering: `scout::*` does not exist yet; do not create it in this story.

### Library / framework requirements
- Keep: hudhook 0.9 (dx11), imgui 0.12, windows 0.62, memchr 2.7, tracing family.
- Add: `sha2` (RustCrypto) 0.10.x — check latest with `cargo add`; no `hex` crate.
- Possibly add the Windows feature `Win32_Storage_FileSystem` if the build check uses `GetFileVersionInfoW` (also needs `Win32_System_LibraryLoader` if querying the module path) — choose the simplest reliable route and note it.
- Do NOT add `serde`, `serde_json`, `uuid`, `dirs`, `Win32_UI_Input_KeyboardAndMouse` here (Stories 1.2/1.3).

### File structure requirements
```
fifa_overlay/
  Cargo.toml            # + sha2 (and any windows feature you need)
  src/
    lib.rs              # UPDATE: mod async_task; mod save_repo; debug section
    async_task.rs       # NEW
    save_repo.rs        # NEW
    memscan.rs / fifa_db.rs / pointer_scan.rs   # keep unchanged unless a helper is strictly needed
PROJECT_MEMORY.md       # UPDATE: Sessão 6 findings
```
If a tiny helper in `fifa_db.rs` (e.g. a signature re-check or a metadata `range_low` constant) is needed, add it without changing existing behaviour; existing functions are used by the PoC window.

### Testing requirements
- `cargo test` in `fifa_overlay/` (Windows; the crate depends on `windows`/hudhook). Unit tests are pure (no game). The game-connected checks are manual and must be recorded. No test framework setup story exists; use plain `#[cfg(test)] mod tests`.
- Testing the AC #3 stability needs three game sessions (same career twice + another career); write down the observed hashes (first 8 hex chars are enough) and the `startdate` values.

### Project Structure Notes
- Brownfield: there is no `project-context.md`; `PROJECT_MEMORY.md` is the authoritative history and must be updated with Task 1 findings.
- Build/deploy gotchas from `PROJECT_MEMORY.md` ("Bugs de infraestrutura"): an old locked `.dll` can make `cargo build --release` silently keep an outdated binary — rename the old DLL and verify a new string is present in the output before debugging "strange behaviour"; `hudhook::eject()` is unreliable, restart the game when the overlay misbehaves; keep `MutexGuard`s in their own block so `&mut self` calls after them compile.
- The Python side (`fifa16_search.py::identify_saves`) identifies saves by file `mtime` — do not reuse that approach here.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.1]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-FIFA_EDITOR-2026-09-22/ARCHITECTURE-SPINE.md#AD-2, AD-4, AD-5, AD-11, AD-12, Consistency Conventions, Stack]
- [Source: _bmad-output/planning-artifacts/prds/prd-FIFA_EDITOR-2026-09-21/prd.md#FR-3, §8 Questões em Aberto #1, #6]
- [Source: PROJECT_MEMORY.md#Nova descoberta: a database completa de jogadores (CZUM) existe como blob no heap, mas não é a fonte "viva"]
- [Source: PROJECT_MEMORY.md#Orçamento do clube (transferbudget) — SUCESSO CONFIRMADO, técnica reutilizável]
- [Source: PROJECT_MEMORY.md#Leitura de memória in-process — SUCESSO CONFIRMADO; Bugs de infraestrutura encontrados nesta sessão]
- [Source: fifa_overlay/src/lib.rs:73 (spawn_scan_thread pattern), :113-121 (range_low +1), :8-13 (never scan in render)]
- [Source: fifa_overlay/src/memscan.rs (read_region_bytes, find_databases_in_memory), fifa_overlay/src/fifa_db.rs (locate_packed_field, read_packed_int)]
- [Source: fifa16_search.py:127,148,257-290 (currdate, mPrV manager, identify_saves)]

## Dev Agent Record

### Agent Model Used

claude-sonnet-5-5

### Debug Log References

- No Rust toolchain on the dev Mac (Darwin arm64): **nothing below has been compiled or run.** The crate is Windows-only (hudhook DX11 + Win32 APIs).

### Completion Notes List

**Code written, UNVERIFIED (no task is checked on purpose):**
- Task 2 (`src/async_task.rs`): `AsyncTask<T>`/`TaskState<T>` per AD-4, panic-safe, with 6 unit tests. Pure logic; should compile and pass on Windows.
- Task 3 (`src/save_repo.rs`): `SaveRepoError` (+ `NaoLocalizado`, `Interno`), `Date`, locate via `AsyncTask`, signature re-validation, `read_current_date`, `read_transfer_budget`, `identify_active_save`, `hash_identity` (SHA-256), build check, 10 unit tests.
  - **Field short names are `????` placeholders** (`save_repo::fields`). Reads return `TabelaNaoEncontrada` until filled. Run `python tools/resolve_short_names.py` on the Windows machine and paste its output. The test `all_field_short_names_are_resolved` is deliberately red until then.
  - `EXPECTED_FILE_VERSION` is `None`: the build check only logs the observed version (AC #5 is not enforced yet). Fill it from the log, then mismatches are refused.
  - `read_main_module_file_version` uses `GetFileVersionInfo*`/`VerQueryValueW` from memory of the windows 0.62 API; the most likely place for a compile error (e.g. the `dwhandle` parameter type).
  - `sha2 = "0.10"` was added without checking the current release; run `cargo add sha2` / `cargo update` and record the version.
- Task 4 (`src/lib.rs`): "Carreira (save_repo)" section in the debug window: locate button, status, "Reler valores" showing date, budget and hash.
- `memscan::DB_SIGNATURE` made `pub` (only change to existing modules besides `lib.rs` wiring).
- `Cargo.toml`: added `sha2`, windows features `Win32_System_LibraryLoader`, `Win32_Storage_FileSystem`.
- New helper `tools/resolve_short_names.py` prints the field consts and all `mPrV` fields.

**Still to do on Windows with the game (Task 1 spike gates everything else):**
1. Task 1.1: run the script, paste consts.
2. Task 1.2/1.3: freshness test of `currdate` and `transferbudget` in the blob; decide the read strategy (not designed yet; `save_repo` currently assumes the blob is live).
3. Task 1.4: check whether manager names are ints or Huffman strings (`read_raw` fails with an explicit message if not int).
4. Task 1.5: write findings into `PROJECT_MEMORY.md`.
5. Task 5.3 manual checks (AC #1, #3, #4, #6) and `cargo test`.

### Change Log

- 2026-09-30: Added `async_task.rs`, `save_repo.rs`, lib.rs debug wiring, Cargo deps, `tools/resolve_short_names.py` (unverified, see notes).

### File List

- fifa_overlay/src/async_task.rs (new)
- fifa_overlay/src/save_repo.rs (new)
- fifa_overlay/src/lib.rs (modified)
- fifa_overlay/src/memscan.rs (modified: `DB_SIGNATURE` pub)
- fifa_overlay/Cargo.toml (modified)
- tools/resolve_short_names.py (new)
- _bmad-output/planning-artifacts/epics.md (modified: Story 1.1/1.2/2.4 ACs)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
