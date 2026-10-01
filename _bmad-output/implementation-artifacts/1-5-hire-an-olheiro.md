---
baseline_commit: 98641f7
---

# Story 1.5: Hire an Olheiro

Status: done

## Story

As Felipe,
I want to confirm the hiring of an Olheiro and have its cost debited from my transfer budget,
so that I have a scout ready to receive a Missão.

## Acceptance Criteria

1. **Given** the budget covers the cost, **when** I press "Contratar" on a card, **then** a confirmation modal shows the exact cost and the resulting budget, with a primary (green) confirm button and a secondary cancel button.
2. **Given** the confirmation modal is open, **when** I press confirm, **then** `save_repo::write_transfer_budget` writes the new value, which is only ever done on explicit confirmation, **and** the value is read back and the panel shows the confirmed balance, **and** a new `Olheiro` with a UUID v4, its Especialização and its Tier is persisted write-through with status "Disponível", **and** it appears immediately in the hired list.
3. **Given** the budget is lower than the cost, **when** the modal opens, **then** the confirm button is disabled, **and** the text shows the exact missing amount, e.g. "Orçamento insuficiente: faltam 2.100.000.", with no exclamation marks or emoji (UX-DR9, UX-DR21).
4. **Given** the write fails or the read-back does not match the expected value, **when** I confirm, **then** no Olheiro is persisted, **and** the panel shows a clear error with retry, and never claims success.
5. **Given** the feature hires an Olheiro, **when** I inspect what it writes to the save, **then** `dqXv.transferbudget` is the only save field written (NFR1).

## Tasks / Subtasks

