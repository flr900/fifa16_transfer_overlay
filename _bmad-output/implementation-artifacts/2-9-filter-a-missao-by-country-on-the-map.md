---
baseline_commit: ae0252d
---

# Story 2.9: Filter a Missão by country on the map

Status: done

## Story

As Felipe,
I want to pick the countries to scout by clicking on a map,
so that I can target regions and see how breadth affects quality.

## Acceptance Criteria

1. **Given** the Nova Missão form, **when** I activate the "Filtro geográfico" row, **then** a full-screen Painel de Seleção Geográfica opens with a per-country tile cartogram (one tile per country, grouped by continent, drawn via `ImDrawList`), and the footer summary stays visible (UX-DR12), **and** the cartogram is a reusable component that the Sonar will share.
2. **Given** the map, **when** I click countries, **then** each click toggles inclusion cumulatively, without a modifier key; selected countries are outlined purple with tenuous fill, **and** confirming returns to the form with the selection summarised on the row.
3. **Given** the selection is broad (e.g. all continents) or narrow, **when** I change it, **then** the estimated Qualidade badge and precision estimate update live, broader meaning lower precision (FR4).
4. **Given** a Missão with countries selected, **when** the search runs, **then** `scout::search` keeps only players whose `Crbb.nationid` belongs to the chosen countries.
5. **Given** no country is selected, **when** I confirm, **then** it means "all countries" and the form says so explicitly.
6. **Given** a nation in the save has no tile, **when** the map renders, **then** it falls under an explicit "outros" tile and is never silently dropped.

## Tasks / Subtasks

- [x] Task 1: Domain
  - [x] 1.1 `FiltrosMissao.paises: Vec<u16>` (`serde(default)`; empty = all countries); `search::NACAO_OUTROS` for the "Outros" tile.
  - [x] 1.2 Nations (`Crbb`: name, ISO, confederation) read once in the background (`state.nacoes()`, `CareerSource::read_nations`).
  - [x] 1.3 `quality::amplitude_da_selecao`: none → Mundo; one → País; several of one continent → Vários países, or Continente when it is the whole continent; more than one continent → Mundo. The form's estimate and the saved Missão use it (precision, cost and time follow the Story 2.1 table).
  - [x] 1.4 `alternar_pais_da_missao` (click again removes), `limpar_paises_da_missao`.
  - [x] 1.5 Search: `do_pais` — chosen nations, plus "Outros" = players whose nation is not on the map.
- [x] Task 2: Screens
  - [x] 2.1 `cartograma.rs` (reusable, states Livre / Selecionado / Missão ativa / Missão concluída for the Sonar): one 132×36 tile per nation, grouped by continent (Europa, América do Sul, América do Norte e Central, África, Ásia, Oceania, Outras + the "Outros" tile), sorted by name; each tile one navigable item (D-pad moves between neighbours, A toggles); selected = purple outline + tenuous fill + "•"; tooltip with the full name.
  - [x] 2.2 `selecao_geografica.rs`: "Confirmar", "Limpar", title, "N países selecionados · amplitude: …" or the "all countries" message; the live summary stays at the bottom.
  - [x] 2.3 Form row "Filtro geográfico" ("Todos os países", "Brazil", "Brazil, Argentina e mais 3") and, when empty, "Nenhum país escolhido: o Olheiro procura em todos os países."
- [x] Task 3: Tests — `cargo test`: 173 passed (new: breadth rules; country filter with "Outros"; live breadth/precision in the form; continent grouping; row summary).
- [ ] Task 4: Manual check in game (Felipe): navigate the map with the D-pad, select Brazil only (precision improves), confirm, check the Relatório has only Brazilians.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.9]
- [Source: EXPERIENCE.md (Painel de Seleção Geográfica), UX-DR12, UX-DR17]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-01: Story 2.9 implemented (country filter on the map).

### File List

- fifa_overlay/src/scout/state.rs, quality.rs, search.rs, mod.rs (modified)
- fifa_overlay/src/scout/screens/cartograma.rs, selecao_geografica.rs (new)
- fifa_overlay/src/scout/screens/nova_missao.rs, mod.rs, aviso.rs (modified)
