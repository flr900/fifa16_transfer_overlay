---
baseline_commit: a73e169
---

# Story 1.4: Browse the Olheiros available to hire

Status: done

## Story

As Felipe,
I want to see every Especialização × Tier Olheiro with its hire cost,
so that I can decide whom to hire before spending any budget.

## Acceptance Criteria

1. **Given** the Olheiros tab is open, **when** it renders, **then** it lists the 12 combinations (Caçador de Jovens, Caçador de Medalhões, Tático, Generalista × Júnior, Experiente, Elite), each as a card with its Especialização, a Tier badge and its hire cost, **and** each combination has a distinct cost from a balancing table kept in one place in code.
2. **Given** a Tier badge or status indicator, **when** it renders, **then** it shows JR / EXP / ELITE as outline plus tenuous fill, in grey / purple / gold, always with text and never colour alone (UX-DR5, NFR5), **and** the available Olheiros are shown as hireable.
3. **Given** no Olheiro has been hired, **when** the tab renders, **then** it shows "Nenhum Olheiro contratado ainda." with the hire list visible below it.
4. **Given** the current budget is shown, **when** the tab opens, **then** it displays the `transferbudget` read through `scout::state` and `save_repo`, not from the screen directly.
5. **Given** a card is hovered or focused, **when** it renders, **then** it shows a purple border, and the hire button is at least 32px tall (UX-DR20).

## Tasks / Subtasks

- [x] Task 1: Balance table (AC #1) — `scout::quality` (new; Story 2.1 adds the Missão rules here)
  - [x] 1.1 `custo_contratacao(Especializacao, Tier)`: one explicit 12-arm table. Tier weighs more than Especialização (Júnior 0.3–0.5 M, Experiente 1.2–1.9 M, Elite 3.6–5.8 M); within a Tier: Generalista < Caçador de Jovens < Tático < Caçador de Medalhões. First version, to tune by playing.
  - [x] 1.2 Tests: 12 distinct positive costs; every higher Tier costs more than any lower one; Generalista Júnior is the cheapest.
- [x] Task 2: Domain (AC #1, #3, #4) — `scout::state`
  - [x] 2.1 `Especializacao` and `Tier` enums (serde `snake_case`, PRD order, names from the glossary); `Olheiro` now carries `especializacao` and `tier`.
  - [x] 2.2 `ofertas_de_olheiros(orcamento)`: 12 offers in Tier order, each with its cost and how much is missing from the live budget. There is no slot limit in v1 (FR-2 assumption), so all 12 always show.
  - [x] 2.3 `ScoutState::{orcamento, olheiros_disponiveis, olheiros_contratados}`. Hired Olheiros are read from the career file. `em_missao` is true for an Olheiro with a non-concluded Missão (AD-8).
- [x] Task 3: Olheiros tab (AC #1–#5) — `scout::screens::olheiros`
  - [x] 3.1 Sections "Contratados" (empty state text) and "Disponíveis para contratação". Each card has an initials avatar, name and Tier badge, a short description, and on the right the cost (mono) with a "Contratar" button (132 × 32 px) or, for hired ones, a status dot with "Disponível" / "Em Missão".
  - [x] 3.2 Card background and border are drawn after the content (draw-list channels), so the border knows about hover or keyboard/gamepad focus: purple then, hairline otherwise.
  - [x] 3.3 Insufficient budget: grey button that does nothing, "faltam X" in red instead of the "custo" label, and a tooltip "Orçamento insuficiente: faltam X.".
  - [x] 3.4 Theme: tier colours (`TIER_JUNIOR` outline, with lighter text per mockup v2), `WARNING`, disabled-button fill, and a new Oswald 600 13 px `badge` font.
  - [x] 3.5 "Contratar" only logs the request for now; the confirmation modal and the budget write are Story 1.5.
- [x] Task 4: Tests — `cargo test`: 85 passed (new: balance table ×3, offers/shortfall, hired list from file with mission status, badges/initials/microcopy).
- [x] Task 5: Manual check in game (Felipe)
  - [x] 5.1 F10 → Olheiros: "Nenhum Olheiro contratado ainda." and below it 12 cards (JR row, EXP row, ELITE row), each with initials, name, badge JR/EXP/ELITE (grey/purple/gold outline), description, cost and "Contratar".
  - [x] 5.2 Hover a card: purple border. Mouse wheel scrolls the list; switching tabs keeps the scroll.
  - [x] 5.3 Costs above the live budget (if any): grey button, "faltam X" in red, tooltip with the exact amount.
  - [x] 5.4 Click "Contratar": nothing changes on screen (log line `[scout::olheiros] Contratar … pedido`).

## Dev Notes

- Differences from the mockup on purpose: section labels and buttons in normal case (DESIGN.md: caps only for badges), and amounts without a currency symbol (the game's currency is not mapped; same decision as the header in Story 1.2).
- No slot limit: the same combination can be hired more than once (FR-2 `[ASSUMPTION]`), so the offer list never shrinks.
- `Tier::nome()` (full name) is for the Story 1.5 confirmation modal.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.4]
- [Source: prd.md#FR-2, Glossário]
- [Source: ux-designs/.../mockups/olheiros.html (v2), DESIGN.md (tier-badge-*, button-*), EXPERIENCE.md (Card de Olheiro, State Patterns)]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 85 passed; `cargo build --release` with no code warnings (`1.4-v1`); clippy clean on the new files.

### Completion Notes List

- Felipe in game (2026-10-01): "Tudo funcionou perfeito". The injector and its window now close with the game.
- Felipe's ideas for later (custom names, nationality, star attributes, market specialities with adaptation, spawn by club popularity/rarity, Missão budget range, Qualidade with false positives) are recorded out of v1 scope in `_bmad-output/planning-artifacts/melhorias-futuras-olheiros.md`.

### Change Log

- 2026-10-01: Story 1.4 implemented (Olheiros tab, balance table).
- 2026-10-01: Story 1.7 follow-up requested by Felipe: `fifa_injector --aguardar` now exits when the game closes, and the `iniciar_fifa.ps1` window closes with it. It stays open only on an error, waiting for Enter. Before this, both kept waiting for the game to reopen until the FIFA Friends server closed.

### File List

- fifa_overlay/src/scout/quality.rs (new)
- fifa_overlay/src/scout/state.rs (modified)
- fifa_overlay/src/scout/persistence.rs (modified: tests use the new `Olheiro` fields)
- fifa_overlay/src/scout/mod.rs (modified: `mod quality`)
- fifa_overlay/src/scout/screens/olheiros.rs (rewritten)
- fifa_overlay/src/scout/screens/mod.rs (modified)
- fifa_overlay/src/scout/screens/theme.rs (modified)
- fifa_overlay/src/lib.rs (modified: BUILD_TAG)
- fifa_injector/src/main.rs (modified: exits when the game closes — Story 1.7 follow-up)
- fifa_overlay/iniciar_fifa.ps1 (modified: no -NoExit, pauses only on error)
- _bmad-output/planning-artifacts/epics.md, 1-7 story file (modified: AC #1 of Story 1.7)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
- _bmad-output/planning-artifacts/melhorias-futuras-olheiros.md (new: post-v1 ideas)
- _bmad-output/planning-artifacts/epics.md (modified: "Future improvements" section)
