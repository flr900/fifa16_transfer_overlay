---
baseline_commit: c5f6b2e
---

# Story 2.6: Switch between Tabular and Cards with real minifaces

Status: done

## Story

As Felipe,
I want a Cards view with player faces,
so that I can recognise players at a glance.

## Acceptance Criteria

1. **Given** the Relatórios tab, **when** I look at the top, **then** an always-visible toggle offers Tabular / Cards, and the choice is persisted in `ui_prefs` across sessions (UX-DR14).
2. **Given** the Cards view, **when** a player card renders, **then** it shows the miniface loaded from `data/ui/imgAssets/heads/p<PLAYERID>.dds`, then name (heading), age, native position and key attributes in `body`/mono, **and** if the file does not exist for that `playerid`, a neutral silhouette is used.
3. **Given** many cards, **when** the list scrolls, **then** textures are loaded lazily and cached, without hitching the game.

## Tasks / Subtasks

- [x] Task 1: `dds.rs` (new): DDS → RGBA8 for DXT5 (the format of the game's 128×128 faces, checked on disk), DXT1 and uncompressed 32-bit; anything else is `None`.
- [x] Task 2: `save_repo::ler_miniface(player_id)` reads `<game folder>\data\ui\imgAssets\heads\p<id>.dds` (game folder = the folder of the `fifa16.exe` that loaded the DLL).
- [x] Task 3: `scout::minifaces` (new): faces are requested only by VISIBLE cards; files are read and decoded in batches of 12 in an `AsyncTask`; at most 4 uploads to the GPU per frame in `before_render`; at most 300 live textures, the least recently used is reused with `replace_texture`.
- [x] Task 4: `ui_prefs.densidade` (`"tabular"`/`"cards"`, default Tabular), `state.densidade()` / `definir_densidade()`.
- [x] Task 5: Screens
  - [x] 5.1 Tabular / Cards toggle at the top of the Relatórios tab and at the top right of the Relatório screen.
  - [x] 5.2 Cards view: grid of 380×128 cards (one focusable item each), 96 px face (or silhouette), name (heading, cut with "…"), "N anos · POS · Nação", club, "OVR … POT …" in Consolas, and the first 3 observed attributes. Tooltip with the full name when something was cut.
- [x] Task 6: Tests — `cargo test`: 164 passed (new: DXT5/DXT1 decoding, bad files, a real face from the game; lazy loading, silhouette for missing files, 4 uploads per frame, LRU reuse above 300; density persisted).
- [ ] Task 7: Manual check in game (Felipe): switch to Cards, scroll a long Relatório (no hitch), a regen shows the silhouette, reopen the game and the choice is kept.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.6]
- [Source: DESIGN.md (player card), EXPERIENCE.md (Relatórios density toggle)]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 164 passed.

### Change Log

- 2026-10-01: Story 2.6 implemented (Cards view with minifaces).

### File List

- fifa_overlay/src/dds.rs (new)
- fifa_overlay/src/lib.rs (modified: texture upload in `before_render`)
- fifa_overlay/src/save_repo.rs, fifa_overlay/src/save_repo/jogadores.rs (modified: `ler_miniface`)
- fifa_overlay/src/scout/minifaces.rs (new)
- fifa_overlay/src/scout/mod.rs, fifa_overlay/src/scout/state.rs, fifa_overlay/src/scout/persistence.rs (modified)
- fifa_overlay/src/scout/screens/relatorio.rs, relatorios.rs, componentes.rs (modified)
