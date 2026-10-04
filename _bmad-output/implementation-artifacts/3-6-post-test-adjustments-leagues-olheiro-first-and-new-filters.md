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

## Dev Agent Record

### Agent Model Used

claude-opus-5-5

### Change Log

- 2026-10-03: post-test adjustments implemented (build `3.6-v1`).
- 2026-10-03: second round (build `3.6-v2`).
