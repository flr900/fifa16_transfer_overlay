//! `scout::cobertura` — Sonar de Cobertura (FR-11, Épico 4): onde a rede
//! de scouting já olhou, por país, a partir das Missões gravadas. Só
//! leitura sobre o estado persistido e as ligas da carreira; nenhuma
//! busca no jogo.
//!
//! Desde 2026-10-03 o filtro geográfico é "onde o jogador joga" (país do
//! clube, liga ou continente inteiro), então o Sonar é por país do CLUBE:
//! - `paises_dos_clubes` pinta o país; uma liga pinta o país dela.
//! - Continente inteiro (ou liga continental, sem país) não pinta os
//!   países um a um: conta em `por_continente`, numa linha própria.
//! - Missão sem filtro geográfico conta só em `globais`, para o mapa
//!   continuar informativo (Story 4.1).
//! - Missões antigas filtravam por nacionalidade (`paises`, Story 2.9):
//!   o id é o mesmo (`Crbb.nationid`), então pintam o mesmo quadro.
//!
//! Missão ativa = `Pendente` ou `EmExecucao` (inclusive uma contínua com
//! Relatório parcial); concluída = `Concluida`, com o Relatório arquivado
//! ou não (arquivar nunca apaga cobertura). Ativa tem precedência no
//! quadro; as duas contagens ficam para o resumo (Story 4.2).

use std::collections::{BTreeSet, HashMap};

use super::search::NACAO_OUTROS;
use super::state::{Confederacao, Liga, Missao, Nacao, StatusMissao};

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
    /// Por `Crbb.nationid` (`search::NACAO_OUTROS` = quadro "Outros" das
    /// Missões antigas).
    por_pais: HashMap<u16, Contagem>,
    /// Missões de continente inteiro.
    por_continente: HashMap<Confederacao, Contagem>,
    /// Missões sem filtro geográfico.
    pub globais: Contagem,
    /// Total de Missões da carreira (0 = Sonar vazio).
    pub total: usize,
}

impl Cobertura {
    /// `ligas`: as da carreira, para achar o país de cada liga escolhida
    /// (uma liga que não está lá é ignorada).
    pub fn de(missoes: &[Missao], ligas: &[Liga]) -> Cobertura {
        let mut cobertura = Cobertura { total: missoes.len(), ..Cobertura::default() };
        for missao in missoes {
            let f = &missao.filtros;
            // conjuntos: cada Missão conta uma vez por lugar
            let mut paises: BTreeSet<u16> = f.paises.iter().chain(&f.paises_dos_clubes).copied().collect();
            let mut continentes: BTreeSet<Confederacao> = f.continentes.iter().copied().collect();
            for liga in f.ligas.iter().filter_map(|id| ligas.iter().find(|l| l.id == *id)) {
                match liga.pais {
                    Some(pais) => paises.insert(pais),
                    None => continentes.insert(liga.continente),
                };
            }
            if paises.is_empty() && continentes.is_empty() {
                // só liga desconhecida: tinha filtro, não é global
                if !f.tem_geografia() {
                    cobertura.globais.somar(missao.status);
                }
                continue;
            }
            for pais in paises {
                cobertura.por_pais.entry(pais).or_default().somar(missao.status);
            }
            for continente in continentes {
                cobertura.por_continente.entry(continente).or_default().somar(missao.status);
            }
        }
        cobertura
    }

    pub fn contagem(&self, pais: u16) -> Contagem {
        self.por_pais.get(&pais).copied().unwrap_or_default()
    }

    /// Missões do continente inteiro (não inclui as de países dele).
    pub fn do_continente(&self, continente: Confederacao) -> Contagem {
        self.por_continente.get(&continente).copied().unwrap_or_default()
    }

