---
baseline_commit: 9ab2b7c
---

# Story 2.3: Follow the progress of active Missões

Status: review

## Story

As Felipe,
I want to see each Missão's progress and estimated completion,
so that I know when to come back for the Relatório.

## Acceptance Criteria

1. **Given** active Missões exist, **when** I open the Missões tab, **then** each shows its Olheiro, Modo de Busca, a progress bar (track `bg-panel-raised`, purple fill) and text like "pronto em ~N dias de carreira", never the bar alone (UX-DR7), **and** progress is computed from `GJUr.currdate`, `criada_em` and `prazo_estimado`, recomputed when the panel opens, with no real-time polling (FR8).
2. **Given** the career date is at or past `prazo_estimado`, **when** I open the tab, **then** the Missão shows as ready or completed, not as a negative or over-100% progress.
3. **Given** no Missão exists, **when** I open the tab, **then** it shows an empty state with the "Nova Missão" button still available.
4. **Given** the date cannot be read, **when** the tab renders, **then** the generic read-error state with retry appears, without losing the Missão list.

## Tasks / Subtasks

- [x] Task 1: Domain (`scout::state`)
  - [x] 1.1 `progresso_missao(criada, prazo, hoje) -> ProgressoMissao { fracao, dias_restantes, prazo_atingido }`, clamped to 0–100%. A date before creation (an older save loaded) gives 0%. A deadline reached is always 100%, including a deadline on the creation day (no division by zero). Works across months and years.
  - [x] 1.2 `data_progresso`: the date used for progress is captured when the panel opens and when a career becomes ready. It stays FIXED while the panel is open; the 1 s re-read keeps updating the header but not the progress (FR8: no polling).
  - [x] 1.3 `missoes()` → `Vec<MissaoNaLista { missao, olheiro, progresso }>`. With a read error it still returns the list of the last ready career, without progress (`ultimo_save`).
- [x] Task 2: Missões tab (`screens/missoes.rs`)
  - [x] 2.1 Card: Olheiro + Tier badge, Qualidade badge on the right; "Modo · status · prazo dd/mm/aaaa"; progress bar (dark track inside the raised card, purple fill; green when Concluída); estimate text always under the bar. Texts: "Relatório pronto em ~N dias de carreira (dd/mm/aaaa).", "~1 dia", "Pronta: prazo cumprido em dd/mm/aaaa." (green), "Gerando o Relatório…", "Concluída: Relatório disponível.", and without a date "Progresso indisponível: não foi possível ler a data da carreira.".
  - [x] 2.2 Empty state "Nenhuma Missão encomendada ainda." with "Nova Missão" above it (from 2.2).
  - [x] 2.3 Read error on the Missões tab: the generic message and "Tentar novamente", then the list (without "Nova Missão", which needs a ready career).
- [x] Task 3: Tests — `cargo test`: 131 passed (new: progress clamping and edges; progress frozen while the panel is open and recomputed on reopen; list kept on read error; estimate texts; bar never out of range; status labels).
- [x] Task 4: Manual check in game (Felipe)
  - [x] 4.1 The Missão created in 2.2 shows a bar and "Relatório pronto em ~N dias de carreira (dd/mm/aaaa)".
  - [x] 4.2 Advance a few days in the career, reopen the panel: the bar moves and N drops. With the panel open, it does not change.
  - [x] 4.3 Advance past the deadline: "Pronta: prazo cumprido em …", full bar, nothing over 100%.

## Dev Notes

- "Pronta" missions wait for Story 2.4 (search runs on the next panel opening and the Relatório is generated); until then they stay "Pronta".
- This build does not include the font sharpness work, which is running separately on branch `claude/nitidez-fontes`.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.3]
- [Source: EXPERIENCE.md (Barra de progresso de Missão, State Patterns "Missão em andamento"), DESIGN.md (progress-bar-*)]
- [Source: prd.md#FR-8]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 131 passed; `cargo build --release` with no code warnings (`2.3-v1`).

### Completion Notes List

- Felipe tested `2.3-v1` in game: "funcionou".

### Change Log

- 2026-10-01: Story 2.3 implemented (Missão progress).

### File List

- fifa_overlay/src/scout/state.rs (modified: progress, frozen date, list on read error)
- fifa_overlay/src/scout/screens/missoes.rs (rewritten: progress cards)
- fifa_overlay/src/scout/screens/mod.rs (modified: Missões list in the read-error state)
- fifa_overlay/src/lib.rs (modified: BUILD_TAG)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
