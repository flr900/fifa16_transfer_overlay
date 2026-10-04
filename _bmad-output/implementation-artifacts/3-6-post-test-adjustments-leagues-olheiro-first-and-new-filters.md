---
baseline_commit: b7a199d
---

# Story 3.6: Post-test adjustments — leagues, Olheiro first, new filters

Status: review

## Story

As Felipe,
after testing Epics 2 and 3 in game, I want the geographic filter to follow where players actually play, the Nova Missão flow to start from the Olheiro, more filters, and a better Olheiros tab,
so that creating a Missão is faster and the filters match how I scout.

## Requests (Felipe, 2026-10-03)

1. League filters. The country picker listed every nation in the world, but there are leagues in only some of them: show only countries with clubs, and their leagues.
2. Continent-level filters, all summarised in the same geographic screen.
3. Bug: selecting the Olheiro did not work.
4. Dominant attribute filter: multi-select.
5. Age range filter.
6. Contract time filter.
7. Nova Missão: choose the Olheiro from a list first, then customise the filters.
8. Each Olheiro profile preloads the filters that suit it.
9. Olheiros tab: Cards and Tabular views of the hired Olheiros, with a final row/card to hire another.
10. Clicking a hired Olheiro: no Missão → create a Missão with him; in a Missão → open that Missão's Relatório.

## Design decisions

- **Geography = where the player plays.** The filter moved from the player's nationality (Story 2.9 map) to the league of his club: `FiltrosMissao.{continentes, paises_dos_clubes, ligas}`; a player passes if his league, its country or its continent is chosen. The old `paises` (nationality) field stays for Missões saved before; the screen no longer sets it.
- **Leagues come from the save** (`leagues`/`onMQ` + `leagueteamlinks`), only those with clubs. Special "countries" of the FIFA Friends database: national teams (75), creation zone (216) and "Clubes do Mundo" (rest of world, league 76) are out; the regional Brazilian federations (156, 217, 220, 221, 223, 224) count as Brazil; "Clubes da UEFA / Concacaf / AFC / CAF/OFC" (211) become country-less leagues of their continent; free agents (210) appear under "Outras". 79 leagues in the save → the ones with clubs.
- **One screen, a tree:** continent ("Europa inteira") → country ("Todas") → league chips, wrapping. Choosing a level absorbs the explicit choices below it; items already included by a level above show as chosen and do nothing (tooltip "Já incluído: …"). Breadth: one country (or its leagues) = País; several countries of one continent = Vários países; a whole continent = Continente; more than one continent = Mundo.
- **Text encoding fix:** team and league names in this database are Latin-1; they showed as "S�o Caetano". `fifa_db::texto` decodes UTF-8 or Latin-1.
- **Olheiro selection bug:** the radio-style Olheiro row of the form is gone (and with it the "focus = choice" behaviour that row had); the Olheiro is chosen in step 1 by activating a card. Clicking a hired Olheiro in the Olheiros tab, which did nothing before, now has an action.
- **Dominant attributes:** up to 3; with k chosen, each must be within the player's top `3 + k − 1` attributes (one: top 3; two: top 4; three: top 5). Old files with `atributo_dominante` load as a one-item list. The panel toggles; "Concluir" returns.
- **Age:** 15–45 (default: no restriction); age max ≤ 21 makes a "Jovens" Missão.
- **Contract:** years left 0–5+ (0 = ends this season; contracts end on 30/06, so the season counted is July–June). Shown as "5+" and explained in words under the row.
- **Ideal filters per Especialização** (`quality::filtros_ideais`), each giving the Missão type that matches the Olheiro: Caçador de Jovens (≤ 21, Potencial ≥ 78, Overall ≤ 72), Caçador de Medalhões (Overall ≥ 75, 24–31), Tático (playmakers: Visão + Passe curto dominant, 18–30), Generalista (broad). "Restaurar sugestão" brings them back.
- **Navigation:** Missões → "Nova Missão" → step 1 (`EscolherOlheiro`) → form; B in the form goes back to step 1. From the Olheiros tab a free Olheiro opens the form directly. Confirming goes to the Missões tab. Olheiros → "Contratar Olheiro" → offers (`ContratarOlheiro`) → confirmation modal (the offers stay visible under it) → back to the tab after hiring.
- **Olheiros tab:** Cards (default) / Tabular toggle saved in `ui_prefs.densidade_olheiros`; each Olheiro shows status, current Missão ("Missão Jovens · pronta em 12/08/2026") and the number of Relatórios delivered; the last card/row is "+ Contratar Olheiro". An Olheiro in a Missão without a Relatório yet sends to the Missões tab.

