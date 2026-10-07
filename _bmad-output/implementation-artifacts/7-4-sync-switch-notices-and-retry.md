---
baseline_commit: 38ca169
---

# Story 7.4: Sync switch, notices and retry

Status: review

## Story

As Felipe,
I want to turn the sync with the FIFA off, see when the Central updated the game, and retry when the game's scout was not found,
so that the Central never writes to the game without me knowing and without a way to stop it.

## Acceptance Criteria

1. Aba **Escolhidos** has "Sincronizar com o FIFA: ligado/desligado" (toggle button) below the "Olheiros do acompanhamento" button, so the initial gamepad focus never flips it by accident. The choice is saved per career in `ui_prefs.sincronizar_com_o_jogo`: on by default, written to the file only when off; files from before Epic 7 load as on. It is applied to `save_repo::nativo` when the career becomes active.
2. Turning it off writes nothing more and undoes nothing in the game. Turning it on reconciles immediately and puts on the game's shortlist the Escolhidos that are not there yet (resolving the team from the save when a Relatório is old).
3. A situation line under the button says: off / on and working / on but the game's scout was not located / locating / last write problem.
4. "Tentar de novo" (shown when on and not fully located) locates the game's scout again in an `AsyncTask` (AD-4) and, when found, reconciles at once.
5. Banners: "Jogo atualizado: N jogadores" when a reconciliation changed the game's knowledge; "Scout do jogo não localizado" once per session (until it is found) when a write finds nothing.

## What was verified

- 2 new tests (the situation line texts; the pref default/save/old-file behaviour), the banner texts added to the existing banner tests. Full suite: 310 passed; clippy clean in the touched files.
- In the game (build `7.4-v1`, test career Real Madrid, 2026-10-07): switch off and on logged; Jan Oblak, added to the Escolhidos while the sync was off, entered the game's shortlist and got his knowledge record right after it was turned on (`entrou na lista de escolhidos do jogo`, `Conhecimento do jogo: 1 criado`). Felipe confirmed the toggle, the situation line and the gamepad focus.

## Notes

- The sync switch is a process-wide flag (`nativo::sincronizacao_ligada`) set from the active career's preference.
- The "not located" path was not forced in game (it needs a career change); it is covered by the situation-line test.
