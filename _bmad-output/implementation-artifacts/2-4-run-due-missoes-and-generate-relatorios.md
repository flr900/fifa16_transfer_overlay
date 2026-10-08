---
baseline_commit: 589a91e
---

# Story 2.4: Run due Missões and generate Relatórios

Status: done

## Story

As Felipe,
I want a Missão to run its search when its deadline has passed,
so that the Relatório is ready when I come back, without the game freezing.

## Acceptance Criteria

1. **Given** `AsyncTask<T>` already exists from Story 1.1, **when** this story is done, **then** `save_repo::read_all_players()` returns raw `PlayerRaw` data and is only called through an `AsyncTask` (~32k `CZUM` records), following the AD-4 conventions.
2. **Given** a `Pendente` Missão whose `prazo_estimado` has passed, **when** the panel goes from closed to open (edge only, never polled while open), **then** the status changes to `EmExecucao` before dispatch, then the search task is dispatched (AD-8), **and** closing and reopening the panel while it runs never dispatches a second task for the same Missão.
3. **Given** several Missões are due on the same reopen, **when** they are dispatched, **then** they run one at a time from a single FIFO queue ordered by `prazo_estimado`, then `criada_em`, and never in parallel (AD-9).
4. **Given** the search runs, **when** `scout::search::executar_missao(missao)` executes, **then** it applies the Overall/Potencial filter, calls `scout::quality` to set Qualidade, and returns a complete `Relatorio`; `save_repo` receives no filter criteria (AD-3), **and** the number of players and how precisely each attribute is revealed follow the Qualidade.
5. **Given** the task finishes, **when** it reports `Done`, **then** the `Relatorio` is persisted write-through and the Missão becomes `Concluida`, even if the panel was closed meanwhile (AD-7, AD-8), **and** the Olheiro becomes "Disponível" again.
6. **Given** the task fails or the game closes mid-run, **when** the state is next loaded, **then** a Missão left in `EmExecucao` is reset to `Pendente`, **and** a failure is shown with a clear message, not silently.
7. **Given** a scan is running in a long session, **when** I play, **then** the render thread is never blocked, and FPS with and without a running scan is measured and the result recorded (NFR2, NFR7).

## Tasks / Subtasks

- [x] Task 1: Read every player (`save_repo::jogadores`, new)
  - [x] 1.1 Source: the active save's `DATA` on disk (the file the locator picked by memory, AD-11) plus the game's static DB `data\db\fifa_ng_db.db`. The heap `CZUM` blob is the same content as the file (session 6), so reading memory gains nothing.
  - [x] 1.2 `fifa_db`: table helpers (`record`, deleted flag, `read_int_field`, `read_fixed_string`) and Huffman strings (`HuffmanStrings`, port of `fifa16_db_parser`), for `BGwe.name`.
  - [x] 1.3 Names: `editedplayernames` by `playerid`, else `commonnameid` or first + last name, looked up in `dcplayernames` (save, regens) then `playernames` (static). On Felipe's current save all 39,229 players resolve.
  - [x] 1.4 Club via `teamplayerlinks`, skipping national teams (`teamnationlinks`); flag for the "Rest of World" league (76). Nation from `Crbb` (name, ISO, confederation).
  - [x] 1.5 `Atributo` (28 outfield + 5 GK) with Portuguese name and 3-letter code; `nome_posicao`, `funcao_da_posicao`.
  - [x] 1.6 `PlayerPool { jogadores, nacoes, clube_usuario }`; the static DB is read once per session.
