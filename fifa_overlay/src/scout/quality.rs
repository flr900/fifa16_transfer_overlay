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
//!
//! ## Perfil do jogador (Épico 3)
//!
//! **Fit Posicional** (Story 3.4): cada posição-alvo tem um perfil ideal —
//! pesos inteiros (somam 100) sobre os atributos que importam nela, no
//! espírito da nota por posição do próprio FIFA (`PERFIS`). A nota de um
//! jogador num perfil é a média ponderada dos atributos dele. A **força do
//! fit** é `nota no perfil-alvo ÷ nota no perfil da posição nativa`, em %
//! (teto 100): mede o formato, não o nível — o nível já tem o filtro de
//! Overall. Entra no Relatório quem tem força ≥ `LIMIAR_FIT` e posição
//! nativa diferente do alvo.
//!
//! **Similaridade** com um Jogador de Referência (Story 3.3), 0–100:
//! `0,75 × forma + 0,25 × nível`, sobre os atributos de linha (ou os de
//! goleiro, se a referência é goleiro). Forma = `100 − 4 × diferença média
//! dos atributos já descontada a média de cada um` (perfil igual em outro
//! nível ainda é "parecido"); nível = `100 − 4 × diferença das médias`.
//! Perfis idênticos dão 100. Entra quem tem ≥ `LIMIAR_SIMILARIDADE`.
//!
//! As duas contas têm uma versão "pelo que o Olheiro viu" (faixas
//! reveladas, pelo meio): é o que o Relatório mostra, nunca o valor real.

use serde::{Deserialize, Serialize};

use super::state::{Atributo, Confederacao, Especializacao, FaixaAtributo, FiltrosMissao, Funcao, ModoBusca, Qualidade, Tier};

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

/// Amplitude do filtro geográfico (Story 2.9, `amplitude_da_selecao`).
/// Em ordem: `Pais < VariosPaises < Continente < Mundo`.
/// No JSON: `"pais"`, `"varios_paises"`, `"continente"`, `"mundo"`.
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
    #[allow(dead_code)] // usado nos testes
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

