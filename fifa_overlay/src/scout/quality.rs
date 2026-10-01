//! `scout::quality` — regras de balanceamento do Scout, num lugar só
//! (AD-3: fórmulas aqui, filtros em `search`, orquestração em `state`).
//!
//! Story 1.4 traz o custo de CONTRATAR cada Olheiro. A Story 2.1
//! acrescenta aqui custo, duração e Qualidade das Missões. Para
//! rebalancear depois de jogar, mexa só nas tabelas deste módulo: nenhuma
//! tela nem outro módulo guarda número de balanceamento.
//!
//! Funções puras e síncronas: nada de `persistence` nem `search` (AD-1).

use super::state::{Especializacao, Tier};

/// Custo de contratação (mesma unidade do `transferbudget` do save).
///
/// Critério da primeira versão (2026-10-01, a calibrar jogando):
/// - o Tier pesa mais que a Especialização: Júnior ≈ 0,3–0,5 M,
///   Experiente ≈ 1,2–1,9 M, Elite ≈ 3,6–5,8 M — numa carreira com ~64 M
///   de orçamento, um Elite custa ~5–9% e um Júnior menos de 1%;
/// - dentro do Tier: Generalista (relatórios rasos) < Caçador de Jovens <
///   Tático < Caçador de Medalhões (jogadores prontos, os mais cobiçados);
/// - os 12 valores são distintos (FR-2), conferido em teste.
pub fn custo_contratacao(especializacao: Especializacao, tier: Tier) -> i32 {
    use Especializacao::*;
    use Tier::*;
    match (tier, especializacao) {
        (Junior, Generalista) => 300_000,
        (Junior, CacadorDeJovens) => 400_000,
        (Junior, Tatico) => 450_000,
        (Junior, CacadorDeMedalhoes) => 500_000,
        (Experiente, Generalista) => 1_200_000,
        (Experiente, CacadorDeJovens) => 1_500_000,
        (Experiente, Tatico) => 1_700_000,
        (Experiente, CacadorDeMedalhoes) => 1_900_000,
        (Elite, Generalista) => 3_600_000,
        (Elite, CacadorDeJovens) => 4_500_000,
        (Elite, Tatico) => 5_200_000,
        (Elite, CacadorDeMedalhoes) => 5_800_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todos() -> Vec<(Especializacao, Tier)> {
        Tier::TODOS
            .iter()
            .flat_map(|&t| Especializacao::TODAS.iter().map(move |&e| (e, t)))
            .collect()
    }

    #[test]
    fn twelve_distinct_positive_costs() {
        let mut custos: Vec<i32> = todos().into_iter().map(|(e, t)| custo_contratacao(e, t)).collect();
        assert_eq!(custos.len(), 12);
        assert!(custos.iter().all(|&c| c > 0));
        custos.sort_unstable();
        custos.dedup();
        assert_eq!(custos.len(), 12, "custos repetidos");
    }

    #[test]
    fn a_higher_tier_always_costs_more_than_any_lower_tier() {
        let maximo = |tier| Especializacao::TODAS.iter().map(|&e| custo_contratacao(e, tier)).max();
        let minimo = |tier| Especializacao::TODAS.iter().map(|&e| custo_contratacao(e, tier)).min();
        assert!(maximo(Tier::Junior) < minimo(Tier::Experiente));
        assert!(maximo(Tier::Experiente) < minimo(Tier::Elite));
    }

    #[test]
    fn generalista_junior_is_the_cheapest() {
        let mais_barato = todos().into_iter().min_by_key(|&(e, t)| custo_contratacao(e, t));
        assert_eq!(mais_barato, Some((Especializacao::Generalista, Tier::Junior)));
    }
}