## Tasks / Subtasks

- [x] save_repo: `Liga`, `read_leagues`, `PlayerPool.ligas`, `PlayerRaw.{liga_id, contrato_ate}` (`qvmK`), Latin-1 text.
- [x] quality: `filtros_ideais`, `EscopoGeografico` + `amplitude_da_geografia` (replaces `amplitude_da_selecao`), `top_para` / `MAX_DOMINANTES`, age and contract constants, `tipo_por_filtros` with age.
- [x] search: age, contract (`anos_de_contrato`), geography (`da_geografia`), several dominant attributes (`tem_dominantes`), `read_leagues` on `CareerSource`.
- [x] state: new filters with serde compatibility, `Carga<T>`, `listar_ligas`, geography toggles, attribute toggles, `abrir_nova_missao(olheiro)`, `restaurar_filtros_ideais`, `OlheiroContratado.{missao, relatorio_atual, relatorios}`, `destino_do_olheiro`, Olheiros density pref.
- [x] Screens: `olheiros.rs` (rewritten), `escolher_olheiro.rs` (new), `selecao_geografica.rs` (rewritten), `campo_atributo.rs` (multi-select), `nova_missao.rs` (Olheiro header, age, contract, geography summary, attributes), navigation in `mod.rs`.
- [x] Tests — `cargo test`: 217 passed (leagues from the real save with Latin-1 names and the special leagues; age/contract/geography/multi-attribute filters; breadth rules; ideal filters match each Especialização; toggles absorb lower levels; old JSON loads; hired Olheiro destinations; B from the form back to step 1).
- [ ] Manual check in game (Felipe).

## Second round (Felipe, 2026-10-03, build `3.6-v2`)

Requests: work-rate filter (attack and defence: high/medium/low), dribble stars, preferred foot (left/right/two-footed), Completa should return more players than Rápida, league chips misaligned and too much on one screen (continent quick filter with a drill-down button next to it), right-backs shown as "ZAD" and full-backs coming into position filters, and an estimate of how much Overall changes in the Fit target.

- **Position codes were shifted by one** (the table had no SW): the FIFA enum is 0 GK, 1 SW, 2 RWB, 3 RB, 4 RCB, 5 CB, 6 LCB, 7 LB, 8 LWB, 9–11 DM, 12 RM, 13–15 CM, 16 LM, 17–19 AM, 20–22 F, 23 RW, 24–26 ST, 27 LW (checked: CB and ST most common; Mbappé 25 with 27 second). Fixed `nome_posicao`, `funcao_da_posicao`, `perfil_da_posicao`, `posicoes_nativas`.
- **Fit excludes trivial moves** (`PosicaoAlvo::posicoes_excluidas`): any defensive target excludes the whole back line (no full-back for right-back or centre-back); wide midfield/winger targets exclude all wide players; others exclude their own position. Thresholds re-calibrated (pass rate ~17–19% defence/striker/CDM, ~28% CM, ~45–58% attacking and wide).
- **Overall change estimate:** `quality::variacao_overall` = target-profile rating − native-profile rating (from revealed values), stored in `JogadorEncontrado.variacao_overall`; shown as "VOL ≈96% (-2)" and in the Ficha as "Como Volante: OVR ≈ 72 (-2) (estimativa)".
- **Work rate** (`BqFe`/`boFm`: 0 medium, 1 low, 2 high — Mbappé high/low), **dribble stars** (`BAPc` 0–4 → 1–5), **weak foot** (`aOBn` 1–5). Filters: work rate in attack and in defence (multi-select chips, none = any), dribble stars range, foot Qualquer/Direito/Esquerdo/Ambidestro (two-footed = weak foot ≥ 4 stars, ~23% of the save). Shown in the Ficha ("Ritmo alto/baixo · dribles 5/5 · pé fraco 4/5").
- **Rápida vs Completa rebalanced:** Rápida 7–11 players, Completa 28–44 (was 17–25 vs 6–10). A test checks that the Rápidas that fit in one Completa's time always bring fewer names.
- **Geographic screen in levels:** continents (quick filter: "Inteiro" + "Países e ligas ›"), then a continent's countries ("Inteiro" + "Ligas ›", plus country-less leagues), then a country's leagues. Fixed columns, everything vertically centred. B goes up one level.
- **Glyphs:** `‹ ›` and `≈` were missing from the font atlas (rendered as "?").

## Third round (Felipe, 2026-10-03, build `3.7-v1`)

