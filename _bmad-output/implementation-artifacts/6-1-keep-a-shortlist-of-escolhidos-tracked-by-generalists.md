---
baseline_commit: 79252a3
---

# Story 6.1: Keep a shortlist of Escolhidos tracked by Generalists

Status: done

## Story

As Felipe,
I want to add players to a list of Escolhidos whose stats stay available for a while and slowly go stale, and to assign Generalist Olheiros to keep them updated until their values are exact,
so that I can follow the players I care about without commissioning a new Missão for each one.

## Request (Felipe, 2026-10-04)

> Adicionar jogadores a uma lista de escolhidos. Nessa lista os jogadores ficam com os stats disponíveis até 1 ano. Passados 6 meses a acurácia pode começar a diminuir até que em 1 ano e 6 meses seria necessária uma análise nova. Olheiros generalistas podem ser atribuídos para manter o scout; cada olheiro, dependendo do nível, mantém X jogadores atualizados. Com o generalista atuando, as stats passam a ficar precisas com o tempo, exatas depois de um tempo de simulação que depende de quão especificado estava o jogador ao ser enviado à lista. Mais de um olheiro pode ser atribuído.

## Acceptance Criteria

1. The Ficha of a Relatório player has "Adicionar aos Escolhidos"; the player is stored with what the Olheiro revealed so far (ranges, observed attributes), the Relatório date and its precision. A player already in the list shows "Já está nos Escolhidos" and an "ESCOLHIDO" badge on his Relatório card.
2. New tab **Escolhidos** (between Relatórios and Sonar) with one card per player: face, name, state badge (ACOMPANHADO / ATUALIZADO / ENVELHECENDO / DESATUALIZADO / VENCIDO, always in words), PRIORIDADE, OVR/POT ranges, observed attributes and a one-line situation. Activating a card opens his Ficha (with Priorizar / Tirar prioridade and Remover dos Escolhidos).
3. **Aging** (from the last observation): up to 6 months as observed; from 6 months to 1½ years the ranges widen linearly up to ±8 (marked DESATUALIZADO from 1 year); at 1½ years the analysis expires and attributes are hidden ("é preciso uma análise nova"). The file always keeps the last observation.
4. **Olheiros do acompanhamento** (satellite of the tab): only free Generalists (focus = Generalista) can be designated; several can be. Each keeps `half-stars of Generalista` players updated (2.5★ → 5, 4.5★ → 9, 5★ → 10). A designated Olheiro does not accept Missões (shown as "Acompanhando"; clicking him opens the Escolhidos tab); one in a Missão cannot be designated.
5. **Vacancies:** prioritised players first, then the oldest in the list. A tracked player does not age; a player who loses his vacancy stops being tracked and ages from his last observation.
6. **Tracking:** on each panel opening (and when designations, priority or the list change), tracked players not yet observed that day are re-observed from the save in an `AsyncTask` (never on the render thread). Precision closes linearly and unobserved attributes appear linearly until exact after `10 × initial ± + 2 × missing attributes` career days (min 7): ~10 days for an Alta-quality player, ~6 months for a Baixa one. An expired player restarts from a fresh analysis (±14, 6 attributes). Bio, club and contract follow the save; the Fit of the origin Missão is recomputed.
7. When the value becomes exact and the player was a false positive of his Missão (Story 5.1), the card and the Ficha say "FORA DO FILTRO / ele não passa no filtro da Missão de origem".
8. **Back in time:** loading an older save removes Escolhidos added after it, undoes designations made after it, and moves later observations back to the save date (the extra tracked days are subtracted).
9. A player who left the save (retired) keeps his last observation and is not re-read endlessly.

## Design decisions

- "Até 1 ano" + "a partir de 6 meses" + "1 ano e 6 meses": fresh ≤ 180 days, widening 180–540 days (DESATUALIZADO from 365), expired ≥ 540.
- Capacity scales with the Generalista stars (the "level"), not the Tier, so a Generalist's quality matters; only focus-Generalista Olheiros qualify, as Felipe asked for "olheiros generalistas".
- Coverage is evaluated when tracking runs (panel openings). Between openings the time since the last observation counts as tracked when the player had a vacancy.
- Re-observation reveals ranges from the current save values (the Olheiro follows the player's development); nothing beyond the current precision is stored.

## Tasks / Subtasks

- [x] quality: `frescor`, `capacidade_acompanhamento`, `dias_para_exato`, `precisao_acompanhada`, `atributos_acompanhados`.
- [x] search: `reobservar`.
- [x] state: `Escolhido`/`Acompanhamento`/`EscolhidoNaLista`, `escolhido_em`, `ordem_de_acompanhamento`, `avancar_escolhido`, add/remove/priority, `designar_acompanhamento`, `atualizar_escolhidos`/`processar_escolhidos`, Ficha from the list, rollback.
- [x] Screens: `escolhidos.rs`, `acompanhamento.rs`, Ficha buttons and situation line, Relatório badge, tab and satellite routing.
- [x] Tests — `cargo test`: 267 passed (aging thresholds, tracking to exact on the fake save, vacancies and priority, designation rules, retired player, false positive reveal, rollback).
- [x] Independent review fixes: no endless save re-reads when the state file can't be written; requests during a running update are re-run; re-observation keeps the already observed attributes first and recomputes similarity; stable ImGui IDs on the toggling Ficha buttons; observation date of continuous Missões follows the player's block.
- [ ] Known gap: a player added from a partial Relatório before his salary was observed shows the salary estimate after a restart (`observacao` is not persisted).
- [ ] Manual check in game (Felipe).
