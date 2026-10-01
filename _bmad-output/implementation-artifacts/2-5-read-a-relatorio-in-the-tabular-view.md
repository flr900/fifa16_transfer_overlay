---
baseline_commit: 81ba285
---

# Story 2.5: Read a Relatório in the Tabular view

Status: review

## Story

As Felipe,
I want to open a finished Relatório and read the players found,
so that I can pick who to go and negotiate for in the game.

## Acceptance Criteria

1. **Given** a Missão is `Concluida` and its Relatório has not been opened, **when** I open the Missões or Relatórios tab, **then** a "novo" indicator is shown on it until I open it the first time.
2. **Given** I open a Relatório, **when** the Tabular view renders, **then** each row shows name → age → native position → numeric attribute columns in Consolas (mono) aligned digit by digit, with dividers and no cards (UX-DR14), **and** long text is truncated with an ellipsis and a full tooltip on hover or focus (NFR5).
3. **Given** the Relatório has low Qualidade, **when** it renders, **then** values appear as ranges (e.g. "Overall: 65-78") and only a subset of attributes is revealed, never invented values, **and** a Qualidade badge (Baixa / Média / Alta) is shown, always with text.
4. **Given** the Relatório has high Qualidade, **when** it renders, **then** values are exact or near-exact and the full attribute profile is revealed.
5. **Given** no Relatório exists, **when** I open the Relatórios tab, **then** it shows an empty state.

## Tasks / Subtasks

- [x] Task 1: Domain (`scout::state`)
  - [x] 1.1 `relatorios(arquivados)` → `RelatorioNaLista { relatorio, missao, olheiro }`, newest first.
  - [x] 1.2 `abrir_relatorio(id)` saves `aberto = true` (the "novo" goes away, also after a restart); `relatorio_aberto()`, `fechar_relatorio()` (also on panel close).
  - [x] 1.3 `MissaoNaLista` gains `relatorio_id` and `relatorio_novo`.
- [x] Task 2: Screens
  - [x] 2.1 New satellite screen `Satelite::Relatorio` (AD-6), opened from a Missão card (Concluída) or a Relatórios card; B / "Voltar" goes back.
  - [x] 2.2 `relatorio.rs`: header (title "Relatório · Missão Jovens", Qualidade and Tier badges, "Olheiro · Modo · gerado em … · N jogadores · precisão de ±P"); low-Qualidade notice; table with frozen name column and header, scroll on both axes, hairline row dividers, subtle alternating rows.
  - [x] 2.3 Columns: Nome, Idade, Pos (Portuguese codes: GOL, ZAG, MC, ATA…), Nação, Clube, OVR, POT, then one column per revealed attribute (3-letter code, full name in the header tooltip). Numbers in Consolas, right-aligned. "72" exact, "65–78" range, "—" not observed.
  - [x] 2.4 The whole row is one focusable item (mouse and gamepad); name/nation/club cut with "…" show the full text in a tooltip.
  - [x] 2.5 `relatorios.rs`: one card per Relatório (title, Tier, NOVO, Qualidade, "Olheiro · N jogadores · gerado em … · Modo"); empty state.
  - [x] 2.6 Missões cards: NOVO badge; Concluída text "ative para abrir o Relatório".
  - [x] 2.7 `imgui` feature `tables-api` turned on.
- [x] Task 3: Tests — `cargo test`: 156 passed (new: opening clears "novo" and persists; range/exact formatting; union of revealed columns in fixed order; best-first row order; ellipsis cut; card details).
- [ ] Task 4: Manual check in game (Felipe): open a Relatório from Missões and from Relatórios, scroll sideways with the stick/D-pad, check the tooltip of a long name.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.5]
- [Source: EXPERIENCE.md (Relatórios, State Patterns "Relatório de baixa Qualidade"), DESIGN.md (table rows, mono)]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 156 passed.

### Completion Notes List

### Change Log

- 2026-10-01: Story 2.5 implemented (Relatório Tabular view).

### File List

- fifa_overlay/Cargo.toml (modified: imgui `tables-api`)
- fifa_overlay/src/scout/state.rs (modified)
- fifa_overlay/src/scout/mod.rs (modified: `Satelite::Relatorio`)
- fifa_overlay/src/scout/screens/mod.rs (modified: routing)
- fifa_overlay/src/scout/screens/relatorio.rs (new)
- fifa_overlay/src/scout/screens/relatorios.rs (rewritten)
- fifa_overlay/src/scout/screens/missoes.rs (modified)
- fifa_overlay/src/scout/screens/componentes.rs (modified: NOVO badge)
- fifa_overlay/src/scout/screens/theme.rs (modified: tokens)
