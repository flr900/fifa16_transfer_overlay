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

use serde::{Deserialize, Serialize};

use super::state::{Atributo, Especializacao, FaixaAtributo, Funcao, ModoBusca, Qualidade, Tier};

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
/// Especialização do Olheiro "combina". Derivado dos filtros
/// (`tipo_por_faixas`); Tática chega com os filtros de atributo / Fit
/// Posicional / Jogador de Referência (Stories 2.8 e Épico 3).
/// No JSON: `"jovens"`, `"medalhoes"`, `"tatica"`, `"geral"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoMissao {
    Jovens,
    Medalhoes,
    Tatica,
    Geral,
}

impl TipoMissao {
    #[allow(dead_code)] // usado nos testes
    pub const TODOS: [TipoMissao; 4] = [TipoMissao::Jovens, TipoMissao::Medalhoes, TipoMissao::Tatica, TipoMissao::Geral];
}

/// Amplitude do filtro geográfico. Até a Story 2.9 (filtro por país) toda
/// Missão é `Mundo`. Em ordem: `Pais < VariosPaises < Continente < Mundo`.
/// No JSON: `"pais"`, `"varios_paises"`, `"continente"`, `"mundo"`.
#[allow(dead_code)] // as demais amplitudes chegam com o filtro geográfico (Story 2.9)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    #[allow(dead_code)] // usado nos testes e pela Story 2.9
    pub const TODAS: [AmplitudeGeografica; 4] = [
        AmplitudeGeografica::Pais,
        AmplitudeGeografica::VariosPaises,
        AmplitudeGeografica::Continente,
        AmplitudeGeografica::Mundo,
    ];
}

/// Tudo o que a estimativa precisa saber da Missão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PedidoMissao {
    pub tier: Tier,
    pub especializacao: Especializacao,
    pub modo: ModoBusca,
    pub tipo: TipoMissao,
    pub amplitude: AmplitudeGeografica,
}

