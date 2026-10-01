---
stepsCompleted: ['step-01-validate-prerequisites', 'step-02-design-epics', 'step-03-create-stories', 'step-04-final-validation']
inputDocuments:
  - _bmad-output/planning-artifacts/prds/prd-FIFA_EDITOR-2026-09-21/prd.md
  - _bmad-output/planning-artifacts/architecture/architecture-FIFA_EDITOR-2026-09-22/ARCHITECTURE-SPINE.md
  - _bmad-output/planning-artifacts/ux-designs/ux-FIFA_EDITOR-2026-09-21/DESIGN.md
  - _bmad-output/planning-artifacts/ux-designs/ux-FIFA_EDITOR-2026-09-21/EXPERIENCE.md
---

# Central de Scout (FIFA 16 Companion) - Epic Breakdown

## Overview

This document provides the complete epic and story breakdown for the Central de Scout, decomposing the requirements from the PRD, UX Design and Architecture Spine into implementable stories. Domain vocabulary (Olheiro, Missão, Relatório, Qualidade, Fit Posicional, Sonar de Cobertura…) follows the PRD glossary and is kept in Portuguese.

## Requirements Inventory

### Functional Requirements

FR1: The user can open/close the Scout panel with a dedicated, configurable shortcut (keyboard or controller combo) distinct from the overlay toggle, working in exclusive fullscreen by reusing the existing `Present()` hook; with no career loaded the panel shows an explanatory empty state.
FR2: The user can list available Olheiros (4 Especializações × 3 Tiers = up to 12 combinations), each with a distinct hire cost visible before confirmation. No simultaneous-slot limit in v1.
FR3: The user can hire an Olheiro, debiting its cost from `dqXv.transferbudget` via the validated memory write; blocked with the missing amount if budget is insufficient; the new value is re-read and displayed; the write happens only on explicit confirmation.
FR4: The user can create a Missão linked to an available Olheiro, combining freely: geographic filter (country/league/continent, multi-select), Overall/Potencial range, dominant attribute, Fit Posicional, Jogador de Referência. Broader geography lowers estimated precision (shown before confirming). Each Olheiro runs one Missão at a time.
FR5: The user can choose Modo de Busca Rápida (more names, lower Qualidade, faster) or Completa (fewer names, higher Qualidade, slower); estimated completion time and Missão cost are shown and recalculated on change before confirming.
FR6: The user can filter by Fit Posicional (players whose attribute profile fits a position other than their `preferredposition1`); the Relatório shows native position, target position and fit strength.
FR7: The user can pick a Jogador de Referência from the current squad (read from the save) and the Missão finds similar-profile players; the Relatório shows a similarity indicator per player.
FR8: The user can see all active Missões with progress based on `GJUr.currdate` (recomputed on panel open) and a clear completion indication; completed Missões stay visible until the Relatório is archived/discarded. Missão state persists between sessions in a Companion-owned state file.
FR9: The user can open a Relatório showing per player: name, age, native position (+ Fit Posicional), attributes — with precision, number of revealed attributes and number of players determined by Qualidade (low = ranges/subset, high = exact/full profile).
FR10: The user can view a Radar de Atributos per player (revealed axes only, never invented values), optionally overlaid with a Jogador de Referência from the squad (UJ-3).
FR11: The user can view a Sonar de Cobertura map (per country) distinguishing active-Missão, completed-Missão (Relatório available) and never-scanned countries.

### NonFunctional Requirements

NFR1 (derived): The only save field written by this feature is `dqXv.transferbudget`; no player attribute writes, no writes to `zlrC`/`apoo`, and no real transfers (PRD §5).
NFR2 (derived): The panel must respond instantly (no long transition animations) over a game running at ~60fps; heavy `CZUM` scans (~32k records, ~17s) must never block the render thread (AD-4).
NFR3 (derived): Scout state persists in a per-save JSON file (write-through, single mutex) and survives overlay inject/eject and game crashes (AD-7, AD-11).
NFR4 (derived): Supports only FIFA 16 build `16.0.2904053`, Career Mode single-player.
NFR5 (derived): Accessibility floor — indicators always use colour + text; visible focus in mouse and gamepad modes; interactive targets ≥ 32px; truncated table text has a tooltip (EXPERIENCE.md).
NFR6 (derived): Rust conventions `[ADOPTED]`: no `panic!`/`unwrap()`/`expect()`, slice access via `.get(a..b)`, `tracing` logs tagged `[scout::…]`, Portuguese `//!` module comments.
NFR7 (derived): Sustained play must not cause perceptible FPS loss from concurrent scans (PRD Open Question 6 — to be measured in early search stories).

> Note: the PRD has no explicit NFR section; NFR1–NFR7 are derived from PRD §5/§8, Architecture and UX. Confirm or correct.

### Additional Requirements

