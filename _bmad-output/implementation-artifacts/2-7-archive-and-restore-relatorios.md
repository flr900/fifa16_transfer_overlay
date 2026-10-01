---
baseline_commit: 03019a7
---

# Story 2.7: Archive and restore Relatórios

Status: review

## Story

As Felipe,
I want to archive Relatórios I have already reviewed,
so that the list stays clean without losing what my budget paid for.

## Acceptance Criteria

1. **Given** a Relatório that has been opened, **when** I press "Arquivar", **then** it disappears from the main list and appears under an "Arquivados" filter; it is never deleted.
2. **Given** the "Arquivados" filter, **when** I press "Restaurar", **then** the Relatório returns to the main list.
3. **Given** a Relatório not yet opened, **when** I view it, **then** "Arquivar" is not offered.

## Tasks / Subtasks

- [x] Task 1: Domain — `pode_arquivar` (opened, not archived, Missão finished), `arquivar_relatorio` / `restaurar_relatorio` (only flip `arquivado`, write-through; nothing is deleted), filter state `vendo_arquivados` (not persisted; closing the panel goes back to the main list). A Missão whose Relatório is archived leaves the Missões tab (FR8: completed Missões stay visible until the Relatório is archived).
- [x] Task 2: Relatórios tab — "Ativos / Arquivados" toggle next to Tabular / Cards; "Arquivar" button at the right of each opened card (empty space for a NOVO card); "Restaurar" in the Arquivados filter; empty state "Nenhum Relatório arquivado.".
- [x] Task 3: Relatório screen — "Arquivar" next to "Voltar" (archives and goes back to the list).
- [x] Task 4: Tests — `cargo test`: 165 passed (new: archive refused before opening, archive/restore round trip, persisted, Missão hidden while archived).
- [ ] Task 5: Manual check in game (Felipe).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.7]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-01: Story 2.7 implemented (archive and restore).

### File List

- fifa_overlay/src/scout/state.rs (modified)
- fifa_overlay/src/scout/screens/relatorios.rs (modified)
- fifa_overlay/src/scout/screens/relatorio.rs (modified)