/// Tipo da Missão a partir de todos os filtros: pedir um perfil — um
/// atributo dominante ("o melhor driblador", Story 2.8), um Fit Posicional
/// (3.4) ou um Jogador de Referência (3.3) — é uma Missão **Tática**, a
/// especialidade do Tático; sem nenhum deles, valem as faixas.
pub fn tipo_por_filtros(filtros: &FiltrosMissao) -> TipoMissao {
    let pede_perfil =
        filtros.atributo_dominante.is_some() || filtros.fit_posicional.is_some() || filtros.referencia.is_some();
    if pede_perfil {
        TipoMissao::Tatica
    } else {
        tipo_por_faixas(filtros.overall, filtros.potencial)
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

/// Amplitude de uma seleção de países (Story 2.9). `selecao`: a
/// confederação de cada país escolhido; `total_da`: quantos países a
/// confederação tem no mapa.
/// - nenhum país = todos os países → `Mundo`;
/// - um país → `Pais`;
/// - vários, todos do mesmo continente: o continente inteiro → `Continente`,
///   senão `VariosPaises`;
/// - países de mais de um continente → `Mundo`.
pub fn amplitude_da_selecao(selecao: &[Confederacao], total_da: impl Fn(Confederacao) -> usize) -> AmplitudeGeografica {
    match selecao {
        [] => AmplitudeGeografica::Mundo,
        [_] => AmplitudeGeografica::Pais,
        [primeira, resto @ ..] if resto.iter().all(|c| c == primeira) => {
            if selecao.len() >= total_da(*primeira) {
                AmplitudeGeografica::Continente
            } else {
                AmplitudeGeografica::VariosPaises
            }
        }
        _ => AmplitudeGeografica::Mundo,
    }
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
// Relatório parcial e Missão contínua (Story 2.10)
// ---------------------------------------------------------------------

/// Uma Missão contínua ("sem prazo") é paga em blocos de tantos dias de
/// carreira; cada bloco custa o mesmo que a Missão de prazo fixo com os
/// mesmos filtros e traz o mesmo número de jogadores, só que espalhados
/// pelo bloco. Nada é cobrado sozinho: cada bloco novo é confirmado pelo
/// jogador (FR-3/NFR1).
pub const DIAS_BLOCO_CONTINUO: u32 = 30;

/// Quantos jogadores do Relatório já apareceram com o progresso `fracao`
/// (0–1): `ceil(fracao × alvo)`, nunca mais que os `encontrados` pela
/// busca. Prazo cumprido (`fracao` 1) mostra todos.
pub fn revelados(fracao: f32, alvo: usize, encontrados: usize) -> usize {
    let fracao = fracao.clamp(0.0, 1.0);
    let n = (fracao * alvo as f32).ceil() as usize;
    n.min(encontrados)
}

// ---------------------------------------------------------------------
// Fit Posicional (Story 3.4)
// ---------------------------------------------------------------------

/// Posições que o filtro Fit Posicional oferece (o goleiro fica de fora:
/// "um zagueiro que jogaria no gol" não é uma pergunta de scout). Lados
/// espelhados usam o mesmo perfil. No JSON: `"zagueiro"`, `"volante"` etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosicaoAlvo {
    Zagueiro,
    LateralDireito,
    LateralEsquerdo,
    AlaDireito,
    AlaEsquerdo,
    Volante,
    MeioCampista,
    MeiaAtacante,
    MeiaDireita,
    MeiaEsquerda,
    PontaDireita,
    PontaEsquerda,
    SegundoAtacante,
    Centroavante,
}

impl PosicaoAlvo {
    /// Todas, da defesa para o ataque.
    #[allow(dead_code)] // usado nos testes
    pub const TODAS: [PosicaoAlvo; 14] = [
        PosicaoAlvo::Zagueiro,
        PosicaoAlvo::LateralDireito,
        PosicaoAlvo::LateralEsquerdo,
        PosicaoAlvo::AlaDireito,
        PosicaoAlvo::AlaEsquerdo,
        PosicaoAlvo::Volante,
        PosicaoAlvo::MeioCampista,
        PosicaoAlvo::MeiaAtacante,
        PosicaoAlvo::MeiaDireita,
        PosicaoAlvo::MeiaEsquerda,
        PosicaoAlvo::PontaDireita,
        PosicaoAlvo::PontaEsquerda,
        PosicaoAlvo::SegundoAtacante,
        PosicaoAlvo::Centroavante,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            PosicaoAlvo::Zagueiro => "Zagueiro",
            PosicaoAlvo::LateralDireito => "Lateral-direito",
            PosicaoAlvo::LateralEsquerdo => "Lateral-esquerdo",
            PosicaoAlvo::AlaDireito => "Ala direito",
            PosicaoAlvo::AlaEsquerdo => "Ala esquerdo",
            PosicaoAlvo::Volante => "Volante",
            PosicaoAlvo::MeioCampista => "Meio-campista",
            PosicaoAlvo::MeiaAtacante => "Meia-atacante",
            PosicaoAlvo::MeiaDireita => "Meia direita",
            PosicaoAlvo::MeiaEsquerda => "Meia esquerda",
            PosicaoAlvo::PontaDireita => "Ponta direita",
            PosicaoAlvo::PontaEsquerda => "Ponta esquerda",
            PosicaoAlvo::SegundoAtacante => "Segundo atacante",
            PosicaoAlvo::Centroavante => "Centroavante",
        }
    }

    /// Sigla no mesmo vocabulário de `save_repo::nome_posicao`.
    pub fn sigla(self) -> &'static str {
        match self {
            PosicaoAlvo::Zagueiro => "ZAG",
            PosicaoAlvo::LateralDireito => "LD",
            PosicaoAlvo::LateralEsquerdo => "LE",
            PosicaoAlvo::AlaDireito => "ALD",
            PosicaoAlvo::AlaEsquerdo => "ALE",
            PosicaoAlvo::Volante => "VOL",
            PosicaoAlvo::MeioCampista => "MC",
            PosicaoAlvo::MeiaAtacante => "MEI",
            PosicaoAlvo::MeiaDireita => "MD",
            PosicaoAlvo::MeiaEsquerda => "ME",
            PosicaoAlvo::PontaDireita => "PD",
            PosicaoAlvo::PontaEsquerda => "PE",
            PosicaoAlvo::SegundoAtacante => "SA",
            PosicaoAlvo::Centroavante => "ATA",
        }
    }

    /// Códigos de `preferredposition1` que JÁ são esta posição (quem joga
    /// nela de origem não é um "fit", é a posição dele).
    pub fn posicoes_nativas(self) -> &'static [u8] {
        match self {
            PosicaoAlvo::Zagueiro => &[3, 4, 5],
            PosicaoAlvo::LateralDireito => &[2],
            PosicaoAlvo::LateralEsquerdo => &[6],
            PosicaoAlvo::AlaDireito => &[1],
            PosicaoAlvo::AlaEsquerdo => &[7],
            PosicaoAlvo::Volante => &[8, 9, 10],
            PosicaoAlvo::MeioCampista => &[12, 13, 14],
            PosicaoAlvo::MeiaAtacante => &[16, 17, 18],
            PosicaoAlvo::MeiaDireita => &[11],
            PosicaoAlvo::MeiaEsquerda => &[15],
            PosicaoAlvo::PontaDireita => &[22],
            PosicaoAlvo::PontaEsquerda => &[26],
            PosicaoAlvo::SegundoAtacante => &[19, 20, 21],
            PosicaoAlvo::Centroavante => &[23, 24, 25],
        }
    }

    pub fn perfil(self) -> Perfil {
        match self {
            PosicaoAlvo::Zagueiro => Perfil::Zagueiro,
            PosicaoAlvo::LateralDireito | PosicaoAlvo::LateralEsquerdo => Perfil::Lateral,
            PosicaoAlvo::AlaDireito | PosicaoAlvo::AlaEsquerdo => Perfil::Ala,
            PosicaoAlvo::Volante => Perfil::Volante,
            PosicaoAlvo::MeioCampista => Perfil::MeioCampista,
            PosicaoAlvo::MeiaAtacante => Perfil::MeiaAtacante,
            PosicaoAlvo::MeiaDireita | PosicaoAlvo::MeiaEsquerda => Perfil::MeiaAberto,
            PosicaoAlvo::PontaDireita | PosicaoAlvo::PontaEsquerda => Perfil::Ponta,
            PosicaoAlvo::SegundoAtacante => Perfil::SegundoAtacante,
            PosicaoAlvo::Centroavante => Perfil::Centroavante,
        }
    }
}

