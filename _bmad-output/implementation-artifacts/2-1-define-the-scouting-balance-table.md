---
baseline_commit: 9a675cb
---

# Story 2.1: Define the scouting balance table

Status: done

## Story

As Felipe,
I want the cost, duration and Qualidade rules of a Missão defined in one tunable place,
so that I can rebalance scouting after real play without touching UI code.

## Acceptance Criteria

1. **Given** a Tier, an Especialização, a Modo de Busca, a Missão type and a geographic breadth, **when** `scout::quality` estimates a Missão, **then** pure synchronous functions return cost, duration in career days and a Qualidade level (Baixa / Média / Alta) with a revealed-attribute count, precision band and player-count target, **and** all constants live in a single documented balance table in one module, with no magic numbers elsewhere.
2. **Given** the rules, **when** unit tests run, **then** they prove: higher Tier gives higher Qualidade; Completa gives higher Qualidade, fewer players and a longer duration than Rápida; broader geography lowers precision; an Especialização matching the Missão type improves Qualidade, **and** outputs are deterministic for the same inputs.
3. **Given** `scout::quality`, **when** I inspect its dependencies, **then** it calls no `persistence` and no `search` (AD-1, AD-3).

## Tasks / Subtasks

- [x] Task 1: Types
  - [x] 1.1 `state::ModoBusca { Rapida, Completa }` and `state::Qualidade { Baixa, Media, Alta }` (serde `snake_case`, ordered) — they will be persisted in `Missao` / `Relatorio`.
  - [x] 1.2 `quality::TipoMissao { Jovens, Medalhoes, Tatica, Geral }` and `quality::AmplitudeGeografica { Pais, VariosPaises, Continente, Mundo }` — estimation inputs that Stories 2.2/2.9 derive from the chosen filters.
  - [x] 1.3 `PedidoMissao` (inputs) and `EstimativaMissao { custo, duracao_dias, qualidade, atributos_revelados, precisao_mais_menos, alvo_jogadores }`.
- [x] Task 2: The balance table (AC #1) — all in `scout/quality.rs`, documented at the top of the module
  - [x] 2.1 Qualidade score 1–5 = Tier (1/2/3) + 1 if the Especialização matches the type (Caçador de Jovens ↔ Jovens, Caçador de Medalhões ↔ Medalhões, Tático ↔ Tática; Generalista matches none) + 1 for Completa. 1–2 Baixa, 3 Média, 4–5 Alta.
  - [x] 2.2 The score, not only the level, sets revealed attributes (6/10/15/22/29) and the base precision (±10/7/5/3/1). Geography widens precision (+0/+1/+2/+4) and does not change the level.
  - [x] 2.3 Players: Rápida 15 + 2 per point (17–23), Completa 5 + 1 per point (7–10).
  - [x] 2.4 Duration (career days, rounded up) = mode base (Rápida 7 / Completa 21) × geography (100/125/150/200 %) × Tier speed (Júnior 120 / Experiente 100 / Elite 85 %). Range: 6–51 days.
  - [x] 2.5 Cost = mode base (150.000 / 400.000) × geography (100/150/200/300 %) × Tier (100/150/220 %), rounded to 10.000. Range: 150.000–2.640.000. The hire costs from Story 1.4 also live in this module.
- [x] Task 3: Tests (AC #2, #3) — over all 384 input combinations:
  - [x] 3.1 One Tier up: level never drops, more attributes, tighter precision. Júnior → Elite always raises the level.
  - [x] 3.2 Completa vs Rápida: level never lower, more attributes, fewer players, longer, more expensive.
  - [x] 3.3 Broader geography: wider precision, longer, more expensive, same level.
  - [x] 3.4 A matching Especialização: level never lower, more attributes. Generalista matches no type.
  - [x] 3.5 Deterministic and sane ranges for every input; both extremes match the documented numbers.
  - [x] 3.6 Architecture: the non-test code of `quality.rs` mentions no `persistence`, `search` or `save_repo`.

## Dev Notes

- **False positives are not part of v1** (default chosen 2026-10-01; Felipe said "continua"). Qualidade follows the PRD: attribute precision, revealed attributes and number of players. False positives stay in `melhorias-futuras-olheiros.md`.
- For v1, a specialist on the "wrong" Missão type gets no bonus but no penalty either. The scope-change and market penalties are in the future-improvements doc.
- `estimar_missao` is not called yet (Story 2.2 form and Story 2.4 search); it carries `#[allow(dead_code)]` until then.
- Settles the "deferred formulas" item of the Architecture Spine (Qualidade, Missão cost/time). Fit Posicional and similarity come with Epic 3.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.1]
- [Source: prd.md#Glossário (Qualidade, Modo de Busca), FR-4, FR-5, §8 Questões em Aberto #3 and #4]
- [Source: ARCHITECTURE-SPINE.md#AD-1, AD-3, AD-4]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 112 passed (10 in `scout::quality`); `cargo build --release` with no code warnings; clippy clean on `quality.rs`.

### Completion Notes List

- Pure logic, no in-game behaviour yet. Felipe can tune the numbers in `scout/quality.rs` alone.

### Change Log

- 2026-10-01: Story 2.1 implemented (Missão balance table).

### File List

- fifa_overlay/src/scout/quality.rs (modified: Missão estimate and balance table)
- fifa_overlay/src/scout/state.rs (modified: `ModoBusca`, `Qualidade`)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified: 1.1/1.2/epic-1 done, epic-2 and 2.1 started)
- _bmad-output/implementation-artifacts/1-1-…, 1-2-… (modified: status done)
