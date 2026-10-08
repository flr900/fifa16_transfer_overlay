---
baseline_commit: d110eca
---

# Story 4.1: See coverage by country on the Sonar map

Status: done (substituída: o Sonar foi removido do main em 2026-10-08, commit 1c6d794; a 5ª aba virou a Base do Scout)

## Story

As Felipe,
I want a map that shows where my scouting network is working,
so that I know where I have and haven't looked.

## Acceptance Criteria

1. **Given** the Sonar tab, **when** it renders, **then** it shows the country tile cartogram from Story 2.9 in view-only mode, drawn via `ImDrawList` (UX-DR17), one tile per country of the career's leagues (country of the CLUB, see "Merge with main" below), **and** each country is one of three states: never scanned (no fill), active Missão (purple outline plus tenuous fill), or completed Missão with a Relatório available (field-green outline plus tenuous fill), **and** no solid saturated fill, glow or gradient is used.
2. **Given** a country has both an active and a completed Missão, **when** it renders, **then** the active state takes precedence visually, and the summary from Story 4.2 shows both counts.
3. **Given** a Missão with no geographic filter ("all countries"), **when** the Sonar renders, **then** it does not colour every tile; it is counted in a "Missões no mundo todo: …" line above the map (renamed from "Missões globais" in the merge with main), so the map stays informative (assumption to confirm).
4. **Given** a legend, **when** it renders, **then** it names the three states in text next to their swatches, so state is never conveyed by colour alone (NFR5).
5. **Given** an archived Relatório, **when** coverage is computed, **then** its Missão still counts as completed coverage.
6. **Given** no Missão exists, **when** the tab renders, **then** the map shows every country as never scanned, with a short explanatory line, not an error.

## Design decisions (defaults proposed when the story was implemented)

- **Active vs completed**: active = `Pendente` or `EmExecucao` (this includes a continuous Missão that already has a partial Relatório); completed = `Concluida`, with the Relatório archived or not (archiving never erases coverage).
- **"Missões no mundo todo" (open assumption)**: implemented as written in the AC: a Missão without geographic filter does not paint any tile and only shows in the line "Missões no mundo todo: N ativas · M concluídas" above the map (was "Missões globais" before the merge with main). **Still to confirm with Felipe in game.**
- **Where the logic lives**: a new module `scout::cobertura` (pure computation over the persisted `Missao` list), not `state.rs`, to keep merge conflicts small with the parallel Epic 3 branch `claude/relatorio-ficha`, which also edits `state.rs`. `state.rs` only gets a thin `cobertura()` accessor.
- **Legend wording** follows the mockup (`mockups/sonar.html`): "Nunca escaneado", "Missão ativa", "Missão concluída". Swatches are drawn with the same colours as the tiles (`cartograma::cores`), so legend and map never diverge.
- **Gamepad**: the tiles are the cartogram's navigable items; focus lands on the first tile when entering the tab (existing `tomar_foco_pendente` rule); tabs still switch only with LB/RB (see the gamepad focus rules).

## Dev Notes

### Merge with main (Epic 3, 2026-10-04)

`main` (Epic 3, story 3-6) changed the Mission geographic filter from player nationality (`FiltrosMissao.paises`, Story 2.9 cartogram) to "where the player plays": `continentes` (whole continents), `paises_dos_clubes` (countries of the leagues) and `ligas` (each `Liga` has `pais: Option<u16>` and `continente`). `paises` is legacy only (old Missões keep filtering by it). The Sonar was adapted:

- **Sonar by country of the CLUB.** The tiles are the league countries of the career, plus countries only covered by old nationality Missões (named via `nacoes`; without it, "País N" in "Outros"): `Cobertura::quadros(ligas, nacoes)`. The id is the same (`Crbb.nationid`), so old Missões paint the same tile.
- `Cobertura::de(missoes, ligas)`: `paises_dos_clubes` and old `paises` paint their country; a league paints its country (once per Missão even with the country also chosen); an unknown league is ignored and never counts as global.
- **Whole-continent Missões and continental leagues (no country)** are not painted tile by tile: they are counted per continent (`por_continente`, `continentes()`) on a "Continentes inteiros — Europa: 1 ativa · 0 concluídas; …" line above the map, and in the country summary ("Mais, pelo continente inteiro (…)").
- **Missões without geography** go on "Missões no mundo todo: …" (renamed from "Missões globais").
- **The "Outros" tile** only appears for old Missões that used it (`tem_outros()`).
- **Loading/error**: the Sonar shows the league loading state and the error with "Tentar novamente", reusing the geographic filter's messages (`selecao_geografica::MSG_LENDO`/`MSG_ERRO`, `reler_ligas()`).
- `ScoutState::cobertura(&self, ligas)` now takes the leagues. The cartogram no longer has the `Selecionado` state (the geographic filter does not use it anymore); new params `com_outros` and `marcado` ("•" on the summary's tile).
- **Decision to confirm**: club country vs nationality was chosen by Claude as the recommended default, because main left it open for Epic 4 ("decidir se mostra nacionalidades ou ligas"). **To be confirmed with Felipe in game.**
- Build tag: "4.2-v2 — Sonar por país do clube (com o main do Épico 3)".

## Tasks / Subtasks

- [x] Task 1: Domain (`scout/cobertura.rs`, new)
  - [x] 1.1 `Contagem { ativas, concluidas }` and `EstadoPais { NuncaEscaneado, MissaoAtiva, MissaoConcluida }`.
  - [x] 1.2 `Cobertura::de(&[Missao], &[Liga])`: counts per `Crbb.nationid` from `paises_dos_clubes`, legacy `paises` (including the `NACAO_OUTROS` tile) and the country of each chosen league (a country repeated in one filter counts once per Missão); whole continents and continental leagues go to `por_continente`; Missões without geography go to `globais`; `total` = number of Missões.
  - [x] 1.3 `Cobertura::estado` (active takes precedence), `contagem`, `do_continente`/`continentes()`, `tem_outros()` and `quadros(ligas, nacoes)` (the tiles).
  - [x] 1.4 `ScoutState::cobertura(ligas)`: recomputed from the saved Missões of the ready career (`None` without a ready career).
- [x] Task 2: Screens
  - [x] 2.1 `cartograma.rs`: removed `#[allow(dead_code)]` from `MissaoAtiva`/`MissaoConcluida`; new `cores(estado)` (shared by tiles and legend) and `amostra()` (legend swatch).
  - [x] 2.2 `sonar.rs`: title "Sonar de Cobertura", explanatory line (with no Missão: "Nenhuma Missão criada ainda: todos os países aparecem como nunca escaneados."), legend with 3 swatches + text, "Missões no mundo todo" line (only when there are Missões without geography), "Continentes inteiros — …" line (only when there are whole-continent Missões), league loading/error state with "Tentar novamente", the cartogram in view-only mode inside a child window (`com_outros`, `marcado`).
  - [x] 2.3 `screens/mod.rs`: the Sonar now receives `state`; removed the `aba_vazia` placeholder and `MSG_ABA_VAZIA`.
- [x] Task 3: Tests — `cargo test --lib` in `fifa_overlay`: 244 passed, 1 ignored after the merge with main (in `cobertura`: no Missões = all never scanned; Pendente/EmExecucao active and Concluida completed; active takes precedence and both counts kept; a league paints its country once; whole continents and continental leagues count per continent without painting; Missões without geography only in `globais`; unknown league ignored; old nationality Missões and "Outros"; tiles = league countries + old extras; in `sonar`: legend names and texts; in `state`: archived Relatório still counts as coverage).
- [ ] Task 4: Manual check in game (Felipe): open the Sonar with no Missão (all tiles empty + explanatory line); create Missões by country of the club, by league, by whole continent and without a filter (the "Continentes inteiros — …" and "Missões no mundo todo" lines; confirm the assumptions, including club country vs nationality); wait for one to conclude (green); archive its Relatório (stays green); check the tiles with the D-pad and the legend text. Build tag in the log: "4.2-v2 — Sonar por país do clube (com o main do Épico 3)". In-game validation still pending.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.1]
- [Source: EXPERIENCE.md (Sonar de Cobertura), mockups/sonar.html, UX-DR17, NFR5]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 4.1 implemented (coverage by country on the Sonar map).
- 2026-10-04: Merged with main (Epic 3): Sonar by country of the club, continent line, "Missões no mundo todo".

### File List

- fifa_overlay/src/scout/cobertura.rs (new; adapted to the leagues in the merge)
- fifa_overlay/src/scout/mod.rs (modified: `pub mod cobertura;`)
- fifa_overlay/src/scout/state.rs (modified: `cobertura(ligas)`; conflict resolved in the merge with main)
- fifa_overlay/src/scout/screens/sonar.rs (rewritten), cartograma.rs (`Selecionado` removed; `com_outros`, `marcado`), mod.rs (modified)
- fifa_overlay/src/lib.rs (BUILD_TAG: "4.2-v2 — Sonar por país do clube (com o main do Épico 3)")
