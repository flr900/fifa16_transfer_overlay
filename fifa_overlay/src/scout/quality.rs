//! `scout::quality` — a tabela de balanceamento do Scout, num lugar só
//! (AD-3: fórmulas aqui, filtros em `search`, orquestração em `state`).
//!
//! Para rebalancear depois de jogar, mexa só nas tabelas deste módulo:
//! nenhuma tela nem outro módulo guarda número de balanceamento. Tudo
//! é função pura e síncrona — o formulário Nova Missão (Story 2.2) chama
//! `estimar_missao` a cada mudança, no thread de render, sem `save_repo`
//! (AD-4). Este módulo não chama `persistence` nem `search` (AD-1).
//!
//! ## Como uma Missão é estimada (Story 2.1, primeira versão a calibrar)
//!
//! **Qualidade** sai de uma pontuação de 1 a 5:
//! `Tier (Júnior 1, Experiente 2, Elite 3) + 1 se a Especialização
//! combina com o tipo da Missão + 1 se o Modo é Completa`, e
//! `1–2 = Baixa, 3 = Média, 4–5 = Alta`. A pontuação (não só o nível)
//! decide quantos atributos o Relatório revela e a precisão de base, então
//! subir o Tier sempre melhora o Relatório, mesmo sem mudar o nível.
//!
//! **Amplitude geográfica** não muda o nível de Qualidade: piora a
//! **precisão** (faixa ± maior) e encarece/alonga a Missão (FR-4).
//!
//! **Jogadores** (alvo do Relatório): Rápida traz mais nomes, Completa
//! menos (FR-5); dentro do Modo, mais pontuação traz mais nomes.
//!
//! Falsos positivos NÃO entram no v1 (decisão de 2026-10-01; ver
//! `_bmad-output/planning-artifacts/melhorias-futuras-olheiros.md`).

use super::state::{Especializacao, ModoBusca, Qualidade, Tier};

// ---------------------------------------------------------------------
// Contratação (Story 1.4)
// ---------------------------------------------------------------------

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

// ---------------------------------------------------------------------
// Entradas da estimativa de Missão (Story 2.1)
// ---------------------------------------------------------------------

/// Que tipo de jogador a Missão procura — é o que decide se a
/// Especialização do Olheiro "combina". A Story 2.2 deriva o tipo dos
/// filtros escolhidos (ex.: Potencial alto → Jovens; Atributo dominante /
/// Fit Posicional / Jogador de Referência → Tática).
#[allow(dead_code)] // derivado dos filtros na Story 2.2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoMissao {
    Jovens,
    Medalhoes,
    Tatica,
    Geral,
}

impl TipoMissao {
    #[allow(dead_code)] // usado nos testes e pela Story 2.2
    pub const TODOS: [TipoMissao; 4] = [TipoMissao::Jovens, TipoMissao::Medalhoes, TipoMissao::Tatica, TipoMissao::Geral];
}

/// Amplitude do filtro geográfico (Stories 2.2/2.9 calculam a partir dos
/// países escolhidos). Em ordem: `Pais < VariosPaises < Continente < Mundo`.
#[allow(dead_code)] // derivada do filtro geográfico nas Stories 2.2/2.9
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AmplitudeGeografica {
    /// Um país só.
    Pais,
    /// Alguns países, sem cobrir um continente.
    VariosPaises,
    Continente,
    /// Sem filtro geográfico, ou mais de um continente.
    Mundo,
}

impl AmplitudeGeografica {
    #[allow(dead_code)] // usado nos testes e pelas Stories 2.2/2.9
    pub const TODAS: [AmplitudeGeografica; 4] = [
        AmplitudeGeografica::Pais,
        AmplitudeGeografica::VariosPaises,
        AmplitudeGeografica::Continente,
        AmplitudeGeografica::Mundo,
    ];
}

/// Tudo o que a estimativa precisa saber da Missão.
#[allow(dead_code)] // montado pelo formulário Nova Missão (Story 2.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PedidoMissao {
    pub tier: Tier,
    pub especializacao: Especializacao,
    pub modo: ModoBusca,
    pub tipo: TipoMissao,
    pub amplitude: AmplitudeGeografica,
}

