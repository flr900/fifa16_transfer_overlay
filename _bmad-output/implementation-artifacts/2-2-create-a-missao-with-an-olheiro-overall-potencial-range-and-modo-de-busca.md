---
baseline_commit: f583960
---

# Story 2.2: Create a Missão with an Olheiro, Overall/Potencial range and Modo de Busca

Status: done

## Story

As Felipe,
I want to configure a Missão and see its cost, time and expected Qualidade before confirming,
so that I know exactly what I'm paying for.

## Acceptance Criteria

1. **Given** at least one Olheiro is "Disponível", **when** I press "Nova Missão" on the Missões tab, **then** the Nova Missão form opens as a vertical list of rows (Olheiro, Overall/Potencial, Modo de Busca) in `bg-panel-raised` with hairline dividers, and pushes onto the navigation stack (UX-DR10, AD-6), **and** only "Disponível" Olheiros are selectable, and those "Em Missão" are visible but disabled.
2. **Given** no Olheiro is hired or available, **when** I open the form, **then** it shows "Nenhum Olheiro disponível." and confirm is disabled.
3. **Given** I change the Olheiro, the Overall/Potencial range (adjusted inline on its row) or the Modo de Busca (Rápida / Completa), **when** the value changes, **then** a fixed footer recalculates cost, estimated time and Qualidade badge synchronously using Story 2.1, without touching `save_repo` (AD-4).
4. **Given** the budget covers the Missão cost, **when** I confirm, **then** the cost is debited via `write_transfer_budget` with read-back (same guarantees as Story 1.5), **and** a `Missao` with UUID v4, filters, Modo de Busca, `criada_em`, `prazo_estimado` and status `Pendente` is persisted write-through, **and** the Olheiro becomes "Em Missão", **and** no search runs at creation time (AD-8).
5. **Given** the budget does not cover the cost, **when** the form is open, **then** confirm is disabled and the exact missing amount is shown (UX-DR21).
6. **Given** the Overall/Potencial range is invalid (min > max), **when** I try to confirm, **then** confirm is disabled with an explanatory message.

## Tasks / Subtasks

- [x] Task 1: Domain (`scout::state`, `scout::quality`, `save_repo::Date`)
  - [x] 1.1 `FaixaAtributo { min, max }` (1–99), `FiltrosMissao { overall, potencial }` (default 50–99 / 50–99), `CampoFaixa`, `RascunhoMissao`, `BloqueioMissao { SemOlheiroDisponivel, FaixaInvalida, OrcamentoInsuficiente }`, `PreviaMissao` (form data recomputed every frame, with `prazo()`).
  - [x] 1.2 `Missao` gains `filtros`, `modo_busca`, `tipo`, `amplitude`, `estimativa`. The estimate is stored at confirmation: the player gets what they paid for even if the balance table changes later.
  - [x] 1.3 Missão type from the ranges (`quality::tipo_por_faixas`): Potencial min ≥ Overall max + 5 → Jovens; else Overall min ≥ 75 → Medalhões; else Geral. Tática comes with the attribute/fit filters (2.8, Epic 3). Geography stays `Mundo` until Story 2.9.
  - [x] 1.4 `Date::mais_dias` / `from_day_number` for `prazo_estimado`.
  - [x] 1.5 Purchase flow shared with Story 1.5: `ErroContratacao` → `ErroCompra` and `ScoutState::comprar(custo, gravar)`, the same compare-and-write + read-back + undo-on-save-failure used for hiring.
  - [x] 1.6 `abrir_nova_missao` (first available Olheiro preselected), `escolher_olheiro_da_missao` (busy ones ignored), `ajustar_faixa_da_missao` (clamped 1–99), `definir_modo_da_missao`, `previa_missao`, `confirmar_nova_missao` (status `Pendente`, no search), `cancelar_nova_missao`, `missoes()`.
- [x] Task 2: Screens
  - [x] 2.1 `screens/componentes.rs` (new): shared buttons (primary / secondary / selected), Tier and Qualidade badges (BAIXA/MÉDIA/ALTA in the Tier colour scale), the navigable card. Olheiros tab and hire modal now use them.
  - [x] 2.2 Missões tab: "Nova Missão" button (always enabled; the form explains when there is no Olheiro), list of Missões (Olheiro + Tier, Qualidade badge, Modo · status · prazo). Progress bars come in Story 2.3.
  - [x] 2.3 Nova Missão form (satellite `NovaMissao`, tab bar still visible):
    - field list: Olheiro as a radio list of cards (busy ones dimmed, "Em Missão", not selectable); Overall and Potencial as inline `[-] value [+]` steppers (hold to repeat; the buttons stop at 1 and 99); Modo de Busca as two toggle buttons with a description;
    - fixed footer: Custo, Prazo ("~N dias de carreira (pronta em dd/mm/aaaa)"), Qualidade badge; the Missão type and whether the Olheiro matches; what the Relatório will bring (players, attributes, ±precision); the blocking/error message; "Confirmar Missão" (or "Tentar novamente") and "Cancelar".
  - [x] 2.4 Navigation: Confirm/Cancel/B go back to the tab; switching tab, closing the panel (F10/combo) or the career stopping being ready discards the draft without saving.