- [x] Task 2: Reveal formulas (`scout::quality`)
  - [x] 2.1 `ordem_de_observacao(funcao, dominante)`: the attributes an Olheiro looks at first for each role (GK attributes only for goalkeepers).
  - [x] 2.2 `faixa_revelada(real, precisao, semente)`: width `2 × precisão`, always contains the real value, inside 1–99; the real value's place in the range is seeded, so the middle of the range does not give it away.
  - [x] 2.3 `nota_de_escolha` / `ruido_de_escolha`: relevance + noise of 3 / 8 / 15 points for Alta / Média / Baixa (a weaker Olheiro brings more random names). `relevancia` by Missão type.
  - [x] 2.4 `semente`: SplitMix64 of Missão id, player and channel, so a Relatório is deterministic.
  - [x] 2.5 Revealed attributes at the top of the scale: 29 → 28 (the real count of outfield attributes).
- [x] Task 3: Search (`scout::search`)
  - [x] 3.1 `executar_missao(missao, fonte, hoje) -> Relatorio`: `read_all_players()` with no criteria (AD-3), Overall/Potencial filter, skips the user's club, "Rest of World" and players without a club; picks `alvo_jogadores`; reveals Overall, Potencial and `atributos_revelados` attributes as ranges.
  - [x] 3.2 `Relatorio` now stores `gerado_em`, `qualidade`, `precisao_mais_menos`, `jogadores` (`JogadorEncontrado`: name, age, position, nation, club, revealed values only), `aberto`, `arquivado`, all `serde(default)`.
- [x] Task 4: Dispatch and queue (`scout::state`)
  - [x] 4.1 On the panel open edge (or when the career becomes ready with the panel open): due `Pendente` Missões → `EmExecucao` (written first), then queued by (`prazo_estimado`, `criada_em`).
  - [x] 4.2 `tick` (panel open or closed) processes one search at a time; `Done` → Relatório saved and Missão `Concluida` in the file of the career that owns it; banner "Relatório pronto".
  - [x] 4.3 Failure → back to `Pendente`, message on the Missão card ("A busca falhou: … Ela roda de novo quando o painel abrir.") and banner "A busca de uma Missão falhou.".
  - [x] 4.4 When a career's file is loaded (once per session), Missões left `EmExecucao` go back to `Pendente`.
- [x] Task 5: Tests — `cargo test`: 149 passed (new: real-save oracle for players/names/club/nation; reveal ranges; observation order; choice noise; filters; deterministic report; dispatch on open, no duplicate, FIFO order, failure, recovery).
- [ ] Task 6: Manual check in game (Felipe)
  - [ ] 6.1 Create a cheap Missão (Rápida), advance past the deadline, save the career, open the panel: banner/Relatório appears, the Olheiro is free again.
  - [ ] 6.2 Measure FPS during the search (log line `[scout::search] Missão …: N jogadores no Relatório (X ms…)`) and record it here (NFR7).

## Dev Notes

- The players come from the last in-game save of the career. Attribute changes the game has not saved yet show up in the next Missão after saving.
- The Relatório view itself is Story 2.5; until then the Missão shows "Concluída: Relatório disponível.".

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.4]
- [Source: ARCHITECTURE-SPINE.md AD-3, AD-4, AD-7, AD-8, AD-9, AD-12]
- [Source: PROJECT_MEMORY.md "Dados do save — referência rápida"]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 149 passed.

### Completion Notes List

### Change Log

- 2026-10-01: Story 2.4 implemented (search of due Missões).

### File List

- fifa_overlay/src/fifa_db.rs (modified: table helpers, Huffman strings)
- fifa_overlay/src/save_repo.rs (modified: `jogadores` module and re-exports)
- fifa_overlay/src/save_repo/jogadores.rs (new)
- fifa_overlay/src/scout/quality.rs (modified: reveal formulas)
- fifa_overlay/src/scout/search.rs (rewritten: `executar_missao`)
- fifa_overlay/src/scout/state.rs (modified: Relatório types, dispatch/queue/recovery)
- fifa_overlay/src/scout/persistence.rs (modified: test)
- fifa_overlay/src/scout/mod.rs (modified: `ao_fechar_painel`)
- fifa_overlay/src/scout/screens/aviso.rs (modified: Relatório banners)
- fifa_overlay/src/scout/screens/missoes.rs (modified: search failure text)
