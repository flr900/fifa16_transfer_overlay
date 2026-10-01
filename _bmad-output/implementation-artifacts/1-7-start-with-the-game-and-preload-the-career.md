---
baseline_commit: b90444a
---

# Story 1.7: Start with the game and preload the career

Status: review

## Story

As Felipe,
I want the Central de Scout to load with the game and to have my career ready before I press F10,
so that I never run the injector by hand or wait in front of an empty panel.

## Acceptance Criteria

1. **Given** I start my session with the launcher script instead of the FIFA Friends shortcut, **when** it runs, **then** it opens the FIFA Friends server (same target as the shortcut), waits for `fifa16.exe`, and injects the overlay once the game has loaded DirectX and shows a visible window, **and** it never injects twice into the same game process, waits again if the game is reopened, and exits when the FIFA Friends server closes.
2. **Given** the overlay has just been injected, **when** the first frame renders, **then** a top-right banner shows "Central de Scout ativa" and disappears after 3 seconds, without taking mouse, keyboard or focus from the game.
3. **Given** the panel is closed and I enter a career, **when** the overlay notices a career is loaded (cheap background check, never on the render thread — AD-4), **then** it locates the career by itself and the banner shows "Carregando carreira…" while it runs, then "Carreira pronta" with manager and date for 3 seconds, or a failure message with the F10 hint for 3 seconds, **and** a failed locate is not retried in a loop.
4. **Given** the career was already located in the background, **when** I press F10, **then** the panel opens directly on the career's saved tab, with no "Localizando carreira…" wait.

## Tasks / Subtasks

