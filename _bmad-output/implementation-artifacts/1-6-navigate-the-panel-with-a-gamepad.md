---
baseline_commit: bfabbc7
---

# Story 1.6: Navigate the panel with a gamepad

Status: done

## Story

As Felipe,
I want to move through the Scout panel with a gamepad,
so that I never have to put the controller down to use it.

## Acceptance Criteria

1. **Given** the panel is open and a controller is connected, **when** I use the D-pad or left analog, **then** focus moves in reading order (tabs → content → actions) across the tab bar, Olheiro cards, hire buttons and modal buttons.
2. **Given** a focused element, **when** I press the confirm button, **then** the element activates, and the cancel button closes a modal or goes back one screen.
3. **Given** I press LB/RB (L1/R1), **when** the panel is open, **then** it switches to the previous or next tab directly.
4. **Given** mouse and gamepad are used in the same session, **when** an element has focus or hover, **then** both show the identical purple solid border, so switching input never loses the focus position (UX-DR19).
5. **Given** this behaviour depends on the hudhook/imgui gamepad support, **when** the story is started, **then** feasibility is checked first, and if it is not achievable, the limitation and a keyboard-navigation fallback are documented.

## Feasibility (AC #5) — checked 2026-10-01

- **Navigation: feasible.** The hudhook bundles imgui **1.89.2**, which has gamepad nav (`NavEnableGamepad`, `ImGuiKey_Gamepad*`, `NavFlattened`). hudhook 0.9 does not read controllers itself, so the overlay polls XInput and feeds the keys to imgui.
- **Input conflict with the game.** This was the real risk. FIFA reads the controller straight from XInput, so the window-message filter (`MessageFilter::InputAll`) does not stop it. With the panel open, A/B would also act in the game. `fifa16.exe` references `xinput1_4/1_3/1_2/1_1/9_1_0`. Solution: hook `XInputGetState` in every loaded XInput DLL with the MinHook already inside hudhook (`hudhook::mh`). While the panel is open, the game gets a "still" controller. The overlay reads through the hook's trampoline, so it is never blocked itself.
- **Keyboard fallback**: `NavEnableKeyboard` is on as well (arrows/Tab, Space/Enter, Esc), useful even with a working controller.

## Tasks / Subtasks

- [x] Task 1: `gamepad.rs` (new, infrastructure next to `memscan`)
  - [x] 1.1 `XInputGetState` hook per loaded XInput DLL (one detour per DLL, each calling its own trampoline); `bloquear_jogo(bool)` makes the detour return a zeroed `XINPUT_GAMEPAD`. The trampoline is published before the hook is enabled. If no XInput DLL is loaded yet, the install is retried every 3 s (logged once) and the overlay reads `xinput1_4` directly in the meantime.
  - [x] 1.2 `Controle::ler()`: first connected controller; the 4 indices are scanned only every 2 s while none answers (polling an empty index is slow in XInput).
  - [x] 1.3 `eventos_imgui` / `alimentar_imgui`: D-pad, A/B/X/Y, LB/RB, START/BACK, L3/R3, left stick (dead zone 7849, normalised) and triggers → imgui gamepad keys. With the panel closed everything is released.
