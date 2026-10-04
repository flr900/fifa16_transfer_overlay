---
baseline_commit: 79252a3
---

# Story 5.1: Olheiros with identity, stars, markets and a monthly market

Status: review

## Story

As Felipe,
I want each Olheiro to be a person — name, nationality, star attributes, markets he knows — offered by a market that depends on how attractive my club is, with penalties when I send him outside his market or his focus, a travel budget per Missão and reports that can contain false positives,
so that hiring and assigning Olheiros becomes a real decision.

Covers all seven items of `_bmad-output/planning-artifacts/melhorias-futuras-olheiros.md` (Felipe, 2026-10-01; "vamos endereçar todas as melhorias", 2026-10-04).

## Acceptance Criteria

1. **Identity (item 1).** Every Olheiro from the market has a generated name that follows his nation (`scout::nomes`, Latin-1 only because the overlay font has no other glyphs) and a nationality; the hire modal lets me change the name before confirming. Nationality changes the hiring cost (Europe +10%, South America +5%, others −5%).
2. **Stars (item 2).** Four attributes 0–5★ in half steps (Caçador de Jovens, Caçador de Medalhões, Tático, Generalista) plus Rede de contatos. The focus is the highest attribute; the Tier is the summary of the focus (≤ 2.5★ Júnior, 3–4★ Experiente, ≥ 4.5★ Elite). Quality uses the attribute of the Missão type (the Geral type uses Generalista; for the others, Generalista − 1★ is a floor); Rede de contatos sets the deadline (`130 − 5 × half-stars` %).
3. **Monthly market (item 3).** The hiring screen lists 4–9 Olheiros generated per career month, more and better for attractive clubs (`quality::atratividade`: domestic/international prestige 0–20 from `teams`, league division, career trophies). Roughly 1 in 4 offers is Elite for a top club and almost none for a small one. Local Olheiros are the most common. A hired offer leaves that month's list.
4. **Markets and adaptation (item 4).** Each new Olheiro knows his country; Experiente may and Elite always know their continent; some know a second country. Distance 0 (home) / 1 (same continent, or a worldwide search) / 2 (another continent) costs up to 1★ quality and 2★ speed, in half steps, reduced linearly by days already worked in that place (180 days to adapt). Working a whole continent counts for its countries.
5. **Out of focus (item 5).** A specialist on a Missão that is not his focus loses 1★ quality until he has worked that type for 6 months (Elite), 9 months (Experiente) or 1 year (Júnior). Geral Missões and Generalists never get it.
6. **Travel budget (item 6).** Nova Missão has a "Verba da viagem" row: Econômica (60% cost, ¾ of the players, ±1 wider, −½★, slower), Padrão, Reforçada (160% cost, 1¼ players, +½★, faster). Each option shows its cost; the range grows with the scale of the search. The chosen budget is saved on the Missão.
7. **False positives (item 7).** Below Alta quality, part of the Relatório (Baixa 25%, Média 10%) comes from players who nearly pass the filter (Overall/Potential ±4, age ±1, contract ±1, caps +25%, team level 3 points of slack; geography, positions and profile filters stay strict). They are not marked in the Relatório; revealed ranges still contain the real values. The Lista de Escolhidos shows "FORA DO FILTRO" once tracking reaches the exact value (Story 6.1).
8. **Legacy Olheiros** (hired before the stars) keep working: they get `PerfilOlheiro::v1`, which reproduces the Story 2.1 table exactly (test `the_v1_profile_reproduces_the_story_2_1_table`), no market (= no market penalty) and the Especialização as display name. Old state files load; the format is now v2 so an older DLL never overwrites the new fields.
9. Nova Missão warns, under the Olheiro, why and how much the estimate is penalised ("a busca fica em outro continente… Qualidade −1 estrela(s), velocidade −2 estrela(s) até ele se adaptar").

## Design decisions (open questions of the improvements doc, settled 2026-10-04)

- **Q1 market distance:** league → its country; same country or a known continent = 0, same continent = 1, other continent = 2; no geography (world) = 1. Worst region of the filter wins.
- **Q2 adaptation:** by career days worked, computed from the Olheiro's own Missões (`trabalhos_do_olheiro`): nothing new is persisted, and "back in time" stays consistent for free.
- **Q3 Tier:** kept as the summary of the focus stars (cost multiplier and badge).
- **Q4 false positives:** unmarked in the Relatório; revealed by exact tracking in the Lista de Escolhidos.
- **Q5 migration:** `#[serde(default)]` fields + `Olheiro::perfil()` derivation; `VERSAO_FORMATO = 2`; v1 files are upgraded in place on the next write.
- **Q6 club data:** `teams.domesticprestige` (`ppLE`), `internationalprestige` (`edvw`), `leagueteamlinks`, `career_trophies.flags` (`KNNX.glmx`, one bit per trophy, one row per season). Checked on the versioned save (Barcelona 20/20, La Liga division 1, 21 trophies in 13 seasons).

## Tasks / Subtasks

- [x] quality: `Estrelas`, `PerfilOlheiro` (+ `v1`), `Mercado`/`Regiao`, `distancia_mercado`, `penalidade_missao` with adaptation, `Investimento`, `PerfilClube`/`atratividade`, `gerar_ofertas`, `custo_contratacao(perfil, …)`, `falsos_positivos`/`quase_no_nivel`; `estimar_missao` on stars.
- [x] nomes: name lists per language group, Latin-1 test.
- [x] save_repo: `read_club_profile` / `DadosDoClube` (+ real-save test).
- [x] search: `read_club_profile` on `CareerSource`, false positives (`missao_folgada`).
- [x] state: Olheiro fields, `mercado_de_olheiros`, hire from an offer with an editable name, `ofertas_contratadas`, penalties and budget in `previa_missao`, rollback of hired offers.
- [x] Screens: Olheiros cards with name/nation/markets/stars (drawn stars, no glyph), monthly market screen with attractiveness, hire modal with name input, Nova Missão warning and "Verba da viagem".
- [x] Tests — `cargo test`: 267 passed.
- [x] Independent review fixes: legacy Olheiros get no focus penalty either (same numbers as v1, test); the month's attractiveness is frozen in the state file (`mercado_do_mes`) so offers don't re-roll mid-month; a failed nations read no longer restarts every frame.
- [ ] Manual check in game (Felipe): market variety for your club, penalty warnings, budget costs, names.
