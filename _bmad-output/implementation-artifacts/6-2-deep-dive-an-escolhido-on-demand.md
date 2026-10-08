---
baseline_commit: 11dbb86
---

# Story 6.2: Deep-dive an Escolhido on demand ("Aprofundar agora")

Status: done

## Story

As Felipe,
I want to tell a Generalist to dedicate himself to one Escolhido and pay for it,
so that a player I care about becomes exact in days instead of waiting for the slow, free tracking.

## Origin (2026-10-08)

The PRD left the "núcleo de olheiros detalhistas" (a second, deeper stage on a player already found) out of the v1 and called it the strongest candidate for the first expansion. Story 6.1 already deepens Escolhidos for free and slowly with Generalists, so Felipe chose to **extend the Generalist** instead of adding a new kind of Olheiro, and to offer it **only from the Escolhidos**.

| | Normal tracking (6.1) | Aprofundar agora (6.2) |
|---|---|---|
| Who | any designated Generalist, by vacancy | the designated Generalist with most stars sets the pace |
| Cost | free | paid from the transfer budget |
| Speed | `quality::dias_para_exato` | 40% (2,5★) down to 15% (5★) of that, min 3 days |
| Queue | by priority, then age | first: ahead of priority |

## Acceptance Criteria

1. The Ficha of an Escolhido has **"Aprofundar agora"**, enabled only when the player is in the list, is not already exact, is not already being deepened and at least one Generalist is designated to the tracking. While it runs the button reads "Aprofundando" and is disabled.
2. Clicking it opens an aviso ("Aprofundar agora?") over the panel with: the Generalist, the days (against the normal deadline), what is left to reveal (±N and missing attributes), the cost, and the vacancy it takes. The money leaves only on confirming; focus starts on "Cancelar"; B cancels.
3. Cost = `quality::custo_aprofundamento(precisao, faltam)`: 20 mil + 5 mil per ± point + 3 mil per missing attribute, rounded to 10 mil (≈30 mil for a Quality Alta player, ≈160 mil for a Baixa one; a Missão Rápida costs 70 mil). It is debited with `comprar` (compare-and-write, read-back, undone if the file cannot be saved), like Missões and hires. Failures say why and nothing is kept (insufficient budget, budget changed, file not writable...).
4. Deadline = `quality::dias_para_aprofundar(normal, percentual)` with `percentual = 65 − 5 × half-stars` of the best designated Generalist, never below 3 days nor above the normal deadline. The percentage is stored in the request, so a later change of Generalists does not change a running deep-dive.
5. The deep-dive restarts the tracking from what the player is worth today (his aged observation, or a fresh analysis if it expired) and reaches precision 0 with every attribute at the short deadline. It sorts **first** in the vacancy order (before priority), so it always takes a vacancy while any Generalist is designated.
6. The card shows an **APROFUNDANDO** state (situation "Aprofundando: ±N agora, exato em ~D dias de carreira.", short "Aprofundando · exato em ~D dias"). When the player is exact the request stops counting (no state, no queue priority).
7. If the vacancy goes away (the Generalist is released or dismissed) the request ends; loading a save older than the request undoes it (the money comes back by itself: the budget is the loaded save's). A reading that was running when the request was made never overwrites it.
8. The game's native knowledge level follows as for any Escolhido (Epic 7): as the observation closes in, `quality::nivel_no_jogo` raises the player toward 198.

## Design decisions

- **No new Olheiro focus.** The Generalist already means "keeps the Escolhidos"; stars already scale vacancies, now also speed.
- **Percentage in the request** (`Aprofundamento { desde, percentual }`) instead of the Generalist id: the Generalist can be released or dismissed without breaking a started request, and nothing in the file depends on which Olheiro served it.
- **`aprofundando_ativo()` is derived** (request present and not exact), so the request is never "cleared" on finishing and the result of an in-flight reading cannot resurrect or lose it.
- Numbers are first guesses to calibrate in play; all live in `scout::quality` (`percentual_aprofundamento`, `dias_para_aprofundar`, `custo_aprofundamento`, `DIAS_MINIMOS_APROFUNDAMENTO`).

## Tasks

- [x] quality: `percentual_aprofundamento`, `dias_para_aprofundar`, `custo_aprofundamento`.
- [x] state: `Escolhido.aprofundando` (`#[serde(default)]`, old files stay valid), `Aprofundamento`, `aprofundando_ativo`, queue order, short deadline in `avancar_escolhido`, merge guard, end on lost vacancy, rollback, `pode_aprofundar` / `pedir_` / `aprofundamento_pendente` / `cancelar_` / `confirmar_aprofundamento`.
- [x] screens: `aprofundamento.rs` (aviso), Ficha button, card state, B to cancel (`mod.rs`), modal wiring (`screens/mod.rs`).
- [x] Tests — `cargo test`: 401 passed (cost and speed tables; full flow: needs a Generalist, cancel charges nothing, confirm debits, jumps the queue, exact at the short deadline; insufficient budget; lost vacancy and older save; first step uses the short deadline; texts).
- [x] Manual check in game (Felipe, 2026-10-08): working as expected.

## Known gaps

- `observacao` (staged observation of a partial Relatório) is not persisted, as in 6.1.
- A deep-dive cannot be cancelled once paid (no refund), like a Missão.
