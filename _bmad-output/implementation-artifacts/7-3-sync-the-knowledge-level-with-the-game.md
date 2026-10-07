---
baseline_commit: 2e90313
---

# Story 7.3: Sync the knowledge level with the game

Status: review

## Story

As Felipe,
I want the game to show what the Central's Olheiros learned (value, wage, attributes) and to follow the Central when that knowledge improves or ages,
so that the two screens tell the same story.

## Request (Felipe, 2026-10-05/07)

> Ao término de um scout feito na Central o jogador teria seus atributos desbloqueados também no FIFA […] já ter revelado valor, salário e contrato quando vai para a lista de escolhidos […] quando terminar de fato o scout o jogador ficar completo também na lista de escolhidos. (…) Pode fazer o downgrade junto à Central também.

## Rules

1. **Formula** (`quality::nivel_no_jogo`): Missão still running → 140 (the game shows value and wage); Missão concluded → `198 − 4 × precision (±)`, floor 140 — the precision already includes aging and Generalist tracking, so the level falls with time and rises to 198 when the value becomes exact; observation expired (Vencido) → back to the level the game had before the Central touched the player.
2. **Who:** every player of a Relatório whose Missão is concluded (raise only); every Escolhido, following his own observation (the only case that lowers). An Escolhido wins over a Relatório for the same player.
3. **Never below the original:** the first time the Central changes a player's level it saves what the game had (`ScoutStateFile.nivel_original`, 0 = no record). A request with an original may lower the level, but never below it nor above the current one; without an original the level only goes up. This amends the earlier "never lower" rule of 2026-10-06.
4. `save_repo::nativo::write_native_knowledge(pedidos, hoje)`: applies a batch to the game's knowledge array (ordered by player): updates existing records, inserts missing ones in place (`a = 16<<16|2`, the pattern of the in-game test), writes the array in one go BEFORE advancing the end pointer, re-reads the owner and the array (compare-and-write: the array must be unchanged between the read and the write), refuses when the array is full (`ConhecimentoCheio`) or the owner moved (`NativoMudou`, cache dropped). Idempotent: nothing changed → nothing written. A changed record gets the career date.
5. `ScoutState::reconciliar_nativo` runs from `tick` at most every 5 s, only with the career ready and the sync switch on; failures log once per distinct message; the originals are persisted only after a successful write.

## What was verified

- 6 new `nativo` tests (insert in order with the end pointer following, raise/never lower, lower with an original but never below it or above the current, no-change writes nothing, a 50-player mixed batch and idempotence, full array and moved owner), the formula test in `quality`, and the `pedidos_de_nivel` test (running/concluded Relatórios, Escolhido wins, original as floor, expiry). Full suite: 308 passed; clippy clean in the new code.
- In the game (build `7.3-v1`, test career Real Madrid, 2026-10-07): the log showed `Conhecimento do jogo: 8 criado(s)` for the one concluded Relatório with players (the 50-player Relatório had a still-pending Missão and wrote nothing), and `Logan Costa entrou na lista de escolhidos do jogo` followed by `1 criado(s)` (level 140). Felipe confirmed the values and attributes on screen.

## Notes

- Aging and expiry are covered by unit tests only (they take game months).
- Removing an Escolhido does not lower anything; if its Relatório is concluded it keeps the Relatório level.
- The `a` field is still written with the default pattern; its meaning is unknown.
