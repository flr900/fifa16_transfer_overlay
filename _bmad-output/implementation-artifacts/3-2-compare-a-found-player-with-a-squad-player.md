---
baseline_commit: d110eca
---

# Story 3.2: Compare a found player with a squad player

Status: review

## Story

As Felipe,
I want to overlay my starter's radar on the found player's,
so that I can decide whether it's worth replacing him.

## Acceptance Criteria

1. **Given** the Ficha, **when** I look at its actions, **then** a "Comparar com jogador do elenco" button is always visible.
2. **Given** I press it, **when** the squad selector opens, **then** `scout::screens::seletor_elenco` is pushed on the stack with context `ComparacaoFicha`, and its label states that it is for comparison (AD-13), **and** it lists the current squad (~30 players) through `scout::state::listar_elenco_atual()` → `save_repo::read_squad_players()`, never directly from the screen (AD-1).
3. **Given** I choose a squad player, **when** I return to the Ficha, **then** a second radar is overlaid with a dashed green outline, distinct from the purple solid one and legible, and the squad player's name appears in a legend, **and** the rest of the Ficha stays visible (FR10, UJ-3).
4. **Given** an axis exists for the squad player but is unrevealed for the found player, **when** the overlay draws, **then** the found player's axis stays empty, and the squad player's value is still shown.
5. **Given** the squad cannot be read, **when** the selector opens, **then** it shows the read-error state with retry.

## Design decisions (deviations from the epic text, documented in the Architecture Spine)

- **The squad is read in an `AsyncTask`, not synchronously.** The epic and AD-4/AD-13 assumed `read_squad_players` was a cheap memory read of ~30 records. Since Story 2.4 the players come from the active save's `DATA` file on disk, and decoding it takes ~0.5 s: a synchronous call would freeze the game for half a second. `listar_elenco_atual()` starts the read on first use and returns `EstadoElenco::{Carregando, Pronto, Erro}`; the selector shows "Lendo o elenco do save…". The result is tagged with the career it belongs to and re-read once per panel opening (transfers between openings show up). It does not join the AD-9 search queue: it does not block anything and nothing waits for it (AD-10).
- **AD-6 depth goes from 2 to 3 satellites:** Relatórios → Relatório → Ficha → squad selector needs three. `MAX_SATELITES = 3`; a fourth push is still refused with a warning.
- **The comparison is not persisted.** It is reset when another Ficha opens or the Relatório closes. "Tirar comparação" removes it.
- The attribute table of the Ficha gets a third column with the squad player's values while comparing, and lists every radar axis (the squad player's value shows even where the Olheiro observed nothing).

## Tasks / Subtasks

- [x] Task 1: Data and state
  - [x] 1.1 `save_repo::read_squad_players()` (players whose club is `mPrV.clubteamid`), `PlayerPool::elenco()`; `CareerSource::read_squad_players` (default: filter `read_all_players`).
  - [x] 1.2 `JogadorElenco` (exact values), `EstadoElenco`, `listar_elenco_atual()` (the only door, AD-13), `reler_elenco()`; squad sorted by role (GK → defence → midfield → attack), then Overall.
  - [x] 1.3 `comparar_com(Option<u32>)`; `FichaAberta.comparacao`.
- [x] Task 2: Navigation — `Satelite::SeletorElenco(ContextoSeletor)` with `FiltroMissao` / `ComparacaoFicha`; `MAX_SATELITES = 3`.
- [x] Task 3: Screens
  - [x] 3.1 `seletor_elenco.rs`: title and explanation per context, loading / error with "Tentar novamente" / empty states, one card per player ("ATA · 27 anos · OVR 84 · POT 86"), current choice marked "Escolhido" and focused when the screen opens.
  - [x] 3.2 Ficha: "Comparar com jogador do elenco" always visible, "Tirar comparação" while comparing, dashed green polygon on the radar on every axis, legend in text ("Fulano (seu elenco)"), extra column in the attribute table.
- [x] Task 4: Tests — squad read in background (never on the render thread), cached during the opening and re-read on the next; read error then retry; comparison shows on the Ficha and resets with another Ficha; squad values appear on axes the Olheiro did not observe; selector labels per context; stack depth 3 accepted, 4 refused.
- [ ] Task 5: Manual check in game (Felipe): compare with two different squad players, check the dashed outline and the legend, check the error state (e.g. no career file), check that no frame stutter appears when the selector opens.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.2]
- [Source: ARCHITECTURE-SPINE.md AD-4, AD-6, AD-13 (amendments of 2026-10-03)]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 3.2 implemented (squad selector, radar overlay).

### File List

- fifa_overlay/src/save_repo/jogadores.rs, save_repo.rs (modified)
- fifa_overlay/src/scout/state.rs, search.rs, mod.rs (modified)
- fifa_overlay/src/scout/screens/seletor_elenco.rs (new)
- fifa_overlay/src/scout/screens/ficha_jogador.rs, radar.rs, mod.rs (modified)