/// Perfil ideal de uma função em campo (o que `PERFIS` pesa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Perfil {
    Goleiro,
    Zagueiro,
    Lateral,
    Ala,
    Volante,
    MeioCampista,
    MeiaAtacante,
    MeiaAberto,
    Ponta,
    SegundoAtacante,
    Centroavante,
}

/// Perfis ideais: `(atributo, peso)`, pesos inteiros que somam 100 em cada
/// perfil (conferido em teste). Primeira versão (2026-10-03), inspirada na
/// nota por posição do FIFA; para rebalancear, mexa só aqui.
const PERFIS: [(Perfil, &[(Atributo, u8)]); 11] = {
    use Atributo::*;
    [
        (
            Perfil::Goleiro,
            &[(GkMergulho, 21), (GkManejo, 21), (GkReposicao, 5), (GkColocacao, 21), (GkReflexos, 21), (Reacao, 11)],
        ),
        (
            Perfil::Zagueiro,
            &[
                (Marcacao, 14),
                (DesarmeEmPe, 17),
                (Carrinho, 14),
                (Interceptacao, 13),
                (Cabeceio, 10),
                (Forca, 10),
                (Agressividade, 7),
                (PasseCurto, 5),
                (Reacao, 5),
                (ControleDeBola, 4),
                (Impulsao, 1),
            ],
        ),
        (
            Perfil::Lateral,
            &[
                (Carrinho, 14),
                (Interceptacao, 12),
                (DesarmeEmPe, 11),
                (Cruzamento, 9),
                (Marcacao, 8),
                (Folego, 8),
                (Reacao, 8),
                (Velocidade, 7),
                (ControleDeBola, 7),
                (PasseCurto, 7),
                (Aceleracao, 5),
                (Cabeceio, 4),
            ],
        ),
        (
            Perfil::Ala,
            &[
                (Cruzamento, 12),
                (Interceptacao, 12),
                (Carrinho, 11),
                (Folego, 10),
                (PasseCurto, 10),
                (Reacao, 8),
                (ControleDeBola, 8),
                (DesarmeEmPe, 8),
                (Marcacao, 7),
                (Velocidade, 6),
                (Aceleracao, 4),
                (Drible, 4),
            ],
        ),
        (
            Perfil::Volante,
            &[
                (PasseCurto, 14),
                (Interceptacao, 14),
                (DesarmeEmPe, 12),
                (PasseLongo, 10),
                (ControleDeBola, 10),
                (Marcacao, 9),
                (Reacao, 7),
                (Folego, 6),
                (Carrinho, 5),
                (Agressividade, 5),
                (Forca, 4),
                (Visao, 4),
            ],
        ),
        (
            Perfil::MeioCampista,
            &[
                (PasseCurto, 17),
                (ControleDeBola, 14),
                (PasseLongo, 13),
                (Visao, 13),
                (Reacao, 8),
                (Folego, 8),
                (Drible, 7),
                (PosicionamentoOfensivo, 6),
                (Interceptacao, 5),
                (DesarmeEmPe, 5),
                (ChuteDeLonge, 4),
            ],
        ),
        (
            Perfil::MeiaAtacante,
            &[
                (PasseCurto, 16),
                (ControleDeBola, 15),
                (Visao, 14),
                (Drible, 13),
                (PosicionamentoOfensivo, 9),
                (Finalizacao, 7),
                (Reacao, 7),
                (Agilidade, 6),
                (ChuteDeLonge, 5),
                (Aceleracao, 4),
                (PasseLongo, 4),
            ],
        ),
        (
            Perfil::MeiaAberto,
            &[
                (Drible, 15),
                (ControleDeBola, 13),
                (PasseCurto, 11),
                (Cruzamento, 10),
                (PosicionamentoOfensivo, 8),
                (Aceleracao, 7),
                (Visao, 7),
                (Reacao, 7),
                (Velocidade, 6),
                (Folego, 6),
                (PasseLongo, 5),
                (Agilidade, 5),
            ],
        ),
        (
            Perfil::Ponta,
            &[
                (Drible, 16),
                (ControleDeBola, 14),
                (Finalizacao, 10),
                (Cruzamento, 9),
                (PasseCurto, 9),
                (PosicionamentoOfensivo, 9),
                (Aceleracao, 7),
                (Reacao, 7),
                (Velocidade, 6),
                (Visao, 6),
                (ChuteDeLonge, 4),
                (Agilidade, 3),
            ],
        ),
        (
            Perfil::SegundoAtacante,
            &[
                (ControleDeBola, 15),
                (Drible, 14),
                (PosicionamentoOfensivo, 13),
                (Finalizacao, 11),
                (PasseCurto, 9),
                (Reacao, 9),
                (Visao, 8),
                (ForcaDoChute, 5),
                (Aceleracao, 5),
                (Velocidade, 5),
                (ChuteDeLonge, 4),
                (Cabeceio, 2),
            ],
        ),
        (
            Perfil::Centroavante,
            &[
                (Finalizacao, 18),
                (PosicionamentoOfensivo, 13),
                (Cabeceio, 10),
                (ForcaDoChute, 10),
                (ControleDeBola, 10),
                (Reacao, 8),
                (Drible, 7),
                (Velocidade, 5),
                (Forca, 5),
                (PasseCurto, 5),
                (Aceleracao, 4),
                (ChuteDeLonge, 3),
                (Voleio, 2),
            ],
        ),
    ]
};

