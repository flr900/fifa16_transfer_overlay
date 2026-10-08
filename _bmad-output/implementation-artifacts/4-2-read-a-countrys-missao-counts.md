---
baseline_commit: d110eca
---

# Story 4.2: Read a country's Missão counts

Status: done (substituída: o Sonar foi removido do main em 2026-10-08, commit 1c6d794; a 5ª aba virou a Base do Scout)

## Story

As Felipe,
I want to click a country and see how many Missões are active or completed there,
so that I can decide whether to scout it again.

## Acceptance Criteria

1. **Given** the Sonar map, **when** I click a country tile, **then** a textual summary appears on the same tab with the country name and the counts of active and completed Missões, with no navigation to another tab (UX-DR17).

## Design decisions (defaults proposed when the story was implemented)

- **Where the summary sits**: above the map, so it does not scroll away with the cartogram. Before any click, the line says "Clique num país para ver as Missões dele."
- **State**: the chosen country is kept in `ScoutState.pais_sonar`; it is **not persisted** and resets when the panel closes (`ao_fechar_painel`).
- **Text**: country name (now `sonar::nome_quadro`, which names the tile from the Sonar's tiles and covers the "Outros" tile) plus "1 Missão ativa · 2 Missões concluídas" (singular/plural handled). A country with no Missão says "Nunca escaneado: nenhuma Missão neste país." If its continent has whole-continent Missões, a "Mais, pelo continente inteiro (…)" line follows. Both counts are always shown, even when the tile is painted as active (Story 4.1, AC 2).
- **Gamepad**: the tiles are the cartogram's navigable items; focus lands on the first tile when entering the tab (existing `tomar_foco_pendente` rule); focus = choice (Felipe's rule from 2026-10-01): the summary follows the tile focused by the controller, and A also shows it. `cartograma::render` returns `Resposta { ativado, focado }`; the Sonar uses `ativado.or(focado)`. The geographic filter no longer uses the cartogram. Tabs only on LB/RB.

## Dev Notes

### Merge with main (Epic 3, 2026-10-04)

`main` (Epic 3, story 3-6) changed the Mission geographic filter to "where the player plays" (`continentes`, `paises_dos_clubes`, `ligas`; `paises` by nationality is legacy). The Sonar is now by country of the CLUB (see Story 4.1, "Merge with main"): the tiles are the league countries of the career, plus countries only covered by old nationality Missões (named via `nacoes`). What changes for the country summary:

- A league paints its country; the country's counts include Missões by `paises_dos_clubes`, by league and old ones by nationality.
- Whole-continent Missões and continental leagues (no country) are not painted tile by tile: they appear on the "Continentes inteiros — …" line and, in the summary of a country of that continent, as "Mais, pelo continente inteiro (Europa): 1 ativa · 0 concluídas".
- Missões without geography go on "Missões no mundo todo: …" (renamed from "Missões globais"), not in any country.
- The "Outros" tile only appears for old Missões that used it; the summary still names it "Outros".
- `ScoutState::cobertura(ligas)` now takes the leagues; the Sonar shows the league loading/error state with "Tentar novamente".
- The cartogram has no `Selecionado` state anymore; `cartograma::render` takes `com_outros` and `marcado` ("•" on the summary's tile) and returns `Resposta { ativado, focado }`.
- **Decision to confirm**: club country vs nationality was chosen by Claude as the recommended default, because main left it open for Epic 4 ("decidir se mostra nacionalidades ou ligas"). **To be confirmed with Felipe in game.**
- Build tag: "4.2-v2 — Sonar por país do clube (com o main do Épico 3)". In-game validation still pending.

## Tasks / Subtasks

- [x] Task 1: State
  - [x] 1.1 `ScoutState.pais_sonar: Option<u16>`, `pais_sonar()`, `escolher_pais_sonar()`; reset in `ao_fechar_painel`.
- [x] Task 2: Screens
  - [x] 2.1 Country name for the summary: `sonar::nome_quadro` (tile name from `Cobertura::quadros`; "Outros" for `NACAO_OUTROS`; "País N" fallback).
  - [x] 2.2 `sonar.rs`: the click/A on a tile (return of `cartograma::render`) chooses the country; summary with the name and `texto_contagem(...)` (or `MSG_NUNCA_ESCANEADO`), plus the "Mais, pelo continente inteiro (…)" line when the continent has whole-continent Missões; the summary's tile is marked with "•" (`marcado`).
- [x] Task 3: Tests — `cargo test --lib` in `fifa_overlay`: 244 passed, 1 ignored after the merge with main (`sonar`: exact and pluralised counts text, including the global and continent lines; `state`: `the_sonar_counts_archived_reports_as_coverage_and_forgets_the_country_on_close` — chosen country is kept and forgotten when the panel closes, together with the coverage of archived Relatórios).
- [ ] Task 4: Manual check in game (Felipe): click a country with an active and a completed Missão (both counts), one never scanned (message), a country whose continent has a whole-continent Missão (the "Mais, pelo continente inteiro" line), the "Outros" tile (only with old Missões); move the gamepad focus across tiles (the summary follows) and press A; close and reopen the panel (the summary is gone).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.2]
- [Source: EXPERIENCE.md (Sonar de Cobertura), UX-DR17]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 4.2 implemented (country summary on the Sonar tab).
- 2026-10-04: Merged with main (Epic 3): summary by country of the club, with the whole-continent line.

### File List

- fifa_overlay/src/scout/state.rs (modified: `pais_sonar`, `escolher_pais_sonar()`, reset on close, test; conflict resolved in the merge with main)
- fifa_overlay/src/scout/screens/sonar.rs (summary), cartograma.rs (`Resposta`, `marcado`) (modified)
- fifa_overlay/src/scout/cobertura.rs (`do_continente`, `continentes()`, used by the summary's continent line)
