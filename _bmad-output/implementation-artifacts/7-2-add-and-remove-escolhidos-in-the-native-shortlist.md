---
baseline_commit: 9cbbe99
---

# Story 7.2: Add and remove Escolhidos in the native shortlist

Status: done

## Story

As Felipe,
I want a player I add to the Central's Escolhidos to also appear in the game's own shortlist, and to leave it when I remove him,
so that the Central and the game agree on who I am following.

## Acceptance Criteria

1. `save_repo::nativo::write_native_shortlist_add(time, jogador)` appends a 28-byte entry (`-1` ×4, mark 1, zeroed padding) at the end of the game's shortlist and moves the end pointer of every coherent owner; the entry is written BEFORE the pointer. `write_native_shortlist_remove(jogador)` shrinks the end first, then rewrites the following entries one slot up.
2. Compare-and-write with read-back: before writing, every cached owner is re-read; owners that no longer match the cached start/capacity (freed and reused structs) are dropped; the survivors must agree on the end and the list must parse (valid entries, no duplicates). Afterwards the whole list is re-read and must equal the expected one. A stale cache (`NativoMudou`) is dropped and nothing is written; a list with 100 entries gives `ListaNativaCheia`; an already-present player is `JaEstava` (nothing written).
3. `SaveRepoError` gains `ListaNativaCheia` and `NativoMudou` (AD-5).
4. On "Adicionar aos Escolhidos" the Central also adds the player to the game's list (when the sync switch is on — default on; the setting UI is Story 7.4) and remembers it in `Escolhido.no_jogo`, saved only when true. A player the game already had is NOT marked, so the Central never removes what it did not add. On removal, only `no_jogo` players are taken off the game's list. Any failure only logs; the Central never depends on the write.
5. The game's shortlist entry stores the player's team. `JogadorEncontrado.clube_id` is now saved with each Relatório player. For an Escolhido without it (Relatórios from before this story) the Central reads the save in an `AsyncTask` (AD-4), fills the team, and then syncs.
6. The tracking rewrite of an Escolhido keeps `no_jogo`.

## What was verified

- 8 new `nativo` tests on a writable simulated memory (append moves every owner's end; already-there writes nothing; full list refused; disagreeing or moved owners → stale without writing; a reused owner is dropped and the live one still works; removing first/last/missing/all and re-adding), plus a file-format test for `no_jogo` and a state test for the team lookup. Full suite 300 passed; clippy has no findings in the new code.
- In the game (build `7.2-v1`, test career Real Madrid, 2026-10-07): "Adicionar aos Escolhidos" on a Relatório player put him in the game's shortlist (log `Isi Palazón entrou na lista de escolhidos do jogo`, single coherent owner `0x8CC5CA68`); removing him from the Central's Escolhidos took him off the game's list (`jogador 232498 → Removido`). Felipe confirmed both on screen.

## Notes

- The sync switch is a process-wide flag (`nativo::sincronizacao_ligada`) defaulting to on; Story 7.4 persists it and adds the setting, a banner and a "Sincronizar agora" retry for the case where the game was not located yet.
- The native list's four reveal fields stay `-1`: the game shows value and wage from the player's knowledge level (Story 7.3).