Requests: remove the Relatório Tabular view; bring back estimated transfer value, wage and contracts ("they disappeared"); predefined filters; searches should respect the budget by default (a scout for XV de Piracicaba must not bring Mbappé) unless a "no spending cap" checkbox is ticked; Olheiro defaults based on the team's level ("muda patamar": a 71 striker changes the level of a team whose striker is 67), kept as the default; Ficha attribute list bigger and scrollable with the right stick; LB/RB switch tabs from any screen, warning before dropping a Missão being configured.

- **Ported from `claude/relatorio-ficha`** (a parallel Story 3.1 Felipe tested on 2026-10-01, never merged): estimated value and wage (`quality::valor_estimado` / `salario_estimado`; neither is in the save nor readable from memory for every player — the save only has the user's own squad contracts), contract year shown as time left ("1 ano 4 meses", highlighted at ≤ 6 months), observation in stages in partial Relatórios (market → wage → attributes), and the Scout rolling back to the save date when an older save is loaded (`Olheiro.contratado_em`, `Missao.renovacoes`, `TipoAviso::VoltouNoTempo`). The wage curve was recalibrated on the real contracts of Felipe's squad (70 ≈ 20 mil, 80 ≈ 120 mil, 87 ≈ 240 mil, 90 ≈ 300 mil per week). Its +12% font sizes came too. Its Ficha and Tabular view were not ported.
- **Relatório:** Cards only (bigger card: face, age/position/nation + Fit badge, club, OVR/POT/SIM, value and wage, contract, comparison with the squad starter, key attributes or "em observação").
- **Team level** (`quality::NivelEquipe`): Muda patamar (≥ starter + 3), Nível titular (starter ± 2), Nível banco (starter − 8 to − 3), Promessa (Potencial ≥ starter + 3). The starter is the best squad Overall in the candidate's position profile (the Fit target if any; squad average of the top 11 when nobody plays there). Default in every Olheiro's ideal filters (Caçador de Jovens: Promessa). The form shows the ruler with the user's starters.
- **Spending cap:** default on; the cap is the budget after paying the Missão, fixed at confirmation (`FiltrosMissao.teto_valor`), compared with the estimated value from real numbers. "[ ] Sem teto de gastos" turns it off. Older Missões have no cap.
- **Shortcuts** (`quality::Atalho`): Muda patamar, Jovens promessas, Nível titular, Nível banco, Fim de contrato (contract ends this season + starter level). They keep geography and the cap; the rest goes back to default.
- **Controls:** right stick scrolls every list (`gamepad::rolagem_do_analogico`, `screens::rolar_com_analogico`); LB/RB and tab clicks work from any screen; with the Nova Missão open, a "Sair da Nova Missão?" dialog asks first (B = keep editing).

## Fourth round (Felipe, 2026-10-03, build `3.7-v2`)

Requests: after moving the focus to the quick filters the gamepad could not get back to the top; the filter screen was too cluttered — prioritise geography, position, level, budget (fee limit, spending cap, contract length), search mode, and a "details" option with the rest.

- **Form in sections:** Olheiro + Atalhos at the top, then **Onde** (geography), **Posição** (new filter: 11 position groups, multi-select, none = all; `FiltrosMissao.posicoes`), **Nível** (team level), **Orçamento** (max transfer value, max weekly wage, contract left), **Busca** (mode and duration on one line), and **Mostrar detalhes** (closed by default, with a one-line summary of what is on): age, Overall, Potencial, dominant attributes, work rate, dribbles, foot, Fit Posicional, Jogador de Referência.
- **Budget limits** (`state::Limite`: do clube / até X / sem limite) for value and wage. Value "do clube" = budget after paying the Missão; wage "do clube" = the live weekly wage budget (`dqXv.wagebudget`, `save_repo::read_wage_budget`, now in `CareerSnapshot.folha_salarial`). −/+ move on a 1-2-5 scale. Both are fixed at confirmation (`teto_valor`, `teto_salario`); the search compares the estimated value and wage from real numbers.
- **Focus back to the top:** the first item ("Restaurar sugestão") scrolls the form to the top when focused; the shortcut descriptions moved from a focus tooltip to the line below the buttons.

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: post-test adjustments implemented (build `3.6-v1`).
- 2026-10-03: second round (build `3.6-v2`).
- 2026-10-03: third round (build `3.7-v1`), with the port from `claude/relatorio-ficha`.
- 2026-10-03: fourth round (build `3.7-v2`): form in sections, position filter, wage cap.
