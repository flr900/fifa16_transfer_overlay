---
baseline_commit: dfd611d
---

# Story 1.2: Open and close the Scout panel with a dedicated shortcut

Status: review

## Story

As Felipe,
I want to toggle the Central de Scout with its own shortcut while FIFA 16 runs in fullscreen,
so that I can check my scouting without leaving the game.

## Acceptance Criteria

1. **Given** the overlay is running over FIFA 16 in exclusive fullscreen, **when** I press the Scout shortcut (**F10**, distinct from the Electron companion's Ctrl+Shift+P), **then** the panel opens, and pressing it again closes it, **and** no video-mode switch happens (the existing `Present()` hook is reused).
2. **Given** the shortcut key is held down, **when** frames render, **then** the panel toggles only once, on the released→pressed transition (edge-trigger via `GetAsyncKeyState` polling in `render()`, AD-14), **and** the `windows` crate has the `Win32_UI_Input_KeyboardAndMouse` feature enabled.
3. **Given** the panel is open, **when** it renders, **then** it shows the themed `panel-window` with the 4 fixed tabs (Olheiros / Missões / Relatórios / Sonar), the active tab filled purple and the others in secondary text, **and** it covers ~70% × 75% of the game resolution, centred, with the game visible around it, **and** it applies the DESIGN.md tokens: colours, Oswald / Inter / Consolas fonts, spacing scale, radii, panel opacity ≥ 90%, no shadows/glow/gradients (UX-DR1–4), **and** tabs with no content yet show a neutral placeholder.
4. **Given** I switch tabs and then close and reopen the panel, **when** it reopens, **then** it opens on the top-level tab (navigation stack reset to `[ultima_aba_ativa]`, AD-6), **and** `pop()` on a stack of length 1 is a no-op, and `push` beyond depth 2 is refused with `tracing::warn!`.
5. **Given** no career is loaded, **when** the panel is open, **then** any tab shows "Nenhuma carreira carregada. Abra uma carreira no FIFA 16 para usar a Central de Scout." and renders no data.
6. **Given** a save read fails with `ProcessoInacessivel`, **when** the panel is open, **then** it shows "Não foi possível ler o save ativo." with a retry button, and the panel does not freeze.
7. **Given** the career is still being located (~15 s on first use), **when** the panel is open, **then** it shows "Localizando carreira…" and stays responsive.
8. **Given** the panel is open, **when** a tab changes, **then** switching is instant, with no long transition animation, and each tab keeps its own scroll position (NFR2, UX-DR19).

## Tasks / Subtasks

- [x] Task 1: Archive the diagnostic window (decision 2026-09-30, Felipe: "guardar como companion em outra pasta e iniciar do zero")
  - [x] 1.1 Copy the crate as of Story 1.1 to `fifa_overlay_debug/` (package `fifa_overlay_debug`, log `fifa_overlay_debug.log`, README); it must still build.
  - [x] 1.2 `fifa_overlay/src/lib.rs` restarts from zero: only tracing, the Scout root and the hudhook render loop. PoC UI (scan/pointer-scan/value-scan/sonda/CZUM write test) removed from this crate. The live-state probe leaves `save_repo` (the debug copy keeps its own).
- [x] Task 2: Navigation and shortcut (AC #1, #2, #4) — `scout/mod.rs`
  - [x] 2.1 `Aba` (4 fixed tabs), `ScoutScreen` (`Aba` | satellite), `Navigation` = `Vec<ScoutScreen>` never empty; `stack[0]` is the active tab; `pop` on len 1 = no-op; `push` beyond 2 satellites refused with `tracing::warn!`; closing resets to `[ultima_aba_ativa]`.
  - [x] 2.2 `Scout` root: `painel_aberto` separate from the stack (AD-6); `tecla_estava_pressionada` edge-trigger; F10 polled with `GetAsyncKeyState` inside `render()`.
  - [x] 2.3 Unit tests for all navigation and edge-trigger rules.
- [x] Task 3: Career status for the panel (AC #5, #6, #7) — `scout/state.rs` + `scout/search.rs` (AD-1: screens → state → search → save_repo)
  - [x] 3.1 `CarreiraStatus { Localizando, SemCarreira, ErroLeitura, Pronta(snapshot) }`; auto-locate once per panel opening (never in a loop); periodic re-read (~1 s) while open; retry button restarts locating.
  - [x] 3.2 Unit tests for the status transitions (error mapping, no relocate loop).
- [x] Task 4: Theme and panel (AC #3, #8) — `scout/screens/{mod,theme,olheiros,missoes,relatorios,sonar}.rs`
  - [x] 4.1 Fonts embedded with `include_bytes!`: Oswald SemiBold (display) / Medium (heading), Inter (body/meta) from Google Fonts (OFL, licences in `assets/fonts/`); Consolas read from `C:\Windows\Fonts\consola.ttf` (not redistributable); glyph ranges cover Portuguese + `…`/dashes.
  - [x] 4.2 Style from DESIGN.md tokens (colours, radii 4/6/8/12, spacing 4–32, panel 93% opaque, hairline borders, no shadows).
  - [x] 4.3 Panel ~70%×75% centred, header (title + live budget + date), custom tab bar (active purple/dark text, inactive secondary text, ≥ 32 px), one child window per tab (own scroll), state messages, neutral placeholders.
  - [x] 4.4 While open: overlay draws the mouse cursor and blocks game input (`MessageFilter::InputAll`); closed: nothing drawn, input untouched.
- [x] Task 5: Verification
  - [x] 5.1 `cargo test` + `cargo build --release` in `fifa_overlay` and `fifa_overlay_debug`.
  - [x] 5.2 Manual in game: F10 toggles once per press (hold test), no video-mode switch, look matches DESIGN.md, tab switch instant + scroll kept, reopen lands on last tab, menu → "Nenhuma carreira carregada…", career → header with live budget/date, "Localizando carreira…" while scanning.

## Dev Notes

### Decisions taken with Felipe (2026-09-30)
- Shortcut **F10** (FIFA 16 does not use it in career mode; Electron companion keeps Ctrl+Shift+P). Configurability (FR1) and the controller combo are not in this story's ACs; the gamepad is Story 1.6.
- Fonts: **download Oswald + Inter** (Google Fonts GitHub, OFL) and embed. Oswald static weights from `googlefonts/OswaldFont` (Medium 500, SemiBold 600); Inter only exists as a variable font in `google/fonts` — imgui/stb_truetype ignores variation axes and renders the default instance (Regular 400 = DESIGN `body`).
- Debug window archived in `fifa_overlay_debug/`; `fifa_overlay` UI starts from zero.

### Architecture compliance
- AD-1: `scout::screens` only call `scout::state`; `state` reads the career through `scout::search` (the orchestrator), which is the only caller of `save_repo` in the Scout. Screens may use `save_repo::Date` as a value type for display formatting (conversion for display is a screens concern — Consistency Conventions).
- AD-6 depth: AD-6 says "profundidade máxima de 2 (aba → tela satélite)" while EXPERIENCE.md names one 2-level case (Formulário Nova Missão → Painel de Seleção Geográfica). Implemented as **at most 2 satellites above the tab** (stack length ≤ 3), which satisfies both; `push` of a 3rd satellite is refused.
- AD-14: `GetAsyncKeyState` in `render()`, edge-trigger in `scout::mod`.
- AD-4: locating the career is the Story 1.1 `AsyncTask`; nothing heavy on the render thread. Per-frame cost while open: one key poll + a few small reads per second.

### Visual notes
- DESIGN.md "uso de maiúsculas reservado a badges" beats the mockups' uppercase tabs/title (spine wins over mocks): tab labels and title in normal case.
- Budget shown with thousands separators and no currency symbol (the game's currency setting is not mapped yet).

### References
- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.2]
- [Source: ARCHITECTURE-SPINE.md#AD-1, AD-4, AD-6, AD-14, Structural Seed]
- [Source: ux-designs/.../DESIGN.md (tokens, components), EXPERIENCE.md (IA, State Patterns, Interaction Primitives, Fluxo 1)]
- [Source: Story 1.1 — `save_repo` API: `start_locating`, `read_transfer_budget`, `read_current_date`, `read_career_identity`]

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Debug Log References

- 2026-10-01 (Windows, Rust 1.98.1 msvc): `cargo test` → 54 passed, 0 failed; `cargo build --release` sem avisos de código (`fifa_overlay`, build `1.2-v1`); `fifa_overlay_debug` também compila.
- Log do jogo (`%TEMP%\fifa_overlay.log`): fontes carregadas (Inter, Oswald, Consolas); F10 abre → "Localizando a carreira em background" → ~16 s depois `Carreira ativa: 20260717|Senhor|Manager|243`; fechar/reabrir durante a localização não atrapalha.

### Completion Notes List

- Task 1: crate da Story 1.1 copiado para `fifa_overlay_debug/` (pacote/DLL `fifa_overlay_debug`, log `fifa_overlay_debug.log`, README). `fifa_overlay/src/lib.rs` reescrito do zero; sonda removida do `save_repo` principal (fica na cópia de debug); `i32_at` mantido.
- Task 2: `scout/mod.rs` — `Aba`, `Satelite`, `ScoutScreen`, `Navigation` (AD-6, máx. 2 satélites → pilha ≤ 3), `Scout` com `painel_aberto` separado da pilha e edge-trigger do F10 via `GetAsyncKeyState` no `render()` (AD-14).
- Task 3: `scout/state.rs` (`CarreiraStatus`, política de leitura: leitura na abertura, UMA localização automática por abertura, releitura a cada 1 s com o painel aberto, sem loop de relocalização, "Tentar novamente" relocaliza) + `scout/search.rs` (`CareerSource`, `SaveRepoSource`, `CareerSnapshot`) — AD-1 telas → state → search → save_repo. Testado com fonte falsa.
- Task 4: `scout/screens/{mod,theme,olheiros,missoes,relatorios,sonar}.rs` — tokens do DESIGN.md, fontes embutidas (Oswald SemiBold/Medium estáticas, Inter variável = Regular), Consolas do Windows; `FontSlots` guarda índices do atlas porque `FontId` não é `Send`/`Sync` (exigido pelo hudhook); painel 70%×75% centralizado; cabeçalho com orçamento/data/técnico; barra de abas própria (ImGui TabBar não troca a cor do texto da aba ativa); um child window por aba (scroll próprio); estados vazios; placeholder neutro. Com o painel aberto: cursor do ImGui + `MessageFilter::InputAll`.
- Task 5 (Felipe, no jogo): F10 abre/fecha; segurar alterna uma vez; reabre na última aba; menu → "Nenhuma carreira carregada…"; carreira → cabeçalho com orçamento e data vivos; visual aprovado ("ficou bem legal").
- Observação: no print o jogo aparece mais do que 93% de opacidade sugeriria (texto do jogo legível atrás do painel). Mantido o token do DESIGN.md; reavaliar quando as abas tiverem tabelas.
- Sem botão de eject na DLL nova: para trocar de build, reabrir o jogo.

### Change Log

- 2026-10-01: Story 1.2 implementada (painel Scout com F10, tema, navegação, estados da carreira); janela de debug arquivada em `fifa_overlay_debug/`.

### File List

- fifa_overlay/src/lib.rs (rewritten)
- fifa_overlay/src/scout/mod.rs (new)
- fifa_overlay/src/scout/state.rs (new)
- fifa_overlay/src/scout/search.rs (new)
- fifa_overlay/src/scout/screens/mod.rs (new)
- fifa_overlay/src/scout/screens/theme.rs (new)
- fifa_overlay/src/scout/screens/{olheiros,missoes,relatorios,sonar}.rs (new)
- fifa_overlay/src/save_repo.rs (modified: sonda removida; allow(dead_code) em APIs futuras)
- fifa_overlay/src/async_task.rs (modified: allow(dead_code) em `is_running`/`reset`)
- fifa_overlay/Cargo.toml (modified: `Win32_UI_Input_KeyboardAndMouse`)
- fifa_overlay/assets/fonts/{Oswald-Medium.ttf, Oswald-SemiBold.ttf, Inter-Variable.ttf, OFL-Oswald.txt, OFL-Inter.txt} (new)
- fifa_overlay_debug/ (new: cópia arquivada + README)
- _bmad-output/implementation-artifacts/sprint-status.yaml (modified)