- [x] Task 3: Tests — `cargo test`: 124 passed (new: date + days; type from ranges; form preselects the free Olheiro and skips the busy one; live estimate changes with mode/ranges; clamping and inverted range block; no Olheiro blocks; confirm debits, saves `Pendente` with the paid estimate, Olheiro becomes busy, written to the file; insufficient budget exact shortfall; failed debit saves nothing; B on the form; badge styles; texts).
- [x] Task 4: Manual check in game (Felipe)
  - [x] 4.1 Missões tab: "Nova Missão" opens the form; tab bar and header stay visible.
  - [x] 4.2 The hired Olheiro appears preselected; −/+ on Overall/Potencial (hold repeats) and Rápida/Completa update the footer at once.
  - [x] 4.3 Overall 50–70 with Potencial 80–99 shows "Tipo de Missão: Jovens" (with a Caçador de Jovens: "combina", Qualidade up).
  - [x] 4.4 Confirm: budget in the header drops by exactly the footer cost; the Missão appears in the list; the Olheiros tab shows the Olheiro "Em Missão"; a new form says "Nenhum Olheiro disponível.".
  - [x] 4.5 Controller: D-pad through rows, A on −/+, B goes back without saving.

## Dev Notes

- Defaults chosen without asking (per Felipe's preference): ranges open at 50–99; type thresholds 75 (Medalhões) and +5 (Jovens); the "Nova Missão" button is always enabled and the form explains the block.
- A Missão with an Olheiro that later disappears from the file shows "Olheiro removido" (cannot happen in v1: there is no firing).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.2]
- [Source: EXPERIENCE.md (Formulário Nova Missão, State Patterns), DESIGN.md (button-*, quality badges)]
- [Source: ARCHITECTURE-SPINE.md#AD-4, AD-6, AD-7, AD-8, AD-12]
- [Source: Story 1.5 (purchase guarantees), Story 2.1 (balance table)]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 126 passed; `cargo build --release` with no code warnings (`2.2-v2`).

### Completion Notes List

- Felipe tested `2.2-v1` in game: "funcionou". `2.2-v2`/`v3` confirmed too ("tudo funcionou"); `2.2-v4` improved text only partly ("continuou meio borrado"), so the sharpness work continues separately (FreeType). He asked for two controller changes, done in `2.2-v2`:
  - the left stick now moves focus like the D-pad. In imgui 1.89 the stick only scrolled, so it is converted to D-pad directions for navigation and no longer sent as scrolling (`gamepad::para_navegacao`);
  - on a tab with no satellite screen open, D-pad ←/→ switches tabs like LB/RB. In the form and the modal, ←/→ still move between buttons. Two new tests cover this.

### Change Log

- 2026-10-01: Story 2.2 implemented (Nova Missão form, Missões list, shared purchase flow and components).
- 2026-10-01: `2.2-v2` — stick navigates; D-pad ←/→ switches tabs at the root (Felipe's request).
- 2026-10-01: `2.2-v3` — less transparency (Felipe: the game behind got in the way, especially in the form). `BG_PANEL` 93% → 98%, `BG_PANEL_RAISED` 96% → 100% (cards, modal, banner). It is still within DESIGN.md's ≥ 90%. hudhook draws straight onto the game image (no compositing step), so the game showing through far more than 7% is probably linear-light blending on an sRGB backbuffer (not confirmed).
- 2026-10-01: `2.2-v4` — sharper text (Felipe: fonts and components looked blurry). Not a resolution issue: the game runs at the monitor's native 2560×1080 at 100% DPI, and hudhook uses the swap-chain size. The fonts used `oversample_h: 2` without pixel snapping, so glyphs landed on fractional positions and were softened by texture filtering. Now: `oversample_h/v: 1`, `pixel_snap_h: true`, `rasterizer_multiply: 1.15`. If text is still soft, the next step is imgui's FreeType rasterizer (hinting), which needs the FreeType library installed through vcpkg (not on this machine).

### File List

- fifa_overlay/src/save_repo.rs (modified: `Date::mais_dias`, `from_day_number`)
- fifa_overlay/src/scout/quality.rs (modified: serde on Missão types, `tipo_por_faixas`, `combina`)
- fifa_overlay/src/scout/state.rs (modified: Missão draft/preview/confirm, `ErroCompra`, `comprar`, `missoes()`)
- fifa_overlay/src/scout/persistence.rs (modified: test uses the new `Missao`)
- fifa_overlay/src/scout/mod.rs (modified: B/close discard the draft)
- fifa_overlay/src/scout/screens/componentes.rs (new)
- fifa_overlay/src/scout/screens/nova_missao.rs (new)
- fifa_overlay/src/scout/screens/missoes.rs (rewritten)
- fifa_overlay/src/scout/screens/olheiros.rs, confirmacao_contratacao.rs, mod.rs (modified: shared components, routing)
- fifa_overlay/src/lib.rs (modified: BUILD_TAG, navigation feed)
- fifa_overlay/src/gamepad.rs (modified: `para_navegacao`)
- fifa_overlay/src/scout/screens/theme.rs (modified: panel opacity)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
