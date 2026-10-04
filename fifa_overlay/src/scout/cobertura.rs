//! `scout::cobertura` — Sonar de Cobertura (FR-11, Épico 4): onde a rede
//! de scouting já olhou, por país, a partir das Missões gravadas. Só
//! leitura sobre o estado persistido; nenhuma leitura do jogo.
//!
//! Regras (Story 4.1):
//! - Missão ativa = `Pendente` ou `EmExecucao` (inclusive uma contínua com
//!   Relatório parcial); concluída = `Concluida`, com o Relatório
//!   arquivado ou não (arquivar nunca apaga cobertura).
//! - Um país com Missão ativa E concluída aparece como ativo; as duas
//!   contagens continuam disponíveis para o resumo (Story 4.2).
//! - Missão sem filtro geográfico ("todos os países") não pinta o mapa
//!   inteiro: conta só em `globais`, para o mapa continuar informativo.

use std::collections::HashMap;

use super::state::{Missao, StatusMissao};

/// Missões ativas e concluídas num lugar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Contagem {
    pub ativas: u32,
    pub concluidas: u32,
}

impl Contagem {
    pub fn vazia(&self) -> bool {
        self.ativas == 0 && self.concluidas == 0
    }

    fn somar(&mut self, status: StatusMissao) {
        match status {
            StatusMissao::Pendente | StatusMissao::EmExecucao => self.ativas += 1,
            StatusMissao::Concluida => self.concluidas += 1,
        }
    }
}

/// Como um país aparece no Sonar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoPais {
    NuncaEscaneado,
    MissaoAtiva,
    MissaoConcluida,
}

/// Cobertura de uma carreira.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cobertura {
    /// Por `Crbb.nationid` (`search::NACAO_OUTROS` = quadro "Outros").
    por_pais: HashMap<u16, Contagem>,
    /// Missões sem filtro geográfico.
    pub globais: Contagem,
    /// Total de Missões da carreira (0 = Sonar vazio).
    pub total: usize,
}

impl Cobertura {
    pub fn de(missoes: &[Missao]) -> Cobertura {
        let mut cobertura = Cobertura { total: missoes.len(), ..Cobertura::default() };
        for missao in missoes {
            let paises = &missao.filtros.paises;
            if paises.is_empty() {
                cobertura.globais.somar(missao.status);
                continue;
            }
            for (i, pais) in paises.iter().enumerate() {
                // um país repetido no filtro conta uma vez só
                if paises.get(..i).is_some_and(|antes| antes.contains(pais)) {
                    continue;
                }
                cobertura.por_pais.entry(*pais).or_default().somar(missao.status);
            }
        }
        cobertura
    }

    pub fn contagem(&self, pais: u16) -> Contagem {
        self.por_pais.get(&pais).copied().unwrap_or_default()
    }

    /// Ativa tem precedência sobre concluída.
    pub fn estado(&self, pais: u16) -> EstadoPais {
        let c = self.contagem(pais);
        if c.ativas > 0 {
            EstadoPais::MissaoAtiva
        } else if c.concluidas > 0 {
            EstadoPais::MissaoConcluida
        } else {
            EstadoPais::NuncaEscaneado
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::search::NACAO_OUTROS;
    use uuid::Uuid;

    const BRASIL: u16 = 54;
    const ARGENTINA: u16 = 52;
    const INGLATERRA: u16 = 14;

    fn missao(status: StatusMissao, paises: &[u16]) -> Missao {
        let mut m = Missao::de_teste(Uuid::new_v4(), status);
        m.filtros.paises = paises.to_vec();
        m
    }

    #[test]
    fn without_missoes_every_country_is_never_scanned() {
        let c = Cobertura::de(&[]);
        assert_eq!(c.total, 0);
        assert_eq!(c.estado(BRASIL), EstadoPais::NuncaEscaneado);
        assert!(c.contagem(BRASIL).vazia());
        assert!(c.globais.vazia());
    }

    #[test]
    fn pending_and_running_missoes_are_active_and_concluded_ones_are_completed() {
        let c = Cobertura::de(&[
            missao(StatusMissao::Pendente, &[BRASIL]),
            missao(StatusMissao::EmExecucao, &[ARGENTINA]),
            missao(StatusMissao::Concluida, &[INGLATERRA]),
        ]);
        assert_eq!(c.estado(BRASIL), EstadoPais::MissaoAtiva);
        assert_eq!(c.estado(ARGENTINA), EstadoPais::MissaoAtiva);
        assert_eq!(c.estado(INGLATERRA), EstadoPais::MissaoConcluida);
        assert_eq!(c.estado(60), EstadoPais::NuncaEscaneado);
    }

    #[test]
    fn active_takes_precedence_and_both_counts_are_kept() {
        let c = Cobertura::de(&[
            missao(StatusMissao::Concluida, &[BRASIL]),
            missao(StatusMissao::Concluida, &[BRASIL, ARGENTINA]),
            missao(StatusMissao::Pendente, &[BRASIL]),
        ]);
        assert_eq!(c.estado(BRASIL), EstadoPais::MissaoAtiva);
        assert_eq!(c.contagem(BRASIL), Contagem { ativas: 1, concluidas: 2 });
        assert_eq!(c.contagem(ARGENTINA), Contagem { ativas: 0, concluidas: 1 });
    }

    #[test]
    fn missoes_without_a_geographic_filter_only_count_as_global() {
        let c = Cobertura::de(&[missao(StatusMissao::Pendente, &[]), missao(StatusMissao::Concluida, &[])]);
        assert_eq!(c.globais, Contagem { ativas: 1, concluidas: 1 });
        assert_eq!(c.estado(BRASIL), EstadoPais::NuncaEscaneado);
        assert_eq!(c.total, 2);
    }

    #[test]
    fn the_outros_tile_and_repeated_countries_are_counted_once_per_missao() {
        let c = Cobertura::de(&[missao(StatusMissao::Pendente, &[NACAO_OUTROS, BRASIL, BRASIL])]);
        assert_eq!(c.estado(NACAO_OUTROS), EstadoPais::MissaoAtiva);
        assert_eq!(c.contagem(BRASIL), Contagem { ativas: 1, concluidas: 0 });
    }
}
