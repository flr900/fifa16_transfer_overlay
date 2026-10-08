---
baseline_commit: d110eca
---

# Story 3.5: See Fit Posicional in the Relatório

Status: done

## Story

As Felipe,
I want to see each player's native position, target position and fit strength,
so that I understand why he was suggested.

## Acceptance Criteria

1. **Given** a Relatório from a Fit Posicional Missão, **when** the Tabular view renders, **then** a Fit Posicional column sits after the native position, showing the target position and strength (UX-DR14).
2. **Given** the Cards view, **when** a card renders, **then** an extra badge on the native-position line shows the target position and fit strength, outline plus tenuous fill, with text and never colour alone (UX-DR5).
3. **Given** the Ficha, **when** it renders for such a player, **then** the header shows native position plus the Fit Posicional.
4. **Given** a Relatório without Fit Posicional, **when** it renders, **then** no fit column or badge appears.

## Design decisions

- The value is "VOL 96%" (target code + strength); "≈" below Qualidade Alta, as for similarity (Story 3.3), because it comes from the revealed ranges. A strength that cannot be computed (no target attribute revealed) shows "—".
- Tabular: column "Fit" right after "Pos" (and before "Sim." when the Missão also has a reference), header tooltip "Fit Posicional: força do perfil dele como Volante". Cards: purple "FIT VOL 96%" badge right after "22 anos · MEI · Brazil"; the line is cut earlier to make room. Ficha: same badge after "22 anos · MEI · Pé esquerdo".
- `componentes::desenhar_badge_texto` draws a badge with a text built at run time (the existing badges have fixed texts).

## Tasks / Subtasks

- [x] Task 1: `relatorio.rs` — `PerfilPedido` (what the Missão asked: target, reference name, approximate), `colunas_fixas`, `texto_fit`, `texto_percentual`, `aproximado`; Fit column; Cards badge; header detail "fit em Volante".
- [x] Task 2: Ficha header badge; Missão card detail "fit em VOL".
- [x] Task 3: `componentes::badge_fit`, `desenhar_badge_texto`.
- [x] Task 4: Tests — column order with fit, with reference, with both, with neither; "≈" only below Alta; "VOL 96%".
- [ ] Task 5: Manual check in game (Felipe): Tabular column, Cards badge, Ficha header, and a Relatório without fit (no column).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.5]
- [Source: DESIGN.md, UX-DR5, UX-DR14]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: Story 3.5 implemented (Fit Posicional in Tabular, Cards and Ficha).

### File List

- fifa_overlay/src/scout/screens/relatorio.rs, ficha_jogador.rs, componentes.rs, missoes.rs (modified)
