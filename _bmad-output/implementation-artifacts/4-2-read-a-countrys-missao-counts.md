---
baseline_commit: d110eca
---

# Story 4.2: Read a country's Missão counts

Status: review

## Story

As Felipe,
I want to click a country and see how many Missões are active or completed there,
so that I can decide whether to scout it again.

## Acceptance Criteria

1. **Given** the Sonar map, **when** I click a country tile, **then** a textual summary appears on the same tab with the country name and the counts of active and completed Missões, with no navigation to another tab (UX-DR17).

## Design decisions (defaults proposed when the story was implemented)

- **Where the summary sits**: above the map, so it does not scroll away with the cartogram. Before any click, the line says "Clique num país para ver as Missões dele."
- **State**: the chosen country is kept in `ScoutState.pais_sonar`; it is **not persisted** and resets when the panel closes (`ao_fechar_painel`).
- **Text**: country name (`selecao_geografica::nome_pais`, extracted from the row summary and also covering the "Outros" tile) plus "1 Missão ativa · 2 Missões concluídas" (singular/plural handled). A country with no Missão says "Nunca escaneado: nenhuma Missão neste país." Both counts are always shown, even when the tile is painted as active (Story 4.1, AC 2).
- **Gamepad**: the tiles are the cartogram's navigable items; focus lands on the first tile when entering the tab (existing `tomar_foco_pendente` rule); focus = choice (Felipe's rule from 2026-10-01): the summary follows the tile focused by the controller, and A also shows it. `cartograma::render` now returns `Resposta { ativado, focado }`; the geographic selection uses only `ativado`, so moving across the map never marks countries there. Tabs only on LB/RB.

## Tasks / Subtasks

- [x] Task 1: State
  - [x] 1.1 `ScoutState.pais_sonar: Option<u16>`, `pais_sonar()`, `escolher_pais_sonar()`; reset in `ao_fechar_painel`.
- [x] Task 2: Screens
  - [x] 2.1 `selecao_geografica::nome_pais()` extracted (reused by `resumo_paises`).
  - [x] 2.2 `sonar.rs`: the click/A on a tile (return of `cartograma::render`) chooses the country; summary with the name and `texto_contagem(...)`, or `MSG_NUNCA_ESCANEADO`.
- [x] Task 3: Tests — `cargo test --lib` in `fifa_overlay`: 186 passed (new: exact and pluralised counts text in `sonar`; in `state`: chosen country is kept and forgotten when the panel closes, together with the coverage of archived Relatórios).
- [ ] Task 4: Manual check in game (Felipe): click a country with an active and a completed Missão (both counts), one never scanned (message), the "Outros" tile; move the gamepad focus across tiles (the summary follows) and press A; close and reopen the panel (the summary is gone).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.2]
- [Source: EXPERIENCE.md (Sonar de Cobertura), UX-DR17]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 4.2 implemented (country summary on the Sonar tab).

### File List

- fifa_overlay/src/scout/state.rs (modified: `pais_sonar`, `escolher_pais_sonar()`, reset on close, test)
- fifa_overlay/src/scout/screens/sonar.rs (summary), selecao_geografica.rs (`nome_pais()`) (modified)
