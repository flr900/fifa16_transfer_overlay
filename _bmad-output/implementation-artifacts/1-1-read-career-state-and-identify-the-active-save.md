---
baseline_commit: b46ea7f9d7e632ebafe1156c7f9f3bd2047f3123
---

# Story 1.1: Read career state and identify the active save

Status: done

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

- [x] Task 1: Spike — resolve field short names and prove which fields are LIVE in the blob (AC: #1, #3) — **do this first, it gates the design**
  - [x] 1.1 Resolve short names from `fifa_ng_db-meta.xml` (`D:\Program Files\FIFA 16\data\db\fifa_ng_db-meta.xml`; reader: `fifa16_db_parser.py::load_metadata`): tables `GJUr`, `mPrV`, `dqXv` (short names are the 4-char ids, e.g. CZUM playerid = `ykFq`) and fields `currdate`, `startdate`, `firstname`, `surname`, `clubteamid`, `transferbudget`. Record the table→field short-name map in a `const` block (Portuguese `//!` comment saying where each came from).
  - [x] 1.2 **Freshness test of the blob.** `PROJECT_MEMORY.md` ("blob ... snapshot somente-lido-uma-vez") says the heap copy of the DB is a disconnected snapshot for player attributes. Check whether `GJUr.currdate` and `dqXv.transferbudget` in the blob change after: (a) advancing the career a few days without saving; (b) spending/receiving transfer budget in game (or after the user's write in the session-4 technique). Record results.
  - [x] 1.3 **If the blob is stale for date and/or budget**, find the live location instead (session-4 technique: exact-value scan for the value shown on screen, e.g. `memscan::scan_for_i32_value`, stability check per the checklist at `PROJECT_MEMORY.md` "Lição geral sobre metodologia"; for date, scan `YYYYMMDD` as i32). Decide and document a strategy (live-scan per call? pointer/anchor? tolerate snapshot + explicit limitation?). Do NOT proceed to Task 3 without a documented decision; if no acceptable strategy exists, stop and report to Felipe (Stories 1.5, 2.2, 2.3 depend on it).
  - [x] 1.4 Check whether `mPrV.firstname`/`surname` are plain integers or Huffman-coded strings in the blob (`fifa_db.rs` only decodes `storage_type == 3` ints — see Dev Notes). If strings need Huffman decoding, either port the minimal decoder from `fifa16_db_parser.py` or propose a numeric-only identity (e.g. `startdate|clubteamid|<numeric manager id>`) — a deviation from AD-11 that Felipe must approve before it is used.
  - [x] 1.5 Append findings to `PROJECT_MEMORY.md` (new section "Sessão 6 — Story 1.1: leitura de estado da carreira") and to this story's Completion Notes.
- [x] Task 2: `async_task.rs` — generic `AsyncTask<T>` (AC: #6)
  - [x] 2.1 `enum TaskState<T> { Idle, Running, Done(T), Failed(SaveRepoError) }`; `AsyncTask<T>` wraps `Arc<Mutex<TaskState<T>>>` + `Arc<AtomicBool>`; `poll(&self) -> TaskState<T> where T: Clone` is non-destructive and idempotent; constructor spawns a `std::thread` (same pattern as `spawn_scan_thread` in `lib.rs:73`). `Failed` is a sibling of `Done`, never nested in `T` (AD-4).
  - [x] 2.2 A second `start` while `Running` must be a no-op (guard via the `AtomicBool`).
  - [x] 2.3 Unit tests with a fake closure: Idle→Running→Done, Failed path, `poll()` called twice returns the same `Done`, double-start ignored, poisoned mutex does not panic.
- [x] Task 3: `save_repo.rs` (AC: #1, #2, #4, #5, #6)
  - [x] 3.1 `SaveRepoError` enum with at least `TabelaNaoEncontrada`, `ProcessoInacessivel`, `CarreiraNaoCarregada` (derive `Debug, Clone, PartialEq`; `impl Display` in Portuguese user-safe text; no extra crate).
  - [x] 3.2 `Date(pub i32)` newtype (`#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]`), YYYYMMDD raw as read from `GJUr.currdate`. Serde derive comes in Story 1.3 (`#[serde(transparent)]`) — do not add serde here.
  - [x] 3.3 **(SUBSTITUÍDO — ver Completion Notes: o blob do heap não é fonte viva; a localização agora é por assinatura de temporada)** Locate step (runs in an `AsyncTask`): `memscan::find_databases_in_memory()` returns several blobs (~9 per PROJECT_MEMORY); choose the one that (a) has `GJUr` with `written_record_count >= 1` and (b) `CZUM` with a plausible record count (full DB ≈ 32k records, 38 tables). Cache `region_base + offset_in_region` and the parsed `TableDescriptor`s (not the 64 MB region bytes). No matching DB → `CarreiraNaoCarregada`.
  - [x] 3.4 **(vale para a leitura dos saves no disco; os valores vivos vêm da struct de finanças)** Field reads: compute the absolute address with `fifa_db::locate_packed_field` (+ `region_base`), read only `byte_count` bytes via `memscan::read_region_bytes(&Region{..})`, decode with `fifa_db::read_packed_int`, apply the metadata `range_low` offset where the field has one (see the `+ 1` for strength/overall in `lib.rs:113-121`; `currdate` is raw YYYYMMDD per `fifa16_search.py:decode_yyyymmdd`). Before each read re-check the 8-byte `DB\0\x08...` signature at the cached start; on mismatch invalidate the cache and return a recoverable error so the caller can re-locate.
  - [x] 3.5 Public API: `read_current_date() -> Result<Date, SaveRepoError>`, `read_transfer_budget() -> Result<i32, SaveRepoError>`, `identify_active_save() -> Result<String, SaveRepoError>`, plus the `AsyncTask` entry point that primes the cache (e.g. `locate_career() -> AsyncTask<()>`). Leave `write_transfer_budget`, `read_squad_players`, `read_all_players` to later stories (1.5, 3.2/2.4).
  - [x] 3.6 Build check (AC #5): verify the FIFA build is `16.0.2904053` (e.g. `GetFileVersionInfo` on the main module, or module size/PE version); mismatch → log via `tracing::warn!("[save_repo] ...")` and return `TabelaNaoEncontrada`. If version info is not readable, treat as "unverified" and log, but do not block — document the choice.
  - [x] 3.7 `identify_active_save`: concatenate `startdate|firstname|surname|clubteamid` (or the approved numeric variant from Task 1.4) as UTF-8, SHA-256, lowercase hex. Add `sha2 = "0.10"` (**not in the Architecture Stack table — verify the current version with `cargo add sha2` and record it**) — hex formatting by hand, no `hex` crate.
  - [x] 3.8 All indexing through `.get(a..b)`; all errors via `Result`; logging `tracing::info!/warn!("[save_repo] ...")`; `//!` module comment in Portuguese explaining the why (incl. AD-2 "única porta para memscan/fifa_db").
- [x] Task 4: Wire into `lib.rs` for manual verification (AC: #1–#3, #6)
  - [x] 4.1 Add `mod async_task; mod save_repo;`. In the existing debug window add a small section "Carreira (save_repo)": a button "Localizar carreira" that starts the `AsyncTask`, shows Localizando…/erro, and, once located, shows currdate, transferbudget and the save hash each frame-cheaply (or on button "Reler"). This is scaffolding for verification; Story 1.2 replaces it with the real panel. Do not remove the existing test sections.
  - [x] 4.2 Never call a `save_repo` function that scans from inside `render()` (see `lib.rs` module comment lines 8–13).
- [x] Task 5: Tests and manual verification (AC: #1–#6)
  - [x] 5.1 Unit tests (pure logic, no process access): SHA-256 hex of a known vector; hash of a name with accents and reserved chars contains only `[0-9a-f]{64}`; `Date` ordering; packed-int decoding with a synthetic buffer (mirror `fifa16_db_parser.read_packed_int` cases, including a field not byte-aligned); error mapping (no DB found → `CarreiraNaoCarregada`).
  - [x] 5.2 Optional integration test marked `#[ignore]` that parses a real `DATA` file from `save_backups/` and checks `GJUr`/`dqXv`/`mPrV` reads and the hash (gives a repeatable oracle without the game).
  - [x] 5.3 **(a)–(d) feitos — ver Completion Notes** Manual, on Windows with the game: (a) values in the debug window equal the game screens (date: career calendar; budget: Transferências screen); (b) AC #3 protocol — career A session 1, restart the game and reload career A (same hash), load career B (different hash); (c) menu with no career loaded → `CarreiraNaoCarregada`; (d) FPS stays smooth while the locate runs. Record every result in Completion Notes.

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

claude-sonnet-5-5 (rascunho no Mac); claude-opus-5-5 (verificação no Windows)

### Debug Log References

- 2026-09-30 (Windows, Rust 1.98.1 msvc): `cargo build --release` OK; `cargo test` → 38 passed, 0 failed (build final `s6-v10`).
- Log do overlay: `%TEMP%\fifa_overlay.log` (testes com o jogo: builds s6 → s6-v9, ver `PROJECT_MEMORY.md` "Sessão 6").
- `python tools/resolve_short_names.py` → short names abaixo.
- Oráculo: `fifa16_db_parser.py` sobre `save_backups/*/DATA` e os saves atuais em `Documents\FIFA 16`.

### Completion Notes List

**Verificado offline nesta sessão (Windows, sem o jogo aberto):**
- Task 1.1: short names resolvidos e colados em `save_repo::fields` — `GJUr.currdate=aLZZ`, `GJUr.startdate=vHhZ` (19 bits, `rangelow=20080101`), `mPrV.firstname=HdeP`, `mPrV.surname=rREd`, `mPrV.clubteamid=NTyS` (`rangelow=-1`), `dqXv.transferbudget=SnDr` (31 bits).
- Task 1.4: `firstname`/`surname` são **strings fixas inline** (storage_type 0, 32 bytes, `\0` no fim), não Huffman → AD-11 sem desvio. `save_repo` ganhou `read_string`.
- **Bug corrigido:** `is_career_db` exigia `GJUr` e `CZUM` no mesmo blob, mas no save são databases separadas (carreira: 34 tabelas; jogadores: 38). A localização sempre devolveria `CarreiraNaoCarregada`. Agora exige `GJUr` + `mPrV` + `dqXv`.
- **AC #5 (build):** `FileVersion` fixo do `fifa16.exe` = `1.0.0.0`, `ProductVersion` fixo = `16.0.0.0`; só a string `ProductVersion` traz `16.0.2904053`. Trocado para comparar essa string (`EXPECTED_PRODUCT_VERSION`); testado contra o exe instalado. Divergência → `TabelaNaoEncontrada`; ilegível → só loga (decisão: não bloquear por recurso de versão ilegível).
- Leitura refatorada sobre um `ByteSource` (memória do processo em produção, buffer nos testes): o mesmo código de leitura é testado contra 3 `DATA` reais (data, orçamento, identidade).
- AC #3 offline: `717036e3` e `705c22c4` (mesma carreira, `currdate` 20351102 vs 20351206) → mesmo hash; `7a096416` (`20350721|Felipe|Careca|73`) → hash diferente. `GJUr.startdate` é estável entre saves da mesma carreira.
- `sha2` resolvido 0.10.9; Stack do Architecture Spine atualizado.
- Debug window: mostra também a identidade legível (`startdate|nome|sobrenome|clube`) além do hash.

**Verificado com o jogo aberto (2026-09-30, carreiras "teste" e Felipe Careca):**
- Task 1.2 — **o blob de carreira do heap NÃO é fonte viva**: é o buffer do último load/save (orçamento alterado em jogo não apareceu nele) e some depois de um save. Mesma natureza do blob `CZUM`.
- Task 1.3 — estratégia decidida com o Felipe ("sondar struct viva", opção 1):
  - **Orçamento**: a struct viva de `dqXv` está no heap como i32 contíguos (`wagebudget` −4, `transferbudget` 0, início de temporada +20/+24/+28); achada com a sonda (`save_repo::start_live_probe`) e confirmada pelo Felipe no Cheat Engine. `locate()` lê os saves recentes em `Documents\FIFA 16`, procura o trio de início de temporada na memória e lê orçamento/salário vivos (revalidando a assinatura a cada leitura). A identidade vem do save cuja assinatura está viva (escolhido pela memória, não por `mtime`).
  - **Data**: fica num ponto fixo do bloco de memória do jogo (região + `0x373E08`, reservas +`0x1D2BD4`/+`0x1D3114`), achada por scan de valor ao longo de vários dias (CE e DLL). Validada pela janela da temporada (`enddate` − 366 dias). Logo após carregar o save o campo fica no dia anterior até o primeiro dia ser processado → devolvemos a data do save.
  - **Carreira ativa**: structs de carreiras carregadas antes ficam como restos na memória; vence a struct cuja temporada contém a data viva (teste "teste" → Careca: escolheu Careca corretamente).
  - **Sem carreira (menu)**: a posição principal da data também fica com resto da última carreira, mas as duas listas de eventos zeram → a data viva só vale com 2 das 3 posições concordando; sem carreira confirmada → `CarreiraNaoCarregada` (sem queda para o save mais recente). Toda leitura revalida (menu → "carreira não carregada"; data que volta → cache descartado).
- Bug corrigido: a varredura achava a assinatura na **própria memória da DLL** (lista de saves); agora exclui lista, pilha e um buffer único de leitura, e zera as cópias.
- AC #1: orçamento de transferência, salário e data batem com as telas e acompanham mudanças (Reler sem relocalizar). AC #5: build `16.0.2904053` verificada no jogo. AC #3: "teste" em sessões diferentes → mesma identidade `20260717|Senhor|Manager|243`; Careca `20280715|Felipe|Careca|234` → outra.
- Ferramentas de diagnóstico mantidas na janela de debug: sonda de estado vivo; scan de valor agora loga endereços + vizinhança (≤ 50 candidatos). Log reduzido para INFO.

**Limitações conhecidas:**
- Duas carreiras na MESMA temporada não são distinguíveis pela data → vence o save mais recente.
- Recarregar o MESMO save na mesma data pode deixar o cache no resto da carga anterior → relocalizar resolve (Story 1.2 deve relocalizar ao abrir o painel ou oferecer o botão).
- Se a temporada virar e o jogo não tiver salvo depois, a assinatura do disco não existe na memória → `CarreiraNaoCarregada` até salvar.
- Offsets da data são empíricos (FIFA 16 `16.0.2904053`); plano B documentado: pointer scan do CE.
- Injeção: o eject do hudhook crashou o jogo 1×; a v7 crashou 2× via `fifa_overlay/inject_dev.ps1` e carregou pelo injetor direto (causa não identificada).

**Task 5.3 (com o jogo, s6-v10):** (a) orçamento/salário/data = telas, acompanham sem relocalizar; (b) identidade estável entre sessões, outra carreira → outro hash; (c) menu principal → `CarreiraNaoCarregada`; (d) localização (~15 s em background) não engasga o jogo.

### Change Log

- 2026-09-30: Added `async_task.rs`, `save_repo.rs`, lib.rs debug wiring, Cargo deps, `tools/resolve_short_names.py` (unverified, see notes).
- 2026-09-30 (Windows): compilado e testado; short names preenchidos; strings inline; correção de `is_career_db`; build check via string `ProductVersion`; `ByteSource` + testes de oráculo com saves reais; docs atualizadas.
- 2026-09-30 (com o jogo): blob do heap descartado como fonte viva; sonda de estado vivo; localização por assinatura de temporada (orçamento/salário vivos); exclusão da memória da própria DLL; data viva por offset da região, validada pela temporada; carreira ativa escolhida pela data viva; `inject_dev.ps1`; log em INFO.
- 2026-09-30 (s6-v10): data viva confirmada por 2 das 3 posições; menu → `CarreiraNaoCarregada`; leituras revalidam a carreira a cada chamada. Story → review.

### File List

- fifa_overlay/src/async_task.rs (new)
- fifa_overlay/src/save_repo.rs (new)
- fifa_overlay/src/lib.rs (modified)
- fifa_overlay/src/memscan.rs (modified: `DB_SIGNATURE` e `MAX_REGION_SIZE` pub; novo `read_region_into`)
- fifa_overlay/inject_dev.ps1 (new)
- fifa_overlay/Cargo.toml (modified)
- tools/resolve_short_names.py (new)
- _bmad-output/planning-artifacts/epics.md (modified: Story 1.1/1.2/2.4 ACs)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
- _bmad-output/planning-artifacts/architecture/architecture-FIFA_EDITOR-2026-09-22/ARCHITECTURE-SPINE.md (modified: Stack)
- PROJECT_MEMORY.md (modified: Sessão 6)