- **No starter template** — brownfield: new modules inside the existing `fifa_overlay` crate (Rust 2021, hudhook 0.9 dx11, imgui 0.12, windows 0.62).
- New dependencies: `serde` (derive) 1.0, `serde_json` 1.0, `uuid` 1.26 (v4, serde), `dirs` 7.0; new `windows` feature `Win32_UI_Input_KeyboardAndMouse`.
- AD-1: strict layering `scout::screens → scout::state → {search, persistence} → save_repo → memscan/fifa_db/pointer_scan`; `search → quality`; `state` is the only caller of `persistence`.
- AD-2/AD-3: `save_repo` is the only door to memscan/fifa_db; it returns raw data (`PlayerRaw`) and exposes `read_squad_players`, `read_all_players`, `write_transfer_budget`, `read_transfer_budget`, `read_current_date`; filters live in `scout::search`, formulas in `scout::quality`; `search::executar_missao(missao) -> Relatorio` is the sole orchestrator.
- AD-4: generic `AsyncTask<T>` (`TaskState { Idle, Running, Done(T), Failed(SaveRepoError) }`, non-destructive `poll()`), required for any full-`CZUM` scan; cost/time/quality preview is synchronous.
- AD-5: `SaveRepoError` enum (`TabelaNaoEncontrada`, `ProcessoInacessivel`, `CarreiraNaoCarregada`, …) mapped to UX states.
- AD-6: navigation stack `Vec<ScoutScreen>` with non-poppable `stack[0]` = active tab, separate `painel_aberto` bool, stack reset on close, max depth 2.
- AD-7: single `Arc<Mutex<ScoutStateFile>>` write-through for all mutations including `ui_prefs` (last tab, density).
- AD-8: Missão status `Pendente | EmExecucao | Concluida`; deadline check only on `painel_aberto` false→true edge; set `EmExecucao` before dispatch (anti-duplicate guard); running tasks survive panel close.
- AD-9/AD-10: single FIFO queue (by `prazo_estimado`, then `criada_em`) for any full-`CZUM` scan; lightweight tasks independent.
- AD-11: save identity via memory (`GJUr.startdate`, `mPrV.firstname/surname`, `mPrV.clubteamid`), file name = SHA-256 hex; state at `%LOCALAPPDATA%\FifaCompanion\scout\<hash>.json`. First task: empirically validate `GJUr.startdate` stability across sessions.
- AD-12: UUID v4 ids for `Olheiro`, `Missao`, `Relatorio`; `JogadorEncontrado` keyed by `playerid`; dates as `Date(i32)` `YYYYMMDD` (serde transparent).
- AD-13: single `seletor_elenco` screen (contexts `FiltroMissao`/`ComparacaoFicha`) reached only through `scout::state::listar_elenco_atual()`.
- AD-14: shortcut via `GetAsyncKeyState` polling inside `render()` with edge-trigger.
- Deferred formulas (design content to settle during implementation): Qualidade, Fit Posicional, similarity, hire/Missão cost table.
- Open item: behaviour on active-save switch is obsolete per AD-11 (one file per save) — UX "ver mesmo assim/descartar" state should be dropped.

### UX Design Requirements

UX-DR1: Implement DESIGN.md colour tokens (bg-base, bg-panel 93%, bg-panel-raised 96%, hairlines, text tiers, accent-primary `#b45cff`, field-green `#3ecf6e`, tier/quality colours, danger) as a central theme applied to ImGui style.
UX-DR2: Load and apply typography roles — Oswald (display 600 / heading 500), Inter (body/meta 400), Consolas mono for numeric attribute columns; no italics; caps only for badges.
UX-DR3: Apply spacing scale (4/8/12/16/24/32), radii (sm 4, md 8, lg 12, default 6) and no shadows/glow/gradients; panel sized ≈70% × 75% of game resolution, centred, game visible around it.
UX-DR4: Panel window (`panel-window`) with hairline border and lg radius; fixed tab bar (Olheiros / Missões / Relatórios / Sonar) with active purple fill; tab switch preserves scroll/selection per tab.
UX-DR5: Tier badge component (JR / EXP / ELITE) and Quality badge (Baixa / Média / Alta) — outline + tenuous fill, colour AND text, reused everywhere.
UX-DR6: Primary (field-green) and secondary (outlined) buttons with visible disabled state.
UX-DR7: Mission progress bar (track + purple fill) always paired with estimate text ("pronto em ~N dias de carreira").
UX-DR8: Olheiros tab — cards with Especialização, Tier badge, status (Disponível / Em Missão), hire button or busy indicator; empty state "Nenhum Olheiro contratado ainda." with hire list visible below.
UX-DR9: Confirmação de Contratação modal showing exact cost and resulting budget; confirm disabled with exact missing amount when insufficient.
UX-DR10: Formulário Nova Missão as a console-style vertical list (Olheiro, Filtro geográfico, Overall/Potencial inline range, Atributo dominante, Fit Posicional, Jogador de Referência, Modo de Busca) with fixed footer summary (cost / time / estimated Qualidade) live-updating and visible inside field panels; confirm disabled without a valid Olheiro.
UX-DR11: Full-screen field panels (Atributo dominante, Fit Posicional, Jogador de Referência) keeping tab bar/header visible.
UX-DR12: Painel de Seleção Geográfica — full-screen country map in selection mode; cumulative multi-select by click or gamepad (D-pad adjacency cursor, A toggle, B/RB confirm).
UX-DR13: Missões tab — active missions with progress bar, "novo" indicator on completed/unseen Relatório.
UX-DR14: Relatórios tab — Tabular/Cards density toggle (persisted); Tabular rows with mono numeric columns, truncated text + tooltip; Cards with real miniface from `data/ui/imgAssets/heads/p<PLAYERID>.dds` (DDS load) and silhouette fallback; Fit Posicional badge; archive action and "Arquivados" filter (never hard-deleted).
UX-DR15: Ficha de Jogador — single screen with bio header, full revealed-attribute list and Radar, always-visible "Comparar com jogador do elenco" button.
UX-DR16: Radar de Atributos via ImDrawList — solid purple main outline, dashed green overlay for Jogador de Referência, unrevealed axes dotted/empty (never zero).
UX-DR17: Mapa Sonar (view mode) per-country shapes via ImDrawList — none / purple outline+tint (active) / green outline+tint (completed); click shows textual summary of active/completed counts.
UX-DR18: State patterns — no career loaded, save read error with retry, insufficient budget, Olheiro busy (not selectable), Relatório de baixa Qualidade.
UX-DR19: Input parity — mouse+keyboard and gamepad (D-pad/analog focus, confirm/cancel, LB/RB tab switch), identical purple-border focus style; no swipe, no long animations.
UX-DR20: Accessibility — colour never the sole indicator, sequential visible focus, ≥32px targets.
UX-DR21: Microcopy in Portuguese per Voice and Tone (factual, no exclamation/emoji, exact values).

