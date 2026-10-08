---
baseline_commit: d110eca
---

# Story 3.1: Open a player's Ficha with the Radar de Atributos

Status: done

## Story

As Felipe,
I want to tap a player in a Relatório and see a complete profile with a radar,
so that I judge his shape at a glance.

## Acceptance Criteria

1. **Given** a Relatório in either Tabular or Cards view, **when** I click a player, **then** the Ficha de Jogador opens as a satellite screen pushed on the navigation stack, and back (or gamepad cancel) returns to the same Relatório scroll position (AD-6).
2. **Given** the Ficha, **when** it renders, **then** one single screen shows the biographic header (name, age, native position, preferred foot, nationality), the full list of revealed numeric attributes, and the Radar de Atributos, with no sub-tabs (UX-DR15), **and** the miniface appears as in Story 2.6.
3. **Given** the Radar, **when** it draws via `ImDrawList`, **then** it has one axis per revealed attribute, a solid purple outline for the found player (UX-DR16), **and** unrevealed axes are dotted and empty, never zero and never invented (FR10).
4. **Given** a low-Qualidade Relatório, **when** I open a Ficha, **then** ranges are shown as ranges, and the radar plots only the axes that have values.
5. **Given** the Ficha is open for a Relatório that has been viewed, **when** I look at its actions, **then** "Arquivar" from Story 2.7 is also reachable here.

## Design decisions

- **Radar axes:** the 28 outfield attributes (a goalkeeper gets the 5 GK attributes plus Reação, Agilidade, Impulsão and Força), plus any other attribute the Olheiro revealed, in the fixed `Atributo::TODOS` order. Every revealed attribute has an axis; unrevealed ones are drawn dotted and empty, with a dimmed label.
- **Ranges on the radar:** the solid purple line goes through the middle of each revealed range, and the range itself is a thicker translucent stroke on the axis from min to max. The line only joins neighbouring revealed axes; it never crosses an unrevealed one.
- **Preferred foot** comes from `CZUM.preferredfoot` (`MDvm`, 1 = right, 2 = left). It is stored in `JogadorEncontrado.pe` (`serde(default)`): Relatórios from before this story show no foot.
- **Archiving from the Ficha** closes the Ficha and the Relatório and returns to the tab, where the Relatório is now under "Arquivados".

## Tasks / Subtasks

- [x] Task 1: Data
  - [x] 1.1 `PlayerRaw.pe` (`save_repo::Pe`), read from `MDvm`; checked against the save in the oracle test (left-footed players exist and are a minority).
  - [x] 1.2 `JogadorEncontrado.pe` (`serde(default)`), filled by `search::revelar`; `valor_visto` (middle of the revealed range).
- [x] Task 2: State
  - [x] 2.1 `abrir_ficha(player_id)` (only for a player of the open Relatório), `fechar_ficha`, `ficha_aberta() -> FichaAberta`; closing the Relatório closes the Ficha.
  - [x] 2.2 `Scout::voltar` (B) closes the Ficha.
- [x] Task 3: Screens
  - [x] 3.1 Relatório: a row (Tabular selectable spanning all columns) or a card (Cards) opens the Ficha, by mouse or A.
  - [x] 3.2 `ficha_jogador.rs`: "Voltar", "Comparar com jogador do elenco" (Story 3.2), "Arquivar" when allowed; left column with miniface (silhouette fallback), name, "22 anos · MEI · Pé esquerdo", nation · club, OVR/POT ranges, Qualidade baixa notice, and the revealed-attribute table ("Observados: 15 de 28 atributos"); right column with the radar and a text legend.
  - [x] 3.3 `radar.rs`: rings at 25/50/75/99, solid axes when revealed, dotted when not, labels with the attribute codes, range strokes, the mid-range line in neighbouring segments.
- [x] Task 4: Tests — radar axes (28 outfield; GK set + extra revealed axes; unrevealed is `None`, not zero), segments never jump an unrevealed axis, axis geometry; bio line with and without the foot; Ficha opens only for a player of the open Relatório and closes with it; B goes back Seletor → Ficha → Relatório; old JSON without the new fields still loads.
- [ ] Task 5: Manual check in game (Felipe): open a Relatório in both views, open a Ficha with the mouse and with A, go back with B and check the scroll position, check the radar for a low and a high Qualidade Relatório, archive from the Ficha.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.1]
- [Source: DESIGN.md / EXPERIENCE.md, UX-DR15, UX-DR16]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 3.1 implemented (Ficha de Jogador, Radar de Atributos).

### File List

- fifa_overlay/src/save_repo/jogadores.rs, save_repo.rs (modified)
- fifa_overlay/src/scout/state.rs, search.rs, mod.rs (modified)
- fifa_overlay/src/scout/screens/ficha_jogador.rs, radar.rs (new)
- fifa_overlay/src/scout/screens/relatorio.rs, mod.rs, theme.rs (modified)
