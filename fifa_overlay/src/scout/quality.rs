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
//! **Jogadores** (alvo do Relatório): Completa traz MAIS nomes que Rápida
//! (mudou em 2026-10-03, pedido do Felipe: do jeito antigo, várias Rápidas
//! seguidas rendiam mais que uma Completa). Rápida é a resposta curta e
//! barata; Completa leva ~3× o tempo e rende ~4× os nomes, com Qualidade
//! maior. Dentro do Modo, mais pontuação traz mais nomes.
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

/// Amplitude do filtro geográfico (`amplitude_da_geografia`).
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
/// Rápida: 7 a 11; Completa: 28 a 44. No mesmo tempo de uma Completa
/// cabem ~3 Rápidas (21 a 33 nomes, de Qualidade menor): a Completa sempre
/// rende mais.
fn alvo_jogadores(modo: ModoBusca, pontos: u8) -> u8 {
    let (base, por_ponto) = match modo {
        ModoBusca::Rapida => (6, 1),
        ModoBusca::Completa => (24, 4),
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

/// Idades que o formulário aceita (anos completos na data da carreira).
pub const IDADE_MENOR: u8 = 15;
pub const IDADE_MAIOR: u8 = 45;
/// Idade máxima até a qual a Missão é de "jovens".
pub const IDADE_JOVEM: u8 = 21;
/// Anos de contrato restantes que o formulário aceita: 0 = termina nesta
/// temporada; este valor = "isso ou mais".
pub const CONTRATO_MAIOR: u8 = 5;
/// Estrelas (dribles, pé fraco): 1 a 5.
pub const ESTRELAS_MENOR: u8 = 1;
pub const ESTRELAS_MAIOR: u8 = 5;
/// "Ambidestro" no filtro de pé: pé fraco com pelo menos estas estrelas
/// (4 e 5 estrelas chutam bem com os dois pés; no save do Felipe, ~23% dos
/// jogadores).
pub const PE_FRACO_AMBIDESTRO: u8 = 4;

/// Tipo da Missão a partir de todos os filtros: pedir um perfil —
/// atributos dominantes ("o melhor driblador", Story 2.8), um Fit
/// Posicional (3.4) ou um Jogador de Referência (3.3) — é uma Missão
/// **Tática**, a especialidade do Tático; idade máxima até `IDADE_JOVEM` é
/// uma Missão de **Jovens**; senão, valem as faixas.
pub fn tipo_por_filtros(filtros: &FiltrosMissao) -> TipoMissao {
    let pede_perfil =
        !filtros.atributos_dominantes.is_empty() || filtros.fit_posicional.is_some() || filtros.referencia.is_some();
    if pede_perfil {
        return TipoMissao::Tatica;
    }
    match filtros.nivel_elenco {
        Some(NivelEquipe::Promessa) => TipoMissao::Jovens,
        // prontos para jogar no nível do time ou acima: medalhões
        Some(NivelEquipe::MudaPatamar | NivelEquipe::Titular) => TipoMissao::Medalhoes,
        _ if filtros.idade.max <= IDADE_JOVEM => TipoMissao::Jovens,
        _ => tipo_por_faixas(filtros.overall, filtros.potencial),
    }
}

/// Os filtros "perfeitos" de cada Especialização: o formulário Nova Missão
/// abre com eles (2026-10-03, pedido do Felipe) e a régua é o padrão da
/// equipe (`NivelEquipe`): por padrão, quem **muda o patamar** do time na
/// posição. Os três especialistas dão o tipo de Missão que combina com
/// eles (bônus de Qualidade), conferido em teste. O jogador pode mudar
/// tudo; "Restaurar sugestão" volta a eles.
/// - Caçador de Jovens: até 21 anos, promessa (Potencial passa o titular);
/// - Caçador de Medalhões: muda patamar, 22 a 31 anos;
/// - Tático: muda patamar, armadores (Visão e Passe curto entre os maiores);
/// - Generalista: muda patamar, 17 a 33 anos.
pub fn filtros_ideais(especializacao: Especializacao) -> FiltrosMissao {
    let faixa = |min, max| FaixaAtributo { min, max };
    let base = FiltrosMissao {
        overall: faixa(40, 99),
        potencial: faixa(40, 99),
        nivel_elenco: Some(NivelEquipe::MudaPatamar),
        ..FiltrosMissao::default()
    };
    match especializacao {
        Especializacao::CacadorDeJovens => {
            FiltrosMissao { idade: faixa(IDADE_MENOR, IDADE_JOVEM), nivel_elenco: Some(NivelEquipe::Promessa), ..base }
        }
        Especializacao::CacadorDeMedalhoes => FiltrosMissao { idade: faixa(22, 31), ..base },
        Especializacao::Tatico => {
            FiltrosMissao { idade: faixa(18, 30), atributos_dominantes: vec![Atributo::Visao, Atributo::PasseCurto], ..base }
        }
        Especializacao::Generalista => FiltrosMissao { idade: faixa(17, 33), ..base },
    }
}

/// Um jogador "tem" um atributo dominante quando ele está entre os seus
/// `TOP_DOMINANTE` maiores atributos (empates contam). Só o maior seria
/// raro demais (muitos jogadores têm Velocidade ou Força no topo).
pub const TOP_DOMINANTE: usize = 3;

/// Quantos atributos dominantes uma Missão pode pedir juntos.
pub const MAX_DOMINANTES: usize = 3;

/// Com `pedidos` atributos dominantes, cada um precisa estar entre os
/// `TOP_DOMINANTE + pedidos − 1` maiores: dois pedidos → top 4; três → top
/// 5 (senão três atributos "todos no top 3" quase nunca acontece).
pub fn top_para(pedidos: usize) -> usize {
    TOP_DOMINANTE + pedidos.saturating_sub(1)
}

/// `valores`: os atributos do jogador que contam para a função dele (os de
/// goleiro só para goleiros). Verdadeiro se `alvo` está entre os `top`
/// maiores (empates incluídos).
pub fn eh_dominante(valores: &[(Atributo, u8)], alvo: Atributo, top: usize) -> bool {
    let Some(&(_, valor_alvo)) = valores.iter().find(|(a, _)| *a == alvo) else {
        return false;
    };
    let maiores = valores.iter().filter(|(_, v)| *v > valor_alvo).count();
    maiores < top
}

/// Um item escolhido no filtro geográfico (onde o jogador joga).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscopoGeografico {
    Continente(Confederacao),
    Pais(Confederacao, u16),
    /// `pais` é `None` nas ligas sem país ("Clubes da UEFA").
    Liga { continente: Confederacao, pais: Option<u16>, liga: u32 },
}

impl EscopoGeografico {
    fn continente(self) -> Confederacao {
        match self {
            EscopoGeografico::Continente(c) | EscopoGeografico::Pais(c, _) => c,
            EscopoGeografico::Liga { continente, .. } => continente,
        }
    }
}

/// Amplitude da geografia escolhida (2026-10-03):
/// - nada escolhido = o mundo todo → `Mundo`;
/// - mais de um continente envolvido → `Mundo`;
/// - um continente inteiro (e só coisas dele) → `Continente`;
/// - um país só (inteiro ou ligas dele) → `Pais`;
/// - vários países do mesmo continente → `VariosPaises`.
///
/// Uma liga sem país conta como um lugar à parte.
pub fn amplitude_da_geografia(escopos: &[EscopoGeografico]) -> AmplitudeGeografica {
    if escopos.is_empty() {
        return AmplitudeGeografica::Mundo;
    }
    let continentes: std::collections::BTreeSet<Confederacao> = escopos.iter().map(|e| e.continente()).collect();
    if continentes.len() > 1 {
        return AmplitudeGeografica::Mundo;
    }
    if escopos.iter().any(|e| matches!(e, EscopoGeografico::Continente(_))) {
        return AmplitudeGeografica::Continente;
    }
    let lugares: std::collections::BTreeSet<(Option<u16>, Option<u32>)> = escopos
        .iter()
        .map(|e| match *e {
            EscopoGeografico::Pais(_, p) | EscopoGeografico::Liga { pais: Some(p), .. } => (Some(p), None),
            EscopoGeografico::Liga { pais: None, liga, .. } => (None, Some(liga)),
            EscopoGeografico::Continente(_) => (None, None),
        })
        .collect();
    if lugares.len() == 1 {
        AmplitudeGeografica::Pais
    } else {
        AmplitudeGeografica::VariosPaises
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
// Valor de mercado e salário estimados (branch `claude/relatorio-ficha`,
// 2026-10-01; salário recalibrado em 2026-10-03)
// ---------------------------------------------------------------------
//
// O FIFA não grava valor nem salário dos jogadores: calcula na hora, ao
// abrir a tela do jogador. O save só tem os contratos do PRÓPRIO elenco
// (`career_playercontract`). O Relatório mostra uma ESTIMATIVA do Olheiro,
// a partir do que ele revelou (meio das faixas) e da idade; o teto de
// gastos da busca usa a mesma conta com os valores reais.
// - valor = 3 M × 1,2^(Overall − 70) × idade × margem de crescimento,
//   goleiro × 0,8 (Overall 80, 27 anos ≈ 18,6 M; 90 ≈ 115 M; 60 ≈ 0,5 M);
// - salário semanal: calibrado com os contratos reais do elenco do Felipe
//   (2026-10-03): Overall 70 ≈ 20 mil, 80 ≈ 120 mil, 87 ≈ 240 mil,
//   90 ≈ 300 mil. Sobe ~19,6% por ponto até 80 e ~9,6% depois.

/// Multiplicador de valor pela idade.
fn fator_idade(idade: u8) -> f64 {
    match idade {
        0..=21 => 1.3,
        22..=25 => 1.15,
        26..=29 => 1.0,
        30..=31 => 0.7,
        32..=33 => 0.45,
        _ => 0.25,
    }
}

/// Arredonda para um número "de mercado": 100 mil acima de 1 M, 5 mil abaixo.
fn arredondar_mercado(valor: f64) -> i64 {
    let passo = if valor >= 1_000_000.0 { 100_000.0 } else { 5_000.0 };
    let v = ((valor / passo).round() * passo) as i64;
    v.max(10_000)
}

/// Valor de mercado estimado (mesma unidade do orçamento).
pub fn valor_estimado(overall: u8, potencial: u8, idade: u8, goleiro: bool) -> i64 {
    let base = 3_000_000.0 * 1.2f64.powi(i32::from(overall) - 70);
    let margem = 1.0 + (f64::from(potencial.saturating_sub(overall)) * 0.04).min(0.8);
    let posicao = if goleiro { 0.8 } else { 1.0 };
    arredondar_mercado(base * fator_idade(idade) * margem * posicao)
}

/// Salário semanal estimado.
pub fn salario_estimado(overall: u8) -> i64 {
    let delta = i32::from(overall) - 70;
    let base = if delta <= 10 {
        20_000.0 * 1.196f64.powi(delta)
    } else {
        120_000.0 * 1.096f64.powi(delta - 10)
    };
    let passo = if base >= 10_000.0 { 1_000.0 } else { 100.0 };
    (((base / passo).round() * passo) as i64).max(500)
}

/// Quanto o Olheiro já sabe de um jogador que apareceu no Relatório
/// parcial (pedido do Felipe, 2026-10-01): primeiro o mercado (valor e
/// contrato), depois a expectativa de salário, por fim os atributos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Observacao {
    /// Só Overall/Potencial, valor e contrato.
    SoMercado,
    /// + expectativa de salário.
    MercadoESalario,
    /// Tudo o que a Qualidade permite (atributos inclusive).
    #[default]
    Completa,
}

/// Quanto do progresso da Missão (0–1) depois de o jogador aparecer o
/// Olheiro leva para saber o salário e para observar os atributos.
const MATURACAO_SALARIO: f32 = 0.08;
const MATURACAO_ATRIBUTOS: f32 = 0.18;

/// Etapa de observação do `indice`-ésimo jogador (ordem de descoberta)
/// com o progresso `fracao` da Missão. Missão concluída: tudo.
pub fn observacao(fracao: f32, indice: usize, alvo: usize, concluida: bool) -> Observacao {
    if concluida || alvo == 0 {
        return Observacao::Completa;
    }
    let apareceu = indice as f32 / alvo as f32;
    let tempo = fracao.clamp(0.0, 1.0) - apareceu;
    if fracao >= 1.0 || tempo >= MATURACAO_ATRIBUTOS {
        Observacao::Completa
    } else if tempo >= MATURACAO_SALARIO {
        Observacao::MercadoESalario
    } else {
        Observacao::SoMercado
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

    /// Códigos de `preferredposition1` que JÁ são esta posição (enum do
    /// FIFA, ver `save_repo::nome_posicao`).
    pub fn posicoes_nativas(self) -> &'static [u8] {
        match self {
            PosicaoAlvo::Zagueiro => &[1, 4, 5, 6],
            PosicaoAlvo::LateralDireito => &[3],
            PosicaoAlvo::LateralEsquerdo => &[7],
            PosicaoAlvo::AlaDireito => &[2],
            PosicaoAlvo::AlaEsquerdo => &[8],
            PosicaoAlvo::Volante => &[9, 10, 11],
            PosicaoAlvo::MeioCampista => &[13, 14, 15],
            PosicaoAlvo::MeiaAtacante => &[17, 18, 19],
            PosicaoAlvo::MeiaDireita => &[12],
            PosicaoAlvo::MeiaEsquerda => &[16],
            PosicaoAlvo::PontaDireita => &[23],
            PosicaoAlvo::PontaEsquerda => &[27],
            PosicaoAlvo::SegundoAtacante => &[20, 21, 22],
            PosicaoAlvo::Centroavante => &[24, 25, 26],
        }
    }

    /// Quem NÃO entra num Fit para esta posição: a posição nativa e as
    /// mudanças triviais (2026-10-03, pedido do Felipe — "o ideal seria não
    /// ter lateral nesse caso"): para qualquer posição da defesa, toda a
    /// linha de defesa (zagueiros, laterais e alas dos dois lados); para
    /// os lados do meio e do ataque, todos os jogadores de lado (meias
    /// abertos e pontas); nas demais, só a posição nativa.
    pub fn posicoes_excluidas(self) -> &'static [u8] {
        const DEFESA: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8];
        const LADOS: &[u8] = &[12, 16, 23, 27];
        match self {
            PosicaoAlvo::Zagueiro
            | PosicaoAlvo::LateralDireito
            | PosicaoAlvo::LateralEsquerdo
            | PosicaoAlvo::AlaDireito
            | PosicaoAlvo::AlaEsquerdo => DEFESA,
            PosicaoAlvo::MeiaDireita | PosicaoAlvo::MeiaEsquerda | PosicaoAlvo::PontaDireita | PosicaoAlvo::PontaEsquerda => {
                LADOS
            }
            outra => outra.posicoes_nativas(),
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

/// Perfil da posição nativa (`preferredposition1`, enum do FIFA).
pub fn perfil_da_posicao(posicao: u8) -> Perfil {
    match posicao {
        0 => Perfil::Goleiro,
        2 | 8 => Perfil::Ala,
        3 | 7 => Perfil::Lateral,
        1 | 4..=6 => Perfil::Zagueiro,
        9..=11 => Perfil::Volante,
        12 | 16 => Perfil::MeiaAberto,
        17..=19 => Perfil::MeiaAtacante,
        20..=22 => Perfil::SegundoAtacante,
        23 | 27 => Perfil::Ponta,
        24..=26 => Perfil::Centroavante,
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

/// Quanto o Overall do jogador mudaria jogando na posição-alvo (estimativa,
/// em pontos): nota no perfil-alvo − nota no perfil da posição nativa. Os
/// perfis imitam a nota por posição do FIFA, então a diferença entre eles
/// aproxima a mudança do Overall que o jogo mostraria (2026-10-03).
pub fn variacao_overall(alvo: PosicaoAlvo, posicao: u8, valor: impl Fn(Atributo) -> Option<f32>) -> Option<i8> {
    let no_alvo = nota_no_perfil(alvo.perfil(), &valor)?;
    let nativa = nota_no_perfil(perfil_da_posicao(posicao), &valor)?;
    Some((no_alvo - nativa).round().clamp(-99.0, 99.0) as i8)
}

/// Força mínima do fit para um jogador entrar no Relatório: perde no
/// máximo ~5% do nível dele na posição-alvo. Calibrado no save do Felipe
/// (2026-10-03, com os códigos de posição corrigidos e sem as mudanças
/// triviais; jogadores com Overall ≥ 60 de fora delas): passam ~17–19%
/// para Centroavante, Volante e as posições da defesa, ~28% para
/// Meio-campista e ~45–58% para meias, pontas e segundo atacante (perfis
/// de ataque se parecem). A ordem do Relatório (nota no perfil-alvo) põe
/// os melhores na frente.
pub const LIMIAR_FIT: u8 = 95;

// ---------------------------------------------------------------------
// Nível em relação ao elenco e atalhos de filtro (2026-10-03)
// ---------------------------------------------------------------------
//
// Pedido do Felipe: o Olheiro procura pelo padrão da equipe. A régua de
// cada candidato é o **titular do elenco na posição dele** — o maior
// Overall do elenco com o mesmo perfil de posição (`perfil_da_posicao`;
// com Fit Posicional, o perfil-alvo). Sem ninguém do elenco nesse perfil,
// vale a média dos 11 melhores do elenco.
// - Muda patamar: Overall ≥ titular + `MARGEM_PATAMAR` (o time tem um
//   atacante 67; um de 71 muda o patamar). É o padrão dos Olheiros.
// - Nível titular: Overall entre titular − 2 e titular + 2.
// - Nível banco: Overall entre titular − 8 e titular − 3.
// - Promessa: Potencial ≥ titular + `MARGEM_PATAMAR` (vai passar o
//   titular), qualquer Overall.

/// Quanto acima do titular um jogador precisa estar para "mudar o patamar".
pub const MARGEM_PATAMAR: u8 = 3;

/// Nível pedido em relação ao elenco. No JSON: `"muda_patamar"` etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NivelEquipe {
    MudaPatamar,
    Titular,
    Banco,
    Promessa,
}

impl NivelEquipe {
    pub const TODOS: [NivelEquipe; 4] = [NivelEquipe::MudaPatamar, NivelEquipe::Titular, NivelEquipe::Banco, NivelEquipe::Promessa];

    pub fn nome(self) -> &'static str {
        match self {
            NivelEquipe::MudaPatamar => "Muda patamar",
            NivelEquipe::Titular => "Nível titular",
            NivelEquipe::Banco => "Nível banco",
            NivelEquipe::Promessa => "Promessa",
        }
    }

    /// Explicação com o titular da posição (`titular`).
    pub fn regra(self, titular: u8) -> String {
        let t = i16::from(titular);
        let m = i16::from(MARGEM_PATAMAR);
        match self {
            NivelEquipe::MudaPatamar => format!("Overall {} ou mais", t + m),
            NivelEquipe::Titular => format!("Overall de {} a {}", t - 2, t + 2),
            NivelEquipe::Banco => format!("Overall de {} a {}", t - 8, t - 3),
            NivelEquipe::Promessa => format!("Potencial {} ou mais", t + m),
        }
    }
}

/// O jogador está no nível pedido, comparado ao titular da posição?
pub fn no_nivel(nivel: NivelEquipe, overall: u8, potencial: u8, titular: u8) -> bool {
    let (ovr, pot, t, m) = (i16::from(overall), i16::from(potencial), i16::from(titular), i16::from(MARGEM_PATAMAR));
    match nivel {
        NivelEquipe::MudaPatamar => ovr >= t + m,
        NivelEquipe::Titular => (t - 2..=t + 2).contains(&ovr),
        NivelEquipe::Banco => (t - 8..=t - 3).contains(&ovr),
        NivelEquipe::Promessa => pot >= t + m,
    }
}

/// O titular de cada perfil de posição do elenco.
#[derive(Debug, Clone, PartialEq)]
pub struct NivelElenco {
    pub por_perfil: Vec<(Perfil, u8)>,
    /// Média dos 11 melhores (perfil sem ninguém no elenco).
    pub geral: u8,
}

impl NivelElenco {
    /// `elenco`: (posição, Overall) de cada jogador do técnico.
    pub fn de(elenco: &[(u8, u8)]) -> NivelElenco {
        let mut por_perfil: Vec<(Perfil, u8)> = Vec::new();
        for &(posicao, overall) in elenco {
            let perfil = perfil_da_posicao(posicao);
            match por_perfil.iter_mut().find(|(p, _)| *p == perfil) {
                Some((_, melhor)) => *melhor = (*melhor).max(overall),
                None => por_perfil.push((perfil, overall)),
            }
        }
        let mut overalls: Vec<u8> = elenco.iter().map(|&(_, o)| o).collect();
        overalls.sort_unstable_by(|a, b| b.cmp(a));
        let onze: Vec<u32> = overalls.iter().take(11).map(|&o| u32::from(o)).collect();
        let geral = if onze.is_empty() { 60 } else { u8::try_from(onze.iter().sum::<u32>() / onze.len() as u32).unwrap_or(60) };
        NivelElenco { por_perfil, geral }
    }

    pub fn titular(&self, perfil: Perfil) -> u8 {
        self.por_perfil.iter().find(|(p, _)| *p == perfil).map_or(self.geral, |&(_, o)| o)
    }
}

impl Perfil {
    /// Nome curto do perfil (resumo dos titulares no formulário).
    pub fn sigla(self) -> &'static str {
        match self {
            Perfil::Goleiro => "GOL",
            Perfil::Zagueiro => "ZAG",
            Perfil::Lateral => "LAT",
            Perfil::Ala => "ALA",
            Perfil::Volante => "VOL",
            Perfil::MeioCampista => "MC",
            Perfil::MeiaAtacante => "MEI",
            Perfil::MeiaAberto => "MAB",
            Perfil::Ponta => "PON",
            Perfil::SegundoAtacante => "SA",
            Perfil::Centroavante => "ATA",
        }
    }
}

/// Atalhos de filtro: um clique monta uma busca comum. Mantêm a geografia
/// e o teto de gastos; o resto volta ao padrão antes de aplicar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atalho {
    JovensPromessas,
    MudaPatamar,
    NivelTitular,
    NivelBanco,
    FimDeContrato,
}

impl Atalho {
    pub const TODOS: [Atalho; 5] =
        [Atalho::MudaPatamar, Atalho::JovensPromessas, Atalho::NivelTitular, Atalho::NivelBanco, Atalho::FimDeContrato];

    pub fn nome(self) -> &'static str {
        match self {
            Atalho::JovensPromessas => "Jovens promessas",
            Atalho::MudaPatamar => "Muda patamar",
            Atalho::NivelTitular => "Nível titular",
            Atalho::NivelBanco => "Nível banco",
            Atalho::FimDeContrato => "Fim de contrato",
        }
    }

    pub fn descricao(self) -> &'static str {
        match self {
            Atalho::JovensPromessas => "Até 21 anos com potencial para passar o seu titular.",
            Atalho::MudaPatamar => "Melhores que o seu titular na posição.",
            Atalho::NivelTitular => "Do nível do seu titular: reposição ou disputa.",
            Atalho::NivelBanco => "Para completar o elenco, abaixo do titular.",
            Atalho::FimDeContrato => "Nível titular com contrato acabando: mais baratos.",
        }
    }

    /// Os filtros do atalho sobre `atuais` (fica a geografia e o teto).
    pub fn aplicar(self, atuais: &FiltrosMissao) -> FiltrosMissao {
        let faixa = |min, max| FaixaAtributo { min, max };
        let base = FiltrosMissao {
            overall: faixa(40, 99),
            potencial: faixa(40, 99),
            continentes: atuais.continentes.clone(),
            paises_dos_clubes: atuais.paises_dos_clubes.clone(),
            ligas: atuais.ligas.clone(),
            sem_teto: atuais.sem_teto,
            ..FiltrosMissao::default()
        };
        match self {
            Atalho::JovensPromessas => {
                FiltrosMissao { idade: faixa(IDADE_MENOR, IDADE_JOVEM), nivel_elenco: Some(NivelEquipe::Promessa), ..base }
            }
            Atalho::MudaPatamar => FiltrosMissao { idade: faixa(18, 31), nivel_elenco: Some(NivelEquipe::MudaPatamar), ..base },
            Atalho::NivelTitular => FiltrosMissao { idade: faixa(18, 33), nivel_elenco: Some(NivelEquipe::Titular), ..base },
            Atalho::NivelBanco => FiltrosMissao { idade: faixa(17, 33), nivel_elenco: Some(NivelEquipe::Banco), ..base },
            Atalho::FimDeContrato => FiltrosMissao {
                idade: faixa(18, 34),
                contrato: faixa(0, 0),
                nivel_elenco: Some(NivelEquipe::Titular),
                ..base
            },
        }
    }
}

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
pub fn ordem_de_observacao(funcao: Funcao, dominantes: &[Atributo], alvo: Option<PosicaoAlvo>) -> Vec<Atributo> {
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
    for &a in dominantes.iter().chain(&do_alvo).chain(prioridade).chain(Atributo::TODOS.iter()) {
        // Atributos de goleiro só entram na observação de goleiros.
        if (a.goleiro() && !goleiro && !dominantes.contains(&a)) || ordem.contains(&a) {
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
    fn completa_has_better_quality_more_players_and_takes_longer_than_rapida() {
        for pedido in todos_os_pedidos().into_iter().filter(|p| p.modo == ModoBusca::Rapida) {
            let completa = PedidoMissao { modo: ModoBusca::Completa, ..pedido };
            let (r, c) = (estimar_missao(&pedido), estimar_missao(&completa));
            assert!(c.qualidade >= r.qualidade && c.atributos_revelados > r.atributos_revelados, "{pedido:?}");
            assert!(c.alvo_jogadores > r.alvo_jogadores, "{pedido:?}");
            assert!(c.duracao_dias > r.duracao_dias, "{pedido:?}");
            assert!(c.custo > r.custo, "{pedido:?}");
            // várias Rápidas no tempo de uma Completa rendem menos nomes
            let rapidas = c.duracao_dias / r.duracao_dias.max(1);
            assert!(u32::from(c.alvo_jogadores) > rapidas * u32::from(r.alvo_jogadores), "{pedido:?}: {c:?} × {r:?}");
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
                alvo_jogadores: 7,
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
                alvo_jogadores: 44,
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
        let atacante = ordem_de_observacao(Funcao::Atacante, &[], None);
        assert_eq!(atacante.first(), Some(&Atributo::Finalizacao));
        assert_eq!(atacante.len(), 28, "sem atributos de goleiro");
        assert!(atacante.iter().all(|a| !a.goleiro()));
        let goleiro = ordem_de_observacao(Funcao::Goleiro, &[], None);
        assert_eq!(goleiro.len(), 33);
        assert!(goleiro.iter().take(5).all(|a| a.goleiro()));
        let drible = ordem_de_observacao(Funcao::Defensor, &[Atributo::Drible], None);
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
        filtros.atributos_dominantes = vec![Atributo::Drible];
        assert_eq!(tipo_por_filtros(&filtros), TipoMissao::Tatica);
        assert!(combina(Especializacao::Tatico, TipoMissao::Tatica));
        let valores = [(Atributo::Velocidade, 90), (Atributo::Drible, 88), (Atributo::Forca, 88), (Atributo::Finalizacao, 85), (Atributo::Marcacao, 40)];
        let top = top_para(1);
        assert!(eh_dominante(&valores, Atributo::Velocidade, top));
        assert!(eh_dominante(&valores, Atributo::Drible, top));
        assert!(eh_dominante(&valores, Atributo::Forca, top), "empate conta");
        assert!(!eh_dominante(&valores, Atributo::Finalizacao, top), "4º maior");
        assert!(eh_dominante(&valores, Atributo::Finalizacao, top_para(2)), "com dois pedidos, top 4");
        assert!(!eh_dominante(&valores, Atributo::Marcacao, top));
        assert!(!eh_dominante(&valores, Atributo::GkReflexos, top), "fora da função");
    }

    #[test]
    fn breadth_comes_from_continents_countries_and_leagues() {
        use Confederacao::*;
        use EscopoGeografico as E;
        let liga = |continente, pais, liga| E::Liga { continente, pais, liga };
        assert_eq!(amplitude_da_geografia(&[]), AmplitudeGeografica::Mundo);
        assert_eq!(amplitude_da_geografia(&[liga(AmericaDoSul, Some(54), 7)]), AmplitudeGeografica::Pais);
        assert_eq!(
            amplitude_da_geografia(&[liga(AmericaDoSul, Some(54), 7), liga(AmericaDoSul, Some(54), 83), E::Pais(AmericaDoSul, 54)]),
            AmplitudeGeografica::Pais,
            "ligas do mesmo país"
        );
        assert_eq!(amplitude_da_geografia(&[E::Pais(Europa, 14), E::Pais(Europa, 45)]), AmplitudeGeografica::VariosPaises);
        assert_eq!(amplitude_da_geografia(&[E::Pais(Europa, 14), liga(Europa, None, 77)]), AmplitudeGeografica::VariosPaises);
        assert_eq!(amplitude_da_geografia(&[E::Continente(Europa), E::Pais(Europa, 14)]), AmplitudeGeografica::Continente);
        assert_eq!(amplitude_da_geografia(&[E::Continente(Europa), E::Pais(AmericaDoSul, 54)]), AmplitudeGeografica::Mundo);
        assert_eq!(amplitude_da_geografia(&[E::Pais(Europa, 14), E::Pais(AmericaDoSul, 54)]), AmplitudeGeografica::Mundo);
    }

    #[test]
    fn each_especializacao_opens_with_filters_that_match_it() {
        for e in Especializacao::TODAS {
            let f = filtros_ideais(e);
            assert!(f.overall.valida() && f.potencial.valida() && f.idade.valida() && f.contrato.valida(), "{e:?}");
            assert!(f.idade.min >= IDADE_MENOR && f.idade.max <= IDADE_MAIOR, "{e:?}");
            let tipo = tipo_por_filtros(&f);
            if e != Especializacao::Generalista {
                assert!(combina(e, tipo), "{e:?} → {tipo:?}");
            }
            assert!(f.nivel_elenco.is_some(), "a régua é o padrão da equipe");
        }
        assert!(filtros_ideais(Especializacao::Tatico).atributos_dominantes.len() <= MAX_DOMINANTES);
    }

    #[test]
    fn a_young_age_cap_makes_a_youth_missao() {
        let f = FiltrosMissao { idade: FaixaAtributo { min: 16, max: IDADE_JOVEM }, ..FiltrosMissao::default() };
        assert_eq!(tipo_por_filtros(&f), TipoMissao::Jovens);
        let f = FiltrosMissao { idade: FaixaAtributo { min: 16, max: IDADE_JOVEM + 1 }, ..f };
        assert_eq!(tipo_por_filtros(&f), TipoMissao::Geral);
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
        // MEI (posição 18) com passe E defesa altos
        let completo = {
            let mut v = perfil(55, &criativo, 82);
            for a in defesa {
                v[a.indice()] = 80;
            }
            v
        };
        let fit = forca_fit(PosicaoAlvo::Volante, 18, de(&completo)).expect("fit");
        assert!(fit >= LIMIAR_FIT, "{fit}");
        // o mesmo MEI sem defesa não serve de volante
        let so_ataque = perfil(45, &criativo, 82);
        let fit_fraco = forca_fit(PosicaoAlvo::Volante, 18, de(&so_ataque)).expect("fit");
        assert!(fit_fraco < LIMIAR_FIT, "{fit_fraco}");
        assert!(fit > fit_fraco);
        // e um centroavante puro não serve de zagueiro
        let atacante = perfil(40, &[Finalizacao, PosicionamentoOfensivo, ForcaDoChute, Cabeceio, ControleDeBola], 85);
        assert!(forca_fit(PosicaoAlvo::Zagueiro, 25, de(&atacante)).expect("fit") < 70);
    }

    #[test]
    fn trivial_moves_are_excluded_and_every_target_excludes_its_own_position() {
        for alvo in PosicaoAlvo::TODAS {
            assert!(alvo.posicoes_nativas().iter().all(|p| alvo.posicoes_excluidas().contains(p)), "{alvo:?}");
        }
        // lateral esquerdo (7) não vira "fit" de lateral direito nem de zagueiro
        assert!(PosicaoAlvo::LateralDireito.posicoes_excluidas().contains(&7));
        assert!(PosicaoAlvo::Zagueiro.posicoes_excluidas().contains(&3));
        // ponta não é fit de meia aberto; volante continua podendo virar zagueiro
        assert!(PosicaoAlvo::MeiaDireita.posicoes_excluidas().contains(&23));
        assert!(!PosicaoAlvo::Zagueiro.posicoes_excluidas().contains(&10));
        // códigos do FIFA: 3 é lateral, 5 zagueiro, 25 centroavante
        assert_eq!(perfil_da_posicao(3), Perfil::Lateral);
        assert_eq!(perfil_da_posicao(5), Perfil::Zagueiro);
        assert_eq!(perfil_da_posicao(25), Perfil::Centroavante);
        assert_eq!(perfil_da_posicao(27), Perfil::Ponta);
    }

    #[test]
    fn the_overall_change_estimate_is_the_profile_difference() {
        use Atributo::*;
        let criativo = [PasseCurto, PasseLongo, Visao, ControleDeBola, Drible, PosicionamentoOfensivo, Finalizacao, Agilidade];
        let so_ataque = perfil(45, &criativo, 82);
        let perda = variacao_overall(PosicaoAlvo::Volante, 18, de(&so_ataque)).expect("estimativa");
        assert!(perda < -5, "MEI sem defesa perde muito como volante: {perda}");
        let zagueiro_que_arma = perfil(50, &[PasseCurto, Visao, ControleDeBola, PasseLongo], 90);
        let ganho = variacao_overall(PosicaoAlvo::MeioCampista, 5, de(&zagueiro_que_arma)).expect("estimativa");
        assert!(ganho > 0, "{ganho}");
        assert_eq!(variacao_overall(PosicaoAlvo::Zagueiro, 5, de(&zagueiro_que_arma)), Some(0), "mesmo perfil");
    }

    #[test]
    fn fit_is_capped_at_one_hundred_and_ignores_unobserved_attributes() {
        // um jogador melhor no alvo que na própria posição: 100, não 130
        let zagueiro_que_arma = perfil(50, &[Atributo::PasseCurto, Atributo::Visao, Atributo::ControleDeBola, Atributo::PasseLongo], 90);
        assert_eq!(forca_fit(PosicaoAlvo::MeioCampista, 5, de(&zagueiro_que_arma)), Some(100));
        // só os atributos observados contam
        let observado = |a: Atributo| (a == Atributo::Finalizacao).then_some(80.0);
        assert_eq!(nota_no_perfil(Perfil::Centroavante, observado), Some(80.0));
        assert_eq!(nota_no_perfil(Perfil::Zagueiro, observado), None, "nenhum atributo do perfil observado");
        assert_eq!(forca_fit(PosicaoAlvo::Zagueiro, 25, observado), None);
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
        let ordem = ordem_de_observacao(Funcao::MeioCampo, &[], Some(PosicaoAlvo::Volante));
        let topo: Vec<Atributo> = ordem.iter().take(3).copied().collect();
        assert_eq!(topo, [Atributo::PasseCurto, Atributo::Interceptacao, Atributo::DesarmeEmPe]);
        assert_eq!(ordem.len(), 28, "sem atributos de goleiro, sem repetir");
        let com_dominante = ordem_de_observacao(Funcao::MeioCampo, &[Atributo::Drible], Some(PosicaoAlvo::Volante));
        assert_eq!(com_dominante.first(), Some(&Atributo::Drible));
        assert_eq!(relevancia(TipoMissao::Tatica, 70, 80, &[80, 90]), 85, "média dos critérios de perfil");
    }

    #[test]
    fn estimated_value_and_wage_follow_overall_age_and_growth() {
        let v80 = valor_estimado(80, 80, 27, false);
        assert!((15_000_000..30_000_000).contains(&v80), "{v80}");
        assert!(valor_estimado(90, 90, 27, false) > 100_000_000);
        assert!(valor_estimado(60, 60, 27, false) < 1_000_000);
        assert!(valor_estimado(75, 88, 19, false) > valor_estimado(75, 75, 27, false), "jovem com potencial vale mais");
        assert!(valor_estimado(80, 80, 34, false) < valor_estimado(80, 80, 27, false), "veterano vale menos");
        assert!(valor_estimado(80, 80, 27, true) < v80, "goleiro um pouco abaixo");
        assert_eq!(v80 % 100_000, 0, "arredondado");
        // salário: calibrado nos contratos reais do elenco do Felipe
        for (overall, real) in [(70u8, 20_000i64), (80, 120_000), (87, 240_000), (90, 300_000)] {
            let estimado = salario_estimado(overall);
            assert!((estimado - real).abs() * 100 <= real * 15, "{overall}: {estimado} vs {real}");
        }
        assert!(salario_estimado(40) >= 500);
    }

    #[test]
    fn a_player_shows_market_first_then_wage_then_attributes() {
        // 3º de 10: aparece com ~20% do progresso
        assert_eq!(observacao(0.21, 2, 10, false), Observacao::SoMercado);
        assert_eq!(observacao(0.30, 2, 10, false), Observacao::MercadoESalario);
        assert_eq!(observacao(0.40, 2, 10, false), Observacao::Completa);
        assert_eq!(observacao(0.21, 2, 10, true), Observacao::Completa, "concluída: tudo");
        assert_eq!(observacao(1.0, 9, 10, false), Observacao::Completa, "no prazo: tudo");
    }

    #[test]
    fn team_level_compares_with_the_squad_starter_of_that_position() {
        // elenco: centroavantes 67 e 71, zagueiro 75; ninguém de meia
        let elenco = NivelElenco::de(&[(25, 67), (24, 71), (5, 75)]);
        assert_eq!(elenco.titular(Perfil::Centroavante), 71);
        assert_eq!(elenco.titular(Perfil::Zagueiro), 75);
        assert_eq!(elenco.titular(Perfil::MeiaAtacante), elenco.geral, "sem ninguém: média do time");
        assert!(no_nivel(NivelEquipe::MudaPatamar, 74, 74, 71));
        assert!(!no_nivel(NivelEquipe::MudaPatamar, 73, 80, 71));
        assert!(no_nivel(NivelEquipe::Titular, 69, 69, 71) && !no_nivel(NivelEquipe::Titular, 74, 74, 71));
        assert!(no_nivel(NivelEquipe::Banco, 64, 64, 71) && !no_nivel(NivelEquipe::Banco, 69, 69, 71));
        assert!(no_nivel(NivelEquipe::Promessa, 58, 75, 71) && !no_nivel(NivelEquipe::Promessa, 58, 73, 71));
        assert_eq!(NivelEquipe::MudaPatamar.regra(71), "Overall 74 ou mais");
    }

    #[test]
    fn shortcuts_keep_geography_and_spending_cap_and_set_the_team_level() {
        let atuais = FiltrosMissao {
            ligas: vec![13],
            sem_teto: true,
            atributos_dominantes: vec![Atributo::Drible],
            ..FiltrosMissao::default()
        };
        for atalho in Atalho::TODOS {
            let f = atalho.aplicar(&atuais);
            assert_eq!((f.ligas.clone(), f.sem_teto), (vec![13], true), "{atalho:?}");
            assert!(f.atributos_dominantes.is_empty(), "o resto volta ao padrão");
            assert!(f.nivel_elenco.is_some());
            assert!(!atalho.descricao().contains('!'));
        }
        assert_eq!(Atalho::FimDeContrato.aplicar(&atuais).contrato, FaixaAtributo { min: 0, max: 0 });
        assert_eq!(tipo_por_filtros(&Atalho::JovensPromessas.aplicar(&atuais)), TipoMissao::Jovens);
        assert_eq!(tipo_por_filtros(&Atalho::MudaPatamar.aplicar(&atuais)), TipoMissao::Medalhoes);
    }
}