### FR Coverage Map

FR1: Epic 1 - Dedicated shortcut opens/closes the panel; empty state without a career
FR2: Epic 1 - List Olheiros (4 Especializações × 3 Tiers) with costs
FR3: Epic 1 - Hire an Olheiro, debit `transferbudget`
FR4: Epic 2 + Epic 3 - Missão with combinable filters (geo, Overall/Potencial, attribute in Epic 2; Fit Posicional, Jogador de Referência in Epic 3)
FR5: Epic 2 - Rápida vs. Completa with cost/time preview
FR6: Epic 3 - Fit Posicional filter
FR7: Epic 3 - Jogador de Referência filter
FR8: Epic 2 - Mission progress and persistence
FR9: Epic 2 - Relatório with variable Qualidade
FR10: Epic 3 - Radar de Atributos with overlay
FR11: Epic 4 - Sonar de Cobertura

## Epic List

### Epic 1: Open the Central de Scout and hire Olheiros
The user opens the themed Scout panel with a dedicated shortcut over the running game, sees the available Olheiros with their costs, and hires one; the cost is debited from the real `transferbudget`, read back and shown, and hires persist between sessions.
**FRs covered:** FR1, FR2, FR3

### Epic 2: Commission a Missão and receive a Relatório
The user creates a Missão with a hired Olheiro using geographic (map), Overall/Potencial and dominant-attribute filters, picks Rápida or Completa with a live cost/time/Qualidade preview, and — once the career date passes the deadline — opens a Relatório whose precision depends on its Qualidade, in Tabular or Cards view, and can archive it.
**FRs covered:** FR4 (geo, Overall/Potencial, attribute filters), FR5, FR8, FR9

### Epic 3: Find players by profile and compare them with your squad
The user adds Fit Posicional and Jogador de Referência filters to a Missão, sees fit strength and similarity in the Relatório, and opens a Ficha de Jogador with the Radar de Atributos, overlaying a squad player for direct comparison.
**FRs covered:** FR4 (Fit Posicional, Jogador de Referência filters), FR6, FR7, FR10

### Epic 4: See your scouting coverage on the Sonar
The user opens the Sonar tab and sees which countries have an active Missão, a completed one, or have never been scanned, and can click a country for its counts.
**FRs covered:** FR11

## Epic 1: Open the Central de Scout and hire Olheiros

Press a shortcut over the running game, open the themed Scout panel, and hire an Olheiro that debits the real `transferbudget` and persists between sessions.

### Story 1.1: Read career state and identify the active save

As Felipe,
I want the Companion to read the career date and transfer budget and to identify which career is loaded,
So that every Scout feature works from the real state of my active save.

**Acceptance Criteria:**

**Given** a career is loaded in FIFA 16
**When** `save_repo::read_current_date()` and `read_transfer_budget()` are called
**Then** they return `GJUr.currdate` as a `Date` (YYYYMMDD) and `dqXv.transferbudget` as `i32`, both wrapped in `Result<_, SaveRepoError>`
**And** the values match what the game shows for the same career.

**Given** a career is loaded
**When** `save_repo::identify_active_save()` runs
**Then** it returns `SHA-256(startdate|firstname|surname|clubteamid)` in lowercase hex, built from `GJUr.startdate`, `mPrV.firstname`, `mPrV.surname` and `mPrV.clubteamid` (AD-11)
**And** the hash is a valid Windows file name even when the manager name contains accents or reserved characters.

**Given** the same career is loaded in two separate game sessions, and a different career is loaded in a third
**When** `identify_active_save()` runs in each session
**Then** the two sessions of the same career produce the same hash
**And** the different career produces a different hash
**And** the result is documented: if `GJUr.startdate` is not stable across sessions, the finding and the fallback chosen are recorded before the story is closed.

**Given** no career is loaded, or the process or table cannot be reached
**When** any `save_repo` function is called
**Then** it returns `CarreiraNaoCarregada`, `ProcessoInacessivel` or `TabelaNaoEncontrada` as appropriate
**And** it never panics; it uses no `unwrap()`/`expect()` and accesses slices via `.get()` (NFR6)
**And** no `scout::*` code calls `memscan`, `fifa_db` or `pointer_scan` directly (AD-2).

**Given** the game build is not FIFA 16 `16.0.2904053`
**When** `save_repo` initialises
**Then** it logs the mismatch and reports `TabelaNaoEncontrada` rather than reading unverified memory (NFR4).

**Given** locating the career database requires a full-memory scan (~17 s)
**When** `save_repo` needs it
**Then** the scan runs only through the generic `AsyncTask<T>` introduced in this story (`async_task.rs`, AD-4), never on the render thread
**And** once located, field reads are cheap synchronous reads at the cached address, re-validated against the DB signature.