/// O que o formulário mostra antes de confirmar e o que a busca (2.4)
/// usa para montar o Relatório.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstimativaMissao {
    /// Mesma unidade do `transferbudget`.
    pub custo: i32,
    /// Dias de carreira (`GJUr.currdate`) até o prazo.
    pub duracao_dias: u32,
    pub qualidade: Qualidade,
    /// Quantos atributos de cada jogador o Relatório revela.
    pub atributos_revelados: u8,
    /// Faixa de precisão: o valor mostrado fica a até ± este número de
    /// pontos do real (0 = exato).
    pub precisao_mais_menos: u8,
    /// Quantos jogadores o Relatório procura trazer.
    pub alvo_jogadores: u8,
}

// ---------------------------------------------------------------------
// A tabela de balanceamento
// ---------------------------------------------------------------------

/// Pontos de Qualidade de cada Tier.
fn pontos_tier(tier: Tier) -> u8 {
    match tier {
        Tier::Junior => 1,
        Tier::Experiente => 2,
        Tier::Elite => 3,
    }
}

/// Bônus de Qualidade quando a Especialização combina com a Missão. O
/// Generalista não combina especialmente com nada (relatórios rasos).
const BONUS_ADERENCIA: u8 = 1;
/// Bônus de Qualidade do Modo Completa.
const BONUS_COMPLETA: u8 = 1;

fn aderente(especializacao: Especializacao, tipo: TipoMissao) -> bool {
    matches!(
        (especializacao, tipo),
        (Especializacao::CacadorDeJovens, TipoMissao::Jovens)
            | (Especializacao::CacadorDeMedalhoes, TipoMissao::Medalhoes)
            | (Especializacao::Tatico, TipoMissao::Tatica)
    )
}

/// Pontuação de 1 a 5 (ver o topo do módulo).
fn pontuacao(pedido: &PedidoMissao) -> u8 {
    let aderencia = if aderente(pedido.especializacao, pedido.tipo) { BONUS_ADERENCIA } else { 0 };
    let modo = if pedido.modo == ModoBusca::Completa { BONUS_COMPLETA } else { 0 };
    pontos_tier(pedido.tier) + aderencia + modo
}

fn qualidade_da_pontuacao(pontos: u8) -> Qualidade {
    match pontos {
        0..=2 => Qualidade::Baixa,
        3 => Qualidade::Media,
        _ => Qualidade::Alta,
    }
}

/// Atributos revelados por pontuação (o save tem 29 atributos de linha +
/// 5 de goleiro; Alta máxima revela o perfil de linha inteiro).
fn atributos_da_pontuacao(pontos: u8) -> u8 {
    match pontos {
        0 | 1 => 6,
        2 => 10,
        3 => 15,
        4 => 22,
        _ => 29,
    }
}

/// Precisão de base (± pontos) por pontuação, antes da amplitude.
fn precisao_base(pontos: u8) -> u8 {
    match pontos {
        0 | 1 => 10,
        2 => 7,
        3 => 5,
        4 => 3,
        _ => 1,
    }
}

/// Quanto a amplitude geográfica alarga a faixa de precisão (± pontos).
fn precisao_extra_amplitude(amplitude: AmplitudeGeografica) -> u8 {
    match amplitude {
        AmplitudeGeografica::Pais => 0,
        AmplitudeGeografica::VariosPaises => 1,
        AmplitudeGeografica::Continente => 2,
        AmplitudeGeografica::Mundo => 4,
    }
}

/// Jogadores no Relatório: `base do Modo + por ponto × pontuação`.
fn alvo_jogadores(modo: ModoBusca, pontos: u8) -> u8 {
    let (base, por_ponto) = match modo {
        ModoBusca::Rapida => (15, 2),
        ModoBusca::Completa => (5, 1),
    };
    base + por_ponto * pontos
}

/// Duração e custo de base por Modo (antes de amplitude e Tier).
fn base_do_modo(modo: ModoBusca) -> (u32, i64) {
    match modo {
        // (dias de carreira, custo)
        ModoBusca::Rapida => (7, 150_000),
        ModoBusca::Completa => (21, 400_000),
    }
}

/// Multiplicadores da amplitude, em %: (duração, custo).
fn fatores_amplitude(amplitude: AmplitudeGeografica) -> (u32, i64) {
    match amplitude {
        AmplitudeGeografica::Pais => (100, 100),
        AmplitudeGeografica::VariosPaises => (125, 150),
        AmplitudeGeografica::Continente => (150, 200),
        AmplitudeGeografica::Mundo => (200, 300),
    }
}