/// O que o formulário mostra antes de confirmar e o que a busca (2.4)
/// usa para montar o Relatório. Guardada na Missão no momento da
/// confirmação: o jogador recebe o que pagou, mesmo se a tabela mudar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Atributos revelados por pontuação (o save tem 28 atributos de linha +
/// 5 de goleiro; Alta máxima revela 28: o perfil de linha inteiro, ou os 5
/// de goleiro + 23 de linha para um goleiro). Era 29 na Story 2.1, antes
/// de a Story 2.4 contar os campos reais do `CZUM`.
fn atributos_da_pontuacao(pontos: u8) -> u8 {
    match pontos {
        0 | 1 => 6,
        2 => 10,
        3 => 15,
        4 => 22,
        _ => 28,
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

/// Overall mínimo a partir do qual a Missão procura "medalhões".
const OVERALL_MEDALHAO: u8 = 75;
/// Quanto o Potencial mínimo precisa passar do Overall máximo para a
/// Missão ser de "jovens" (jogadores que ainda vão crescer).
const MARGEM_POTENCIAL_JOVENS: u8 = 5;

/// Tipo da Missão a partir das faixas de Overall/Potencial (Story 2.2):
/// - Potencial mínimo ≥ Overall máximo + 5 → **Jovens** (busca crescimento);
/// - senão, Overall mínimo ≥ 75 → **Medalhões** (prontos para jogar);
/// - senão → **Geral**.
pub fn tipo_por_faixas(overall: FaixaAtributo, potencial: FaixaAtributo) -> TipoMissao {
    if potencial.min >= overall.max.saturating_add(MARGEM_POTENCIAL_JOVENS) {
        TipoMissao::Jovens
    } else if overall.min >= OVERALL_MEDALHAO {
        TipoMissao::Medalhoes
    } else {
        TipoMissao::Geral
    }
}

/// Tipo da Missão a partir de todos os filtros (Story 2.8): pedir um
/// atributo dominante ("o melhor driblador") é uma Missão **Tática** — a
/// especialidade do Tático; sem ele, valem as faixas.
pub fn tipo_por_filtros(overall: FaixaAtributo, potencial: FaixaAtributo, dominante: Option<Atributo>) -> TipoMissao {
    match dominante {
        Some(_) => TipoMissao::Tatica,
        None => tipo_por_faixas(overall, potencial),
    }
}

/// Um jogador "tem" um atributo dominante quando ele está entre os seus
/// `TOP_DOMINANTE` maiores atributos (empates contam). Só o maior seria
/// raro demais (muitos jogadores têm Velocidade ou Força no topo).
pub const TOP_DOMINANTE: usize = 3;

/// `valores`: os atributos do jogador que contam para a função dele (os de
/// goleiro só para goleiros). Verdadeiro se `alvo` está no top
/// `TOP_DOMINANTE` (empates incluídos).
pub fn eh_dominante(valores: &[(Atributo, u8)], alvo: Atributo) -> bool {
    let Some(&(_, valor_alvo)) = valores.iter().find(|(a, _)| *a == alvo) else {
        return false;
    };
    let maiores = valores.iter().filter(|(_, v)| *v > valor_alvo).count();
    maiores < TOP_DOMINANTE
}

/// A Especialização do Olheiro combina com o tipo da Missão (bônus de
/// Qualidade)? O formulário mostra isso ao jogador.
pub fn combina(especializacao: Especializacao, tipo: TipoMissao) -> bool {
    aderente(especializacao, tipo)
}

// ---------------------------------------------------------------------
// Estimativa
// ---------------------------------------------------------------------

/// Custo, duração e Qualidade de uma Missão. Pura e determinística.
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

// ---------------------------------------------------------------------
// Como o Relatório revela cada jogador (Story 2.4)
// ---------------------------------------------------------------------
//
// Nada aqui inventa valor: o Relatório mostra uma FAIXA que sempre contém
// o valor real, com largura `2 × precisão`, e só para os atributos que o
// Olheiro observou. A posição do valor real dentro da faixa é sorteada
// (determinística, pela semente), senão o meio da faixa entregaria o
// número exato.

/// Ordem em que o Olheiro observa os atributos de um jogador, pela função
/// dele em campo. O Relatório revela os N primeiros (N =
/// `atributos_revelados`). Um atributo dominante pedido na Missão (Story
/// 2.8) é sempre o primeiro.
pub fn ordem_de_observacao(funcao: Funcao, dominante: Option<Atributo>) -> Vec<Atributo> {
    use Atributo::*;
    let prioridade: &[Atributo] = match funcao {
        Funcao::Goleiro => &[GkReflexos, GkMergulho, GkColocacao, GkManejo, GkReposicao, Reacao, Impulsao, Forca],
        Funcao::Defensor => &[Marcacao, DesarmeEmPe, Carrinho, Interceptacao, Cabeceio, Forca, Velocidade, Reacao],
        Funcao::MeioCampo => &[PasseCurto, Visao, ControleDeBola, PasseLongo, Drible, Reacao, Folego, Interceptacao],
        Funcao::Atacante => &[Finalizacao, PosicionamentoOfensivo, Velocidade, Aceleracao, Drible, ControleDeBola, ForcaDoChute, Reacao],
    };
    let goleiro = funcao == Funcao::Goleiro;
    let mut ordem: Vec<Atributo> = Vec::with_capacity(Atributo::TODOS.len());
    for &a in dominante.iter().chain(prioridade).chain(Atributo::TODOS.iter()) {
        // Atributos de goleiro só entram na observação de goleiros.
        if (a.goleiro() && !goleiro && Some(a) != dominante) || ordem.contains(&a) {
            continue;
        }
        ordem.push(a);
    }
    ordem
}

/// Faixa revelada para um valor `real` (1–99) com precisão ± `precisao`:
/// largura `2 × precisao`, sempre contendo o real, dentro de 1–99.
pub fn faixa_revelada(real: u8, precisao: u8, semente: u64) -> FaixaAtributo {
    let real = real.clamp(FaixaAtributo::MENOR, FaixaAtributo::MAIOR);
    if precisao == 0 {
        return FaixaAtributo { min: real, max: real };
    }
    let largura = u16::from(precisao) * 2;
    let deslocamento = u16::try_from(semente % (u64::from(largura) + 1)).unwrap_or(0);
    let menor = u16::from(FaixaAtributo::MENOR);
    let maior = u16::from(FaixaAtributo::MAIOR);
    let min = u16::from(real).saturating_sub(deslocamento).max(menor);
    // empurra a janela para dentro de 1–99 sem perder o valor real
    let min = min.min(maior.saturating_sub(largura).max(menor));
    let max = (min + largura).min(maior);
    FaixaAtributo { min: u8::try_from(min).unwrap_or(real), max: u8::try_from(max).unwrap_or(real) }
}

/// Quanto o Olheiro "erra" na hora de escolher quem entra no Relatório: a
/// relevância de cada candidato recebe um ruído de até este número de
/// pontos. Qualidade alta escolhe quase sempre os melhores; baixa traz
/// nomes bem mais aleatórios.
pub fn ruido_de_escolha(qualidade: Qualidade) -> u32 {
    match qualidade {
        Qualidade::Alta => 3,
        Qualidade::Media => 8,
        Qualidade::Baixa => 15,
    }
}

/// Nota de um candidato: relevância (0–99) + ruído da Qualidade. Maior =
/// entra antes no Relatório.
pub fn nota_de_escolha(relevancia: u8, qualidade: Qualidade, semente: u64) -> u32 {
    let ruido = ruido_de_escolha(qualidade) * 100;
    u32::from(relevancia) * 100 + u32::try_from(semente % (u64::from(ruido) + 1)).unwrap_or(0)
}

/// Relevância de um jogador para o tipo de Missão (0–99).
pub fn relevancia(tipo: TipoMissao, overall: u8, potencial: u8, dominante: Option<u8>) -> u8 {
    match (tipo, dominante) {
        (_, Some(valor)) => valor,
        (TipoMissao::Jovens, None) => potencial,
        (TipoMissao::Medalhoes, None) => overall,
        (TipoMissao::Tatica | TipoMissao::Geral, None) => {
            u8::try_from((u16::from(overall) + u16::from(potencial)) / 2).unwrap_or(overall)
        }
    }
}

/// Semente determinística (SplitMix64) a partir da Missão, do jogador e de
/// um "canal" (atributo, escolha…): o mesmo Relatório sai sempre igual.
pub fn semente(missao: u128, jogador: u32, canal: u32) -> u64 {
    let mut x = (missao as u64) ^ ((missao >> 64) as u64).rotate_left(17) ^ (u64::from(jogador) << 20) ^ u64::from(canal);
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
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
                atributos_revelados: 28,
                precisao_mais_menos: 1,
                alvo_jogadores: 10,
            }
        );
    }

    #[test]
    fn missao_type_comes_from_the_overall_and_potencial_ranges() {
        let faixa = |min, max| FaixaAtributo { min, max };
        assert_eq!(tipo_por_faixas(faixa(50, 70), faixa(80, 99)), TipoMissao::Jovens);
        assert_eq!(tipo_por_faixas(faixa(50, 70), faixa(75, 99)), TipoMissao::Jovens, "exatamente +5");
        assert_eq!(tipo_por_faixas(faixa(50, 70), faixa(74, 99)), TipoMissao::Geral);
        assert_eq!(tipo_por_faixas(faixa(78, 99), faixa(78, 99)), TipoMissao::Medalhoes);
        assert_eq!(tipo_por_faixas(faixa(50, 99), faixa(50, 99)), TipoMissao::Geral, "padrão do formulário");
        // jovens vence medalhões quando as duas regras valem
        assert_eq!(tipo_por_faixas(faixa(75, 80), faixa(90, 99)), TipoMissao::Jovens);
        assert!(combina(Especializacao::CacadorDeJovens, TipoMissao::Jovens));
        assert!(!combina(Especializacao::Generalista, TipoMissao::Geral));
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
            assert!((6..=28).contains(&e.atributos_revelados), "{pedido:?} {e:?}");
            assert!(e.alvo_jogadores > 0, "{pedido:?} {e:?}");
        }
    }

    #[test]
    fn revealed_ranges_always_contain_the_real_value_and_stay_in_bounds() {
        for real in 1..=99u8 {
            for precisao in 0..=14u8 {
                for s in 0..40u64 {
                    let f = faixa_revelada(real, precisao, semente(7, u32::from(real), s as u32));
                    assert!(f.min <= real && real <= f.max, "{real} ±{precisao}: {f:?}");
                    assert!(f.min >= 1 && f.max <= 99, "{f:?}");
                    if precisao == 0 {
                        assert_eq!(f.min, f.max);
                    } else if real > 2 * precisao && real < 99 - 2 * precisao {
                        assert_eq!(f.max - f.min, 2 * precisao, "largura fixa longe das pontas");
                    }
                }
            }
        }
    }

    #[test]
    fn the_real_value_is_not_always_the_middle_of_the_range() {
        let meios = (0..50u32).filter(|&c| {
            let f = faixa_revelada(70, 5, semente(1, 2, c));
            f.min + 5 == 70
        });
        assert!(meios.count() < 20);
    }

    #[test]
    fn observation_order_starts_with_the_role_and_the_dominant_attribute() {
        let atacante = ordem_de_observacao(Funcao::Atacante, None);
        assert_eq!(atacante.first(), Some(&Atributo::Finalizacao));
        assert_eq!(atacante.len(), 28, "sem atributos de goleiro");
        assert!(atacante.iter().all(|a| !a.goleiro()));
        let goleiro = ordem_de_observacao(Funcao::Goleiro, None);
        assert_eq!(goleiro.len(), 33);
        assert!(goleiro.iter().take(5).all(|a| a.goleiro()));
        let drible = ordem_de_observacao(Funcao::Defensor, Some(Atributo::Drible));
        assert_eq!(drible.first(), Some(&Atributo::Drible));
        let mut sem_repetir = drible.clone();
        sem_repetir.sort_unstable();
        sem_repetir.dedup();
        assert_eq!(sem_repetir.len(), drible.len());
    }

    #[test]
    fn better_quality_picks_more_faithfully() {
        // um candidato 10 pontos melhor quase sempre vence com Alta e nem
        // sempre com Baixa
        let vence = |q| (0..200u32).filter(|&c| nota_de_escolha(80, q, semente(3, c, 0)) > nota_de_escolha(70, q, semente(3, c, 1))).count();
        assert_eq!(vence(Qualidade::Alta), 200);
        assert!(vence(Qualidade::Baixa) < 200);
        assert_eq!(relevancia(TipoMissao::Jovens, 60, 88, None), 88);
        assert_eq!(relevancia(TipoMissao::Medalhoes, 82, 84, None), 82);
        assert_eq!(relevancia(TipoMissao::Geral, 70, 80, None), 75);
        assert_eq!(relevancia(TipoMissao::Tatica, 70, 80, Some(91)), 91);
        assert_eq!(semente(9, 9, 9), semente(9, 9, 9));
        assert_ne!(semente(9, 9, 9), semente(9, 9, 8));
    }

    #[test]
    fn a_dominant_attribute_makes_a_tactical_missao_and_counts_the_top_three() {
        let faixa = |min, max| FaixaAtributo { min, max };
        assert_eq!(tipo_por_filtros(faixa(50, 70), faixa(80, 99), Some(Atributo::Drible)), TipoMissao::Tatica);
        assert_eq!(tipo_por_filtros(faixa(50, 70), faixa(80, 99), None), TipoMissao::Jovens);
        assert!(combina(Especializacao::Tatico, TipoMissao::Tatica));
        let valores = [(Atributo::Velocidade, 90), (Atributo::Drible, 88), (Atributo::Forca, 88), (Atributo::Finalizacao, 85), (Atributo::Marcacao, 40)];
        assert!(eh_dominante(&valores, Atributo::Velocidade));
        assert!(eh_dominante(&valores, Atributo::Drible));
        assert!(eh_dominante(&valores, Atributo::Forca), "empate conta");
        assert!(!eh_dominante(&valores, Atributo::Finalizacao), "4º maior");
        assert!(!eh_dominante(&valores, Atributo::Marcacao));
        assert!(!eh_dominante(&valores, Atributo::GkReflexos), "fora da função");
    }
}
