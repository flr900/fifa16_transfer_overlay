---
baseline_commit: d110eca
---

# Story 4.1: See coverage by country on the Sonar map

Status: review

## Story

As Felipe,
I want a map that shows where my scouting network is working,
so that I know where I have and haven't looked.

## Acceptance Criteria

1. **Given** the Sonar tab, **when** it renders, **then** it shows the country tile cartogram from Story 2.9 in view-only mode, drawn via `ImDrawList` (UX-DR17), **and** each country is one of three states: never scanned (no fill), active Missão (purple outline plus tenuous fill), or completed Missão with a Relatório available (field-green outline plus tenuous fill), **and** no solid saturated fill, glow or gradient is used.
2. **Given** a country has both an active and a completed Missão, **when** it renders, **then** the active state takes precedence visually, and the summary from Story 4.2 shows both counts.
3. **Given** a Missão with no geographic filter ("all countries"), **when** the Sonar renders, **then** it does not colour every tile; it is counted in a "Missões globais" line above the map, so the map stays informative (assumption to confirm).
4. **Given** a legend, **when** it renders, **then** it names the three states in text next to their swatches, so state is never conveyed by colour alone (NFR5).
5. **Given** an archived Relatório, **when** coverage is computed, **then** its Missão still counts as completed coverage.
6. **Given** no Missão exists, **when** the tab renders, **then** the map shows every country as never scanned, with a short explanatory line, not an error.

## Design decisions (defaults proposed when the story was implemented)

- **Active vs completed**: active = `Pendente` or `EmExecucao` (this includes a continuous Missão that already has a partial Relatório); completed = `Concluida`, with the Relatório archived or not (archiving never erases coverage).
- **"Missões globais" (open assumption)**: implemented as written in the AC: a Missão without geographic filter does not paint any tile and only shows in the line "Missões globais (todos os países): N ativas · M concluídas" above the map. **Still to confirm with Felipe in game.**
- **Where the logic lives**: a new module `scout::cobertura` (pure computation over the persisted `Missao` list), not `state.rs`, to keep merge conflicts small with the parallel Epic 3 branch `claude/relatorio-ficha`, which also edits `state.rs`. `state.rs` only gets a thin `cobertura()` accessor.
- **Legend wording** follows the mockup (`mockups/sonar.html`): "Nunca escaneado", "Missão ativa", "Missão concluída". Swatches are drawn with the same colours as the tiles (`cartograma::cores`), so legend and map never diverge.
- **Gamepad**: the tiles are the cartogram's navigable items; focus lands on the first tile when entering the tab (existing `tomar_foco_pendente` rule); tabs still switch only with LB/RB (see the gamepad focus rules).

## Tasks / Subtasks

- [x] Task 1: Domain (`scout/cobertura.rs`, new)
  - [x] 1.1 `Contagem { ativas, concluidas }` and `EstadoPais { NuncaEscaneado, MissaoAtiva, MissaoConcluida }`.
  - [x] 1.2 `Cobertura::de(&[Missao])`: counts per `Crbb.nationid` (including the `NACAO_OUTROS` tile; a country repeated in one filter counts once per Missão); Missões without countries go to `globais`; `total` = number of Missões.
  - [x] 1.3 `Cobertura::estado` (active takes precedence) and `contagem`.
  - [x] 1.4 `ScoutState::cobertura()`: recomputed from the saved Missões of the ready career (`None` without a ready career).
- [x] Task 2: Screens
  - [x] 2.1 `cartograma.rs`: removed `#[allow(dead_code)]` from `MissaoAtiva`/`MissaoConcluida`; new `cores(estado)` (shared by tiles and legend) and `amostra()` (legend swatch).
  - [x] 2.2 `sonar.rs`: title "Sonar de Cobertura", explanatory line (with no Missão: "Nenhuma Missão criada ainda: todos os países aparecem como nunca escaneados."), legend with 3 swatches + text, "Missões globais" line (only when there are global Missões), the cartogram in view-only mode inside a child window.
  - [x] 2.3 `screens/mod.rs`: the Sonar now receives `state`; removed the `aba_vazia` placeholder and `MSG_ABA_VAZIA`.
- [x] Task 3: Tests — `cargo test --lib` in `fifa_overlay`: 186 passed (new in `cobertura`: no Missões = all never scanned; Pendente/EmExecucao active and Concluida completed; active takes precedence and both counts kept; global Missões only in `globais`; "Outros" tile and repeated countries; in `sonar`: legend names; in `state`: archived Relatório still counts as coverage).
- [ ] Task 4: Manual check in game (Felipe): open the Sonar with no Missão (all tiles empty + explanatory line); create Missões with one country, with several, and without a filter (the "Missões globais" line, confirm the assumption); wait for one to conclude (green); archive its Relatório (stays green); check the tiles with the D-pad and the legend text. Build tag in the log: "4.2-v1 — Sonar de Cobertura (worktree épico 4)".

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.1]
- [Source: EXPERIENCE.md (Sonar de Cobertura), mockups/sonar.html, UX-DR17, NFR5]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 4.1 implemented (coverage by country on the Sonar map).

### File List

- fifa_overlay/src/scout/cobertura.rs (new)
- fifa_overlay/src/scout/mod.rs (modified: `pub mod cobertura;`)
- fifa_overlay/src/scout/state.rs (modified: `cobertura()`)
- fifa_overlay/src/scout/screens/sonar.rs (rewritten), cartograma.rs, mod.rs (modified)
- fifa_overlay/src/lib.rs (BUILD_TAG)
