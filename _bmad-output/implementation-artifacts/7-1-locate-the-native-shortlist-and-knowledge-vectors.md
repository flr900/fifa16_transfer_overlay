---
baseline_commit: 4f68ef5
---

# Story 7.1: Locate the native shortlist and knowledge vectors

Status: done

## Story

As Felipe,
I want the Central de Scout to find, inside the running game, the game's own shortlist and its per-player knowledge array,
so that the next stories can sync Escolhidos and scout results with the native scout (GTN).

## Request (Felipe, 2026-10-06)

> Sincronizar o escolhido no jogo no momento em que o valor de transferência em Escolhidos for definido, pondo na lista de escolhidos [nativa] com o valor anterior ao completo; quando o scout terminar de fato, o jogador fica completo também na lista do jogo.

NFR1 was amended the same day (see `epics.md`): the Central may write the native shortlist and knowledge level, only through `save_repo`, behind the "Sincronizar com o FIFA" setting, with compare-and-write and never lowering a level. This story is **read-only**.

## Acceptance Criteria

1. `save_repo::nativo` (child of `save_repo`, so AD-2 holds) decodes the two formats mapped on 2026-10-06 (`integracao-scout-nativo.md` §3b): the shortlist entry (28 bytes: `time`, `jogador`, four `i32` that are `-1` until revealed, 1-byte mark; capacity 100) and the knowledge record (20 bytes: `jogador`, `a`, `nivel` 0–198, `data`, `-1`; ordered by `jogador`).
2. The locator finds both vectors (three 8-byte pointers in an owner struct: start, end, end of capacity). The knowledge sequence is found by its record pattern in the regions with a live date; the owners are searched first in neighbouring regions and then in all of them. A stale or inconsistent owner (copy of the last save buffer, freed owner) is ignored because its end/capacity do not match; an empty shortlist is still a valid owner.
3. It runs inside the career-locating `AsyncTask` (AD-4), never on the render thread, skips the DLL's own read buffer, and a failure only logs a warning — it never breaks the career locator.
4. `read_native_shortlist()` / `read_native_knowledge()` return typed lists, revalidate the cached vector on every call (owner moved or content malformed → drop the cache entry and return `NaoLocalizado`), and refuse duplicate players / unordered records.
5. Nothing is written to game memory.

## What was verified

- 9 unit tests (`save_repo::nativo`) built from real captured bytes: the 29 real knowledge records, the real shortlist entries with garbage padding, stale owners, empty list, duplicate/torn lists, two coherent owners. Full suite: 290 passed; clippy clean for the module.
- In the game (build `7.1-v1`, test career Real Madrid, 2026-10-07): after a `recarregar_dev.ps1` the log shows the locator finding both vectors on its own in ~0.5 s — shortlist `[268737, 71532]` (Nypan, Smit) and **30** knowledge records, max level 198 — with the same knowledge owner (`0x8D0110C8`) found by hand with `scout_probe.py`. The shortlist owner had moved since the manual test (`0x8CC6CA68` → `0x8CC5CA68`; the old address became unrelated memory), which is why the locator revalidates.

## Notes for the next stories

- The log reported **two coherent shortlist owners**; the locator takes the one with most entries and, on a tie, the one nearest the knowledge array. Build `7.1-v1` did not print their addresses; the current source does. Story 7.2 must decide which one to write (write only fields that still match the expected pointers, and update every coherent owner with the same begin/capacity, or find a liveness test) before it writes.
- Other record sequences exist in the same region (60, 70 and 149 records, no owner): copies/other lists of the same format. Ignored.
- `a` is still unexplained (`65535`, `16<<16 | n`, `6<<16 | 2` for a dedicated request).