impl Perfil {
    pub fn pesos(self) -> &'static [(Atributo, u8)] {
        PERFIS.iter().find(|(p, _)| *p == self).map_or(&[], |(_, pesos)| pesos)
    }
}

/// Perfil da posição nativa (`preferredposition1`).
pub fn perfil_da_posicao(posicao: u8) -> Perfil {
    match posicao {
        0 => Perfil::Goleiro,
        1 | 7 => Perfil::Ala,
        2 | 6 => Perfil::Lateral,
        3..=5 => Perfil::Zagueiro,
        8..=10 => Perfil::Volante,
        11 | 15 => Perfil::MeiaAberto,
        16..=18 => Perfil::MeiaAtacante,
        19..=21 => Perfil::SegundoAtacante,
        22 | 26 => Perfil::Ponta,
        23..=25 => Perfil::Centroavante,
        _ => Perfil::MeioCampista,
    }
}

/// Nota (0–99) num perfil: média ponderada dos atributos conhecidos
/// (`valor` devolve `None` para atributo não observado; os pesos dos que
/// faltam saem da conta). `None` se nenhum atributo do perfil é conhecido.
pub fn nota_no_perfil(perfil: Perfil, valor: impl Fn(Atributo) -> Option<f32>) -> Option<f32> {
    let (soma, pesos) = perfil.pesos().iter().fold((0.0f32, 0u32), |(soma, pesos), &(a, peso)| match valor(a) {
        Some(v) => (soma + v * f32::from(peso), pesos + u32::from(peso)),
        None => (soma, pesos),
    });
    (pesos > 0).then(|| soma / pesos as f32)
}

