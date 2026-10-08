---
baseline_commit: d110eca
---

# Story 3.4: Filter a Missão by Fit Posicional

Status: done

## Story

As Felipe,
I want to search for players who could play a different position than their listed one,
so that I find, say, a midfielder who can work as a holding player.

## Acceptance Criteria

1. **Given** the Nova Missão form, **when** I activate the "Fit Posicional" row, **then** a full-screen field panel lists the target positions, with the footer summary visible (UX-DR11), and choosing one returns to the form.
2. **Given** a target position, **when** `scout::quality` computes fit, **then** it compares the player's attribute profile with a documented ideal profile for that position and returns a strength score; the ideal profiles are in one place in the balance table (FR6, AD-3), **and** the formula is unit tested with a representative case (an attacking midfielder with high defensive attributes scores high for holding midfielder).
3. **Given** a Missão with Fit Posicional, **when** the search runs, **then** `scout::search` keeps players whose fit for the target position exceeds the threshold and whose native position is different from the target, **and** it is combinable (AND) with every other filter.
4. **Given** a Tático Olheiro vs. a non-Tático one on this Missão type, **when** the estimate is computed, **then** the Tático Olheiro yields the higher Qualidade (FR6, Story 2.1 rule), **and** the form footer reflects it live.

## Design decisions

- **14 target positions** (`quality::PosicaoAlvo`): Zagueiro, laterals, wing-backs, Volante, Meio-campista, Meia-atacante, wide midfielders, wingers, Segundo atacante, Centroavante. Goalkeeper is left out. Mirrored sides share a profile; each target knows which `preferredposition1` codes already are that position (those players are excluded: it is their position, not a fit).
- **Ideal profiles** (`quality::PERFIS`, one table, integer weights summing to 100, checked in a test): 11 profiles (GK, CB, full-back, wing-back, CDM, CM, CAM, wide mid, winger, CF, ST), in the spirit of FIFA's own per-position rating.
- **Fit strength** = `rating in the target profile ÷ rating in the native-position profile`, in %, capped at 100. It measures shape, not level (the Overall filter handles level).
- **Threshold `LIMIAR_FIT = 95`** (loses at most ~5% of his level at the target). Calibrated on Felipe's save (2026-10-03, Overall ≥ 60, other positions): ~15% pass for Centroavante/Zagueiro/Volante, up to ~65% for wide midfielders and wingers, because neighbouring positions have similar profiles. 92 let half the database through. Candidates are ranked by their rating in the target profile, so the best performers there come first.
- With a target position, the Olheiro **observes the target profile's heaviest attributes first** (after a requested dominant attribute), so the fit shown in the Relatório is computed on the attributes that matter.
- A target position makes the Missão **Tática** → the Tático gets the bonus; the form footer updates live (same rule as Story 2.8).

## Tasks / Subtasks

- [x] Task 1: Domain — `PosicaoAlvo` (`serde` snake_case, `nome`, `sigla`, `posicoes_nativas`, `perfil`), `Perfil`, `PERFIS`, `perfil_da_posicao`, `nota_no_perfil` (weights of unobserved attributes leave the sum), `forca_fit`, `LIMIAR_FIT`; `FiltrosMissao.fit_posicional` (`serde(default)`); `definir_fit_da_missao`; `ordem_de_observacao(…, alvo)`.
- [x] Task 2: Search — `serve_no_alvo` (different native position AND strength ≥ threshold), AND with every filter, relevance with the target-profile rating; `revelar` stores the fit from the revealed ranges.
- [x] Task 3: Screens — form row "Fit Posicional" (value "Volante (VOL)" or "Nenhum") → `campo_fit.rs` (Voltar, explanation, "Nenhum", Defesa / Meio-campo / Ataque groups, live summary; focus starts on the current choice).
- [x] Task 4: Tests — every profile sums to 100 with no repeated attribute and no GK attribute outside the GK profile; every native position has a profile and every target's native codes map to its profile; CAM with high defence ≥ threshold as Volante, the same CAM without defence below it, a pure striker far from Zagueiro; cap at 100 and unobserved attributes ignored; filter keeps other positions only and combines with Overall; Tático beats the other three Especializações on this Missão type; target attributes observed first.
- [ ] Task 5: Manual check in game (Felipe): run a Volante fit Missão and a Lateral one; check the names make sense and the threshold is not too loose or too strict.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.4]
- [Source: ARCHITECTURE-SPINE.md AD-3]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 3.4 implemented (Fit Posicional filter, ideal profiles).

### File List

- fifa_overlay/src/scout/quality.rs, search.rs, state.rs, mod.rs (modified)
- fifa_overlay/src/scout/screens/campo_fit.rs (new)
- fifa_overlay/src/scout/screens/nova_missao.rs, mod.rs (modified)