**Given** the heap copy of the database may be a stale snapshot
**When** this story is implemented
**Then** it first proves, with a documented test, whether `GJUr.currdate` and `dqXv.transferbudget` in that copy track live changes, and records the read strategy chosen (Stories 1.5, 2.2 and 2.3 depend on it).

### Story 1.2: Open and close the Scout panel with a dedicated shortcut

As Felipe,
I want to toggle the Central de Scout with its own shortcut while FIFA 16 runs in fullscreen,
So that I can check my scouting without leaving the game.

**Acceptance Criteria:**

**Given** the overlay is running over FIFA 16 in exclusive fullscreen
**When** I press the Scout shortcut (distinct from the overlay toggle)
**Then** the panel opens, and pressing it again closes it
**And** no video-mode switch happens (the existing `Present()` hook is reused).

**Given** the shortcut key is held down
**When** frames render
**Then** the panel toggles only once, on the released-to-pressed transition (edge-trigger via `GetAsyncKeyState` polling in `render()`, AD-14)
**And** the `windows` crate has the `Win32_UI_Input_KeyboardAndMouse` feature enabled.

**Given** the panel is open
**When** it renders
**Then** it shows the themed `panel-window` with the 4 fixed tabs (Olheiros / Missões / Relatórios / Sonar), the active tab filled purple and the others in secondary text
**And** it covers roughly 70% × 75% of the game resolution, centred, with the game visible around it
**And** it applies the DESIGN.md tokens: colours, Oswald / Inter / Consolas fonts, spacing scale, radii, panel opacity ≥ 90%, and no shadows, glow or gradients (UX-DR1–4)
**And** tabs that have no content yet show a neutral placeholder.

**Given** I switch tabs and then close and reopen the panel
**When** it reopens
**Then** it opens on the top-level tab (navigation stack reset to `[ultima_aba_ativa]`, AD-6)
**And** `pop()` on a stack of length 1 is a no-op, and `push` beyond depth 2 is refused with `tracing::warn!`.

**Given** no career is loaded
**When** the panel is open
**Then** any tab shows "Nenhuma carreira carregada. Abra uma carreira no FIFA 16 para usar a Central de Scout." and renders no data.

**Given** a save read fails with `ProcessoInacessivel`
**When** the panel is open
**Then** it shows "Não foi possível ler o save ativo." with a retry button, and the panel does not freeze.

**Given** the career database is still being located (about 17 s on first use)
**When** the panel is open
**Then** it shows "Localizando carreira…" and stays responsive.

**Given** the panel is open
**When** a tab changes
**Then** switching is instant, with no long transition animation, and each tab keeps its own scroll position (NFR2, UX-DR19).

### Story 1.3: Persist Scout state per save

As Felipe,
I want the Scout's data to be saved per career,
So that my Olheiros and preferences survive closing the game or a crash.

**Acceptance Criteria:**

**Given** a career is loaded
**When** the Scout state is first needed
**Then** a JSON file is created at `%LOCALAPPDATA%\FifaCompanion\scout\<hash>.json`, where `<hash>` is the Story 1.1 save hash
**And** the file carries the `Olheiro`/`Missao`/`Relatorio` collections (empty at first) and a `ui_prefs` section, with `Uuid` v4 ids as canonical strings and dates as plain `YYYYMMDD` integers (AD-12).

**Given** any state mutation, whether domain data or a `ui_prefs` value such as the active tab
**When** it is applied
**Then** it goes through the single `Arc<Mutex<ScoutStateFile>>` in `scout::persistence`: lock, update, serialize, write the whole file, unlock (AD-7)
**And** only `scout::state` calls `persistence` (AD-1).

**Given** the active tab was Sonar when the panel closed
**When** the game is restarted and the panel reopened for the same career
**Then** it opens on the Sonar tab.

**Given** the state file is missing, empty or corrupt
**When** the state loads
**Then** the Scout starts with empty state and logs `[scout::persistence]` via `tracing::warn!`
**And** it does not overwrite a corrupt file without first keeping a backup copy.

**Given** two different careers
**When** each is loaded
**Then** each reads and writes only its own file.

### Story 1.4: Browse the Olheiros available to hire

As Felipe,
I want to see every Especialização × Tier Olheiro with its hire cost,
So that I can decide whom to hire before spending any budget.

**Acceptance Criteria:**

**Given** the Olheiros tab is open
**When** it renders
**Then** it lists the 12 combinations (Caçador de Jovens, Caçador de Medalhões, Tático, Generalista × Júnior, Experiente, Elite), each as a card with its Especialização, a Tier badge and its hire cost
**And** each combination has a distinct cost from a balancing table kept in one place in code.

**Given** a Tier badge or status indicator
**When** it renders
**Then** it shows JR / EXP / ELITE as outline plus tenuous fill, in grey / purple / gold, always with text and never colour alone (UX-DR5, NFR5)
**And** the available Olheiros are shown as hireable.

**Given** no Olheiro has been hired
**When** the tab renders
**Then** it shows "Nenhum Olheiro contratado ainda." with the hire list visible below it.

**Given** the current budget is shown
**When** the tab opens
**Then** it displays the `transferbudget` read through `scout::state` and `save_repo`, not from the screen directly.

**Given** a card is hovered or focused
**When** it renders
**Then** it shows a purple border, and the hire button is at least 32px tall (UX-DR20).

### Story 1.5: Hire an Olheiro

As Felipe,
I want to confirm the hiring of an Olheiro and have its cost debited from my transfer budget,
So that I have a scout ready to receive a Missão.

**Acceptance Criteria:**