/// Força do fit (%) para quem nasceu em `posicao`: nota no perfil-alvo ÷
/// nota no perfil da posição nativa, com teto 100. `None` sem dados.
pub fn forca_fit(alvo: PosicaoAlvo, posicao: u8, valor: impl Fn(Atributo) -> Option<f32>) -> Option<u8> {
    let no_alvo = nota_no_perfil(alvo.perfil(), &valor)?;
    let nativa = nota_no_perfil(perfil_da_posicao(posicao), &valor)?;
    if nativa <= 0.0 {
        return None;
    }
    Some((100.0 * no_alvo / nativa).round().clamp(0.0, 100.0) as u8)
}

/// Força mínima do fit para um jogador entrar no Relatório: perde no
/// máximo ~5% do nível dele na posição-alvo. Calibrado no save do Felipe
/// (2026-10-03, jogadores com Overall ≥ 60 de outra posição): passam de
/// ~15% (Centroavante, Zagueiro, Volante) a ~65% (Meia aberto, Ponta) —
/// posições vizinhas têm perfis parecidos. A ordem do Relatório (nota no
/// perfil-alvo) põe os melhores na frente.
pub const LIMIAR_FIT: u8 = 95;

// ---------------------------------------------------------------------
// Similaridade com o Jogador de Referência (Story 3.3)
// ---------------------------------------------------------------------

/// Similaridade mínima para um jogador entrar no Relatório. No save do
/// Felipe (2026-10-03, Overall ≥ 60), passam de ~30 (centroavante,
/// goleiro, meia) a ~3.300 (zagueiros, de perfis bem homogêneos) jogadores
/// por referência; o melhor parecido fica em 81–92%.
pub const LIMIAR_SIMILARIDADE: u8 = 75;
/// Pontos de similaridade perdidos por ponto de diferença média.
const PERDA_FORMA: f32 = 4.0;
const PERDA_NIVEL: f32 = 4.0;
const PESO_FORMA: f32 = 0.75;
/// Menos eixos em comum que isso não dá para comparar perfis.
const MINIMO_EIXOS_SIMILARIDADE: usize = 3;

/// Atributos comparados: os 5 de goleiro + reflexo/físico se a referência
/// é goleiro; senão os 28 de linha.
pub fn atributos_comparados(goleiro: bool) -> Vec<Atributo> {
    if goleiro {
        let extras = [Atributo::Reacao, Atributo::Agilidade, Atributo::Impulsao, Atributo::Forca];
        Atributo::TODOS.iter().copied().filter(|a| a.goleiro() || extras.contains(a)).collect()
    } else {
        Atributo::TODOS.iter().copied().filter(|a| !a.goleiro()).collect()
    }
}

