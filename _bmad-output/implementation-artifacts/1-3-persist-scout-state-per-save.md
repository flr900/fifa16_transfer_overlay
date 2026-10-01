---
baseline_commit: 7ddff7d
---

# Story 1.3: Persist Scout state per save

Status: done

## Story

As Felipe,
I want the Scout's data to be saved per career,
so that my Olheiros and preferences survive closing the game or a crash.

## Acceptance Criteria

1. **Given** a career is loaded, **when** the Scout state is first needed, **then** a JSON file is created at `%LOCALAPPDATA%\FifaCompanion\scout\<hash>.json`, where `<hash>` is the Story 1.1 save hash, **and** the file carries the `Olheiro`/`Missao`/`Relatorio` collections (empty at first) and a `ui_prefs` section, with `Uuid` v4 ids as canonical strings and dates as plain `YYYYMMDD` integers (AD-12).
2. **Given** any state mutation, whether domain data or a `ui_prefs` value such as the active tab, **when** it is applied, **then** it goes through the single `Arc<Mutex<ScoutStateFile>>` in `scout::persistence`: lock, update, serialize, write the whole file, unlock (AD-7), **and** only `scout::state` calls `persistence` (AD-1).
3. **Given** the active tab was Sonar when the panel closed, **when** the game is restarted and the panel reopened for the same career, **then** it opens on the Sonar tab.
4. **Given** the state file is missing, empty or corrupt, **when** the state loads, **then** the Scout starts with empty state and logs `[scout::persistence]` via `tracing::warn!`, **and** it does not overwrite a corrupt file without first keeping a backup copy.
5. **Given** two different careers, **when** each is loaded, **then** each reads and writes only its own file.

## Tasks / Subtasks