- [x] Task 1: Launcher (AC #1) — decision 2026-10-01 (Felipe): a script that starts the game session, not a scheduled task or a proxy DLL; must go through the FIFA Friends mod.
  - [x] 1.1 `fifa_overlay/iniciar_fifa.ps1`: asks for Administrator once (the game runs elevated); copies the release DLL to `target\fifa_overlay_dev.dll`; opens the FIFA Friends server from the target of `Desktop\FIFA FRIENDS PREMIUM!.lnk` (reuses it if already open); runs the injector in wait mode tied to the server's pid. UTF-8 with BOM so PowerShell 5.1 shows the accents.
  - [x] 1.2 `fifa_injector --aguardar [--enquanto-pid <PID>] [dll]`: polls every 1 s; "ready" = `d3d11.dll` loaded + a visible window, held for a 5 s grace; injects once per game process (also skips a process that already has a `fifa_overlay*` module); waits again when the game closes; exits when the server pid disappears; says so when it cannot read the game modules (not elevated). The old one-shot mode is unchanged, now without `panic!`.
- [x] Task 2: Cheap career signal + fast locate (AC #3, #4) — `save_repo`
  - [x] 2.1 `start_career_probe(AsyncTask<bool>)`: enumerates regions and reads 3 × 4 bytes per region (`LIVE_DATE_REGION_OFFSETS`); "on" when 2 of 3 agree on a plausible date (in the menu the two event lists are 0 — session 6).
  - [x] 2.2 `locate()` first scans only the regions where the signal is on (the finance struct lives in the same `VirtualQuery` region as the date) and falls back to the full scan if that finds nothing. Logs the time of the fast path.
- [x] Task 3: Sentinel in `scout::state` (AC #3, #4)
  - [x] 3.1 `tick` runs every frame, panel open or closed (still cheap: two polls and at most a few bytes per second).
  - [x] 3.2 Without a ready career, a probe every 2 s; the first "on" result triggers one locate. The signal re-arms only after it goes "off" or after a successful locate, so a failing locate does not loop.
  - [x] 3.3 `Aviso { Injetado, Carregando, Pronta(snapshot), Falhou }` with `DURACAO_AVISO = 3 s`; "Carregando" stays while the locate runs.
- [x] Task 4: Banner (AC #2, #3) — `scout::screens::aviso`
  - [x] 4.1 Top-right window, `NO_INPUTS | NO_DECORATION | NO_FOCUS_ON_APPEARING | NO_NAV`, raised panel colour, hairline border, coloured side bar + title + detail (colour never alone). Only drawn with the panel closed (the header already shows the state when open).
  - [x] 4.2 `BUILD_TAG` = `1.7-v1 — vigia da carreira + aviso`.
- [x] Task 5: Tests
  - [x] 5.1 `fifa_overlay`: 77 passed (new: signal regions, injection banner timing, signal → locate → ready banner, failed locate + re-arm, signal off never locates, banner texts).
  - [x] 5.2 `fifa_injector`: 4 passed (ready classification, grace period, grace reset, already injected / reopened game). Smoke run without admin: reports "Sem acesso ao jogo"; `--enquanto-pid` with a dead pid exits.
- [x] Task 6: Manual check in game (Felipe)
  - [x] 6.1 Close FIFA and FIFA Friends; run `iniciar_fifa.ps1`; open FIFA as usual. Expect: the injector console shows "Jogo pronto…" then "DLL injetada"; the banner "Central de Scout ativa" for 3 s.
  - [x] 6.2 Enter the career without pressing F10: "Carregando carreira…" then "Carreira pronta · … · F10 abre o painel." Check the log for `Localização rápida: … ms`.
  - [x] 6.3 F10: the panel opens straight on the saved tab (no "Localizando…").
  - [ ] 6.4 Back to the main menu, then load a different career: a new "Carregando…/Carreira pronta" cycle.
  - [x] 6.5 FPS stays smooth in the menu and in the career (the probe runs every 2 s in the background).

## Dev Notes

- The shortcut target is `D:\Program Files\FIFA 16\Server16Python.exe` (working dir `D:\Program Files\FIFA 16\`). It shows up as two processes (packer parent + child); the script uses the oldest one.
- Running the script elevated also starts the FIFA Friends server elevated. If that changes how the mod behaves, the server can be started from the normal shortcut and the script run afterwards (it reuses the running server).
- Fast-path assumption (session 6): the region whose base + `0x373E08` holds the live date is the same `VirtualQuery` region that contains the finance struct. If the in-game log shows the fast path never succeeds, the full scan still runs, so nothing breaks.
- Probe false positives (two equal plausible dates at those offsets in an unrelated region) cost at most one locate per signal streak; the locate validates everything.

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.7]
- [Source: ARCHITECTURE-SPINE.md#AD-4 (AsyncTask), AD-14 (shortcut polling)]
- [Source: PROJECT_MEMORY.md#Sessão 6 (live date offsets, menu state), Sessão 8]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 77 + 4 passed; `cargo build --release` for both crates with no code warnings; clippy clean on the new code.

### Completion Notes List

- In game 2026-10-01 (build `1.7-v1`, per `%TEMP%ifa_overlay.log`): injected by `iniciar_fifa.ps1` at 15:26:58. On entering the career (15:30:45.409) the sentinel detected it without F10. Fast path: 1 region with a live date, **4 ms**; whole locate including the saves read **~110 ms** (was ~15 s). Career `20260717|Senhor|Manager|243`, state loaded, saved tab Sonar restored with the panel closed. The full-memory scan never ran. Confirms the session 6 assumption that the finance struct and the live date share a region (`0x8CCB0000`, struct at +0xEE74C).
- Felipe confirmed in game ("tudo funcionou"): banners on screen, F10 opens straight on the saved tab, FPS normal. The career switch (6.4) was optional and was not confirmed separately.

### File List

- fifa_overlay/iniciar_fifa.ps1 (new)
- fifa_overlay/src/save_repo.rs (modified: probe, fast locate, `agreed_date_at`)
- fifa_overlay/src/scout/state.rs (modified: sentinel, avisos)
- fifa_overlay/src/scout/search.rs (modified: `start_career_probe`)
- fifa_overlay/src/scout/mod.rs (modified: tick always, banner)
- fifa_overlay/src/scout/screens/aviso.rs (new)
- fifa_overlay/src/scout/screens/mod.rs (modified)
- fifa_overlay/src/scout/screens/theme.rs (modified: `DANGER` in use)
- fifa_overlay/src/lib.rs (modified: BUILD_TAG)
- fifa_injector/src/main.rs (modified: `--aguardar`, `--enquanto-pid`)
- fifa_injector/Cargo.toml, Cargo.lock (modified: windows 0.62)
- _bmad-output/planning-artifacts/epics.md (modified: Story 1.7)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