/// Similaridade 0–100 entre um candidato e a referência, nos atributos
/// comparados que os dois têm (`None` = não observado). `None` com menos
/// de `MINIMO_EIXOS_SIMILARIDADE` eixos em comum.
pub fn similaridade(
    candidato: impl Fn(Atributo) -> Option<f32>,
    referencia: impl Fn(Atributo) -> Option<f32>,
    goleiro: bool,
) -> Option<u8> {
    let pares: Vec<(f32, f32)> =
        atributos_comparados(goleiro).into_iter().filter_map(|a| Some((candidato(a)?, referencia(a)?))).collect();
    if pares.len() < MINIMO_EIXOS_SIMILARIDADE {
        return None;
    }
    let n = pares.len() as f32;
    let media_c = pares.iter().map(|p| p.0).sum::<f32>() / n;
    let media_r = pares.iter().map(|p| p.1).sum::<f32>() / n;
    let diferenca_forma = pares.iter().map(|(c, r)| ((c - media_c) - (r - media_r)).abs()).sum::<f32>() / n;
    let forma = (100.0 - PERDA_FORMA * diferenca_forma).clamp(0.0, 100.0);
    let nivel = (100.0 - PERDA_NIVEL * (media_c - media_r).abs()).clamp(0.0, 100.0);
    Some((PESO_FORMA * forma + (1.0 - PESO_FORMA) * nivel).round() as u8)
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
/// 2.8) é sempre o primeiro; com Fit Posicional (3.4), vêm logo depois os
/// atributos que mais pesam no perfil-alvo, para a força do fit ser
/// calculada sobre o que importa.
pub fn ordem_de_observacao(funcao: Funcao, dominante: Option<Atributo>, alvo: Option<PosicaoAlvo>) -> Vec<Atributo> {
    use Atributo::*;
    let prioridade: &[Atributo] = match funcao {
        Funcao::Goleiro => &[GkReflexos, GkMergulho, GkColocacao, GkManejo, GkReposicao, Reacao, Impulsao, Forca],
        Funcao::Defensor => &[Marcacao, DesarmeEmPe, Carrinho, Interceptacao, Cabeceio, Forca, Velocidade, Reacao],
        Funcao::MeioCampo => &[PasseCurto, Visao, ControleDeBola, PasseLongo, Drible, Reacao, Folego, Interceptacao],
        Funcao::Atacante => &[Finalizacao, PosicionamentoOfensivo, Velocidade, Aceleracao, Drible, ControleDeBola, ForcaDoChute, Reacao],
    };
    let goleiro = funcao == Funcao::Goleiro;
    let mut do_alvo: Vec<(Atributo, u8)> = alvo.map(|p| p.perfil().pesos().to_vec()).unwrap_or_default();
    do_alvo.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let do_alvo: Vec<Atributo> = do_alvo.into_iter().map(|(a, _)| a).collect();
    let mut ordem: Vec<Atributo> = Vec::with_capacity(Atributo::TODOS.len());
    for &a in dominante.iter().chain(&do_alvo).chain(prioridade).chain(Atributo::TODOS.iter()) {
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

/// Relevância de um jogador para a Missão (0–99). `perfil`: as notas dos
/// critérios de perfil pedidos — valor do atributo dominante (2.8),
/// similaridade com a referência (3.3), nota no perfil-alvo (3.4). Com
/// algum, vale a média deles; sem nenhum, o tipo decide.
pub fn relevancia(tipo: TipoMissao, overall: u8, potencial: u8, perfil: &[u8]) -> u8 {
    if !perfil.is_empty() {
        let soma: u32 = perfil.iter().map(|&v| u32::from(v)).sum();
        return u8::try_from(soma / perfil.len() as u32).unwrap_or(u8::MAX);
    }
    match tipo {
        TipoMissao::Jovens => potencial,
        TipoMissao::Medalhoes => overall,
        TipoMissao::Tatica | TipoMissao::Geral => {
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
        let atacante = ordem_de_observacao(Funcao::Atacante, None, None);
        assert_eq!(atacante.first(), Some(&Atributo::Finalizacao));
        assert_eq!(atacante.len(), 28, "sem atributos de goleiro");
        assert!(atacante.iter().all(|a| !a.goleiro()));
        let goleiro = ordem_de_observacao(Funcao::Goleiro, None, None);
        assert_eq!(goleiro.len(), 33);
        assert!(goleiro.iter().take(5).all(|a| a.goleiro()));
        let drible = ordem_de_observacao(Funcao::Defensor, Some(Atributo::Drible), None);
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
        assert_eq!(relevancia(TipoMissao::Jovens, 60, 88, &[]), 88);
        assert_eq!(relevancia(TipoMissao::Medalhoes, 82, 84, &[]), 82);
        assert_eq!(relevancia(TipoMissao::Geral, 70, 80, &[]), 75);
        assert_eq!(relevancia(TipoMissao::Tatica, 70, 80, &[91]), 91);
        assert_eq!(semente(9, 9, 9), semente(9, 9, 9));
        assert_ne!(semente(9, 9, 9), semente(9, 9, 8));
    }

    #[test]
    fn a_dominant_attribute_makes_a_tactical_missao_and_counts_the_top_three() {
        let faixa = |min, max| FaixaAtributo { min, max };
        let mut filtros = FiltrosMissao { overall: faixa(50, 70), potencial: faixa(80, 99), ..FiltrosMissao::default() };
        assert_eq!(tipo_por_filtros(&filtros), TipoMissao::Jovens);
        filtros.atributo_dominante = Some(Atributo::Drible);
        assert_eq!(tipo_por_filtros(&filtros), TipoMissao::Tatica);
        assert!(combina(Especializacao::Tatico, TipoMissao::Tatica));
        let valores = [(Atributo::Velocidade, 90), (Atributo::Drible, 88), (Atributo::Forca, 88), (Atributo::Finalizacao, 85), (Atributo::Marcacao, 40)];
        assert!(eh_dominante(&valores, Atributo::Velocidade));
        assert!(eh_dominante(&valores, Atributo::Drible));
        assert!(eh_dominante(&valores, Atributo::Forca), "empate conta");
        assert!(!eh_dominante(&valores, Atributo::Finalizacao), "4º maior");
        assert!(!eh_dominante(&valores, Atributo::Marcacao));
        assert!(!eh_dominante(&valores, Atributo::GkReflexos), "fora da função");
    }

    #[test]
    fn breadth_comes_from_the_selected_countries() {
        let total = |c| if c == Confederacao::AmericaDoSul { 3 } else { 50 };
        use Confederacao::*;
        assert_eq!(amplitude_da_selecao(&[], total), AmplitudeGeografica::Mundo);
        assert_eq!(amplitude_da_selecao(&[Europa], total), AmplitudeGeografica::Pais);
        assert_eq!(amplitude_da_selecao(&[AmericaDoSul, AmericaDoSul], total), AmplitudeGeografica::VariosPaises);
        assert_eq!(amplitude_da_selecao(&[AmericaDoSul; 3], total), AmplitudeGeografica::Continente);
        assert_eq!(amplitude_da_selecao(&[Europa, AmericaDoSul], total), AmplitudeGeografica::Mundo);
    }

    #[test]
    fn partial_reports_grow_with_progress_and_never_exceed_what_was_found() {
        assert_eq!(revelados(0.0, 17, 17), 0);
        assert_eq!(revelados(0.01, 17, 17), 1, "arredonda para cima: logo aparece o primeiro");
        assert_eq!(revelados(0.5, 17, 17), 9);
        assert_eq!(revelados(1.0, 17, 17), 17);
        assert_eq!(revelados(1.0, 17, 5), 5, "pool pequeno");
        assert_eq!(revelados(2.0, 17, 17), 17);
        assert_eq!(revelados(-1.0, 17, 17), 0);
    }

    // -----------------------------------------------------------------
    // Épico 3
    // -----------------------------------------------------------------

    /// Perfil sintético: tudo em `base`, com os atributos de `altos` em `alto`.
    fn perfil(base: u8, altos: &[Atributo], alto: u8) -> [u8; 33] {
        let mut v = [base; 33];
        for a in altos {
            v[a.indice()] = alto;
        }
        v
    }

    fn de(v: &[u8; 33]) -> impl Fn(Atributo) -> Option<f32> + '_ {
        move |a| v.get(a.indice()).map(|&x| f32::from(x))
    }

    #[test]
    fn every_ideal_profile_weighs_one_hundred_without_repeating_attributes() {
        for (perfil, pesos) in PERFIS {
            let soma: u32 = pesos.iter().map(|(_, p)| u32::from(*p)).sum();
            assert_eq!(soma, 100, "{perfil:?}");
            let mut atributos: Vec<Atributo> = pesos.iter().map(|(a, _)| *a).collect();
            atributos.sort_unstable();
            atributos.dedup();
            assert_eq!(atributos.len(), pesos.len(), "{perfil:?} repete atributo");
            if perfil != Perfil::Goleiro {
                assert!(pesos.iter().all(|(a, _)| !a.goleiro()), "{perfil:?}");
            }
        }
        // toda posição do jogo tem um perfil nativo, e todo alvo tem pesos
        for posicao in 0..=27u8 {
            assert!(!perfil_da_posicao(posicao).pesos().is_empty());
        }
        for alvo in PosicaoAlvo::TODAS {
            assert!(!alvo.perfil().pesos().is_empty());
            assert!(!alvo.posicoes_nativas().is_empty());
            assert!(alvo.posicoes_nativas().iter().all(|&p| perfil_da_posicao(p) == alvo.perfil()), "{alvo:?}");
        }
    }

    #[test]
    fn an_attacking_midfielder_who_defends_well_fits_as_a_holding_midfielder() {
        use Atributo::*;
        let criativo = [PasseCurto, PasseLongo, Visao, ControleDeBola, Drible, PosicionamentoOfensivo, Finalizacao, Agilidade];
        let defesa = [Interceptacao, DesarmeEmPe, Marcacao, Carrinho, Agressividade, Folego, Forca, Reacao];
        // MEI (posição 17) com passe E defesa altos
        let completo = {
            let mut v = perfil(55, &criativo, 82);
            for a in defesa {
                v[a.indice()] = 80;
            }
            v
        };
        let fit = forca_fit(PosicaoAlvo::Volante, 17, de(&completo)).expect("fit");
        assert!(fit >= LIMIAR_FIT, "{fit}");
        // o mesmo MEI sem defesa não serve de volante
        let so_ataque = perfil(45, &criativo, 82);
        let fit_fraco = forca_fit(PosicaoAlvo::Volante, 17, de(&so_ataque)).expect("fit");
        assert!(fit_fraco < LIMIAR_FIT, "{fit_fraco}");
        assert!(fit > fit_fraco);
        // e um centroavante puro não serve de zagueiro
        let atacante = perfil(40, &[Finalizacao, PosicionamentoOfensivo, ForcaDoChute, Cabeceio, ControleDeBola], 85);
        assert!(forca_fit(PosicaoAlvo::Zagueiro, 24, de(&atacante)).expect("fit") < 70);
    }

    #[test]
    fn fit_is_capped_at_one_hundred_and_ignores_unobserved_attributes() {
        // um jogador melhor no alvo que na própria posição: 100, não 130
        let zagueiro_que_arma = perfil(50, &[Atributo::PasseCurto, Atributo::Visao, Atributo::ControleDeBola, Atributo::PasseLongo], 90);
        assert_eq!(forca_fit(PosicaoAlvo::MeioCampista, 4, de(&zagueiro_que_arma)), Some(100));
        // só os atributos observados contam
        let observado = |a: Atributo| (a == Atributo::Finalizacao).then_some(80.0);
        assert_eq!(nota_no_perfil(Perfil::Centroavante, observado), Some(80.0));
        assert_eq!(nota_no_perfil(Perfil::Zagueiro, observado), None, "nenhum atributo do perfil observado");
        assert_eq!(forca_fit(PosicaoAlvo::Zagueiro, 24, observado), None);
    }

    #[test]
    fn identical_profiles_are_one_hundred_percent_similar_and_unrelated_ones_are_low() {
        use Atributo::*;
        let atacante = perfil(45, &[Finalizacao, PosicionamentoOfensivo, ForcaDoChute, Velocidade, Aceleracao, Drible], 85);
        let zagueiro = perfil(45, &[Marcacao, DesarmeEmPe, Carrinho, Interceptacao, Cabeceio, Forca], 85);
        assert_eq!(similaridade(de(&atacante), de(&atacante), false), Some(100));
        let diferente = similaridade(de(&zagueiro), de(&atacante), false).expect("similaridade");
        assert!(diferente < 50, "{diferente}");
        // o mesmo formato 8 pontos abaixo ainda é parecido (e passa no limiar)
        let mais_fraco = atacante.map(|v| v - 8);
        let parecido = similaridade(de(&mais_fraco), de(&atacante), false).expect("similaridade");
        assert!((LIMIAR_SIMILARIDADE..100).contains(&parecido), "{parecido}");
        // poucos eixos em comum: não dá para dizer
        let um_so = |a: Atributo| (a == Finalizacao).then_some(80.0);
        assert_eq!(similaridade(um_so, de(&atacante), false), None);
        // goleiro compara os atributos de goleiro
        assert!(atributos_comparados(true).iter().filter(|a| a.goleiro()).count() == 5);
        assert!(atributos_comparados(false).iter().all(|a| !a.goleiro()));
    }

    #[test]
    fn profile_filters_make_a_tactical_missao_and_the_tatico_gets_the_better_quality() {
        let mut filtros = FiltrosMissao::default();
        assert_eq!(tipo_por_filtros(&filtros), TipoMissao::Geral);
        filtros.fit_posicional = Some(PosicaoAlvo::Volante);
        assert_eq!(tipo_por_filtros(&filtros), TipoMissao::Tatica);
        let pedido = |especializacao| PedidoMissao {
            tier: Tier::Experiente,
            especializacao,
            modo: ModoBusca::Rapida,
            tipo: tipo_por_filtros(&filtros),
            amplitude: AmplitudeGeografica::Pais,
        };
        for outra in [Especializacao::Generalista, Especializacao::CacadorDeJovens, Especializacao::CacadorDeMedalhoes] {
            assert!(estimar_missao(&pedido(Especializacao::Tatico)).qualidade > estimar_missao(&pedido(outra)).qualidade);
        }
    }

    #[test]
    fn with_a_target_position_its_heaviest_attributes_are_observed_early() {
        let ordem = ordem_de_observacao(Funcao::MeioCampo, None, Some(PosicaoAlvo::Volante));
        let topo: Vec<Atributo> = ordem.iter().take(3).copied().collect();
        assert_eq!(topo, [Atributo::PasseCurto, Atributo::Interceptacao, Atributo::DesarmeEmPe]);
        assert_eq!(ordem.len(), 28, "sem atributos de goleiro, sem repetir");
        let com_dominante = ordem_de_observacao(Funcao::MeioCampo, Some(Atributo::Drible), Some(PosicaoAlvo::Volante));
        assert_eq!(com_dominante.first(), Some(&Atributo::Drible));
        assert_eq!(relevancia(TipoMissao::Tatica, 70, 80, &[80, 90]), 85, "média dos critérios de perfil");
    }
}