**Given** the budget covers the cost
**When** I press "Contratar" on a card
**Then** a confirmation modal shows the exact cost and the resulting budget, with a primary (green) confirm button and a secondary cancel button.

**Given** the confirmation modal is open
**When** I press confirm
**Then** `save_repo::write_transfer_budget` writes the new value, which is only ever done on explicit confirmation
**And** the value is read back and the panel shows the confirmed balance
**And** a new `Olheiro` with a UUID v4, its Especialização and its Tier is persisted write-through with status "Disponível"
**And** it appears immediately in the hired list.

**Given** the budget is lower than the cost
**When** the modal opens
**Then** the confirm button is disabled
**And** the text shows the exact missing amount, e.g. "Orçamento insuficiente: faltam R$ 2.1M.", with no exclamation marks or emoji (UX-DR9, UX-DR21).

**Given** the write fails or the read-back does not match the expected value
**When** I confirm
**Then** no Olheiro is persisted
**And** the panel shows a clear error with retry, and never claims success.

**Given** the feature hires an Olheiro
**When** I inspect what it writes to the save
**Then** `dqXv.transferbudget` is the only save field written (NFR1).

### Story 1.6: Navigate the panel with a gamepad

As Felipe,
I want to move through the Scout panel with a gamepad,
So that I never have to put the controller down to use it.

**Acceptance Criteria:**

**Given** the panel is open and a controller is connected
**When** I use the D-pad or left analog
**Then** focus moves in reading order (tabs → content → actions) across the tab bar, Olheiro cards, hire buttons and modal buttons.

**Given** a focused element
**When** I press the confirm button
**Then** the element activates, and the cancel button closes a modal or goes back one screen.

**Given** I press LB/RB (L1/R1)
**When** the panel is open
**Then** it switches to the previous or next tab directly.

**Given** mouse and gamepad are used in the same session
**When** an element has focus or hover
**Then** both show the identical purple solid border, so switching input never loses the focus position (UX-DR19).

**Given** this behaviour depends on the hudhook/imgui gamepad support
**When** the story is started
**Then** feasibility is checked first, and if it is not achievable, the limitation and a keyboard-navigation fallback are documented.

### Story 1.7: Start with the game and preload the career

