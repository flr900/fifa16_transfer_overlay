---
baseline_commit: 43826e2
---

# Story 7.6: Exact transfer value from the game

Status: done

## Story

As Felipe,
I want the Central to show the exact transfer value the game itself calculated,
so that the value on the Central matches the game instead of an estimate that was 1.1–1.7× too high.

## Findings behind it (2026-10-06/07)

The save does not store the market value; the game computes it when a screen shows the player and leaves the one for the player IN FOCUS (search, shortlist, squad) in a single row of a UI buffer at a fixed place of the career's memory region: region + `0x365020` = `playerid i32`, `team i32`, `? i32`, `value i32` (multiple of 5,000), then the name as a C string. The AI shortlist entries `[team, player, V1, V2, wage]` do NOT match the displayed value, so they are not a source. The wage is not on this row.

## Acceptance Criteria

1. `save_repo::foco::read_focused_value()` reads 48 bytes at that fixed place (no scan) and decodes id, team, value and name, refusing anything that does not look like the rows seen in the game (id range, value multiple of 5,000, name readable).
2. Every 0.5 s (career ready) the Central looks at the focused player; if he is one the Central knows (an Escolhido or a Relatório player) AND the name matches (ASCII letters only, case-insensitive, one name may contain the other), it saves `(player, value, career date)` in the career file (`valores_do_jogo`, omitted when empty). It writes the file only when the player or value changes; a row with an unknown id or a different name is ignored (logged).
3. The Relatório card and the Ficha show "Valor 5,0 M (exato)" while the reading is at most 90 career days old; afterwards, or without a reading, "Valor ≈ …" as before. The wage stays an estimate.

## What was verified

- Tests: row decoding with the real rows (Nypan, Kostoulas, Palazón in Latin-1 and UTF-8, Oblak), garbage rejections, the offset pinned to the addresses seen in the game (a first build used the VALUE address as the id address and read nothing), name matching, the 90-day validity, and the harvest on the state with a fake source (unknown id and wrong name ignored, a known player harvested and saved, a new value replaces the old, expiry after 4 months). Full suite: 317 passed.
- In the game (builds `7.6-v1` failed because of that offset, `7.6-v2` fine): the log showed `Valor exato de Jan Oblak lido no jogo: 45500000` and `… Logan Costa …: 11000000`; Felipe confirmed "exato" on the Central.

## Notes

- Still unverified: whether the offset is the same after closing and reopening the game (it was the same across careers and DLL reloads). If the exact value stops appearing in a new session, find the row again with `scout_probe.py` (search for a known value and player id).
- A value is only exact for players who passed through the game's screens while the Central was running; the others keep the estimate. Using the harvested values to calibrate the estimate (the current one is 1.1–1.7× high against six real values) is left for a later story.

## Addendum (2026-10-07): estimate recalibrated, new-session check, last-field fix

- **Estimate recalibrated** (`quality::valor_estimado`): fitted to 9 exact values from the 2026 test career (OVR 70–90, ages 19–33): `ln(value) = 2.854 + 0.174·OVR + 0.045·max(0, 24−age) − 0.085·max(0, age−27)`; mean error 3% (5% leave-one-out) against 30–60% for the old formula (it ran 1.1–1.7× high). Potential and goalkeeper needed no term. The proposals' `valuation`/`offeredfee` are NOT the displayed value (Bellingham: 149.5 M / 121.5 M against 108.5 M), so only exact readings were used.
- **Self-adjusting layer:** every exact reading of a player the Olheiro saw precisely (OVR and POT ranges ≤ 4) stores the estimate of that moment; with ≥ 5 such readings the estimate is multiplied by a factor (geometric mean of real ÷ estimated, pulled toward 1, clamped to [0.75, 1.33]).
- **New game session (PSG career, 2029):** the region base changed (`0x8CC30000`) but the value row (`+0x365020`), the date (`+0x373E08`) and the knowledge array (`+0x359890`) kept the same offsets; the locator then took 286 ms.
- **Bug found only in that career:** the last field of a knowledge record is `-1` OR a market value (14.5 M, 21 M); the validator required `-1`, so the array split into 5 pieces and was not located, and a rewrite would have erased those values. `RegistroConhecimento.extra` now keeps it (valid: `-1` or a positive multiple of 5,000) and a rewrite preserves it; two tests.
- In the game (build `7.6-v4`, PSG career): `Localização em 286 ms`, 311 knowledge records read, `Conhecimento do jogo: 84 criado(s)`, and an exact value read (`Kees Smit: 37500000`). Felipe confirmed everything worked.
