---
baseline_commit: d110eca
---

# Story 3.3: Filter a Missão by Jogador de Referência

Status: done

## Story

As Felipe,
I want to find players whose profile resembles one of my squad players,
so that I scout "another one like him".

## Acceptance Criteria

1. **Given** the Nova Missão form, **when** I activate the "Jogador de Referência" row, **then** the same `seletor_elenco` opens, now with context `FiltroMissao` and a label stating it guides the search (AD-13), full screen with the footer summary still visible (UX-DR11), **and** choosing a player returns to the form with the row showing his name.
2. **Given** a Missão with a reference player, **when** the search runs, **then** `scout::quality` computes a profile similarity between each candidate and the reference, and `scout::search` keeps candidates above a documented threshold, ranked by similarity (AD-3), **and** the formula is documented and unit tested: identical profiles score 100%, and an unrelated profile scores low.
3. **Given** a Relatório from such a Missão, **when** it renders, **then** each player shows a similarity indicator (%) with the reference player's name, in both Tabular and Cards views (FR7).
4. **Given** a low-Qualidade Relatório, **when** similarity is shown, **then** it is marked approximate, computed from the revealed attributes only.
5. **Given** this filter is combined with Overall/Potencial, dominant attribute and geographic filters, **when** the search runs, **then** all apply together (FR4).

## Design decisions

- **Formula** (`quality::similaridade`, documented at the top of `quality.rs`): `0.75 × shape + 0.25 × level`, over the 28 outfield attributes (or the 5 GK attributes plus Reação, Agilidade, Impulsão, Força when the reference is a goalkeeper). Shape = `100 − 4 × mean absolute difference after removing each player's own mean`, so the same profile at a lower level is still "alike"; level = `100 − 4 × difference of the means`. Identical profiles score 100.
- **Threshold `LIMIAR_SIMILARIDADE = 75`**, calibrated on Felipe's save (2026-10-03, players with Overall ≥ 60): 30 to 3,300 players pass per reference (centre-backs have very uniform profiles; strikers, goalkeepers and playmakers far fewer), and the closest match scores 81–92%. The Overall/Potencial filter narrows it further.
- **The reference is a snapshot**: `FiltrosMissao.referencia` stores the player's id, name, position and all 33 attributes at confirmation time, so the search still works if he leaves the squad, and the Missão searches for "him as he was when I asked".
- **Filtering uses real values** (the Olheiro knows what he is looking for); **what the Relatório shows is recomputed from the revealed ranges only** (middle of each range, revealed axes only) and stored in `JogadorEncontrado.similaridade`. It is marked "≈" for every Qualidade below Alta, not only Baixa: Média also reveals only part of the profile with ranges.
- **Ranking:** the candidate's relevance is the mean of the requested profile criteria (dominant attribute value, similarity, target-position rating), so combining filters still ranks by all of them.
- A reference makes the Missão **Tática** (Tático Olheiro → Qualidade bonus).

## Tasks / Subtasks

- [x] Task 1: Domain
  - [x] 1.1 `JogadorReferencia` and `FiltrosMissao.referencia` (`serde(default)`); `definir_referencia_da_missao(Option<u32>)` (only squad players; `None` = no filter).
  - [x] 1.2 `quality::similaridade`, `atributos_comparados`, `LIMIAR_SIMILARIDADE`; `tipo_por_filtros(&FiltrosMissao)` (Tática with any profile filter); `relevancia(…, perfil: &[u8])`.
  - [x] 1.3 `search::passa_nos_filtros` (AND), `similaridade_real`, `notas_de_perfil`; `revelar` stores the similarity from the revealed ranges.
- [x] Task 2: Screens
  - [x] 2.1 Form row "Jogador de Referência" (value: name or "Nenhum") → `SeletorElenco(FiltroMissao)` with "Nenhum", the explanation, and the live summary.
  - [x] 2.2 Relatório: "Sim." column (header tooltip "Similaridade com Fulano") in Tabular; "SIM ≈87%" on the Overall line and a tooltip in Cards; header detail "parecidos com Fulano"; Missão card "como Fulano"; Ficha line "Similaridade com Fulano: ≈87%".
- [x] Task 3: Tests — identical = 100, unrelated < 50, same shape 8 points lower passes the threshold, too few common axes = no value; filter AND fit; ranking puts the closest profile first; Relatório stores the similarity from revealed values; reference snapshot is saved with the Missão and survives a restart; Tática + Tático bonus. Calibration test against the versioned save (`cargo test --release calibracao -- --ignored --nocapture`).
- [ ] Task 4: Manual check in game (Felipe): pick a starter as reference, run a Missão, check that the names look like him and that "≈" disappears in an Alta Relatório.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.3]
- [Source: ARCHITECTURE-SPINE.md AD-3, AD-13]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 3.3 implemented (Jogador de Referência filter and similarity).

### File List

- fifa_overlay/src/scout/quality.rs, search.rs, state.rs (modified)
- fifa_overlay/src/scout/screens/seletor_elenco.rs (new)
- fifa_overlay/src/scout/screens/nova_missao.rs, relatorio.rs, missoes.rs, ficha_jogador.rs (modified)
