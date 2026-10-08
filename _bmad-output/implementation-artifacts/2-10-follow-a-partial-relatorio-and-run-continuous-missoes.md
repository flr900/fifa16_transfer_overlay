---
baseline_commit: 7dfdcad
---

# Story 2.10: Follow a partial Relatório and run continuous Missões

Status: done

## Story

As Felipe,
I want to see the players an Olheiro has already found while the Missão is still running, keep an Olheiro on an open-ended search, and be told when the Relatório gets new names,
so that scouting feels like a living process instead of a single delivery at the deadline.

## Acceptance Criteria

1. **Given** a Missão in progress, **when** I open it, **then** a partial Relatório shows the players found so far, and more appear as career days pass, **and** the number shown follows the Missão progress, up to the Relatório target at the deadline. The precision and revealed attributes of each player follow the Missão Qualidade (Story 2.1).
2. **Given** I create a Missão, **when** I choose "sem prazo" (continuous), **then** the Olheiro stays "Em Missão" on that search until I cancel it, **and** new players keep being added over time, **and** cancelling frees the Olheiro and keeps the players found so far as a Relatório.
3. **Given** a Relatório received new players since I last looked, **when** the panel is closed, **then** the top-right banner (Story 1.7) says so, e.g. "Relatório atualizado: +2 jogadores · Missão Jovens", for 3 seconds, without taking input from the game.

## Design decisions (defaults proposed when the story was added, kept)

- **AD-8 amended** (Architecture Spine): the search runs at the first panel opening after the Missão is created (or renewed), not at the deadline. Found players are stored in a deterministic discovery order (shuffled by the Missão id) and revealed as `ceil(progress × target)`, with the progress taken from the date saved when the panel opened (FR-8: no polling).
- **Continuous Missão = prepaid blocks of 30 career days.** A block costs the same as the fixed Missão with the same filters and brings the same number of players, spread over the 30 days. At the end of a block the Missão waits: "Renovar por X" (an explicit purchase with the Story 1.5 guarantees; never an automatic charge, FR-3/NFR1) or "Encerrar Missão" (the players already revealed become the final Relatório; the ones not yet revealed are dropped; the Olheiro is free).
- **Banner** while the panel is closed: checked on the existing 1 s re-read, only when the career date changes; each Relatório remembers what was already announced (`notificados`), so a player is announced once.

## Tasks / Subtasks

- [x] Task 1: Domain
  - [x] 1.1 `Missao`: `continua`, `blocos` (default 1), `blocos_buscados` (all `serde(default)`); `alvo_total()`, `revelados(encontrados, hoje)`.
  - [x] 1.2 `Relatorio`: `vistos` (the "novo" comes back when the partial Relatório grows), `notificados` (banner).
  - [x] 1.3 `quality::revelados`, `quality::DIAS_BLOCO_CONTINUO`.
  - [x] 1.4 Dispatch (`despachar_missoes`): search for Missões that have not searched for their paid blocks (EmExecucao first, FIFO, one at a time); fixed Missão with search done and deadline passed → Concluída.
  - [x] 1.5 `search::executar_missao` returns only the NEW players (excluding the ones already in the Relatório), in discovery order; the state appends them.
  - [x] 1.6 Lists show only revealed players (`RelatorioNaLista { previstos, parcial, novo }`, `MissaoNaLista { revelados, previstos }`); opening marks `vistos`/`notificados`.
  - [x] 1.7 `renovar_missao` (purchase + one more block, search right away), `encerrar_missao`, `bloco_encerrado`, `erro_da_missao`.
  - [x] 1.8 `avisar_jogadores_novos` + `TipoAviso::RelatorioAtualizado`.
- [x] Task 2: Screens
  - [x] 2.1 Form row "Duração": Prazo fixo / Contínua with explanation; footer shows "X por bloco", "blocos de 30 dias (o 1º termina em …)", "até N jogadores por bloco".
  - [x] 2.2 Missões: partial text "Relatório parcial: 9 de 17. Relatório pronto em ~5 dias…"; the card opens the partial Relatório as soon as one player is revealed; NOVO when it grows; continuous Missões show "Contínua · bloco 2 até … · N jogadores até agora" or "Bloco 1 encerrado …" with "Renovar por X" (enabled only at the end of the block and with enough budget; missing amount shown) and "Encerrar Missão".
  - [x] 2.3 Relatórios/Relatório: "parcial: N de M jogadores", notice "Relatório parcial: mais jogadores aparecem conforme os dias de carreira passam."
  - [x] 2.4 Banner "Relatório atualizado: +2 jogadores · Missão Jovens".
- [x] Task 3: Tests — `cargo test`: 177 passed (new: reveal formula; partial grows with the days and keeps the first players; deadline concludes; continuous: no automatic charge, renewal debits once and searches new players without repeating, ending keeps only the revealed ones; banner once per new player; updated 2.4 tests for the early search).
- [ ] Task 4: Manual check in game (Felipe): create a Missão, advance a few days and save, reopen the panel (partial players); advance with the panel closed (banner); create a continuous Missão, reach the end of the block, renew and end it.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.10]
- [Source: ARCHITECTURE-SPINE.md AD-8 (amendment)]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-01: Story 2.10 implemented (partial Relatório, continuous Missões, update banner).

### File List

- _bmad-output/planning-artifacts/architecture/architecture-FIFA_EDITOR-2026-09-22/ARCHITECTURE-SPINE.md (AD-8 amendment)
- _bmad-output/planning-artifacts/epics.md (2.10 note)
- fifa_overlay/src/scout/state.rs, quality.rs, search.rs (modified)
- fifa_overlay/src/scout/screens/missoes.rs, relatorios.rs, relatorio.rs, nova_missao.rs, aviso.rs (modified)