- [x] Task 2: Commands in `scout::mod` (AC #2, #3)
  - [x] 2.1 **L3 + START** toggles the panel (edge-triggered). This is the combo validated with the Electron companion: L3+R3 conflicts with FIFA, and the 8BitDo paddles send no XInput. F10 and the combo in the same frame toggle once.
  - [x] 2.2 LB/RB: previous/next tab with wrap-around, only at the root (not over a satellite screen); the choice is persisted like a click.
  - [x] 2.3 B: closes the satellite (the hire modal cancels, nothing is written); at the root it closes the panel.
  - [x] 2.4 `bloqueia_controle()`: true while the panel is open and, after closing, until every button and trigger is released, so the B/START that closed the panel never reaches the game.
- [x] Task 3: Focus visuals (AC #1, #4)
  - [x] 3.1 Tab content child windows use `NavFlattened` (the D-pad moves between tabs and cards without "entering" the child).
  - [x] 3.2 Mouse hover draws the same 2 px purple outline, 4 px outside the item, as imgui's nav focus (tabs, "Tentar novamente", modal buttons). Card border on hover or focus is now 2 px purple.
  - [x] 3.3 Modal default focus: "Confirmar contratação", or "Cancelar" when the budget is short.
- [x] Task 4: Tests — `cargo test`: 105 passed (new: stick dead zone, key mapping, no controller = released, "released" rule; combo edge + blocked until release; F10 + combo same frame; LB/RB with wrap and not over a satellite; B modal → panel; tab neighbours).
- [x] Task 6: Fixes after the first in-game test (Felipe, build `1.6-v1`: navigation worked, but two problems)
  - [x] 6.1 **Scroll**: going back up never showed what was above. Only the "Contratar" buttons were navigable, so imgui scrolled to the button and left the section labels and the top of the card hidden. Now each card is ONE navigable item (an invisible button over the whole card, everything inside drawn by the draw list), so imgui brings the whole card into view. The first card of the tab scrolls to the top, and the first card of a section also reveals its label.
  - [x] 6.2 **Hired Olheiro not selectable**: hired cards are now navigable items too (no action yet; Missões come in Epic 2). On an offer, activating the card (click or A) is "Contratar".
  - [x] 6.3 One focus drawing: imgui's native `NavHighlight` is transparent in the theme. Hover and controller/keyboard focus both draw the same 2 px purple outline (tabs, buttons) or card border.
  - [x] 6.4 Dev reload without restarting the game (Felipe's request): `fifa_overlay/recarregar_dev.ps1` creates `%TEMP%/fifa_overlay_eject.pedido`. The DLL checks for it every 1 s, removes its XInput hook and calls `hudhook::eject()`. The script waits for the loaded copy to be released, copies the new build and injects again. Works from build `1.6-v2` on; ejecting already crashed the game once in about 5 tries (session 6).
- [x] Task 5: Manual check in game (Felipe)
  - [x] 5.1 Log shows `[gamepad] XInputGetState enganchado (DLL #n)` and `Controle conectado no índice n`.
  - [x] 5.2 In the career, L3 + START opens the panel; D-pad/analog move the purple focus tabs → cards → "Contratar"; A on "Contratar" opens the modal with focus on confirm; B cancels it; LB/RB switch tabs.
  - [x] 5.3 **While the panel is open the game does not react** to A/B/D-pad (the career menu behind it stays put).
  - [x] 5.4 Closing with B or L3 + START: the game does not get the press (no menu change/pause); right after, the controller works normally in the game.
  - [x] 5.5 Mouse hover and controller focus look the same; switching between them keeps the place.

## Dev Notes

- Opening with L3 + START: the game can see START go down in the same instant the overlay detects the combo, before the block applies. The Electron companion reported no conflict with this combo; check 5.4 for the closing side, which is blocked.
- The hook runs on game threads; it touches only atomics.
- `MhHook` has no `Drop`: the hook lives until the process ends (the product DLL is never ejected).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.6]
- [Source: EXPERIENCE.md#Interaction Primitives]
- [Source: PROJECT_MEMORY.md#3. Por que o atalho de controle usa LEFT_THUMB+START]
- [Source: hudhook 0.9.3 `mh.rs` (MinHook), imgui 1.89.2 nav]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01: `cargo test` 105 passed; `cargo build --release` with no code warnings (`1.6-v1`); clippy clean on the new code.

### Completion Notes List

- First in-game test (`1.6-v1`): navigation, A/B, LB/RB and game blocking worked. The scroll problem and the non-selectable hired card were fixed in `1.6-v2` (Task 6). Second test (`1.6-v2`, 2026-10-01), confirmed by Felipe ("tudo funcionou"): scroll back up shows the labels, hired cards take focus. Log: `XInputGetState enganchado (DLL #0)` and `(DLL #2)`, controller on index 0. Dev reload: at 18:42:19 the DLL saw the request, removed both XInput hooks and ejected; `1.6-v2` was injected again at 18:42:21 (about 2 s), and the game kept running.

### Change Log

- 2026-10-01: Story 1.6 implemented (XInput reading + game block hook, imgui gamepad/keyboard nav, L3+START/LB/RB/B, matching hover/focus outline).

### File List

- fifa_overlay/src/gamepad.rs (new; `remover_ganchos` for the dev reload)
- fifa_overlay/recarregar_dev.ps1 (new: dev reload without closing the game)
- fifa_overlay/src/lib.rs (modified: nav flags, controller read/feed, block per frame, BUILD_TAG)
- fifa_overlay/src/scout/mod.rs (modified: controller commands, block rule)
- fifa_overlay/src/scout/screens/mod.rs (modified: `contorno_hover`, `NavFlattened` content)
- fifa_overlay/src/scout/screens/olheiros.rs (modified: whole card is one navigable item, scroll reveal, 2 px focus/hover border)
- fifa_overlay/src/scout/screens/theme.rs (modified: `NavHighlight` transparent)
- fifa_overlay/src/scout/screens/confirmacao_contratacao.rs (modified: default focus, hover outline)
- fifa_overlay/Cargo.toml (modified: `Win32_UI_Input_XboxController`)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