- [x] Task 1: `save_repo::write_transfer_budget(anterior, novo)` (AC #2, #4, #5)
  - [x] 1.1 Compare-and-write on the cached live struct: re-validates the season signature, writes only if the live value is still `anterior` (else `OrcamentoMudou(atual)`, nothing written), writes the 4 bytes of `transferbudget` and nothing else, reads the struct back and returns the re-read value; a mismatch is an error. Negative values are refused.
  - [x] 1.2 New `SaveRepoError::OrcamentoMudou(i32)`; `ByteSink` trait (only the process implementation writes) so the logic is tested on fake memory.
  - [x] 1.3 Tests: only bytes 12..16 of the struct change; moved value → nothing written; refused write, game overwriting right after, negative → errors.
- [x] Task 2: Hiring flow in `scout::state` (AC #2, #4)
  - [x] 2.1 `preparar_contratacao` / `cancelar_contratacao` / `previa_contratacao` (cost, current and resulting budget, shortfall — recomputed from the live budget every frame).
  - [x] 2.2 `confirmar_contratacao`: insufficient budget or a read-only state file → refuse before writing. Order: debit (compare-and-write + read-back), then persist the Olheiro write-through. If persisting fails, undo the debit; if undoing also fails, say plainly that the money left (`OrcamentoDebitadoSemOlheiro`). On success the panel re-reads the budget from the game (header shows the confirmed balance).
  - [x] 2.3 `ErroContratacao` keeps the modal open with the reason and a retry; `OrcamentoMudou` re-reads so the modal shows the new value.
- [x] Task 3: Confirmação de Contratação (AC #1, #3, #4) — `scout::screens::confirmacao_contratacao`
  - [x] 3.1 Satellite screen on the nav stack (AD-6) drawn as its own centred window over the panel. The panel underneath is disabled and veiled. This is not an ImGui popup, which could stay open and block input if F10 closes the panel mid-confirmation.
  - [x] 3.2 Content: "Contratar Olheiro", name and Tier badge, "Tier … · description", rows for cost / current budget / budget after (mono, green when affordable), red shortfall or error text, "Confirmar contratação" (green; "Tentar novamente" after a failure; grey and inert when short) and "Cancelar" (outlined).
  - [x] 3.3 The Olheiros tab returns the clicked combination; `render_painel` opens the modal. F10 during the modal cancels it (nothing written); if the career stops being ready, the modal closes.
- [x] Task 4: Tests — `cargo test`: 96 passed (new: write ×3, hiring success with file check, insufficient, failed/unconfirmed writes ×3, no writable state, persist failure with undo ×2, cancel, F10 cancels, error texts).
- [x] Task 5: Manual check in game (Felipe) — **first real write from the overlay**
  - [x] 5.1 Hire a cheap Olheiro (Generalista Júnior, 300.000): modal values match; after confirming, the header budget drops by exactly the cost and stays there (watch it for ~10 s: if the game rewrites it back, the re-read shows it).
  - [x] 5.2 The Olheiro appears in "Contratados" as "Disponível"; close and reopen the panel, and restart the game: it is still there.
  - [x] 5.3 The game's Transferências screen (change screen and come back) shows the new budget.
  - [x] 5.4 Save the career in the game; Claude reads the `DATA` file from disk and confirms `dqXv.transferbudget` = the new value (AC #5: nothing else written).
  - [x] 5.5 Cancel and F10 during the modal: budget unchanged.

## Dev Notes

- The write targets the decoded `dqXv` struct found by the season signature (session 6). Session 4 proved writing the budget the game shows sticks (visible after a screen change, saved to `DATA`), but session 6 also saw a second representation of the budget (16-byte stride). If the game keeps that one as the source, the read-back right after writing passes but the periodic re-read (1 s) would show the old value again; Task 5.1 checks exactly that.
- Order "debit first, then save the Olheiro" means a game crash between the two loses the money without an Olheiro; the opposite order would risk an Olheiro without a debit. The rollback covers every failure that does not kill the process.
- Amounts shown without a currency symbol, as elsewhere in the panel.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.5]
- [Source: prd.md#FR-3]
- [Source: ARCHITECTURE-SPINE.md#AD-2, AD-5, AD-6, AD-7, AD-12]
- [Source: EXPERIENCE.md (Fluxo 1, State Patterns "Orçamento insuficiente"), DESIGN.md (button-primary/secondary)]
- [Source: PROJECT_MEMORY.md#Orçamento do clube (transferbudget) — sessão 4; Sessão 6 (struct viva)]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 96 passed; `cargo build --release` with no code warnings (`1.5-v1`); clippy clean on the new code.

### Completion Notes List

- In game 2026-10-01 (build `1.5-v1`, career Careca, club 234). At 18:04:37 Felipe hired a Caçador de Jovens Júnior. The log shows `Orçamento de transferências: 45955973 -> 45555973 (0x8CD1AF3C)`; the Olheiro `305de8aa…` was saved as `cacador_de_jovens`/`junior` in `382e8c01….json`.
- After Felipe saved in game, the new `DATA` (`4db95dbb`, 15:05 local) parses fine (the game recalculated the checksum) and has `dqXv.transferbudget = 45.555.973`. The previous save of the same career and date (`e7d9cf01`) had 45.955.973, and **all other 25 `dqXv` fields are identical** (AC #5 / NFR1). The second budget representation seen in session 6 did not undo the write.
- Felipe confirmed 5.2 (persists after restart), 5.3 (Transferências screen) and 5.5 (cancel/F10): "tudo funcionou".

### Change Log

- 2026-10-01: Story 1.5 implemented (budget write, hiring flow, confirmation modal).

### File List

- fifa_overlay/src/save_repo.rs (modified: `write_transfer_budget`, `ByteSink`, `OrcamentoMudou`)
- fifa_overlay/src/scout/search.rs (modified: `CareerSource::write_transfer_budget`)
- fifa_overlay/src/scout/state.rs (modified: hiring flow)
- fifa_overlay/src/scout/persistence.rs (modified: `gravavel` in use)
- fifa_overlay/src/scout/mod.rs (modified: F10 cancels a pending hire; nav APIs in use)
- fifa_overlay/src/scout/screens/confirmacao_contratacao.rs (new)
- fifa_overlay/src/scout/screens/olheiros.rs (modified: returns the click, `nome_com_badge`)
- fifa_overlay/src/scout/screens/mod.rs (modified: modal wiring, disabled/veiled panel)
- fifa_overlay/src/scout/screens/theme.rs (modified: `FUNDO_MODAL`)
- fifa_overlay/src/lib.rs (modified: BUILD_TAG)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