    /// Continentes com Missão de continente inteiro, na ordem do mapa.
    pub fn continentes(&self) -> Vec<(Confederacao, Contagem)> {
        Confederacao::TODAS.into_iter().map(|c| (c, self.do_continente(c))).filter(|(_, c)| !c.vazia()).collect()
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

    /// Há Missão antiga no quadro "Outros" (nacionalidades fora da lista).
    pub fn tem_outros(&self) -> bool {
        !self.contagem(NACAO_OUTROS).vazia()
    }

    /// Quadros do mapa: os países com liga na carreira (onde uma Missão
    /// pode procurar hoje) mais os países que alguma Missão cobriu sem
    /// estar nessa lista (Missão antiga por nacionalidade). O nome e o
    /// continente desses vêm de `nacoes`; sem elas, "País N" em "Outros".
    pub fn quadros(&self, ligas: &[Liga], nacoes: &[Nacao]) -> Vec<Nacao> {
        let mut quadros: Vec<Nacao> = Vec::new();
        for liga in ligas {
            let Some(pais) = liga.pais else { continue };
            if !quadros.iter().any(|q| q.id == pais) {
                quadros.push(Nacao { id: pais, nome: liga.pais_nome.clone(), iso: String::new(), confederacao: liga.continente });
            }
        }
        let mut extras: Vec<u16> =
            self.por_pais.keys().copied().filter(|id| *id != NACAO_OUTROS && !quadros.iter().any(|q| q.id == *id)).collect();
        extras.sort_unstable();
        for id in extras {
            let nacao = nacoes.iter().find(|n| n.id == id).cloned().unwrap_or_else(|| Nacao {
                id,
                nome: format!("País {id}"),
                iso: String::new(),
                confederacao: Confederacao::Outras,
            });
            quadros.push(nacao);
        }
        quadros
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    const BRASIL: u16 = 54;
    const ARGENTINA: u16 = 52;
    const INGLATERRA: u16 = 14;
    const ESPANHA: u16 = 45;

    fn liga(id: u32, pais: Option<u16>, nome_pais: &str, continente: Confederacao) -> Liga {
        Liga { id, nome: format!("Liga {id}"), pais, pais_nome: nome_pais.to_string(), continente, nivel: 1, clubes: 20 }
    }

    fn ligas() -> Vec<Liga> {
        vec![
            liga(13, Some(INGLATERRA), "England", Confederacao::Europa),
            liga(14, Some(INGLATERRA), "England", Confederacao::Europa),
            liga(53, Some(ESPANHA), "Spain", Confederacao::Europa),
            liga(7, Some(BRASIL), "Brazil", Confederacao::AmericaDoSul),
            liga(900, None, "", Confederacao::Europa),
        ]
    }

    fn missao(status: StatusMissao, ajustar: impl FnOnce(&mut Missao)) -> Missao {
        let mut m = Missao::de_teste(Uuid::new_v4(), status);
        ajustar(&mut m);
        m
    }

    fn por_pais(status: StatusMissao, paises: &[u16]) -> Missao {
        missao(status, |m| m.filtros.paises_dos_clubes = paises.to_vec())
    }

    #[test]
    fn without_missoes_every_country_is_never_scanned() {
        let c = Cobertura::de(&[], &ligas());
        assert_eq!(c.total, 0);
        assert_eq!(c.estado(BRASIL), EstadoPais::NuncaEscaneado);
        assert!(c.contagem(BRASIL).vazia());
        assert!(c.globais.vazia());
        assert!(c.continentes().is_empty());
    }

    #[test]
    fn pending_and_running_missoes_are_active_and_concluded_ones_are_completed() {
        let c = Cobertura::de(
            &[
                por_pais(StatusMissao::Pendente, &[BRASIL]),
                por_pais(StatusMissao::EmExecucao, &[ARGENTINA]),
                por_pais(StatusMissao::Concluida, &[INGLATERRA]),
            ],
            &ligas(),
        );
        assert_eq!(c.estado(BRASIL), EstadoPais::MissaoAtiva);
        assert_eq!(c.estado(ARGENTINA), EstadoPais::MissaoAtiva);
        assert_eq!(c.estado(INGLATERRA), EstadoPais::MissaoConcluida);
        assert_eq!(c.estado(60), EstadoPais::NuncaEscaneado);
    }

    #[test]
    fn active_takes_precedence_and_both_counts_are_kept() {
        let c = Cobertura::de(
            &[
                por_pais(StatusMissao::Concluida, &[BRASIL]),
                por_pais(StatusMissao::Concluida, &[BRASIL, ARGENTINA]),
                por_pais(StatusMissao::Pendente, &[BRASIL]),
            ],
            &ligas(),
        );
        assert_eq!(c.estado(BRASIL), EstadoPais::MissaoAtiva);
        assert_eq!(c.contagem(BRASIL), Contagem { ativas: 1, concluidas: 2 });
        assert_eq!(c.contagem(ARGENTINA), Contagem { ativas: 0, concluidas: 1 });
    }

    #[test]
    fn a_league_paints_its_country_once_even_with_the_country_also_chosen() {
        let c = Cobertura::de(
            &[missao(StatusMissao::Pendente, |m| {
                m.filtros.ligas = vec![13, 14];
                m.filtros.paises_dos_clubes = vec![INGLATERRA];
            })],
            &ligas(),
        );
        assert_eq!(c.contagem(INGLATERRA), Contagem { ativas: 1, concluidas: 0 });
        assert!(c.globais.vazia());
    }

    #[test]
    fn whole_continents_and_continental_leagues_count_per_continent_without_painting_countries() {
        let c = Cobertura::de(
            &[
                missao(StatusMissao::Pendente, |m| m.filtros.continentes = vec![Confederacao::Europa]),
                missao(StatusMissao::Concluida, |m| m.filtros.ligas = vec![900]),
            ],
            &ligas(),
        );
        assert_eq!(c.do_continente(Confederacao::Europa), Contagem { ativas: 1, concluidas: 1 });
        assert_eq!(c.estado(INGLATERRA), EstadoPais::NuncaEscaneado);
        assert_eq!(c.continentes(), vec![(Confederacao::Europa, Contagem { ativas: 1, concluidas: 1 })]);
        assert!(c.globais.vazia());
    }

    #[test]
    fn missoes_without_a_geographic_filter_only_count_as_global() {
        let c = Cobertura::de(&[missao(StatusMissao::Pendente, |_| {}), missao(StatusMissao::Concluida, |_| {})], &ligas());
        assert_eq!(c.globais, Contagem { ativas: 1, concluidas: 1 });
        assert_eq!(c.estado(BRASIL), EstadoPais::NuncaEscaneado);
        assert_eq!(c.total, 2);
    }

    #[test]
    fn an_unknown_league_is_ignored_and_never_counts_as_global() {
        let c = Cobertura::de(&[missao(StatusMissao::Pendente, |m| m.filtros.ligas = vec![4242])], &ligas());
        assert!(c.globais.vazia());
        assert!(c.continentes().is_empty());
    }

    #[test]
    fn old_nationality_missoes_paint_the_same_tile_and_keep_outros() {
        let c = Cobertura::de(&[missao(StatusMissao::Concluida, |m| m.filtros.paises = vec![BRASIL, NACAO_OUTROS])], &ligas());
        assert_eq!(c.estado(BRASIL), EstadoPais::MissaoConcluida);
        assert!(c.tem_outros());
        assert!(c.globais.vazia());
    }

    #[test]
    fn tiles_are_the_league_countries_plus_countries_only_old_missoes_covered() {
        let c = Cobertura::de(&[missao(StatusMissao::Concluida, |m| m.filtros.paises = vec![ARGENTINA, 999, NACAO_OUTROS])], &ligas());
        let nacoes = vec![Nacao { id: ARGENTINA, nome: "Argentina".into(), iso: String::new(), confederacao: Confederacao::AmericaDoSul }];
        let quadros = c.quadros(&ligas(), &nacoes);
        let ids: Vec<(u16, &str)> = quadros.iter().map(|q| (q.id, q.nome.as_str())).collect();
        assert_eq!(ids, [(INGLATERRA, "England"), (ESPANHA, "Spain"), (BRASIL, "Brazil"), (ARGENTINA, "Argentina"), (999, "País 999")]);
        assert_eq!(quadros.last().map(|q| q.confederacao), Some(Confederacao::Outras));
    }
}