/// Multiplicadores do Tier, em %: (duração — Elite é mais rápido, custo —
/// Elite cobra mais caro por Missão).
fn fatores_tier(tier: Tier) -> (u32, i64) {
    match tier {
        Tier::Junior => (120, 100),
        Tier::Experiente => (100, 150),
        Tier::Elite => (85, 220),
    }
}

/// Custos de Missão são arredondados para este múltiplo.
const ARREDONDAMENTO_CUSTO: i64 = 10_000;

// ---------------------------------------------------------------------
// Estimativa
// ---------------------------------------------------------------------

/// Custo, duração e Qualidade de uma Missão. Pura e determinística.
#[allow(dead_code)] // chamada pelo formulário Nova Missão (Story 2.2) e pela busca (2.4)
pub fn estimar_missao(pedido: &PedidoMissao) -> EstimativaMissao {
    let pontos = pontuacao(pedido);
    let (dias_base, custo_base) = base_do_modo(pedido.modo);
    let (dias_amplitude, custo_amplitude) = fatores_amplitude(pedido.amplitude);
    let (dias_tier, custo_tier) = fatores_tier(pedido.tier);

    // Dias: arredonda para cima (uma Missão nunca termina "antes" do que
    // a conta diz).
    let duracao_dias = (dias_base * dias_amplitude * dias_tier).div_ceil(100 * 100);
    let custo = custo_base * custo_amplitude * custo_tier / (100 * 100);
    let custo = (custo + ARREDONDAMENTO_CUSTO / 2) / ARREDONDAMENTO_CUSTO * ARREDONDAMENTO_CUSTO;

    EstimativaMissao {
        custo: i32::try_from(custo).unwrap_or(i32::MAX),
        duracao_dias,
        qualidade: qualidade_da_pontuacao(pontos),
        atributos_revelados: atributos_da_pontuacao(pontos),
        precisao_mais_menos: precisao_base(pontos) + precisao_extra_amplitude(pedido.amplitude),
        alvo_jogadores: alvo_jogadores(pedido.modo, pontos),
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

    /// Todas as 4 × 3 × 2 × 4 × 4 = 384 combinações de entrada.
    fn todos_os_pedidos() -> Vec<PedidoMissao> {
        let mut pedidos = Vec::new();
        for tier in Tier::TODOS {
            for especializacao in Especializacao::TODAS {
                for modo in ModoBusca::TODOS {
                    for tipo in TipoMissao::TODOS {
                        for amplitude in AmplitudeGeografica::TODAS {
                            pedidos.push(PedidoMissao { tier, especializacao, modo, tipo, amplitude });
                        }
                    }
                }
            }
        }
        pedidos
    }

    #[test]
    fn a_higher_tier_gives_a_better_report() {
        for pedido in todos_os_pedidos().into_iter().filter(|p| p.tier != Tier::Elite) {
            let acima = PedidoMissao { tier: if pedido.tier == Tier::Junior { Tier::Experiente } else { Tier::Elite }, ..pedido };
            let (a, b) = (estimar_missao(&pedido), estimar_missao(&acima));
            assert!(b.qualidade >= a.qualidade, "{pedido:?}");
            assert!(b.atributos_revelados > a.atributos_revelados, "{pedido:?}");
            assert!(b.precisao_mais_menos < a.precisao_mais_menos, "{pedido:?}");
        }
        // de Júnior para Elite o nível sempre sobe
        for pedido in todos_os_pedidos().into_iter().filter(|p| p.tier == Tier::Junior) {
            let elite = PedidoMissao { tier: Tier::Elite, ..pedido };
            assert!(estimar_missao(&elite).qualidade > estimar_missao(&pedido).qualidade, "{pedido:?}");
        }
    }

    #[test]
    fn completa_has_better_quality_fewer_players_and_takes_longer_than_rapida() {
        for pedido in todos_os_pedidos().into_iter().filter(|p| p.modo == ModoBusca::Rapida) {
            let completa = PedidoMissao { modo: ModoBusca::Completa, ..pedido };
            let (r, c) = (estimar_missao(&pedido), estimar_missao(&completa));
            assert!(c.qualidade >= r.qualidade && c.atributos_revelados > r.atributos_revelados, "{pedido:?}");
            assert!(c.alvo_jogadores < r.alvo_jogadores, "{pedido:?}");
            assert!(c.duracao_dias > r.duracao_dias, "{pedido:?}");
            assert!(c.custo > r.custo, "{pedido:?}");
        }
    }

    #[test]
    fn broader_geography_lowers_precision_and_costs_more_time_and_money() {
        for pedido in todos_os_pedidos() {
            let mais_ampla = match pedido.amplitude {
                AmplitudeGeografica::Pais => AmplitudeGeografica::VariosPaises,
                AmplitudeGeografica::VariosPaises => AmplitudeGeografica::Continente,
                AmplitudeGeografica::Continente => AmplitudeGeografica::Mundo,
                AmplitudeGeografica::Mundo => continue,
            };
            let ampla = PedidoMissao { amplitude: mais_ampla, ..pedido };
            let (a, b) = (estimar_missao(&pedido), estimar_missao(&ampla));
            assert!(b.precisao_mais_menos > a.precisao_mais_menos, "{pedido:?}");
            assert!(b.duracao_dias > a.duracao_dias && b.custo > a.custo, "{pedido:?}");
            assert_eq!(b.qualidade, a.qualidade, "amplitude não muda o nível");
        }
    }

    #[test]
    fn a_matching_especializacao_improves_the_report() {
        let casos = [
            (Especializacao::CacadorDeJovens, TipoMissao::Jovens),
            (Especializacao::CacadorDeMedalhoes, TipoMissao::Medalhoes),
            (Especializacao::Tatico, TipoMissao::Tatica),
        ];
        for pedido in todos_os_pedidos() {
            for (especializacao, tipo) in casos {
                if pedido.especializacao != especializacao || pedido.tipo == tipo {
                    continue;
                }
                let combinando = PedidoMissao { tipo, ..pedido };
                let (fora, dentro) = (estimar_missao(&pedido), estimar_missao(&combinando));
                assert!(dentro.qualidade >= fora.qualidade, "{pedido:?}");
                assert!(dentro.atributos_revelados > fora.atributos_revelados, "{pedido:?}");
            }
        }
        // Generalista: nenhum tipo é "dele"
        for tipo in TipoMissao::TODOS {
            assert!(!aderente(Especializacao::Generalista, tipo));
        }
    }

    #[test]
    fn extremes_match_the_documented_table() {
        let pior = PedidoMissao {
            tier: Tier::Junior,
            especializacao: Especializacao::Generalista,
            modo: ModoBusca::Rapida,
            tipo: TipoMissao::Geral,
            amplitude: AmplitudeGeografica::Mundo,
        };
        let melhor = PedidoMissao {
            tier: Tier::Elite,
            especializacao: Especializacao::CacadorDeJovens,
            modo: ModoBusca::Completa,
            tipo: TipoMissao::Jovens,
            amplitude: AmplitudeGeografica::Pais,
        };
        assert_eq!(
            estimar_missao(&pior),
            EstimativaMissao {
                custo: 450_000,
                duracao_dias: 17,
                qualidade: Qualidade::Baixa,
                atributos_revelados: 6,
                precisao_mais_menos: 14,
                alvo_jogadores: 17,
            }
        );
        assert_eq!(
            estimar_missao(&melhor),
            EstimativaMissao {
                custo: 880_000,
                duracao_dias: 18,
                qualidade: Qualidade::Alta,
                atributos_revelados: 29,
                precisao_mais_menos: 1,
                alvo_jogadores: 10,
            }
        );
    }

    #[test]
    fn quality_depends_on_neither_persistence_nor_search() {
        // AC #3 da Story 2.1 (AD-1/AD-3): só o código fora dos testes conta.
        let fonte = include_str!("quality.rs");
        let codigo = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        for proibido in ["persistence", "search", "save_repo"] {
            let usos = codigo.lines().filter(|l| !l.trim_start().starts_with("//") && l.contains(proibido));
            assert_eq!(usos.count(), 0, "scout::quality não pode usar {proibido}");
        }
    }

    #[test]
    fn every_estimate_is_sane_and_deterministic() {
        for pedido in todos_os_pedidos() {
            let e = estimar_missao(&pedido);
            assert_eq!(e, estimar_missao(&pedido));
            assert!(e.custo > 0 && e.custo % 10_000 == 0, "{pedido:?} {e:?}");
            assert!((5..=60).contains(&e.duracao_dias), "{pedido:?} {e:?}");
            assert!((6..=29).contains(&e.atributos_revelados), "{pedido:?} {e:?}");
            assert!(e.alvo_jogadores > 0, "{pedido:?} {e:?}");
        }
    }
}