- [x] Task 1: Dependencies (AC #1)
  - [x] 1.1 `serde` 1.0 (derive), `serde_json` 1.0, `uuid` 1.26 (v4, serde), `dirs` 7.0 — resolved 1.0.229 / 1.0.151 / 1.26.1 / 7.0.0, matching the Architecture Stack table.
  - [x] 1.2 `save_repo::Date` gets `Serialize`/`Deserialize` with `#[serde(transparent)]` (planned in Story 1.1 Task 3.2).
- [x] Task 2: `scout::persistence` (AC #1, #2, #4, #5)
  - [x] 2.1 `ScoutStateFile { versao, olheiros, missoes, relatorios, ui_prefs }`; every field has a serde default so a file from an older format stays valid.
  - [x] 2.2 `EstadoPersistido` = `Arc<Mutex<ScoutStateFile>>` + file path; clones share the mutex (for the Epic 2 background Missão tasks). `mutar` applies the change to a copy, writes the whole file (`.tmp` + rename), and only then swaps the in-memory state, so memory and disk never diverge.
  - [x] 2.3 Load: missing → create empty file; empty → recreate; corrupt → rename to `<hash>.json.corrompido-<ms>` then create empty; unreadable (not NotFound) or backup failed → empty, read-only for the session (never overwrite what could not be kept); `versao` newer than the DLL → read, never written. Every case logs `[scout::persistence]` with `warn!`.
  - [x] 2.4 Invalid `ui_prefs` (e.g. unknown tab name) resets only `ui_prefs`, keeping the domain data.
  - [x] 2.5 The id becomes a file name only if it is 1–64 lowercase hex chars (no path traversal).
- [x] Task 3: Entities in `scout::state` (AC #1)
  - [x] 3.1 `Olheiro { id }`, `Missao { id, olheiro_id, status, criada_em, prazo_estimado }`, `StatusMissao { Pendente, EmExecucao, Concluida }`, `Relatorio { id, missao_id }` — only what the ERD/AD-8 already fix. Especialização/Tier (1.4/1.5), filters/Modo de Busca/Qualidade (Epic 2) are added later with `#[serde(default)]`.
- [x] Task 4: Wiring (AC #2, #3, #5)
  - [x] 4.1 `CareerSnapshot.id_save` = `CareerIdentity::hash()` (same read that fills the header; no extra memory access).
  - [x] 4.2 `ScoutState`: a map `hash → EstadoPersistido` (one mutex per file for the whole session); `save_ativo` follows `status` through a single `definir_status`. Mutations are accepted only while the career is `Pronta`.
  - [x] 4.3 When a career becomes active, its saved tab is handed once to `Scout` (`tomar_aba_restaurada`), which moves the navigation there. Tab clicks go through `ScoutState::definir_aba_ativa` (no write if the tab did not change).
  - [x] 4.4 `BUILD_TAG` = `1.3-v1 — estado do Scout por save`.
- [x] Task 5: Tests (`cargo test`: 70 passed)
  - [x] 5.1 `persistence`: file created with exact JSON shape; mutation + reload; clones share the mutex across threads; AD-12 ids/dates; corrupt file backed up byte-for-byte; empty file; invalid `ui_prefs`; newer format never overwritten; two careers, two files; no data dir / bad id → read-only; data dir created on demand.
  - [x] 5.2 `state`: file created on first ready read and tab restored after a "restart" (including the ~15 s locate path); two careers A → B → A; tab changes without a career are not saved; no data dir still works.
  - [x] 5.3 `scout`: a new `Scout` (game restarted) opens on the tab saved for the career.
- [ ] Task 6: Manual check in game (Felipe)
  - [x] 6.1 Open a career, F10, switch to Sonar, close the panel; check `%LOCALAPPDATA%\FifaCompanion\scout\<hash>.json` exists with `"aba_ativa": "sonar"` (first 8 hex chars in the log line `[scout::state] Carreira ativa: estado xxxxxxxx…`).
  - [x] 6.2 Restart FIFA, load the same career, F10: after "Localizando carreira…" the panel lands on Sonar.
  - [ ] 6.3 Load a different career: a second file appears, starting on Olheiros.
  - [ ] 6.4 Corrupt the file by hand (e.g. delete the last `}`), reload the career: a `.corrompido-<ms>` copy appears and the Scout starts empty, with `warn` lines in `%TEMP%\fifa_overlay.log`.

## Dev Notes

### Decisions taken during implementation
- **Tab clicked before the career is ready is not saved.** During the ~15 s locate the panel shows the empty state; when the career becomes ready the navigation jumps to the career's saved tab. Clicks while the panel shows "Nenhuma carreira carregada" are only visual (there is no file to write to).
- **One `EstadoPersistido` per hash for the whole session**, kept even when the panel shows no career (menu). Reloading from disk would create a second mutex for the same file and break AD-7 once Epic 2 background tasks hold a clone.
- **Write-through on the render thread**: a tab click writes a few hundred bytes (`.tmp` + rename, no `fsync`). AD-4 is about memory scans; this cost is negligible. `fsync` was left out on purpose: the risk is the game process dying, not the machine losing power.
- **Format version** (`"versao": 1`): a file from a newer DLL is read but never overwritten (writing would drop fields this DLL does not know).
- **Elevated FIFA**: `%LOCALAPPDATA%` resolves through `dirs::data_local_dir()` for the user running the game; with UAC elevation of the same account this is Felipe's normal profile.

### Architecture compliance
- AD-1: only `scout::state` calls `scout::persistence` (screens call `ScoutState::definir_aba_ativa`). `persistence` imports the entity types from `state` (types only, no calls).
- AD-7: one `Arc<Mutex<ScoutStateFile>>` per file; lock → apply to a copy → serialize → write whole file → swap → unlock.
- AD-11: file name is the Story 1.1 SHA-256 hex; the hash is never built from raw components here.
- AD-12: `Uuid` canonical strings, `Date` as plain integer (`#[serde(transparent)]`), cross-references by UUID.
- NFR6: no `unwrap()`/`expect()`/`panic!` outside tests; poisoned mutex recovered with `into_inner()`.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.3]
- [Source: ARCHITECTURE-SPINE.md#AD-1, AD-7, AD-11, AD-12, Structural Seed (ERD), Stack]
- [Source: Story 1.1 — `save_repo::read_career_identity`, `CareerIdentity::hash`]
- [Source: Story 1.2 — `scout::state`, `Navigation`, `barra_de_abas`]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01 (Windows, Rust 1.98.1 msvc): `cargo test` → 70 passed, 0 failed; `cargo build --release` with no code warnings; `cargo clippy --all-targets` reports nothing in the new code.

### Completion Notes List

- `scout::persistence` (new): file format, write-through store, load/backup rules, 11 unit tests.
- `scout::state`: entities, per-hash store map, `definir_status`, tab persistence/restoration, 4 new tests.
- `scout::search`: `CareerSnapshot.id_save`.
- `scout::mod`: `Aba` serialized as `"olheiros"`/`"missoes"`/`"relatorios"`/`"sonar"`; `Scout` applies the restored tab after `tick`; 1 new test.
- `scout::screens`: tab bar reports clicks to `ScoutState`.
- `save_repo`: `Date` serde transparent; `hash`/`hash_identity` no longer dead code.
- Manual in-game check 2026-10-01 (build `1.3-v1`): 6.1 ok, file `f0ff73d5….json` created at 14:53:06 and saved with `"aba_ativa": "sonar"`. 6.2 ok per the log, after the restart: panel opened at 14:56:25 on Olheiros while locating; career located at 14:56:43, state loaded, `Restaurando a aba salva da carreira: Sonar`. Felipe saw Olheiros during the ~18 s locate and found that acceptable. Possible later UX tweak: highlight no tab while locating. 6.3/6.4 not run.

### Change Log

- 2026-10-01: Story 1.3 implemented (per-save JSON state, write-through, tab restored per career).

### File List

- fifa_overlay/Cargo.toml (modified: serde, serde_json, uuid, dirs)
- fifa_overlay/Cargo.lock (modified)
- fifa_overlay/src/lib.rs (modified: BUILD_TAG)
- fifa_overlay/src/save_repo.rs (modified: `Date` serde; dead-code attributes)
- fifa_overlay/src/scout/persistence.rs (new)
- fifa_overlay/src/scout/state.rs (modified)
- fifa_overlay/src/scout/search.rs (modified)
- fifa_overlay/src/scout/mod.rs (modified)
- fifa_overlay/src/scout/screens/mod.rs (modified)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
- PROJECT_MEMORY.md (modified: Sessão 8)