*(Added 2026-10-01 at Felipe's request after Story 1.3: manual injection and a ~15 s locate on the first F10 made a poor first experience.)*

As Felipe,
I want the Central de Scout to load with the game and to have my career ready before I press F10,
So that I never run the injector by hand or wait in front of an empty panel.

**Acceptance Criteria:**

**Given** I start my session with the launcher script instead of the FIFA Friends shortcut
**When** it runs
**Then** it opens the FIFA Friends server (the same target as the shortcut), waits for `fifa16.exe`, and injects the overlay once the game has loaded DirectX and shows a visible window
**And** it never injects twice into the same game process, waits again if the game is reopened, and exits when the FIFA Friends server closes.

**Given** the overlay has just been injected
**When** the first frame renders
**Then** a banner in the top-right corner shows "Central de Scout ativa" and disappears after 3 seconds, without taking mouse, keyboard or focus from the game.

**Given** the panel is closed and I enter a career
**When** the overlay notices a career is loaded (cheap background check, never on the render thread — AD-4)
**Then** it locates the career by itself and the banner shows "Carregando carreira…" while it runs, then "Carreira pronta" with the manager and date for 3 seconds, or a short failure message with the F10 hint for 3 seconds
**And** a failed locate is not retried in a loop: only after leaving the career, or through F10 / "Tentar novamente".

**Given** the career was already located in the background
**When** I press F10
**Then** the panel opens directly on the career's saved tab, with no "Localizando carreira…" wait.

## Epic 2: Commission a Missão and receive a Relatório

Create a Missão with a hired Olheiro, see its cost, time and expected Qualidade before confirming, and when the career date passes the deadline, open a Relatório whose precision depends on Qualidade.

### Story 2.1: Define the scouting balance table

As Felipe,
I want the cost, duration and Qualidade rules of a Missão defined in one tunable place,
So that I can rebalance scouting after real play without touching UI code.

**Acceptance Criteria:**

**Given** a Tier, an Especialização, a Modo de Busca, a Missão type and a geographic breadth
**When** `scout::quality` estimates a Missão
**Then** pure synchronous functions return cost, duration in career days and a Qualidade level (Baixa / Média / Alta) with a revealed-attribute count, precision band and player-count target
**And** all constants live in a single documented balance table in one module, with no magic numbers elsewhere.

**Given** the rules
**When** unit tests run
**Then** they prove: higher Tier gives higher Qualidade; Completa gives higher Qualidade, fewer players and a longer duration than Rápida; broader geography lowers precision; an Especialização matching the Missão type improves Qualidade
**And** outputs are deterministic for the same inputs.

**Given** `scout::quality`
**When** I inspect its dependencies
**Then** it calls no `persistence` and no `search` (AD-1, AD-3).

### Story 2.2: Create a Missão with an Olheiro, Overall/Potencial range and Modo de Busca

As Felipe,
I want to configure a Missão and see its cost, time and expected Qualidade before confirming,
So that I know exactly what I'm paying for.

**Acceptance Criteria:**

**Given** at least one Olheiro is "Disponível"
**When** I press "Nova Missão" on the Missões tab
**Then** the Nova Missão form opens as a vertical list of rows (Olheiro, Overall/Potencial, Modo de Busca) in `bg-panel-raised` with hairline dividers, and pushes onto the navigation stack (UX-DR10, AD-6)
**And** only "Disponível" Olheiros are selectable, and those "Em Missão" are visible but disabled.

**Given** no Olheiro is hired or available
**When** I open the form
**Then** it shows "Nenhum Olheiro disponível." and confirm is disabled.

**Given** I change the Olheiro, the Overall/Potencial range (adjusted inline on its row) or the Modo de Busca (Rápida / Completa)
**When** the value changes
**Then** a fixed footer recalculates cost, estimated time and Qualidade badge synchronously using Story 2.1, without touching `save_repo` (AD-4).

**Given** the budget covers the Missão cost
**When** I confirm
**Then** the cost is debited via `write_transfer_budget` with read-back (same guarantees as Story 1.5)
**And** a `Missao` with UUID v4, filters, Modo de Busca, `criada_em`, `prazo_estimado` and status `Pendente` is persisted write-through
**And** the Olheiro becomes "Em Missão"
**And** no search runs at creation time (AD-8).

**Given** the budget does not cover the cost
**When** the form is open
**Then** confirm is disabled and the exact missing amount is shown (UX-DR21).

**Given** the Overall/Potencial range is invalid (min > max)
**When** I try to confirm
**Then** confirm is disabled with an explanatory message.

### Story 2.3: Follow the progress of active Missões

As Felipe,
I want to see each Missão's progress and estimated completion,
So that I know when to come back for the Relatório.

**Acceptance Criteria:**

**Given** active Missões exist
**When** I open the Missões tab
**Then** each shows its Olheiro, Modo de Busca, a progress bar (track `bg-panel-raised`, purple fill) and text like "pronto em ~N dias de carreira", never the bar alone (UX-DR7)
**And** progress is computed from `GJUr.currdate`, `criada_em` and `prazo_estimado`, recomputed when the panel opens, with no real-time polling (FR8).

**Given** the career date is at or past `prazo_estimado`
**When** I open the tab
**Then** the Missão shows as ready or completed, not as a negative or over-100% progress.

**Given** no Missão exists
**When** I open the tab
**Then** it shows an empty state with the "Nova Missão" button still available.

**Given** the date cannot be read
**When** the tab renders
**Then** the generic read-error state with retry appears, without losing the Missão list.

### Story 2.4: Run due Missões and generate Relatórios

As Felipe,
I want a Missão to run its search when its deadline has passed,
So that the Relatório is ready when I come back, without the game freezing.

**Acceptance Criteria:**

**Given** `AsyncTask<T>` already exists from Story 1.1
**When** this story is done
**Then** `save_repo::read_all_players()` returns raw `PlayerRaw` data and is only called through an `AsyncTask` (~32k `CZUM` records), following the AD-4 conventions.

**Given** a `Pendente` Missão whose `prazo_estimado` has passed
**When** the panel goes from closed to open (edge only, never polled while open)
**Then** the status changes to `EmExecucao` before dispatch, then the search task is dispatched (AD-8)
**And** closing and reopening the panel while it runs never dispatches a second task for the same Missão.

**Given** several Missões are due on the same reopen
**When** they are dispatched
**Then** they run one at a time from a single FIFO queue ordered by `prazo_estimado`, then `criada_em`, and never in parallel (AD-9).

**Given** the search runs
**When** `scout::search::executar_missao(missao)` executes
**Then** it applies the Overall/Potencial filter, calls `scout::quality` to set Qualidade, and returns a complete `Relatorio`; `save_repo` receives no filter criteria (AD-3)
**And** the number of players and how precisely each attribute is revealed follow the Qualidade (low = fewer attributes, ranges; high = exact).

**Given** the task finishes
**When** it reports `Done`
**Then** the `Relatorio` is persisted write-through and the Missão becomes `Concluida`, even if the panel was closed meanwhile (AD-7, AD-8)
**And** the Olheiro becomes "Disponível" again.

**Given** the task fails or the game closes mid-run
**When** the state is next loaded
**Then** a Missão left in `EmExecucao` is reset to `Pendente` so it cannot stay stuck
**And** a failure is shown with a clear message, not silently.

**Given** a scan is running in a long session
**When** I play
**Then** the render thread is never blocked, and FPS with and without a running scan is measured and the result recorded (NFR2, NFR7).

### Story 2.5: Read a Relatório in the Tabular view

As Felipe,
I want to open a finished Relatório and read the players found,
So that I can pick who to go and negotiate for in the game.

**Acceptance Criteria:**

**Given** a Missão is `Concluida` and its Relatório has not been opened
**When** I open the Missões or Relatórios tab
**Then** a "novo" indicator is shown on it until I open it the first time.

**Given** I open a Relatório
**When** the Tabular view renders
**Then** each row shows name → age → native position → numeric attribute columns in Consolas (mono) aligned digit by digit, with dividers and no cards (UX-DR14)
**And** long text is truncated with an ellipsis and a full tooltip on hover or focus (NFR5).

**Given** the Relatório has low Qualidade
**When** it renders
**Then** values appear as ranges (e.g. "Overall: 65-78") and only a subset of attributes is revealed, never invented values
**And** a Qualidade badge (Baixa / Média / Alta) is shown, always with text.

**Given** the Relatório has high Qualidade
**When** it renders
**Then** values are exact or near-exact and the full attribute profile is revealed.

**Given** no Relatório exists
**When** I open the Relatórios tab
**Then** it shows an empty state.

### Story 2.6: Switch between Tabular and Cards with real minifaces

As Felipe,
I want a Cards view with player faces,
So that I can recognise players at a glance.

**Acceptance Criteria:**

**Given** the Relatórios tab
**When** I look at the top
**Then** an always-visible toggle offers Tabular / Cards, and the choice is persisted in `ui_prefs` across sessions (UX-DR14).

**Given** the Cards view
**When** a player card renders
**Then** it shows the miniface loaded from `data/ui/imgAssets/heads/p<PLAYERID>.dds`, then name (heading), age, native position and key attributes in `body`/mono
**And** if the file does not exist for that `playerid`, a neutral silhouette is used.

**Given** many cards
**When** the list scrolls
**Then** textures are loaded lazily and cached, without hitching the game.

### Story 2.7: Archive and restore Relatórios

As Felipe,
I want to archive Relatórios I have already reviewed,
So that the list stays clean without losing what my budget paid for.

**Acceptance Criteria:**

**Given** a Relatório that has been opened
**When** I press "Arquivar"
**Then** it disappears from the main list and appears under an "Arquivados" filter; it is never deleted.

**Given** the "Arquivados" filter
**When** I press "Restaurar"
**Then** the Relatório returns to the main list.

**Given** a Relatório not yet opened
**When** I view it
**Then** "Arquivar" is not offered.

### Story 2.8: Filter a Missão by dominant attribute

As Felipe,
I want to ask for "the best dribbler" or "the best defender",
So that I find profiles instead of just Overall.

**Acceptance Criteria:**

**Given** the Nova Missão form
**When** I activate the "Atributo dominante" row
**Then** a full-screen field panel opens with the tab bar and header still visible and the footer summary still visible (UX-DR11), listing the dominant-attribute options
**And** choosing one returns to the form with the row showing the choice.

**Given** a Missão with this filter
**When** the search runs
**Then** `scout::search` keeps only players whose dominant attribute matches, combined (AND) with the other filters.

**Given** the filter is combined with the Overall/Potencial filter
**When** both are set
**Then** both apply.

### Story 2.9: Filter a Missão by country on the map

As Felipe,
I want to pick the countries to scout by clicking on a map,
So that I can target regions and see how breadth affects quality.

**Acceptance Criteria:**

**Given** the Nova Missão form
**When** I activate the "Filtro geográfico" row
**Then** a full-screen Painel de Seleção Geográfica opens with a per-country tile cartogram (one tile per country, grouped by continent, drawn via `ImDrawList`), and the footer summary stays visible (UX-DR12)
**And** the cartogram is a reusable component that the Sonar will share.

**Given** the map
**When** I click countries
**Then** each click toggles inclusion cumulatively, without a modifier key; selected countries are outlined purple with tenuous fill
**And** confirming returns to the form with the selection summarised on the row.

**Given** the selection is broad (e.g. all continents) or narrow
**When** I change it
**Then** the estimated Qualidade badge and precision estimate update live, broader meaning lower precision (FR4).

**Given** a Missão with countries selected
**When** the search runs
**Then** `scout::search` keeps only players whose `Crbb.nationid` belongs to the chosen countries.

**Given** no country is selected
**When** I confirm
**Then** it means "all countries" and the form says so explicitly.

**Given** a nation in the save has no tile
**When** the map renders
**Then** it falls under an explicit "outros" tile and is never silently dropped.

## Epic 3: Find players by profile and compare them with your squad

Add Fit Posicional and Jogador de Referência filters to a Missão, and open any found player in a Ficha with a Radar that can be overlaid on a squad player.

### Story 3.1: Open a player's Ficha with the Radar de Atributos

As Felipe,
I want to tap a player in a Relatório and see a complete profile with a radar,
So that I judge his shape at a glance.

**Acceptance Criteria:**

**Given** a Relatório in either Tabular or Cards view
**When** I click a player
**Then** the Ficha de Jogador opens as a satellite screen pushed on the navigation stack, and back (or gamepad cancel) returns to the same Relatório scroll position (AD-6).

**Given** the Ficha
**When** it renders
**Then** one single screen shows the biographic header (name, age, native position, preferred foot, nationality), the full list of revealed numeric attributes, and the Radar de Atributos, with no sub-tabs (UX-DR15)
**And** the miniface appears as in Story 2.6.

**Given** the Radar
**When** it draws via `ImDrawList`
**Then** it has one axis per revealed attribute, a solid purple outline for the found player (UX-DR16)
**And** unrevealed axes are dotted and empty, never zero and never invented (FR10).

**Given** a low-Qualidade Relatório
**When** I open a Ficha
**Then** ranges are shown as ranges, and the radar plots only the axes that have values.

**Given** the Ficha is open for a Relatório that has been viewed
**When** I look at its actions
**Then** "Arquivar" from Story 2.7 is also reachable here.

### Story 3.2: Compare a found player with a squad player

As Felipe,
I want to overlay my starter's radar on the found player's,
So that I can decide whether it's worth replacing him.

**Acceptance Criteria:**

**Given** the Ficha
**When** I look at its actions
**Then** a "Comparar com jogador do elenco" button is always visible.

**Given** I press it
**When** the squad selector opens
**Then** `scout::screens::seletor_elenco` is pushed on the stack with context `ComparacaoFicha`, and its label states that it is for comparison (AD-13)
**And** it lists the current squad (~30 players) through `scout::state::listar_elenco_atual()` → `save_repo::read_squad_players()`, called synchronously, never directly from the screen (AD-1, AD-4).

**Given** I choose a squad player
**When** I return to the Ficha
**Then** a second radar is overlaid with a dashed green outline, distinct from the purple solid one and legible, and the squad player's name appears in a legend
**And** the rest of the Ficha stays visible, with no navigation away (FR10, UJ-3).

**Given** an axis exists for the squad player but is unrevealed for the found player
**When** the overlay draws
**Then** the found player's axis stays empty, and the squad player's value is still shown, so the comparison does not mislead.

**Given** the squad cannot be read
**When** the selector opens
**Then** it shows the read-error state with retry.

### Story 3.3: Filter a Missão by Jogador de Referência

As Felipe,
I want to find players whose profile resembles one of my squad players,
So that I scout "another one like him".

**Acceptance Criteria:**

**Given** the Nova Missão form
**When** I activate the "Jogador de Referência" row
**Then** the same `seletor_elenco` opens, now with context `FiltroMissao` and a label stating it guides the search (AD-13), full screen with the footer summary still visible (UX-DR11)
**And** choosing a player returns to the form with the row showing his name.

**Given** a Missão with a reference player
**When** the search runs
**Then** `scout::quality` computes a profile similarity between each candidate and the reference, and `scout::search` keeps candidates above a documented threshold or ranks them by similarity (AD-3)
**And** the formula is documented and unit tested: identical profiles score 100%, and an unrelated profile scores low.

**Given** a Relatório from such a Missão
**When** it renders
**Then** each player shows a similarity indicator (%) with the reference player's name, in both Tabular and Cards views (FR7).

**Given** a low-Qualidade Relatório
**When** similarity is shown
**Then** it is marked approximate, computed from the revealed attributes only.

**Given** this filter is combined with Overall/Potencial, dominant attribute and geographic filters
**When** the search runs
**Then** all apply together (FR4 combinability).

### Story 3.4: Filter a Missão by Fit Posicional

As Felipe,
I want to search for players who could play a different position than their listed one,
So that I find, say, a midfielder who can work as a holding player.

**Acceptance Criteria:**

**Given** the Nova Missão form
**When** I activate the "Fit Posicional" row
**Then** a full-screen field panel lists the target positions, with the footer summary visible (UX-DR11), and choosing one returns to the form.

**Given** a target position
**When** `scout::quality` computes fit
**Then** it compares the player's attribute profile with a documented ideal profile for that position and returns a strength score; the ideal profiles are in one place in the balance table (FR6, AD-3)
**And** the formula is unit tested with a representative case (an attacking midfielder with high defensive attributes scores high for holding midfielder).

**Given** a Missão with Fit Posicional
**When** the search runs
**Then** `scout::search` keeps players whose fit for the target position exceeds the threshold and whose native position is different from the target
**And** it is combinable (AND) with every other filter.

**Given** a Tático Olheiro vs. a non-Tático one on this Missão type
**When** the estimate is computed
**Then** the Tático Olheiro yields the higher Qualidade (FR6, Story 2.1 rule)
**And** the form footer reflects it live.

### Story 3.5: See Fit Posicional in the Relatório

As Felipe,
I want to see each player's native position, target position and fit strength,
So that I understand why he was suggested.

**Acceptance Criteria:**

**Given** a Relatório from a Fit Posicional Missão
**When** the Tabular view renders
**Then** a Fit Posicional column sits after the native position, showing the target position and strength (UX-DR14).

**Given** the Cards view
**When** a card renders
**Then** an extra badge on the native-position line shows the target position and fit strength, outline plus tenuous fill, with text and never colour alone (UX-DR5).

**Given** the Ficha
**When** it renders for such a player
**Then** the header shows native position plus the Fit Posicional.

**Given** a Relatório without Fit Posicional
**When** it renders
**Then** no fit column or badge appears.

## Epic 4: See your scouting coverage on the Sonar

Open the Sonar tab and see which countries have an active Missão, a completed one, or have never been scanned, and click a country for its counts.

### Story 4.1: See coverage by country on the Sonar map

As Felipe,
I want a map that shows where my scouting network is working,
So that I know where I have and haven't looked.

**Acceptance Criteria:**

**Given** the Sonar tab
**When** it renders
**Then** it shows the country tile cartogram from Story 2.9 in view-only mode, drawn via `ImDrawList` (UX-DR17)
**And** each country is one of three states: never scanned (no fill), active Missão (purple outline plus tenuous fill), or completed Missão with a Relatório available (field-green outline plus tenuous fill)
**And** no solid saturated fill, glow or gradient is used.

**Given** a country has both an active and a completed Missão
**When** it renders
**Then** the active state takes precedence visually, and the summary from Story 4.2 shows both counts.

**Given** a Missão with no geographic filter ("all countries")
**When** the Sonar renders
**Then** it does not colour every tile; it is counted in a "Missões globais" line above the map, so the map stays informative (assumption to confirm).

**Given** a legend
**When** it renders
**Then** it names the three states in text next to their swatches, so state is never conveyed by colour alone (NFR5).

**Given** an archived Relatório
**When** coverage is computed
**Then** its Missão still counts as completed coverage.

**Given** no Missão exists
**When** the tab renders
**Then** the map shows every country as never scanned, with a short explanatory line, not an error.

### Story 4.2: Read a country's Missão counts

As Felipe,
I want to click a country and see how many Missões are active or completed there,
So that I can decide whether to scout it again.

**Acceptance Criteria:**

**Given** the Sonar map
**When** I click a country tile
**Then** a textual summary appears on the same tab with the country name and the counts of active and completed Missões, with no navigation to another tab (UX-DR17).

**Given** a country with no Missão
**When** I click it
**Then** the summary says it has never been scanned.

**Given** a gamepad
**When** I move between tiles and press confirm
**Then** the same summary appears, and the focused tile shows the purple focus border (UX-DR19).
