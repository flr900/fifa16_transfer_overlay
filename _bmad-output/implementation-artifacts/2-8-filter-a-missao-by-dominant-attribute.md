---
baseline_commit: a42c446
---

# Story 2.8: Filter a Missão by dominant attribute

Status: review

## Story

As Felipe,
I want to ask for "the best dribbler" or "the best defender",
so that I find profiles instead of just Overall.

## Acceptance Criteria

1. **Given** the Nova Missão form, **when** I activate the "Atributo dominante" row, **then** a full-screen field panel opens with the tab bar and header still visible and the footer summary still visible (UX-DR11), listing the dominant-attribute options, **and** choosing one returns to the form with the row showing the choice.
2. **Given** a Missão with this filter, **when** the search runs, **then** `scout::search` keeps only players whose dominant attribute matches, combined (AND) with the other filters.
3. **Given** the filter is combined with the Overall/Potencial filter, **when** both are set, **then** both apply.

## Tasks / Subtasks

- [x] Task 1: Domain
  - [x] 1.1 `FiltrosMissao.atributo_dominante: Option<Atributo>` (`serde(default)`: old files keep working); `definir_atributo_da_missao`.
  - [x] 1.2 Rule (`quality::eh_dominante`): the attribute is among the player's **3 highest** (ties count), counting only the attributes of the player's role (GK attributes only for goalkeepers). Only the single highest would be too rare: many players have Sprint Speed or Strength on top.
  - [x] 1.3 `quality::tipo_por_filtros`: a dominant attribute makes the Missão **Tática** (the Tático's specialty → Qualidade bonus).
  - [x] 1.4 Search: AND with the other filters; candidates are ranked by that attribute; the Olheiro observes it first (first column/attribute in the Relatório).
- [x] Task 2: Screens
  - [x] 2.1 Form row "Atributo dominante" with the current value ("Qualquer um" or the name) → pushes `Satelite::CampoAtributo` (stack: tab, form, panel = AD-6 max depth).
  - [x] 2.2 `campo_atributo.rs`: "Voltar", explanation, "Qualquer um" + the 33 attributes in 7 groups (Ritmo, Drible, Finalização, Passe, Defesa, Físico, Goleiro); the live summary (cost / time / Qualidade / type) stays at the bottom. Choosing returns to the form.
  - [x] 2.3 The form draft lives while the form is anywhere in the stack (`Navigation::contem`); B on the panel goes back to the form.
  - [x] 2.4 Missão card and Relatório header show "foco em Drible".
- [x] Task 3: Tests — `cargo test`: 168 passed (new: top-3 rule with ties and role; Tática type; filter AND ranges and ranking by the attribute; every attribute once in the panel groups).
- [ ] Task 4: Manual check in game (Felipe).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.8]
- [Source: EXPERIENCE.md (Painel de campo), UX-DR11]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-01: Story 2.8 implemented (dominant attribute filter).

### File List

- fifa_overlay/src/scout/state.rs, quality.rs, search.rs, mod.rs (modified)
- fifa_overlay/src/scout/screens/campo_atributo.rs (new)
- fifa_overlay/src/scout/screens/nova_missao.rs, mod.rs, missoes.rs, relatorio.rs (modified)
