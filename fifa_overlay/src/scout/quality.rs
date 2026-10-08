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
//! **Qualidade** sai de uma pontuação de 0 a 5: as estrelas inteiras do
//! atributo do Olheiro que vale para o tipo da Missão (no máximo 4; menos
//! a penalidade de mercado/foco, mais ou menos a verba — Épico 5) + 1 se o
//! Modo é Completa, e `0–2 = Baixa, 3 = Média, 4–5 = Alta`. No v1 era
//! `Tier (1–3) + 1 se a Especialização combina + 1 se Completa`; o perfil
//! equivalente (`PerfilOlheiro::v1`) dá os mesmos pontos. A pontuação (não
//! só o nível) decide quantos atributos o Relatório revela e a precisão de
//! base, então mais estrelas sempre melhoram o Relatório.
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
//! **Falsos positivos** (Épico 5, item 7): abaixo da Qualidade Alta, parte
//! do Relatório é de jogadores que quase passam no filtro
//! (`falsos_positivos`).
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
// Olheiros com estrelas (Épico 5, 2026-10-04)
// ---------------------------------------------------------------------
//
// A Especialização deixou de ser uma escolha única: cada Olheiro tem
// quatro atributos de 0 a 5 estrelas, em passos de meia (Caçador de
// Jovens, Caçador de Medalhões, Tático, Generalista), mais a Rede de
// contatos, que decide a velocidade. O **foco** é o atributo mais alto
// (empate: a ordem do PRD) e o **Tier** virou o resumo do foco: até 2,5★
// Júnior, de 3★ a 4★ Experiente, 4,5★ ou mais Elite.
//
// Olheiros contratados antes das estrelas não têm perfil gravado: ganham o
// perfil equivalente ao v1 (`PerfilOlheiro::v1`), com o qual a tabela da
// Story 2.1 dá exatamente os mesmos números.

/// Estrelas em MEIAS estrelas: 0 a 10 (0★ a 5★ em passos de 0,5). No JSON,
/// o número de meias (`7` = 3,5★).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Estrelas(pub u8);

impl Estrelas {
    /// Presa entre 0 e 5★.
    pub fn de_meias(meias: i32) -> Estrelas {
        Estrelas(u8::try_from(meias.clamp(0, 10)).unwrap_or(0))
    }

    pub fn meias(self) -> u8 {
        self.0.min(10)
    }

    /// Estrelas inteiras (arredondado para baixo).
    pub fn inteiras(self) -> u8 {
        self.meias() / 2
    }

    pub fn menos(self, meias: u8) -> Estrelas {
        Estrelas(self.meias().saturating_sub(meias))
    }

    pub fn mais(self, meias: u8) -> Estrelas {
        Estrelas((self.meias() + meias).min(10))
    }

    /// "3,5" / "4".
    pub fn texto(self) -> String {
        let m = self.meias();
        if m.is_multiple_of(2) {
            format!("{}", m / 2)
        } else {
            format!("{},5", m / 2)
        }
    }
}

/// Os atributos de um Olheiro (ver o topo da seção).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PerfilOlheiro {
    pub jovens: Estrelas,
    pub medalhoes: Estrelas,
    pub tatico: Estrelas,
    pub generalista: Estrelas,
    /// Rede de contatos: velocidade de resposta das Missões.
    pub rede: Estrelas,
}

impl PerfilOlheiro {
    /// O perfil de um Olheiro contratado antes das estrelas (Especialização
    /// × Tier do v1): o foco com 2,5 / 3,5 / 4,5★, os outros uma estrela
    /// abaixo e a Rede de contatos que reproduz o fator de prazo do Tier
    /// (120% / 100% / 85%). Com ele, `estimar_missao` dá os números do v1 —
    /// só o Generalista melhora numa Missão Geral, que agora é a dele.
    pub fn v1(especializacao: Especializacao, tier: Tier) -> PerfilOlheiro {
        let (foco, rede) = match tier {
            Tier::Junior => (5, 2),
            Tier::Experiente => (7, 6),
            Tier::Elite => (9, 9),
        };
        let outro = Estrelas(foco - 2);
        let mut perfil = PerfilOlheiro { jovens: outro, medalhoes: outro, tatico: outro, generalista: outro, rede: Estrelas(rede) };
        *perfil.atributo_mut(especializacao) = Estrelas(foco);
        perfil
    }

    pub fn atributo(&self, especializacao: Especializacao) -> Estrelas {
        match especializacao {
            Especializacao::CacadorDeJovens => self.jovens,
            Especializacao::CacadorDeMedalhoes => self.medalhoes,
            Especializacao::Tatico => self.tatico,
            Especializacao::Generalista => self.generalista,
        }
    }

    fn atributo_mut(&mut self, especializacao: Especializacao) -> &mut Estrelas {
        match especializacao {
            Especializacao::CacadorDeJovens => &mut self.jovens,
            Especializacao::CacadorDeMedalhoes => &mut self.medalhoes,
            Especializacao::Tatico => &mut self.tatico,
            Especializacao::Generalista => &mut self.generalista,
        }
    }

    /// O atributo mais alto (empate: a ordem do PRD).
    pub fn foco(&self) -> Especializacao {
        let mut foco = Especializacao::TODAS[0];
        for e in Especializacao::TODAS {
            if self.atributo(e) > self.atributo(foco) {
                foco = e;
            }
        }
        foco
    }

    /// Estrelas do foco.
    pub fn principal(&self) -> Estrelas {
        self.atributo(self.foco())
    }

    /// O Tier é o resumo do foco.
    pub fn tier(&self) -> Tier {
        match self.principal().meias() {
            0..=5 => Tier::Junior,
            6..=8 => Tier::Experiente,
            _ => Tier::Elite,
        }
    }

    /// Estrelas que valem para uma Missão do `tipo`: a Geral usa o
    /// Generalista; as outras, o atributo delas — e o Generalista, uma
    /// estrela abaixo, serve de piso (ele sabe um pouco de tudo).
    pub fn para_tipo(&self, tipo: TipoMissao) -> Estrelas {
        let especifico = match tipo {
            TipoMissao::Jovens => self.jovens,
            TipoMissao::Medalhoes => self.medalhoes,
            TipoMissao::Tatica => self.tatico,
            TipoMissao::Geral => return self.generalista,
        };
        especifico.max(self.generalista.menos(2))
    }
}

// ---------------------------------------------------------------------
// Habilidades dos Olheiros (2026-10-08)
// ---------------------------------------------------------------------
//
// Uma habilidade DESTRAVA algo na busca (um filtro, um dado); a qualidade da
// resposta continua sendo das estrelas e da precisão. Cada uma é liga/desliga
// (os Atributos Dominantes têm dois tetos) e nasce no mercado ligada a um
// atributo do Olheiro: o Fit só em Tático de 4,5★ ou mais, a Referência e os
// Atributos Dominantes em Tático de 3★ ou mais, os Contratos em Caçador de
// Medalhões de 3★ ou mais, as Promessas em Caçador de Jovens de 3★ ou mais e o
// Perfil Físico em qualquer um. Júnior tem 0 ou 1, Experiente até 2, Elite até
// 3. Olheiros de antes não têm a lista gravada e valem como se tivessem todas.

/// O que um Olheiro sabe fazer. No JSON: `"fit_posicional"` etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Habilidade {
    /// Busca também quem joga em outra posição mas tem fit para as
    /// posições pedidas.
    FitPosicional,
    /// "Parecidos com X": o filtro do Jogador de Referência.
    JogadorDeReferencia,
    /// "O melhor driblador, o melhor passador": os atributos dominantes.
    AtributosDominantes,
    /// O contrato exato dos jogadores e o filtro de contrato.
    OlhoParaContratos,
    /// Ritmos de trabalho, pé preferido e dribles.
    PerfilFisico,
    /// O filtro de Potencial e o Potencial com a margem de erro pela metade.
    CacaAPromessas,
}

impl Habilidade {
    pub const TODAS: [Habilidade; 6] = [
        Habilidade::FitPosicional,
        Habilidade::JogadorDeReferencia,
        Habilidade::AtributosDominantes,
        Habilidade::OlhoParaContratos,
        Habilidade::PerfilFisico,
        Habilidade::CacaAPromessas,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Habilidade::FitPosicional => "Fit Posicional",
            Habilidade::JogadorDeReferencia => "Jogador de Referência",
            Habilidade::AtributosDominantes => "Atributos Dominantes",
            Habilidade::OlhoParaContratos => "Olho para Contratos",
            Habilidade::PerfilFisico => "Perfil Físico",
            Habilidade::CacaAPromessas => "Caça a Promessas",
        }
    }

    /// O texto curto do selo no card.
    pub fn selo(self) -> &'static str {
        match self {
            Habilidade::FitPosicional => "FIT",
            Habilidade::JogadorDeReferencia => "REFERÊNCIA",
            Habilidade::AtributosDominantes => "DOMINANTES",
            Habilidade::OlhoParaContratos => "CONTRATOS",
            Habilidade::PerfilFisico => "FÍSICO",
            Habilidade::CacaAPromessas => "PROMESSAS",
        }
    }

    /// O que ela destrava.
    pub fn descricao(self) -> &'static str {
        match self {
            Habilidade::FitPosicional => "Busca, nas posições pedidas, também jogadores de outras posições que têm fit para elas.",
            Habilidade::JogadorDeReferencia => "Aceita o filtro \"parecidos com\" um jogador de referência.",
            Habilidade::AtributosDominantes => "Aceita pedir os atributos em que o jogador é dos melhores (até 2; até 3 com Tático de 4,5★).",
            Habilidade::OlhoParaContratos => "Descobre o contrato exato dos jogadores e aceita o filtro de contrato.",
            Habilidade::PerfilFisico => "Aceita filtrar por ritmo de ataque e defesa, pé preferido e dribles.",
            Habilidade::CacaAPromessas => "Aceita o filtro de Potencial e lê o Potencial com a margem de erro pela metade.",
        }
    }

    /// Quem pode tê-la (o que o mercado exige do perfil dele).
    pub fn exige(self) -> &'static str {
        match self {
            Habilidade::FitPosicional => "Tático de 4,5★ ou mais",
            Habilidade::JogadorDeReferencia | Habilidade::AtributosDominantes => "Tático de 3★ ou mais",
            Habilidade::OlhoParaContratos => "Caçador de Medalhões de 3★ ou mais",
            Habilidade::CacaAPromessas => "Caçador de Jovens de 3★ ou mais",
            Habilidade::PerfilFisico => "qualquer Olheiro",
        }
    }

    /// O perfil dele permite ter esta habilidade?
    pub fn elegivel(self, perfil: &PerfilOlheiro) -> bool {
        let minimo = |e: Estrelas, meias: u8| e.meias() >= meias;
        match self {
            Habilidade::FitPosicional => minimo(perfil.tatico, 9),
            Habilidade::JogadorDeReferencia | Habilidade::AtributosDominantes => minimo(perfil.tatico, 6),
            Habilidade::OlhoParaContratos => minimo(perfil.medalhoes, 6),
            Habilidade::CacaAPromessas => minimo(perfil.jovens, 6),
            Habilidade::PerfilFisico => true,
        }
    }

    /// Chance (%) de um Olheiro elegível ter a habilidade.
    fn chance(self) -> u32 {
        match self {
            Habilidade::FitPosicional => 60,
            Habilidade::JogadorDeReferencia => 50,
            Habilidade::AtributosDominantes | Habilidade::OlhoParaContratos | Habilidade::CacaAPromessas => 55,
            Habilidade::PerfilFisico => 45,
        }
    }

    /// Quanto ela encarece a contratação (%): o Perfil Físico, que destrava
    /// só filtros simples, pouco; as outras, mais.
    fn acrescimo_no_preco(self) -> i64 {
        match self {
            Habilidade::PerfilFisico => 5,
            _ => 12,
        }
    }
}

/// Quantas habilidades um Olheiro do `tier` pode ter: Júnior 1, Experiente 2,
/// Elite 3.
pub fn maximo_de_habilidades(tier: Tier) -> usize {
    match tier {
        Tier::Junior => 1,
        Tier::Experiente => 2,
        Tier::Elite => 3,
    }
}

/// Quantos atributos dominantes o Olheiro com a habilidade deixa pedir: 2, ou
/// 3 com Tático de 4,5★ ou mais.
pub fn maximo_de_dominantes(perfil: &PerfilOlheiro) -> usize {
    if perfil.tatico.meias() >= 9 {
        MAX_DOMINANTES
    } else {
        MAX_DOMINANTES - 1
    }
}

/// Sorteia as habilidades de um Olheiro do mercado (determinístico pela
/// semente da oferta): cada habilidade que o perfil permite sai pela chance
/// dela; passando do teto do Tier, ficam as primeiras de um embaralhamento
/// da própria semente.
pub fn sortear_habilidades(perfil: &PerfilOlheiro, tier: Tier, semente: u64) -> Vec<Habilidade> {
    let mut sorteadas: Vec<(u64, Habilidade)> = Habilidade::TODAS
        .iter()
        .zip(40u32..)
        .filter(|(h, _)| h.elegivel(perfil))
        .filter(|(h, canal)| sortear(semente, *canal, 100) < h.chance())
        .map(|(h, canal)| (semente_de(semente, canal + 100), *h))
        .collect();
    sorteadas.sort_by_key(|(ordem, _)| *ordem);
    sorteadas.truncate(maximo_de_habilidades(tier));
    let mut habilidades: Vec<Habilidade> = sorteadas.into_iter().map(|(_, h)| h).collect();
    habilidades.sort();
    habilidades
}

/// O preço da contratação com as habilidades: cada uma soma o acréscimo dela
/// (12%, ou 5% no Perfil Físico) ao preço de base, arredondado a 10 mil.
pub fn custo_contratacao_com(perfil: &PerfilOlheiro, continente: Option<Confederacao>, mercados: usize, habilidades: &[Habilidade]) -> i32 {
    let base = i64::from(custo_contratacao(perfil, continente, mercados));
    let acrescimo: i64 = habilidades.iter().map(|h| h.acrescimo_no_preco()).sum();
    let total = base * (100 + acrescimo) / 100;
    i32::try_from((total + 5_000) / 10_000 * 10_000).unwrap_or(i32::MAX).max(100_000)
}

/// Um mercado que o Olheiro conhece bem (Épico 5): um país ou um
/// continente inteiro. No JSON: `{"pais": {"id": 54, "continente":
/// "america_do_sul"}}` ou `{"continente": "europa"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mercado {
    Pais { id: u16, continente: Confederacao },
    Continente(Confederacao),
}

impl Mercado {
    pub fn continente(self) -> Confederacao {
        match self {
            Mercado::Pais { continente, .. } | Mercado::Continente(continente) => continente,
        }
    }
}

/// Um lugar onde uma Missão procura (derivado do filtro geográfico).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Regiao {
    Pais { id: u16, continente: Confederacao },
    Continente(Confederacao),
    /// Sem filtro geográfico: o mundo todo.
    Mundo,
}

/// Distância entre os mercados do Olheiro e um lugar (Questão em aberto 1,
/// decidida em 2026-10-04): 0 = dentro do mercado dele; 1 = mesmo
/// continente (ou uma busca no mundo todo); 2 = outro continente. Sem
/// mercado gravado (Olheiro de antes das estrelas) é sempre 0: ele segue
/// como era.
pub fn distancia_mercado(mercados: &[Mercado], regiao: Regiao) -> u8 {
    if mercados.is_empty() {
        return 0;
    }
    let conhece_continente = |c: Confederacao| mercados.contains(&Mercado::Continente(c));
    let tem_algo_no = |c: Confederacao| mercados.iter().any(|m| m.continente() == c);
    match regiao {
        Regiao::Pais { id, continente } => {
            let conhece_pais = mercados.iter().any(|m| matches!(m, Mercado::Pais { id: p, .. } if *p == id));
            if conhece_pais || conhece_continente(continente) {
                0
            } else if tem_algo_no(continente) {
                1
            } else {
                2
            }
        }
        Regiao::Continente(c) if conhece_continente(c) => 0,
        Regiao::Continente(c) if tem_algo_no(c) => 1,
        Regiao::Continente(_) => 2,
        Regiao::Mundo => 1,
    }
}

/// Penalidade de uma Missão, em MEIAS estrelas: o que sai do atributo que
/// vale para ela (Qualidade) e da Rede de contatos (velocidade).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Penalidade {
    pub qualidade: u8,
    pub velocidade: u8,
}

/// Dias de carreira trabalhando num mercado novo até a penalidade sumir
/// (Felipe: "cerca de 6 meses").
pub const DIAS_ADAPTACAO_MERCADO: u32 = 180;

/// Penalidade de mercado sem adaptação, por distância (Felipe: velocidade
/// cai até 2★ e qualidade até 1★, em passos de 0,5).
fn penalidade_por_distancia(distancia: u8) -> Penalidade {
    match distancia {
        0 => Penalidade::default(),
        1 => Penalidade { qualidade: 1, velocidade: 2 },
        _ => Penalidade { qualidade: 2, velocidade: 4 },
    }
}

/// `meias × (1 − dias/total)`, arredondado para a meia estrela mais próxima.
fn com_adaptacao(meias: u8, dias: u32, total: u32) -> u8 {
    let total = total.max(1);
    let faltam = total.saturating_sub(dias.min(total));
    u8::try_from((u32::from(meias) * faltam * 2 + total) / (total * 2)).unwrap_or(meias)
}

/// Um trabalho já feito pelo Olheiro (uma Missão dele até hoje): é a
/// experiência que adapta (Questão 2: por dias de carreira trabalhados).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrabalhoPassado {
    pub tipo: TipoMissao,
    pub regioes: Vec<Regiao>,
    pub dias: u32,
}

/// Dias trabalhados num lugar: num país contam os trabalhos nele e no
/// continente dele inteiro; num continente, os do continente; no mundo, as
/// buscas sem filtro geográfico.
pub fn dias_na_regiao(trabalhos: &[TrabalhoPassado], regiao: Regiao) -> u32 {
    trabalhos
        .iter()
        .filter(|t| {
            t.regioes.iter().any(|&r| match (regiao, r) {
                (Regiao::Pais { id, .. }, Regiao::Pais { id: outro, .. }) => id == outro,
                (Regiao::Pais { continente, .. }, Regiao::Continente(c)) => continente == c,
                (Regiao::Continente(a), Regiao::Continente(b)) => a == b,
                (Regiao::Mundo, Regiao::Mundo) => true,
                _ => false,
            })
        })
        .map(|t| t.dias)
        .sum()
}

/// Dias trabalhados em Missões do `tipo`.
pub fn dias_no_tipo(trabalhos: &[TrabalhoPassado], tipo: TipoMissao) -> u32 {
    trabalhos.iter().filter(|t| t.tipo == tipo).map(|t| t.dias).sum()
}

/// Penalidade de fora do foco (item 5): pôr um especialista numa Missão de
/// outro tipo tira 1★ da Qualidade até ele se habituar. A Missão Geral e o
/// Generalista (que é de tudo um pouco) nunca têm essa penalidade.
const PENALIDADE_ESCOPO: u8 = 2;

/// Dias para se habituar a outro tipo de Missão: mais experiente, mais
/// rápido (Felipe: de 6 meses a 1 ano).
pub fn dias_readaptacao(tier: Tier) -> u32 {
    match tier {
        Tier::Elite => 180,
        Tier::Experiente => 270,
        Tier::Junior => 365,
    }
}

/// Penalidade de uma Missão para um Olheiro: a pior região do filtro
/// geográfico (mercado) mais a de fora do foco, cada uma já descontada a
/// adaptação dos trabalhos anteriores.
pub fn penalidade_missao(
    perfil: &PerfilOlheiro,
    mercados: &[Mercado],
    regioes: &[Regiao],
    tipo: TipoMissao,
    trabalhos: &[TrabalhoPassado],
) -> Penalidade {
    let mut pior = Penalidade::default();
    let regioes: &[Regiao] = if regioes.is_empty() { &[Regiao::Mundo] } else { regioes };
    for &regiao in regioes {
        let base = penalidade_por_distancia(distancia_mercado(mercados, regiao));
        let dias = dias_na_regiao(trabalhos, regiao);
        pior.qualidade = pior.qualidade.max(com_adaptacao(base.qualidade, dias, DIAS_ADAPTACAO_MERCADO));
        pior.velocidade = pior.velocidade.max(com_adaptacao(base.velocidade, dias, DIAS_ADAPTACAO_MERCADO));
    }
    let foco = perfil.foco();
    let fora_do_foco = tipo != TipoMissao::Geral && foco != Especializacao::Generalista && !aderente(foco, tipo);
    if fora_do_foco {
        let dias = dias_no_tipo(trabalhos, tipo);
        pior.qualidade += com_adaptacao(PENALIDADE_ESCOPO, dias, dias_readaptacao(perfil.tier()));
    }
    pior
}

/// Verba da viagem de uma Missão (item 6): o jogador escolhe quanto investe
/// em viagem e hospedagem. A faixa cresce com a escala da busca (o custo
/// de base já cresce com a amplitude e o Modo). No JSON: `"economica"`,
/// `"padrao"`, `"reforcada"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Investimento {
    /// 60% do custo: menos nomes, faixas mais largas, um pouco mais lenta.
    Economica,
    #[default]
    Padrao,
    /// 160% do custo: mais nomes, meia estrela a mais, um pouco mais rápida.
    Reforcada,
}

impl Investimento {
    pub const TODOS: [Investimento; 3] = [Investimento::Economica, Investimento::Padrao, Investimento::Reforcada];

    pub fn nome(self) -> &'static str {
        match self {
            Investimento::Economica => "Econômica",
            Investimento::Padrao => "Padrão",
            Investimento::Reforcada => "Reforçada",
        }
    }

    /// (custo %, duração %, jogadores %).
    fn fatores(self) -> (i64, u32, u32) {
        match self {
            Investimento::Economica => (60, 110, 75),
            Investimento::Padrao => (100, 100, 100),
            Investimento::Reforcada => (160, 90, 125),
        }
    }
}

// ---------------------------------------------------------------------
// Mercado de Olheiros (Épico 5, item 3)
// ---------------------------------------------------------------------

/// O que o save diz do clube do técnico para a oferta de Olheiros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfilClube {
    /// `teams.domesticprestige` e `internationalprestige`, 0–20.
    pub prestigio_nacional: u8,
    pub prestigio_internacional: u8,
    /// Divisão da liga do clube (1 = primeira).
    pub nivel_liga: u8,
    /// Troféus da carreira (`career_trophies`, um bit por troféu).
    pub titulos: u16,
    /// País do clube (para os Olheiros locais).
    pub pais: Option<u16>,
    pub continente: Confederacao,
}

/// Atratividade do clube para Olheiros, 0–100: prestígio (internacional
/// pesa mais), menos para divisões de baixo, mais com títulos.
pub fn atratividade(clube: &PerfilClube) -> u8 {
    let prestigio = i32::from(clube.prestigio_nacional.min(20)) * 2 + i32::from(clube.prestigio_internacional.min(20)) * 3;
    let divisao = match clube.nivel_liga {
        0 | 1 => 0,
        2 => -10,
        3 => -20,
        _ => -30,
    };
    let titulos = i32::from(clube.titulos.min(15));
    u8::try_from((prestigio + divisao + titulos).clamp(0, 100)).unwrap_or(50)
}

/// "Alta", "Média", "Baixa" — como a tela fala da atratividade.
pub fn nome_atratividade(valor: u8) -> &'static str {
    match valor {
        70..=100 => "Alta",
        40..=69 => "Média",
        _ => "Baixa",
    }
}

/// Período do mercado de Olheiros: a oferta muda toda semana (era todo mês;
/// o Felipe não quer esperar meses de carreira por um Olheiro de outro
/// continente, 2026-10-05). `dia` é `Date::day_number` da data da carreira;
/// o período é a semana (7 dias) desde 1970.
pub fn periodo_do_mercado(dia: i64) -> u32 {
    u32::try_from(dia.div_euclid(7)).unwrap_or(0)
}

/// Primeiro dia do período seguinte (`Date::from_day_number`).
pub fn inicio_do_periodo(periodo: u32) -> i64 {
    i64::from(periodo) * 7
}

/// Um país de onde pode vir um Olheiro (países com clubes no save).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaisCandidato {
    pub id: u16,
    pub continente: Confederacao,
    /// Relevância das ligas do país (`peso_da_liga`, somadas): decide
    /// quantos Olheiros o continente tem e de qual país eles saem.
    pub peso: u32,
}

/// Quanto uma liga conta para o mercado: as primeiras divisões de liga cheia
/// (16 clubes ou mais) valem mais; divisões de baixo e ligas pequenas, pouco.
pub fn peso_da_liga(nivel: u8, clubes: u16) -> u32 {
    match (nivel, clubes) {
        (0 | 1, c) if c >= 16 => 8,
        (0 | 1, _) => 5,
        (2, _) => 3,
        _ => 1,
    }
}

/// Olheiros por continente no mercado da semana: o mínimo e o máximo.
pub const OFERTAS_CONTINENTE_MIN: u32 = 3;
pub const OFERTAS_CONTINENTE_MAX: u32 = 10;

/// Quantos Olheiros cada continente (com países no mercado) tem: de 3 a 10,
/// na proporção da relevância das ligas dele frente ao continente mais
/// forte. Quem tem poucas ligas, e pequenas, fica nos 3; a Europa fica nos
/// 10. Na ordem de `Confederacao::TODAS`.
pub fn ofertas_por_continente(paises: &[PaisCandidato]) -> Vec<(Confederacao, u32)> {
    let pesos: Vec<(Confederacao, u32)> = Confederacao::TODAS
        .iter()
        .filter_map(|&c| {
            let do_continente: Vec<&PaisCandidato> = paises.iter().filter(|p| p.continente == c).collect();
            (!do_continente.is_empty()).then(|| (c, do_continente.iter().map(|p| p.peso.max(1)).sum::<u32>()))
        })
        .collect();
    let maior = pesos.iter().map(|(_, p)| *p).max().unwrap_or(1).max(1);
    let faixa = OFERTAS_CONTINENTE_MAX - OFERTAS_CONTINENTE_MIN;
    pesos.into_iter().map(|(c, p)| (c, OFERTAS_CONTINENTE_MIN + (faixa * p + maior / 2) / maior)).collect()
}

/// Um Olheiro gerado para o mercado (sem nome: `scout::nomes` dá um pela
/// nação, com a mesma semente).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatoOlheiro {
    /// Índice dentro do período (a identidade da oferta).
    pub indice: u32,
    pub semente: u64,
    pub perfil: PerfilOlheiro,
    pub pais: Option<u16>,
    pub mercados: Vec<Mercado>,
    /// O que ele sabe fazer (`sortear_habilidades`).
    pub habilidades: Vec<Habilidade>,
}

/// Parte das ofertas que vêm com perfil médio (equilibrado): os quatro
/// atributos a até 1★ um do outro, sem um extremo (Felipe, 2026-10-05: os
/// Olheiros estavam todos nos extremos). O resto segue especialista.
const PCT_EQUILIBRADOS: u32 = 40;

/// Sorteio determinístico em `0..n`.
fn sortear(semente: u64, canal: u32, n: u32) -> u32 {
    u32::try_from(semente_de(semente, canal) % u64::from(n.max(1))).unwrap_or(0)
}

fn semente_de(base: u64, canal: u32) -> u64 {
    semente(u128::from(base), canal, 77)
}

/// Os Olheiros à venda num período (item 3), de todos os continentes.
/// Determinístico pela carreira e pelo período. Cada continente com países
/// no mercado tem de 3 a 10 Olheiros (`ofertas_por_continente`: mais para
/// quem tem mais ligas, e mais relevantes), cada um com a nação num país do
/// continente; assim um clube acha rápido um Olheiro da Ásia, por exemplo.
/// A raridade vem da atratividade: num clube grande, cerca de 1 em 4
/// ofertas é Elite; num clube pequeno, quase todas são Júnior. Dois em cada
/// cinco vêm com perfil médio (`PCT_EQUILIBRADOS`). Dentro do continente, a
/// nação pesa pela relevância das ligas (`PaisCandidato::peso`), e o país do
/// clube pesa mais.
pub fn gerar_ofertas(atratividade: u8, periodo: u32, carreira: u64, clube: &PerfilClube, paises: &[PaisCandidato]) -> Vec<CandidatoOlheiro> {
    let mut indice = 0u32;
    let mut ofertas = Vec::new();
    for (continente, quantos) in ofertas_por_continente(paises) {
        let do_continente: Vec<PaisCandidato> = paises.iter().filter(|p| p.continente == continente).cloned().collect();
        for _ in 0..quantos {
            ofertas.push(gerar_um(indice, atratividade, periodo, carreira, clube, paises, &do_continente));
            indice += 1;
        }
    }
    ofertas
}

/// Um Olheiro do mercado: `do_continente` são os países de onde ele pode vir.
fn gerar_um(
    indice: u32,
    atratividade: u8,
    periodo: u32,
    carreira: u64,
    clube: &PerfilClube,
    paises: &[PaisCandidato],
    do_continente: &[PaisCandidato],
) -> CandidatoOlheiro {
    let atr = u32::from(atratividade.min(100));
    let s = semente_de(carreira ^ (u64::from(periodo) << 32), indice);
    // raridade: Elite < atr/4 %, Experiente até +20% +0,3×atr
    let rolagem = sortear(s, 1, 100);
    let limite_elite = atr / 4;
    let limite_experiente = limite_elite + 20 + atr * 3 / 10;
    let (foco_meias, rede) = if rolagem < limite_elite {
        let cinco = atr >= 70 && sortear(s, 2, 4) == 0;
        (if cinco { 10 } else { 9 }, 6 + sortear(s, 3, 5))
    } else if rolagem < limite_experiente {
        (6 + sortear(s, 2, 3), 4 + sortear(s, 3, 4))
    } else {
        (3 + sortear(s, 2, 3), 1 + sortear(s, 3, 4))
    };
    let foco = Especializacao::TODAS[usize::try_from(sortear(s, 4, 4)).unwrap_or(0)];
    let generalista = foco == Especializacao::Generalista;
    let equilibrado = sortear(s, 5, 100) < PCT_EQUILIBRADOS;
    // o perfil médio também tem a Rede perto do resto, não só no nível da raridade
    let rede = if equilibrado { (foco_meias.saturating_sub(1) + sortear(s, 6, 3)).clamp(1, 10) } else { rede };
    let mut perfil = PerfilOlheiro {
        jovens: Estrelas(0),
        medalhoes: Estrelas(0),
        tatico: Estrelas(0),
        generalista: Estrelas(0),
        rede: Estrelas::de_meias(i32::try_from(rede).unwrap_or(2)),
    };
    for (canal, e) in (10u32..).zip(Especializacao::TODAS) {
        // o equilibrado fica a até 1★ do foco; o Generalista também é
        // chegado ao foco; o especialista despenca fora dele
        let queda = if equilibrado {
            sortear(s, canal, 3)
        } else if generalista {
            1 + sortear(s, canal, 3)
        } else {
            2 + sortear(s, canal, 4)
        };
        let meias = if e == foco { foco_meias } else { foco_meias.saturating_sub(queda).max(1) };
        *perfil.atributo_mut(e) = Estrelas::de_meias(i32::try_from(meias).unwrap_or(1));
    }
    let pais = escolher_pais(s, clube, do_continente);
    let mut mercados: Vec<Mercado> = pais.iter().map(|p| Mercado::Pais { id: p.id, continente: p.continente }).collect();
    let tier = perfil.tier();
    let conhece_continente = match tier {
        Tier::Elite => true,
        Tier::Experiente => sortear(s, 20, 2) == 0,
        Tier::Junior => false,
    };
    if let (true, Some(p)) = (conhece_continente, &pais) {
        mercados.push(Mercado::Continente(p.continente));
    }
    if tier != Tier::Junior && sortear(s, 21, 10) < 3 {
        if let Some(outro) = escolher_pais(semente_de(s, 22), clube, paises).filter(|o| Some(o.id) != pais.as_ref().map(|p| p.id)) {
            mercados.push(Mercado::Pais { id: outro.id, continente: outro.continente });
        }
    }
    let habilidades = sortear_habilidades(&perfil, tier, s);
    CandidatoOlheiro { indice, semente: s, perfil, pais: pais.map(|p| p.id), mercados, habilidades }
}

/// Sorteia um país pelo peso (relevância das ligas); o país do clube pesa 4×.
fn escolher_pais(s: u64, clube: &PerfilClube, paises: &[PaisCandidato]) -> Option<PaisCandidato> {
    let peso = |p: &PaisCandidato| -> u32 {
        let base = p.peso.max(1);
        if Some(p.id) == clube.pais {
            base * 4
        } else {
            base
        }
    };
    let total: u32 = paises.iter().map(peso).sum();
    if total == 0 {
        return None;
    }
    let mut alvo = sortear(s, 30, total);
    for p in paises {
        let w = peso(p);
        if alvo < w {
            return Some(p.clone());
        }
        alvo -= w;
    }
    paises.last().cloned()
}

/// Custo de contratação (mesma unidade do `transferbudget`), a calibrar
/// jogando: o foco manda (≈ 0,4 M com 2,5★, 1,4 M com 3,5★, 4,6 M com
/// 4,5★, ×3,4 por estrela), a Rede de contatos e os outros atributos somam
/// (+8% e +4% por estrela acima do meio), cada mercado a mais +10%, e a
/// nacionalidade mexe no preço (Felipe, item 1): europeus +10%,
/// sul-americanos +5%, os demais −5%. Arredondado a 10 mil.
pub fn custo_contratacao(perfil: &PerfilOlheiro, continente: Option<Confederacao>, mercados: usize) -> i32 {
    let estrelas = |e: Estrelas| f64::from(e.meias()) / 2.0;
    let foco = perfil.foco();
    let base = 400_000.0 * 3.4f64.powf(estrelas(perfil.principal()) - 2.5);
    let rede = 1.0 + 0.08 * (estrelas(perfil.rede) - 2.5);
    let outros: f64 = Especializacao::TODAS.iter().filter(|&&e| e != foco).map(|&e| estrelas(perfil.atributo(e))).sum();
    let amplitude = 1.0 + 0.04 * (outros - 4.5);
    let extras = 1.0 + 0.1 * mercados.saturating_sub(1) as f64;
    let nacao = match continente {
        Some(Confederacao::Europa) => 1.1,
        Some(Confederacao::AmericaDoSul) => 1.05,
        _ => 0.95,
    };
    let custo = (base * rede.max(0.6) * amplitude.max(0.6) * extras * nacao / 10_000.0).round() * 10_000.0;
    (custo as i32).max(100_000)
}

// ---------------------------------------------------------------------
// Lista de Escolhidos (Épico 6, 2026-10-04)
// ---------------------------------------------------------------------
//
// Pedido do Felipe: os jogadores escolhidos ficam com os stats por até 1
// ano; depois de 6 meses a precisão começa a cair, e com 1 ano e meio é
// preciso uma análise nova. Generalistas designados para o acompanhamento
// mantêm X jogadores atualizados cada (pelo nível) e, enquanto acompanham,
// as faixas fecham até o valor exato num tempo que depende de quão
// especificado o jogador chegou à lista.

/// Até aqui (6 meses) a observação vale como foi feita.
pub const DIAS_FRESCO: u32 = 180;
/// A partir daqui (1 ano) o jogador aparece como desatualizado.
pub const DIAS_DESATUALIZADO: u32 = 365;
/// Com 1 ano e meio a observação vence: os atributos somem.
pub const DIAS_VENCIDO: u32 = 540;
/// Quanto as faixas alargam (± pontos) até vencer.
pub const PERDA_MAXIMA: u8 = 8;

/// Como está a observação de um jogador da lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frescor {
    /// Até 6 meses: como observado.
    Atualizado,
    /// De 6 meses a 1 ano: faixas `extra` pontos mais largas.
    Envelhecendo { extra: u8 },
    /// De 1 ano a 1 ano e meio: ainda aparece, marcado.
    Desatualizado { extra: u8 },
    /// 1 ano e meio ou mais: precisa de uma análise nova.
    Vencido,
}

impl Frescor {
    pub fn extra(self) -> u8 {
        match self {
            Frescor::Envelhecendo { extra } | Frescor::Desatualizado { extra } => extra,
            _ => 0,
        }
    }
}

/// Frescor de uma observação feita há `dias` dias de carreira: de 6 meses a
/// 1 ano e meio as faixas alargam em linha reta até `PERDA_MAXIMA`.
pub fn frescor(dias: u32) -> Frescor {
    if dias >= DIAS_VENCIDO {
        return Frescor::Vencido;
    }
    if dias <= DIAS_FRESCO {
        return Frescor::Atualizado;
    }
    let passados = dias - DIAS_FRESCO;
    let janela = DIAS_VENCIDO - DIAS_FRESCO;
    let extra = u8::try_from((u32::from(PERDA_MAXIMA) * passados).div_ceil(janela)).unwrap_or(PERDA_MAXIMA);
    if dias >= DIAS_DESATUALIZADO {
        Frescor::Desatualizado { extra }
    } else {
        Frescor::Envelhecendo { extra }
    }
}

/// Jogadores que um Generalista mantém atualizados: duas por estrela de
/// Generalista (2,5★ → 5; 4,5★ → 9; 5★ → 10).
pub fn capacidade_acompanhamento(generalista: Estrelas) -> usize {
    usize::from(generalista.meias())
}

/// Dias de acompanhamento até o valor exato: 1 por ponto de precisão e 1
/// por dois atributos ainda não observados, no mínimo 5. Um jogador de
/// Relatório de Qualidade Alta (±1, tudo observado) fica exato em 5 dias;
/// um de Qualidade Baixa (±14, 6 de 28 atributos), em 25 (eram 10 e 2 por
/// ponto, ~6 meses; 4 e 1, 78 dias: ainda longo para o Felipe, 2026-10-05).
pub fn dias_para_exato(precisao: u8, observados: usize, total: usize) -> u32 {
    let faltam = u32::try_from(total.saturating_sub(observados)).unwrap_or(0);
    (u32::from(precisao) + faltam.div_ceil(2)).max(5)
}

/// Precisão (±) depois de `dias` de acompanhamento, de `inicial` até 0.
pub fn precisao_acompanhada(inicial: u8, dias: u32, total: u32) -> u8 {
    if dias >= total {
        return 0;
    }
    let restante = u32::from(inicial) * (total - dias);
    u8::try_from(restante.div_ceil(total.max(1))).unwrap_or(inicial)
}

/// Atributos observados depois de `dias` de acompanhamento.
pub fn atributos_acompanhados(iniciais: usize, total_atributos: usize, dias: u32, total: u32) -> usize {
    if dias >= total {
        return total_atributos;
    }
    let faltam = total_atributos.saturating_sub(iniciais);
    iniciais + faltam * dias as usize / total.max(1) as usize
}

/// Precisão de partida de uma análise refeita do zero (observação vencida).
pub const PRECISAO_ANALISE_NOVA: u8 = 14;
/// Atributos de partida de uma análise refeita do zero.
pub const ATRIBUTOS_ANALISE_NOVA: usize = 6;

// ---------------------------------------------------------------------
// Falsos positivos (item 7)
// ---------------------------------------------------------------------

/// Parte do Relatório que vem de "quase": jogadores que o Olheiro achou que
/// passavam no filtro, mas não passam (Felipe: Qualidade também é
/// "a quantidade de falsos positivos"). Baixa: 1 em 4; Média: 1 em 10;
/// Alta: nenhum. Eles não vêm marcados; ficam visíveis quando o
/// acompanhamento da Lista de Escolhidos chega ao valor exato.
pub fn falsos_positivos(qualidade: Qualidade, quantos: usize) -> usize {
    let pct = match qualidade {
        Qualidade::Baixa => 25,
        Qualidade::Media => 10,
        Qualidade::Alta => 0,
    };
    quantos * pct / 100
}

/// Dias de carreira que a curadoria da Base do Scout leva para entregar um
/// jogador a uma Missão nova (2026-10-08), pelo detalhe que o clube já tem
/// dele: com os 28 atributos mapeados, na hora; com quase nada, 4 dias. Os
/// jogadores da Base não ocupam o limite do Olheiro (`Relatorio::da_base`).
pub fn dias_de_curadoria(atributos_mapeados: usize) -> u8 {
    match atributos_mapeados {
        28.. => 0,
        20..=27 => 1,
        12..=19 => 2,
        6..=11 => 3,
        _ => 4,
    }
}

/// Folgas do filtro "quase" dos falsos positivos.
pub const FOLGA_OVERALL: u8 = 4;
pub const FOLGA_IDADE: u8 = 1;
/// Tetos de valor e salário com 25% a mais.
pub const FOLGA_TETO_PCT: i64 = 125;

/// O jogador está "quase" no nível pedido (3 pontos de folga na régua do
/// elenco)?
pub fn quase_no_nivel(nivel: NivelEquipe, overall: u8, potencial: u8, titular: u8) -> bool {
    no_nivel(nivel, overall.saturating_add(3), potencial.saturating_add(3), titular)
        || match nivel {
            NivelEquipe::Titular | NivelEquipe::Banco => no_nivel(nivel, overall.saturating_sub(3), potencial, titular),
            _ => false,
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
    /// Os atributos do Olheiro (Épico 5).
    pub perfil: PerfilOlheiro,
    pub modo: ModoBusca,
    pub tipo: TipoMissao,
    pub amplitude: AmplitudeGeografica,
    /// Mercado e fora do foco, já com a adaptação (`penalidade_missao`).
    pub penalidade: Penalidade,
    pub investimento: Investimento,
}

#[cfg(test)]
impl PedidoMissao {
    /// Pedido de um Olheiro do v1 (Especialização × Tier), sem penalidade e
    /// com a verba Padrão.
    pub fn v1(tier: Tier, especializacao: Especializacao, modo: ModoBusca, tipo: TipoMissao, amplitude: AmplitudeGeografica) -> Self {
        PedidoMissao {
            perfil: PerfilOlheiro::v1(especializacao, tier),
            modo,
            tipo,
            amplitude,
            penalidade: Penalidade::default(),
            investimento: Investimento::Padrao,
        }
    }
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

/// Bônus de Qualidade do Modo Completa.
const BONUS_COMPLETA: u8 = 1;

/// O foco do Olheiro é o do tipo da Missão? (O Generalista é o da Geral.)
fn aderente(especializacao: Especializacao, tipo: TipoMissao) -> bool {
    matches!(
        (especializacao, tipo),
        (Especializacao::CacadorDeJovens, TipoMissao::Jovens)
            | (Especializacao::CacadorDeMedalhoes, TipoMissao::Medalhoes)
            | (Especializacao::Tatico, TipoMissao::Tatica)
            | (Especializacao::Generalista, TipoMissao::Geral)
    )
}

/// Estrelas que valem para a Missão, já com a penalidade e a verba.
pub fn estrelas_efetivas(pedido: &PedidoMissao) -> Estrelas {
    let base = pedido.perfil.para_tipo(pedido.tipo).menos(pedido.penalidade.qualidade);
    match pedido.investimento {
        Investimento::Economica => base.menos(1),
        Investimento::Padrao => base,
        Investimento::Reforcada => base.mais(1),
    }
}

/// Pontuação de 0 a 5 (ver o topo do módulo): as estrelas inteiras que
/// valem para a Missão (no máximo 4, como Elite + aderência no v1) + 1 se
/// o Modo é Completa. No perfil do v1, 2,5★ = 2 pontos = Júnior aderente.
fn pontuacao(pedido: &PedidoMissao) -> u8 {
    let modo = if pedido.modo == ModoBusca::Completa { BONUS_COMPLETA } else { 0 };
    estrelas_efetivas(pedido).inteiras().min(4) + modo
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
        // custos cortados em ~55% em 2026-10-05 (eram 150 mil e 400 mil)
        ModoBusca::Rapida => (7, 70_000),
        ModoBusca::Completa => (21, 180_000),
    }
}

/// Multiplicadores da amplitude, em %: (duração, custo).
fn fatores_amplitude(amplitude: AmplitudeGeografica) -> (u32, i64) {
    match amplitude {
        AmplitudeGeografica::Pais => (100, 100),
        AmplitudeGeografica::VariosPaises => (125, 135),
        AmplitudeGeografica::Continente => (150, 170),
        AmplitudeGeografica::Mundo => (200, 240),
    }
}

/// Multiplicador de custo do Tier, em % (Elite cobra mais caro por Missão).
fn fator_custo_tier(tier: Tier) -> i64 {
    match tier {
        Tier::Junior => 100,
        Tier::Experiente => 130,
        Tier::Elite => 180,
    }
}

/// Multiplicador de duração da Rede de contatos, em %: `130 − 5 × meias`
/// (1★ → 120%, 3★ → 100%, 4,5★ → 85%, como os Tiers do v1; 5★ → 80%).
fn fator_duracao_rede(rede: Estrelas) -> u32 {
    130 - 5 * u32::from(rede.meias())
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
    let pede_perfil = !filtros.atributos_dominantes.is_empty()
        || filtros.fit_posicional.is_some()
        || (filtros.fit_nas_posicoes && !filtros.posicoes.is_empty())
        || filtros.referencia.is_some();
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
///
/// Épico 5: a Rede de contatos (menos a penalidade de velocidade) decide o
/// prazo; a verba mexe no custo, no prazo e nos jogadores; a Econômica
/// ainda alarga a precisão em 1 ponto.
pub fn estimar_missao(pedido: &PedidoMissao) -> EstimativaMissao {
    let pontos = pontuacao(pedido);
    let (dias_base, custo_base) = base_do_modo(pedido.modo);
    let (dias_amplitude, custo_amplitude) = fatores_amplitude(pedido.amplitude);
    let dias_rede = fator_duracao_rede(pedido.perfil.rede.menos(pedido.penalidade.velocidade));
    let custo_tier = fator_custo_tier(pedido.perfil.tier());
    let (custo_verba, dias_verba, jogadores_verba) = pedido.investimento.fatores();

    // Dias: arredonda para cima (uma Missão nunca termina "antes" do que
    // a conta diz).
    let duracao_dias = (dias_base * dias_amplitude * dias_rede * dias_verba).div_ceil(100 * 100 * 100);
    let custo = custo_base * custo_amplitude * custo_tier * custo_verba / (100 * 100 * 100);
    let custo = (custo + ARREDONDAMENTO_CUSTO / 2) / ARREDONDAMENTO_CUSTO * ARREDONDAMENTO_CUSTO;
    let alvo = (u32::from(alvo_jogadores(pedido.modo, pontos)) * jogadores_verba).div_ceil(100);
    let extra_verba = u8::from(pedido.investimento == Investimento::Economica);

    EstimativaMissao {
        custo: i32::try_from(custo).unwrap_or(i32::MAX),
        duracao_dias,
        qualidade: qualidade_da_pontuacao(pontos),
        atributos_revelados: atributos_da_pontuacao(pontos),
        precisao_mais_menos: precisao_base(pontos) + precisao_extra_amplitude(pedido.amplitude) + extra_verba,
        alvo_jogadores: u8::try_from(alvo.max(1)).unwrap_or(u8::MAX),
    }
}

// ---------------------------------------------------------------------
// Valor de mercado e salário estimados (branch `claude/relatorio-ficha`,
// 2026-10-01; salário recalibrado em 2026-10-03; valor recalibrado em
// 2026-10-07 com valores EXATOS lidos do jogo)
// ---------------------------------------------------------------------
//
// O FIFA não grava valor nem salário dos jogadores: calcula na hora, ao
// abrir a tela do jogador. O save só tem os contratos do PRÓPRIO elenco
// (`career_playercontract`). O Relatório mostra uma ESTIMATIVA do Olheiro,
// a partir do que ele revelou (meio das faixas) e da idade, até a Central
// ler o valor exato na tela do jogo (`save_repo::foco`).
//
// - valor: ajustado em 2026-10-07 a 9 valores exatos da carreira de teste
//   (Overall 70–90, idades 19–33; erro médio 3%, 5% deixando cada ponto de
//   fora): ln(valor) = 2,854 + 0,174 × Overall + 0,045 × max(0, 24 − idade)
//   − 0,085 × max(0, idade − 27). O potencial e a posição (goleiro) não
//   precisaram de termo: a juventude já carrega o prêmio. A fórmula antiga
//   (3 M × 1,2^(OVR−70) × idade × crescimento) passava de 1,1 a 1,7 vez o
//   valor real. Abaixo de Overall 65 não há exato para conferir.
// - salário semanal: calibrado com os contratos reais do elenco do Felipe
//   (2026-10-03): Overall 70 ≈ 20 mil, 80 ≈ 120 mil, 87 ≈ 240 mil,
//   90 ≈ 300 mil. Sobe ~19,6% por ponto até 80 e ~9,6% depois.

/// Arredonda para um número "de mercado": 100 mil acima de 1 M, 5 mil abaixo.
pub fn arredondar_mercado(valor: f64) -> i64 {
    let passo = if valor >= 1_000_000.0 { 100_000.0 } else { 5_000.0 };
    let v = ((valor / passo).round() * passo) as i64;
    v.max(10_000)
}

/// Valor de mercado estimado (mesma unidade do orçamento). `potencial` e
/// `goleiro` ficam na assinatura (os filtros da busca os têm), mas o ajuste
/// aos valores reais não encontrou efeito próprio deles.
pub fn valor_estimado(overall: u8, _potencial: u8, idade: u8, _goleiro: bool) -> i64 {
    let jovem = f64::from(24u8.saturating_sub(idade));
    let velho = f64::from(idade.saturating_sub(27));
    let ln_valor = 2.854 + 0.174 * f64::from(overall) + 0.045 * jovem - 0.085 * velho;
    arredondar_mercado(ln_valor.exp())
}

/// Quantos valores exatos lidos do jogo são preciso para a estimativa se
/// ajustar sozinha, e quanto ela pode se mexer.
const MIN_LEITURAS_PARA_AJUSTE: usize = 5;
const AJUSTE_MIN: f64 = 0.75;
const AJUSTE_MAX: f64 = 1.33;

/// Fator que corrige a estimativa a partir dos valores exatos que a Central
/// já leu: média geométrica de `valor real ÷ estimativa` (cada razão em
/// logaritmo), puxada para 1 enquanto há poucas leituras e limitada a
/// [0,75; 1,33]. Menos de 5 leituras: 1.
pub fn ajuste_de_valor(razoes_ln: &[f64]) -> f64 {
    let n = razoes_ln.len();
    if n < MIN_LEITURAS_PARA_AJUSTE {
        return 1.0;
    }
    let n = n as f64;
    let media = razoes_ln.iter().sum::<f64>() / n;
    (media * n / (n + MIN_LEITURAS_PARA_AJUSTE as f64)).exp().clamp(AJUSTE_MIN, AJUSTE_MAX)
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
// Nível de conhecimento no jogo (Épico 7, Story 7.3)
// ---------------------------------------------------------------------
//
// O FIFA guarda, por jogador, um nível de conhecimento de 0 a 198 (ver
// `save_repo::nativo`). Já visto em jogo (2026-10-06): qualquer nível acima
// de ~27 mostra estimativas de atributos; a partir de 140 o jogo mostra a
// taxa de transferência e o salário e refina as estimativas; 198 = tudo.

/// Nível em que o jogo passa a mostrar valor e salário.
pub const NIVEL_JOGO_VALOR: i32 = 140;
/// Jogador totalmente conhecido pelo jogo.
pub const NIVEL_JOGO_COMPLETO: i32 = 198;

/// Nível de conhecimento que o jogo deve ter de um jogador, a partir do que
/// a Central sabe dele agora:
/// - observação VENCIDA: volta ao que o jogo tinha antes (`original`);
/// - Missão ainda rodando (o Relatório é parcial): só valor e salário
///   (`NIVEL_JOGO_VALOR`, "o valor anterior ao completo");
/// - Missão concluída: `198 − 4 × precisão (±)`, nunca abaixo de 140 — a
///   precisão já inclui o envelhecimento e o acompanhamento de um
///   Generalista, então o nível desce com o tempo e sobe até 198 quando o
///   valor fica exato (Alta ±1 ≈ 194; Média ±5 ≈ 178; Baixa ±10 ≈ 158).
pub fn nivel_no_jogo(precisao: u8, parcial: bool, vencido: bool, original: i32) -> i32 {
    if vencido {
        return original.clamp(0, NIVEL_JOGO_COMPLETO);
    }
    if parcial {
        return NIVEL_JOGO_VALOR;
    }
    (NIVEL_JOGO_COMPLETO - 4 * i32::from(precisao)).clamp(NIVEL_JOGO_VALOR, NIVEL_JOGO_COMPLETO)
}

// ---------------------------------------------------------------------
// Relatório parcial e Missão contínua (Story 2.10)
// ---------------------------------------------------------------------

/// Uma Missão contínua ("sem prazo") é um **contrato de 12 meses** com o
/// Olheiro (2026-10-07; antes era paga mês a mês): o contrato é pago uma vez
/// e renova sozinho a cada 12 meses, se houver verba (o jogador pode
/// desligar a renovação). Por dentro, a busca anda em blocos de tantos dias
/// de carreira: a cada bloco o Olheiro traz mais jogadores, o mesmo número
/// que uma Missão de prazo fixo com os mesmos filtros, espalhados pelo bloco
/// e buscados com os dados do momento. O último bloco do contrato absorve os
/// dias que sobram.
pub const DIAS_BLOCO_CONTINUO: u32 = 30;

/// Duração do contrato de uma Missão contínua: 12 meses.
pub const DIAS_CONTRATO_CONTINUO: u32 = 365;

/// Quanto o contrato de 12 meses custa frente à Missão de prazo fixo com os
/// mesmos filtros, em %: mais caro que ela, bem mais barato que os 12 meses
/// pagos um a um.
pub const PCT_CUSTO_CONTRATO: i64 = 300;

/// Custo do contrato de 12 meses de uma Missão contínua cujo equivalente de
/// prazo fixo custa `custo_fixo` (arredondado como os outros custos).
pub fn custo_do_contrato(custo_fixo: i32) -> i32 {
    let total = i64::from(custo_fixo) * PCT_CUSTO_CONTRATO / 100;
    i32::try_from((total + ARREDONDAMENTO_CUSTO / 2) / ARREDONDAMENTO_CUSTO * ARREDONDAMENTO_CUSTO).unwrap_or(i32::MAX)
}

/// Quanto da contratação o Olheiro cobra de multa por ser tirado de um
/// contrato no meio para outra localidade, em %.
pub const PCT_MULTA_RESCISAO: i64 = 20;

/// A multa de rescisão de um Olheiro: fixa para ele (um quinto do que custou
/// contratá-lo, então Olheiros melhores cobram mais), no mínimo 50 mil.
pub fn multa_de_rescisao(perfil: &PerfilOlheiro, continente: Option<Confederacao>, mercados: usize, habilidades: &[Habilidade]) -> i32 {
    let total = i64::from(custo_contratacao_com(perfil, continente, mercados, habilidades)) * PCT_MULTA_RESCISAO / 100;
    let arredondada = (total + ARREDONDAMENTO_CUSTO / 2) / ARREDONDAMENTO_CUSTO * ARREDONDAMENTO_CUSTO;
    i32::try_from(arredondada).unwrap_or(i32::MAX).max(50_000)
}

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

/// As posições-alvo do Fit para as posições pedidas (os grupos de `Perfil`):
/// todas as `PosicaoAlvo` cujo perfil está entre elas.
pub fn alvos_do_fit(posicoes: &[Perfil]) -> Vec<PosicaoAlvo> {
    PosicaoAlvo::TODAS.into_iter().filter(|alvo| posicoes.contains(&alvo.perfil())).collect()
}

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

/// Perfil ideal de uma função em campo (o que `PERFIS` pesa). Também é o
/// grupo do filtro de Posição (2026-10-03). No JSON: `"zagueiro"` etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Um degrau a mais (`direcao` > 0) ou a menos na escala de dinheiro dos
/// limites de orçamento: 1-2-5 (10 mil, 20 mil, 50 mil, 100 mil… 1 bi).
/// Um valor fora da escala vai para o degrau seguinte na direção pedida.
pub fn degrau_dinheiro(valor: i64, direcao: i32) -> i64 {
    let mut escala = Vec::new();
    let mut base: i64 = 10_000;
    while base <= 1_000_000_000 {
        escala.extend([base, base * 2, base * 5]);
        base *= 10;
    }
    escala.retain(|&d| d <= 1_000_000_000);
    if direcao > 0 {
        escala.iter().copied().find(|&d| d > valor).unwrap_or(1_000_000_000)
    } else {
        escala.iter().rev().copied().find(|&d| d < valor).unwrap_or(10_000)
    }
}

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
    /// Os grupos do filtro de Posição, da defesa para o ataque.
    pub const TODOS: [Perfil; 11] = [
        Perfil::Goleiro,
        Perfil::Zagueiro,
        Perfil::Lateral,
        Perfil::Ala,
        Perfil::Volante,
        Perfil::MeioCampista,
        Perfil::MeiaAtacante,
        Perfil::MeiaAberto,
        Perfil::Ponta,
        Perfil::SegundoAtacante,
        Perfil::Centroavante,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Perfil::Goleiro => "Goleiro",
            Perfil::Zagueiro => "Zagueiro",
            Perfil::Lateral => "Lateral (direito ou esquerdo)",
            Perfil::Ala => "Ala (direito ou esquerdo)",
            Perfil::Volante => "Volante",
            Perfil::MeioCampista => "Meio-campista",
            Perfil::MeiaAtacante => "Meia-atacante",
            Perfil::MeiaAberto => "Meia aberto (direita ou esquerda)",
            Perfil::Ponta => "Ponta (direita ou esquerda)",
            Perfil::SegundoAtacante => "Segundo atacante",
            Perfil::Centroavante => "Centroavante",
        }
    }

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

/// Atalhos de filtro: um clique monta uma busca comum. Mantêm a geografia,
/// as posições e o orçamento; o resto volta ao padrão antes de aplicar.
/// Sem botão na Nova Missão desde 2026-10-05 (retirados por ora, a pedido do
/// Felipe); o código fica para quando voltarem.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atalho {
    JovensPromessas,
    MudaPatamar,
    NivelTitular,
    NivelBanco,
    FimDeContrato,
}

#[allow(dead_code)]
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
            posicoes: atuais.posicoes.clone(),
            limite_valor: atuais.limite_valor,
            limite_salario: atuais.limite_salario,
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

    /// Um pedido do v1 com a Especialização e o Tier que o geraram.
    #[derive(Debug, Clone, Copy)]
    struct Caso {
        tier: Tier,
        especializacao: Especializacao,
        pedido: PedidoMissao,
    }

    impl Caso {
        fn com_tier(self, tier: Tier) -> Caso {
            let p = self.pedido;
            Caso { tier, pedido: PedidoMissao::v1(tier, self.especializacao, p.modo, p.tipo, p.amplitude), ..self }
        }
    }

    /// Todas as 4 × 3 × 2 × 4 × 4 = 384 combinações de entrada do v1.
    fn todos_os_casos() -> Vec<Caso> {
        let mut casos = Vec::new();
        for tier in Tier::TODOS {
            for especializacao in Especializacao::TODAS {
                for modo in ModoBusca::TODOS {
                    for tipo in TipoMissao::TODOS {
                        for amplitude in AmplitudeGeografica::TODAS {
                            casos.push(Caso { tier, especializacao, pedido: PedidoMissao::v1(tier, especializacao, modo, tipo, amplitude) });
                        }
                    }
                }
            }
        }
        casos
    }

    fn todos_os_pedidos() -> Vec<PedidoMissao> {
        todos_os_casos().into_iter().map(|c| c.pedido).collect()
    }

    /// A pontuação da tabela da Story 2.1: `Tier (1–3) + 1 se combina + 1
    /// se Completa` (o Generalista, que no v1 não combinava com nada, agora
    /// combina com a Geral).
    fn pontuacao_v1(c: &Caso) -> u8 {
        let tier = match c.tier {
            Tier::Junior => 1,
            Tier::Experiente => 2,
            Tier::Elite => 3,
        };
        tier + u8::from(aderente(c.especializacao, c.pedido.tipo)) + u8::from(c.pedido.modo == ModoBusca::Completa)
    }

    #[test]
    fn the_v1_profile_reproduces_the_story_2_1_table() {
        for c in todos_os_casos() {
            assert_eq!(pontuacao(&c.pedido), pontuacao_v1(&c), "{c:?}");
            assert_eq!(c.pedido.perfil.tier(), c.tier, "o Tier é o resumo do foco");
            assert_eq!(c.pedido.perfil.foco(), c.especializacao);
        }
        // prazo: a Rede do v1 reproduz os fatores de Tier (120/100/85%)
        let r = |tier| fator_duracao_rede(PerfilOlheiro::v1(Especializacao::Tatico, tier).rede);
        assert_eq!((r(Tier::Junior), r(Tier::Experiente), r(Tier::Elite)), (120, 100, 85));
    }

    #[test]
    fn hiring_cost_grows_with_the_focus_stars_and_the_extras() {
        let perfil = |foco: u8, rede: u8| PerfilOlheiro {
            jovens: Estrelas(foco),
            medalhoes: Estrelas(foco.saturating_sub(3)),
            tatico: Estrelas(foco.saturating_sub(3)),
            generalista: Estrelas(foco.saturating_sub(3)),
            rede: Estrelas(rede),
        };
        let custo = |p: &PerfilOlheiro| custo_contratacao(p, Some(Confederacao::Africa), 1);
        let junior = custo(&perfil(5, 5));
        let experiente = custo(&perfil(7, 5));
        let elite = custo(&perfil(9, 5));
        assert!((250_000..600_000).contains(&junior), "{junior}");
        assert!((1_000_000..2_000_000).contains(&experiente), "{experiente}");
        assert!((3_000_000..6_000_000).contains(&elite), "{elite}");
        assert!(custo(&perfil(7, 9)) > experiente, "Rede maior custa mais");
        assert!(custo_contratacao(&perfil(7, 5), Some(Confederacao::Africa), 3) > experiente, "mais mercados custam mais");
        assert!(custo_contratacao(&perfil(7, 5), Some(Confederacao::Europa), 1) > experiente, "europeu custa mais");
        assert!(junior % 10_000 == 0 && custo(&perfil(0, 0)) >= 100_000);
    }

    #[test]
    fn a_higher_tier_gives_a_better_report() {
        for c in todos_os_casos().into_iter().filter(|c| c.tier != Tier::Elite) {
            let acima = c.com_tier(if c.tier == Tier::Junior { Tier::Experiente } else { Tier::Elite });
            let (a, b) = (estimar_missao(&c.pedido), estimar_missao(&acima.pedido));
            assert!(b.qualidade >= a.qualidade, "{c:?}");
            assert!(b.atributos_revelados > a.atributos_revelados, "{c:?}");
            assert!(b.precisao_mais_menos < a.precisao_mais_menos, "{c:?}");
        }
        // de Júnior para Elite o nível sempre sobe
        for c in todos_os_casos().into_iter().filter(|c| c.tier == Tier::Junior) {
            let elite = c.com_tier(Tier::Elite);
            assert!(estimar_missao(&elite.pedido).qualidade > estimar_missao(&c.pedido).qualidade, "{c:?}");
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
            (Especializacao::Generalista, TipoMissao::Geral),
        ];
        for c in todos_os_casos() {
            for (especializacao, tipo) in casos {
                if c.especializacao != especializacao || c.pedido.tipo == tipo {
                    continue;
                }
                let combinando = PedidoMissao { tipo, ..c.pedido };
                let (fora, dentro) = (estimar_missao(&c.pedido), estimar_missao(&combinando));
                assert!(dentro.qualidade >= fora.qualidade, "{c:?}");
                assert!(dentro.atributos_revelados > fora.atributos_revelados, "{c:?}");
            }
        }
    }

    #[test]
    fn extremes_match_the_documented_table() {
        let pior = PedidoMissao::v1(Tier::Junior, Especializacao::Tatico, ModoBusca::Rapida, TipoMissao::Geral, AmplitudeGeografica::Mundo);
        let melhor =
            PedidoMissao::v1(Tier::Elite, Especializacao::CacadorDeJovens, ModoBusca::Completa, TipoMissao::Jovens, AmplitudeGeografica::Pais);
        assert_eq!(
            estimar_missao(&pior),
            EstimativaMissao {
                custo: 170_000,
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
                custo: 320_000,
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
        assert!(combina(Especializacao::Generalista, TipoMissao::Geral), "Épico 5: a Geral é a do Generalista");
        assert!(!combina(Especializacao::Generalista, TipoMissao::Jovens));
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
        let pedido = |especializacao| {
            PedidoMissao::v1(Tier::Experiente, especializacao, ModoBusca::Rapida, tipo_por_filtros(&filtros), AmplitudeGeografica::Pais)
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
    fn the_game_level_follows_precision_and_never_drops_below_the_value_level_until_expired() {
        assert_eq!(nivel_no_jogo(0, false, false, 0), 198, "valor exato = jogador completo");
        assert_eq!(nivel_no_jogo(1, false, false, 0), 194, "Alta");
        assert_eq!(nivel_no_jogo(5, false, false, 0), 178, "Média");
        assert_eq!(nivel_no_jogo(10, false, false, 0), 158, "Baixa");
        assert_eq!(nivel_no_jogo(14, false, false, 0), 142, "análise nova");
        assert_eq!(nivel_no_jogo(22, false, false, 0), 140, "piso: o jogo continua mostrando valor e salário");
        assert_eq!(nivel_no_jogo(2, true, false, 0), 140, "Missão ainda rodando: valor anterior ao completo");
        assert_eq!(nivel_no_jogo(2, false, true, 0), 0, "vencido sem nada antes: volta a nada");
        assert_eq!(nivel_no_jogo(2, false, true, 77), 77, "vencido: volta ao que o jogo tinha");
        // mais precisão nunca dá nível menor
        let niveis: Vec<i32> = (0..=30u8).map(|p| nivel_no_jogo(p, false, false, 0)).collect();
        assert!(niveis.windows(2).all(|par| par[0] >= par[1]));
    }

    /// Valores EXATOS lidos do jogo (carreira de teste, 10/07/2026): overall,
    /// potencial, idade e o valor que a tela do jogo mostrou.
    const VALORES_REAIS: [(u8, u8, u8, i64); 9] = [
        (72, 92, 20, 6_000_000),   // Kees Smit
        (70, 87, 19, 3_900_000),   // Kostoulas
        (88, 88, 33, 45_500_000),  // Oblak (goleiro)
        (81, 81, 31, 16_000_000),  // Palazón
        (88, 91, 27, 80_000_000),  // Valverde
        (85, 88, 28, 39_500_000),  // Militão
        (77, 84, 25, 11_000_000),  // Logan Costa
        (90, 94, 23, 108_500_000), // Bellingham
        (71, 87, 19, 5_000_000),   // Nypan
    ];

    #[test]
    fn the_value_estimate_matches_the_exact_values_read_from_the_game() {
        for (overall, potencial, idade, real) in VALORES_REAIS {
            let estimado = valor_estimado(overall, potencial, idade, false);
            let razao = estimado as f64 / real as f64;
            assert!((0.9..=1.1).contains(&razao), "OVR {overall} ({idade} anos): estimado {estimado} contra {real} (×{razao:.2})");
        }
        // o goleiro não ganha desconto: o Oblak (88, 33 anos) bate sem termo próprio
        assert_eq!(valor_estimado(88, 88, 33, true), valor_estimado(88, 88, 33, false));
    }

    #[test]
    fn the_value_estimate_orders_players_sensibly() {
        let v80 = valor_estimado(80, 80, 27, false);
        assert!((15_000_000..30_000_000).contains(&v80), "{v80}");
        assert!(valor_estimado(90, 90, 27, false) > 100_000_000);
        assert!(valor_estimado(60, 60, 27, false) < 1_000_000);
        assert!(valor_estimado(75, 88, 19, false) > valor_estimado(75, 75, 27, false), "jovem vale mais");
        assert!(valor_estimado(80, 80, 34, false) < valor_estimado(80, 80, 27, false), "veterano vale menos");
        assert!(valor_estimado(80, 80, 40, false) >= 10_000, "nunca abaixo do piso");
        assert_eq!(v80 % 100_000, 0, "arredondado");
        let niveis: Vec<i64> = (50..=95u8).map(|o| valor_estimado(o, o, 26, false)).collect();
        assert!(niveis.windows(2).all(|p| p[0] <= p[1]), "mais overall nunca vale menos");
    }

    #[test]
    fn the_value_adjusts_only_after_enough_readings_and_never_too_far() {
        assert_eq!(ajuste_de_valor(&[]), 1.0);
        assert_eq!(ajuste_de_valor(&[0.3; 4]), 1.0, "poucas leituras: nada muda");
        // cinco leituras 20% acima do estimado: sobe, puxado para 1 (meio caminho)
        let a = ajuste_de_valor(&[0.2f64.ln_1p(); 5]);
        assert!(a > 1.0 && a < 1.2, "{a}");
        // leituras iguais ao estimado: 1
        assert!((ajuste_de_valor(&[0.0; 30]) - 1.0).abs() < 1e-9);
        // absurdo: limitado
        assert_eq!(ajuste_de_valor(&[3.0; 50]), 1.33);
        assert_eq!(ajuste_de_valor(&[-3.0; 50]), 0.75);
    }

    #[test]
    fn estimated_wage_follows_the_squad_contracts() {
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
        use crate::scout::state::Limite;
        let atuais = FiltrosMissao {
            ligas: vec![13],
            posicoes: vec![Perfil::Centroavante],
            limite_valor: Limite::SemLimite,
            atributos_dominantes: vec![Atributo::Drible],
            ..FiltrosMissao::default()
        };
        for atalho in Atalho::TODOS {
            let f = atalho.aplicar(&atuais);
            assert_eq!((f.ligas.clone(), f.limite_valor, f.posicoes.clone()), (vec![13], Limite::SemLimite, vec![Perfil::Centroavante]), "{atalho:?}");
            assert!(f.atributos_dominantes.is_empty(), "o resto volta ao padrão");
            assert!(f.nivel_elenco.is_some());
            assert!(!atalho.descricao().contains('!'));
        }
        assert_eq!(Atalho::FimDeContrato.aplicar(&atuais).contrato, FaixaAtributo { min: 0, max: 0 });
        assert_eq!(tipo_por_filtros(&Atalho::JovensPromessas.aplicar(&atuais)), TipoMissao::Jovens);
        assert_eq!(tipo_por_filtros(&Atalho::MudaPatamar.aplicar(&atuais)), TipoMissao::Medalhoes);
    }

    #[test]
    fn money_limits_move_on_a_1_2_5_scale() {
        assert_eq!(degrau_dinheiro(15_000_000, 1), 20_000_000);
        assert_eq!(degrau_dinheiro(15_000_000, -1), 10_000_000);
        assert_eq!(degrau_dinheiro(20_000_000, 1), 50_000_000);
        assert_eq!(degrau_dinheiro(10_000, -1), 10_000, "não passa do mínimo");
        assert_eq!(degrau_dinheiro(1_000_000_000, 1), 1_000_000_000, "nem do máximo");
        assert_eq!(Perfil::TODOS.len(), 11);
    }

    // -----------------------------------------------------------------
    // Épico 5: estrelas, mercados, adaptação, verba, mercado de Olheiros
    // -----------------------------------------------------------------

    fn brasil() -> Regiao {
        Regiao::Pais { id: 54, continente: Confederacao::AmericaDoSul }
    }

    fn china() -> Regiao {
        Regiao::Pais { id: 155, continente: Confederacao::Asia }
    }

    fn argentina() -> Regiao {
        Regiao::Pais { id: 52, continente: Confederacao::AmericaDoSul }
    }

    fn do_brasil() -> Vec<Mercado> {
        vec![Mercado::Pais { id: 54, continente: Confederacao::AmericaDoSul }]
    }

    #[test]
    fn stars_are_half_steps_and_the_focus_is_the_highest_attribute() {
        assert_eq!(Estrelas(7).texto(), "3,5");
        assert_eq!(Estrelas(8).texto(), "4");
        assert_eq!(Estrelas::de_meias(14), Estrelas(10));
        assert_eq!(Estrelas::de_meias(-3), Estrelas(0));
        assert_eq!((Estrelas(9).inteiras(), Estrelas(9).menos(4), Estrelas(9).mais(4)), (4, Estrelas(5), Estrelas(10)));
        let perfil = PerfilOlheiro { jovens: Estrelas(4), medalhoes: Estrelas(8), tatico: Estrelas(8), generalista: Estrelas(6), rede: Estrelas(5) };
        assert_eq!(perfil.foco(), Especializacao::CacadorDeMedalhoes, "empate: a ordem do PRD");
        assert_eq!(perfil.tier(), Tier::Experiente);
        assert_eq!(perfil.para_tipo(TipoMissao::Geral), Estrelas(6), "a Geral usa o Generalista");
        assert_eq!(perfil.para_tipo(TipoMissao::Jovens), Estrelas(4), "Generalista 3★ − 1 = 2★ de piso");
    }

    #[test]
    fn market_distance_goes_from_home_to_same_continent_to_elsewhere() {
        let mercados = do_brasil();
        assert_eq!(distancia_mercado(&mercados, brasil()), 0);
        assert_eq!(distancia_mercado(&mercados, argentina()), 1);
        assert_eq!(distancia_mercado(&mercados, china()), 2);
        assert_eq!(distancia_mercado(&mercados, Regiao::Continente(Confederacao::AmericaDoSul)), 1);
        assert_eq!(distancia_mercado(&mercados, Regiao::Continente(Confederacao::Europa)), 2);
        assert_eq!(distancia_mercado(&mercados, Regiao::Mundo), 1);
        let continental = vec![Mercado::Continente(Confederacao::AmericaDoSul)];
        assert_eq!(distancia_mercado(&continental, argentina()), 0, "conhece o continente inteiro");
        assert_eq!(distancia_mercado(&[], china()), 0, "Olheiro de antes das estrelas: sem penalidade");
    }

    #[test]
    fn the_market_penalty_shrinks_with_six_months_of_work_there() {
        let perfil = PerfilOlheiro::v1(Especializacao::Generalista, Tier::Elite);
        let mercados = do_brasil();
        let sem_trabalho = penalidade_missao(&perfil, &mercados, &[china()], TipoMissao::Geral, &[]);
        assert_eq!(sem_trabalho, Penalidade { qualidade: 2, velocidade: 4 }, "−1★ e −2★ em outro continente");
        let meio = [TrabalhoPassado { tipo: TipoMissao::Geral, regioes: vec![china()], dias: 90 }];
        assert_eq!(penalidade_missao(&perfil, &mercados, &[china()], TipoMissao::Geral, &meio), Penalidade { qualidade: 1, velocidade: 2 });
        let adaptado = [TrabalhoPassado { tipo: TipoMissao::Geral, regioes: vec![Regiao::Continente(Confederacao::Asia)], dias: 200 }];
        assert_eq!(
            penalidade_missao(&perfil, &mercados, &[china()], TipoMissao::Geral, &adaptado),
            Penalidade::default(),
            "trabalhar no continente vale para os países dele"
        );
        assert_eq!(penalidade_missao(&perfil, &mercados, &[brasil(), china()], TipoMissao::Geral, &[]).qualidade, 2, "vale a pior região");
        assert_eq!(penalidade_missao(&perfil, &mercados, &[], TipoMissao::Geral, &[]), Penalidade { qualidade: 1, velocidade: 2 }, "mundo todo");
    }

    #[test]
    fn a_specialist_out_of_focus_loses_quality_until_he_gets_used_to_it() {
        let cj = PerfilOlheiro::v1(Especializacao::CacadorDeJovens, Tier::Experiente);
        let p = penalidade_missao(&cj, &[], &[], TipoMissao::Medalhoes, &[]);
        assert_eq!(p, Penalidade { qualidade: 2, velocidade: 0 });
        let meses = |dias| [TrabalhoPassado { tipo: TipoMissao::Medalhoes, regioes: vec![Regiao::Mundo], dias }];
        assert_eq!(penalidade_missao(&cj, &[], &[], TipoMissao::Medalhoes, &meses(270)).qualidade, 0, "Experiente: 9 meses");
        let elite = PerfilOlheiro::v1(Especializacao::CacadorDeJovens, Tier::Elite);
        assert_eq!(penalidade_missao(&elite, &[], &[], TipoMissao::Medalhoes, &meses(180)).qualidade, 0, "Elite: 6 meses");
        let junior = PerfilOlheiro::v1(Especializacao::CacadorDeJovens, Tier::Junior);
        assert!(penalidade_missao(&junior, &[], &[], TipoMissao::Medalhoes, &meses(180)).qualidade > 0, "Júnior: 1 ano");
        // a Geral e o Generalista nunca têm penalidade de foco
        assert_eq!(penalidade_missao(&cj, &[], &[], TipoMissao::Geral, &[]), Penalidade::default());
        let g = PerfilOlheiro::v1(Especializacao::Generalista, Tier::Junior);
        assert_eq!(penalidade_missao(&g, &[], &[], TipoMissao::Tatica, &[]), Penalidade::default());
        assert_eq!(penalidade_missao(&cj, &[], &[], TipoMissao::Jovens, &[]), Penalidade::default());
    }

    #[test]
    fn penalties_lower_quality_and_slow_the_missao() {
        let base = PedidoMissao::v1(Tier::Elite, Especializacao::Tatico, ModoBusca::Completa, TipoMissao::Tatica, AmplitudeGeografica::Pais);
        let longe = PedidoMissao { penalidade: Penalidade { qualidade: 2, velocidade: 4 }, ..base };
        let (a, b) = (estimar_missao(&base), estimar_missao(&longe));
        assert!(b.atributos_revelados < a.atributos_revelados && b.precisao_mais_menos > a.precisao_mais_menos, "{a:?} {b:?}");
        assert!(b.duracao_dias > a.duracao_dias);
        let junior = PedidoMissao::v1(Tier::Junior, Especializacao::Tatico, ModoBusca::Rapida, TipoMissao::Tatica, AmplitudeGeografica::Pais);
        let junior_longe = PedidoMissao { penalidade: Penalidade { qualidade: 2, velocidade: 0 }, ..junior };
        assert!(estimar_missao(&junior_longe).qualidade <= estimar_missao(&junior).qualidade);
        assert_eq!(b.custo, a.custo, "a penalidade não muda o custo");
    }

    #[test]
    fn the_travel_budget_trades_money_for_players_quality_and_speed() {
        let base = PedidoMissao::v1(Tier::Experiente, Especializacao::Tatico, ModoBusca::Completa, TipoMissao::Tatica, AmplitudeGeografica::Continente);
        let com = |investimento| estimar_missao(&PedidoMissao { investimento, ..base });
        let (eco, pad, ref_) = (com(Investimento::Economica), com(Investimento::Padrao), com(Investimento::Reforcada));
        assert_eq!(pad, estimar_missao(&base));
        assert!(eco.custo < pad.custo && pad.custo < ref_.custo);
        assert!(eco.alvo_jogadores < pad.alvo_jogadores && pad.alvo_jogadores < ref_.alvo_jogadores);
        assert!(eco.precisao_mais_menos > pad.precisao_mais_menos);
        assert!(eco.duracao_dias > pad.duracao_dias && ref_.duracao_dias < pad.duracao_dias);
        assert!(ref_.atributos_revelados >= pad.atributos_revelados);
        // a faixa de valores cresce com a escala da busca
        let mundo = PedidoMissao { amplitude: AmplitudeGeografica::Mundo, ..base };
        let faixa = |p: PedidoMissao| {
            estimar_missao(&PedidoMissao { investimento: Investimento::Reforcada, ..p }).custo
                - estimar_missao(&PedidoMissao { investimento: Investimento::Economica, ..p }).custo
        };
        assert!(faixa(mundo) > faixa(base));
    }

    fn clube(nacional: u8, internacional: u8, nivel: u8, titulos: u16) -> PerfilClube {
        PerfilClube { prestigio_nacional: nacional, prestigio_internacional: internacional, nivel_liga: nivel, titulos, pais: Some(45), continente: Confederacao::Europa }
    }

    fn paises() -> Vec<PaisCandidato> {
        [
            (45, Confederacao::Europa, 16),
            (14, Confederacao::Europa, 8),
            (54, Confederacao::AmericaDoSul, 8),
            (52, Confederacao::AmericaDoSul, 5),
            (155, Confederacao::Asia, 5),
            (103, Confederacao::Africa, 1),
        ]
        .into_iter()
        .map(|(id, continente, peso)| PaisCandidato { id, continente, peso })
        .collect()
    }

    #[test]
    fn club_attractiveness_comes_from_prestige_division_and_titles() {
        assert_eq!(atratividade(&clube(20, 20, 1, 21)), 100);
        assert!(atratividade(&clube(10, 5, 1, 0)) < 50);
        assert!(atratividade(&clube(10, 5, 3, 0)) < atratividade(&clube(10, 5, 1, 0)));
        assert!(atratividade(&clube(10, 5, 1, 8)) > atratividade(&clube(10, 5, 1, 0)));
        assert_eq!(nome_atratividade(100), "Alta");
        assert_eq!(nome_atratividade(50), "Média");
        assert_eq!(nome_atratividade(10), "Baixa");
        assert_eq!(periodo_do_mercado(700), periodo_do_mercado(693) + 1);
    }

    #[test]
    fn abilities_follow_the_eligibility_and_tier_caps_and_are_deterministic() {
        let fraco = PerfilOlheiro { jovens: Estrelas(4), medalhoes: Estrelas(4), tatico: Estrelas(4), generalista: Estrelas(4), rede: Estrelas(5) };
        let forte = PerfilOlheiro { jovens: Estrelas(9), medalhoes: Estrelas(9), tatico: Estrelas(9), generalista: Estrelas(9), rede: Estrelas(5) };
        for semente in 0..400u64 {
            // 2★ em tudo: só o Perfil Físico é possível
            assert!(sortear_habilidades(&fraco, Tier::Elite, semente).iter().all(|&h| h == Habilidade::PerfilFisico));
            for (tier, teto) in [(Tier::Junior, 1), (Tier::Experiente, 2), (Tier::Elite, 3)] {
                let h = sortear_habilidades(&forte, tier, semente);
                assert!(h.len() <= teto, "{tier:?} {h:?}");
                assert_eq!(h, sortear_habilidades(&forte, tier, semente), "mesma semente, mesmas habilidades");
                assert!(h.windows(2).all(|w| w[0] < w[1]), "sem repetir, em ordem");
            }
        }
        let todas = |tier| (0..400u64).map(|s| sortear_habilidades(&forte, tier, s).len()).collect::<Vec<_>>();
        assert!(todas(Tier::Elite).contains(&3) && todas(Tier::Elite).contains(&0), "há de tudo no mercado");
        assert!(!todas(Tier::Junior).contains(&2));
        // o Fit pede Tático de 4,5★; a Referência, 3★
        let tatico = |meias| PerfilOlheiro { tatico: Estrelas(meias), ..fraco };
        assert!(!Habilidade::FitPosicional.elegivel(&tatico(8)) && Habilidade::FitPosicional.elegivel(&tatico(9)));
        assert!(!Habilidade::JogadorDeReferencia.elegivel(&tatico(5)) && Habilidade::JogadorDeReferencia.elegivel(&tatico(6)));
        assert_eq!((maximo_de_dominantes(&tatico(6)), maximo_de_dominantes(&tatico(9))), (2, 3));
    }

    #[test]
    fn abilities_make_the_hire_more_expensive_and_the_cheap_one_costs_less() {
        let perfil = PerfilOlheiro { jovens: Estrelas(4), medalhoes: Estrelas(8), tatico: Estrelas(8), generalista: Estrelas(6), rede: Estrelas(5) };
        let c = |h: &[Habilidade]| custo_contratacao_com(&perfil, Some(Confederacao::Africa), 1, h);
        let base = custo_contratacao(&perfil, Some(Confederacao::Africa), 1);
        assert!((c(&[]) - base).abs() <= 10_000, "sem habilidades custa o preço de base");
        assert!(c(&[Habilidade::PerfilFisico]) > c(&[]));
        assert!(c(&[Habilidade::FitPosicional]) > c(&[Habilidade::PerfilFisico]), "o Físico é o acréscimo menor");
        assert!(c(&[Habilidade::FitPosicional, Habilidade::OlhoParaContratos]) > c(&[Habilidade::FitPosicional]));
        // a multa de rescisão acompanha
        assert!(multa_de_rescisao(&perfil, Some(Confederacao::Africa), 1, &[Habilidade::FitPosicional]) > multa_de_rescisao(&perfil, Some(Confederacao::Africa), 1, &[]));
    }

    #[test]
    fn the_market_hands_out_abilities_that_match_each_olheiros_profile() {
        let grande = clube(20, 20, 1, 21);
        let ofertas: Vec<CandidatoOlheiro> = (0..60).flat_map(|m| gerar_ofertas(atratividade(&grande), m, 0xABCDEF, &grande, &paises())).collect();
        assert!(ofertas.iter().any(|o| !o.habilidades.is_empty()), "alguns têm habilidades");
        assert!(ofertas.iter().any(|o| o.habilidades.is_empty()), "outros não têm nenhuma");
        for o in &ofertas {
            assert!(o.habilidades.iter().all(|h| h.elegivel(&o.perfil)), "{o:?}");
            assert!(o.habilidades.len() <= maximo_de_habilidades(o.perfil.tier()), "{o:?}");
        }
    }

    #[test]
    fn the_monthly_market_is_deterministic_and_bigger_clubs_see_more_and_better_olheiros() {
        let grande = clube(20, 20, 1, 21);
        let pequeno = clube(4, 1, 3, 0);
        let ofertas = |c: &PerfilClube, periodo| gerar_ofertas(atratividade(c), periodo, 0xABCDEF, c, &paises());
        assert_eq!(ofertas(&grande, 24_400), ofertas(&grande, 24_400), "mesmo mês, mesmas ofertas");
        assert_ne!(ofertas(&grande, 24_400), ofertas(&grande, 24_401), "o mercado muda no mês seguinte");
        // 24 por semana: Europa 10, América do Sul 7, Ásia 4, África 3
        assert_eq!(ofertas(&grande, 1).len(), 24);
        assert_eq!(ofertas(&pequeno, 1).len(), 24, "a quantidade não depende do clube, só a raridade");
        // em muitos meses: o clube grande vê bem mais Elites
        let elites = |c: &PerfilClube| (0..60).flat_map(|m| ofertas(c, m)).filter(|o| o.perfil.tier() == Tier::Elite).count();
        let total = |c: &PerfilClube| (0..60).flat_map(|m| ofertas(c, m)).count();
        let (eg, tg) = (elites(&grande), total(&grande));
        let (ep, tp) = (elites(&pequeno), total(&pequeno));
        assert!(eg * 100 / tg >= 15, "grande: {eg}/{tg}");
        assert!(ep * 100 / tp <= 5, "pequeno: {ep}/{tp}");
        for o in (0..60).flat_map(|m| ofertas(&grande, m)) {
            assert!(o.perfil.principal() >= Estrelas(3), "{o:?}");
            assert!(Especializacao::TODAS.iter().all(|&e| o.perfil.atributo(e) <= o.perfil.principal()));
            assert!(o.pais.is_some() && !o.mercados.is_empty(), "todo Olheiro novo tem nação e mercado");
            if o.perfil.tier() == Tier::Elite {
                assert!(o.mercados.iter().any(|m| matches!(m, Mercado::Continente(_))), "Elite conhece o continente");
            }
        }
        // na Europa, o país do clube (Espanha, 45) é o mais comum
        let europeus: Vec<_> = (0..60).flat_map(|m| ofertas(&grande, m)).filter(|o| o.pais == Some(45) || o.pais == Some(14)).collect();
        let locais = europeus.iter().filter(|o| o.pais == Some(45)).count();
        assert!(locais * 100 / europeus.len() >= 60, "{locais}/{}", europeus.len());
    }

    #[test]
    fn every_continent_has_three_to_ten_olheiros_scaled_by_its_leagues() {
        let quantos = |lista: &[PaisCandidato]| ofertas_por_continente(lista);
        let todos = quantos(&paises());
        let de = |c| todos.iter().find(|(x, _)| *x == c).map(|(_, n)| *n);
        assert_eq!(de(Confederacao::Europa), Some(10), "o continente mais forte");
        assert_eq!(de(Confederacao::AmericaDoSul), Some(7));
        assert_eq!(de(Confederacao::Asia), Some(4));
        assert_eq!(de(Confederacao::Africa), Some(3), "com poucas ligas fica no mínimo");
        assert_eq!(de(Confederacao::Oceania), None, "sem países no mercado, sem Olheiros");
        assert!(todos.iter().all(|(_, n)| (OFERTAS_CONTINENTE_MIN..=OFERTAS_CONTINENTE_MAX).contains(n)));
        // uma liga relevante sozinha vale mais que várias pequenas
        let pequenas: Vec<_> = (0..6).map(|i| PaisCandidato { id: 100 + i, continente: Confederacao::Asia, peso: peso_da_liga(3, 10) }).collect();
        let grande = vec![PaisCandidato { id: 1, continente: Confederacao::Europa, peso: peso_da_liga(1, 20) * 3 }];
        let mistura: Vec<_> = pequenas.into_iter().chain(grande).collect();
        let n = quantos(&mistura);
        assert!(n[0].1 > n[1].1 || n[0].0 == Confederacao::Europa, "{n:?}");
        assert!(peso_da_liga(1, 20) > peso_da_liga(1, 10) && peso_da_liga(1, 10) > peso_da_liga(2, 20) && peso_da_liga(2, 20) > peso_da_liga(4, 20));
    }

    #[test]
    fn every_olheiro_of_a_continent_has_a_market_in_it_and_the_ids_are_unique_per_week() {
        let c = clube(20, 20, 1, 21);
        let ofertas = gerar_ofertas(atratividade(&c), 2900, 0xABCDEF, &c, &paises());
        let indices: std::collections::HashSet<_> = ofertas.iter().map(|o| o.indice).collect();
        assert_eq!(indices.len(), ofertas.len(), "um índice (identidade) por oferta");
        for continente in [Confederacao::Europa, Confederacao::AmericaDoSul, Confederacao::Asia, Confederacao::Africa] {
            let n = ofertas.iter().filter(|o| o.mercados.iter().any(|m| m.continente() == continente)).count();
            assert!((3..=24).contains(&n), "{continente:?}: {n}");
        }
        assert_eq!(periodo_do_mercado(7), periodo_do_mercado(13) , "a mesma semana");
        assert_eq!(periodo_do_mercado(14), periodo_do_mercado(13) + 1);
        assert_eq!(inicio_do_periodo(periodo_do_mercado(20_000)) % 7, 0);
    }

    #[test]
    fn the_market_has_both_specialists_with_extremes_and_balanced_olheiros() {
        let grande = clube(20, 20, 1, 21);
        let todas: Vec<_> = (0..60).flat_map(|m| gerar_ofertas(atratividade(&grande), m, 0xABCDEF, &grande, &paises())).collect();
        // distância entre o melhor e o pior dos quatro atributos, em meias estrelas
        let abertura = |o: &CandidatoOlheiro| {
            let meias: Vec<u8> = Especializacao::TODAS.iter().map(|&e| o.perfil.atributo(e).meias()).collect();
            meias.iter().max().copied().unwrap_or(0) - meias.iter().min().copied().unwrap_or(0)
        };
        let equilibrados = todas.iter().filter(|o| abertura(o) <= 2).count();
        let extremos = todas.iter().filter(|o| abertura(o) >= 4).count();
        assert!(equilibrados * 100 / todas.len() >= 30, "equilibrados: {equilibrados}/{}", todas.len());
        assert!(extremos * 100 / todas.len() >= 25, "extremos: {extremos}/{}", todas.len());
        // o equilibrado continua com um foco, então com Tier e preço
        assert!(todas.iter().filter(|o| abertura(o) <= 2).any(|o| o.perfil.tier() == Tier::Experiente));
    }

    #[test]
    fn the_twelve_month_contract_costs_more_than_one_search_and_far_less_than_twelve() {
        for fixo in [70_000, 170_000, 700_000, 2_400_000] {
            let contrato = custo_do_contrato(fixo);
            assert!(contrato > fixo, "{fixo}: {contrato}");
            assert!(i64::from(contrato) < 12 * i64::from(fixo) / 2, "bem menos que 12 pesquisas: {fixo}: {contrato}");
            assert_eq!(contrato % 10_000, 0, "arredondado como os outros custos");
        }
        assert_eq!(custo_do_contrato(170_000), 510_000);
        assert_eq!(DIAS_CONTRATO_CONTINUO, 365);
    }

    #[test]
    fn the_rescission_fine_is_fixed_per_olheiro_and_grows_with_his_quality() {
        let junior = PerfilOlheiro::v1(Especializacao::Tatico, Tier::Junior);
        let elite = PerfilOlheiro::v1(Especializacao::Tatico, Tier::Elite);
        let multa = |p: &PerfilOlheiro| multa_de_rescisao(p, Some(Confederacao::Africa), 1, &[]);
        assert!(multa(&junior) >= 50_000, "tem piso");
        assert!(multa(&elite) > multa(&junior) * 3, "{} contra {}", multa(&elite), multa(&junior));
        assert_eq!(multa(&elite), multa(&elite), "fixa: não muda entre chamadas");
        // um quinto do que custou contratá-lo (acima do piso)
        let contratar = i64::from(custo_contratacao(&elite, Some(Confederacao::Africa), 1));
        assert!((i64::from(multa(&elite)) - contratar / 5).abs() <= 10_000);
    }

    #[test]
    fn freshness_ages_from_six_months_to_eighteen() {
        assert_eq!(frescor(0), Frescor::Atualizado);
        assert_eq!(frescor(180), Frescor::Atualizado);
        assert!(matches!(frescor(181), Frescor::Envelhecendo { extra: 1 }));
        assert!(matches!(frescor(364), Frescor::Envelhecendo { .. }));
        let um_ano = frescor(365);
        assert!(matches!(um_ano, Frescor::Desatualizado { .. }), "{um_ano:?}");
        assert!(um_ano.extra() >= 4 && um_ano.extra() < PERDA_MAXIMA);
        assert_eq!(frescor(539).extra(), PERDA_MAXIMA);
        assert_eq!(frescor(540), Frescor::Vencido);
    }

    #[test]
    fn following_closes_the_ranges_in_a_time_that_depends_on_how_specified_the_player_was() {
        assert_eq!(dias_para_exato(1, 28, 28), 5, "Qualidade Alta");
        assert_eq!(dias_para_exato(14, 6, 28), 25, "Qualidade Baixa: 25 dias");
        assert_eq!(dias_para_exato(0, 28, 28), 5, "mínimo");
        assert_eq!(precisao_acompanhada(14, 0, 184), 14);
        assert_eq!(precisao_acompanhada(14, 92, 184), 7);
        assert_eq!(precisao_acompanhada(14, 184, 184), 0);
        assert_eq!(precisao_acompanhada(14, 183, 184), 1, "só exato no fim");
        assert_eq!(atributos_acompanhados(6, 28, 0, 184), 6);
        assert_eq!(atributos_acompanhados(6, 28, 92, 184), 17);
        assert_eq!(atributos_acompanhados(6, 28, 500, 184), 28);
        assert_eq!(capacidade_acompanhamento(Estrelas(5)), 5);
        assert_eq!(capacidade_acompanhamento(Estrelas(9)), 9);
        assert_eq!(capacidade_acompanhamento(Estrelas(10)), 10);
    }

    #[test]
    fn the_scout_base_delivers_faster_the_more_the_club_already_knows() {
        assert_eq!(dias_de_curadoria(28), 0, "tudo mapeado: na hora");
        assert_eq!(dias_de_curadoria(22), 1);
        assert_eq!(dias_de_curadoria(15), 2);
        assert_eq!(dias_de_curadoria(6), 3);
        assert_eq!(dias_de_curadoria(0), 4, "quase nada: 4 dias");
        let dias: Vec<u8> = (0..=33).map(dias_de_curadoria).collect();
        assert!(dias.windows(2).all(|w| w[0] >= w[1]), "mais detalhe nunca atrasa");
        assert!(dias.iter().all(|d| *d <= 4));
    }

    #[test]
    fn false_positives_shrink_with_quality_and_near_misses_are_close() {
        assert_eq!(falsos_positivos(Qualidade::Baixa, 8), 2);
        assert_eq!(falsos_positivos(Qualidade::Media, 30), 3);
        assert_eq!(falsos_positivos(Qualidade::Media, 7), 0);
        assert_eq!(falsos_positivos(Qualidade::Alta, 44), 0);
        // titular 71, "muda patamar" pede 74+: 72 é quase, 68 não
        assert!(quase_no_nivel(NivelEquipe::MudaPatamar, 72, 72, 71));
        assert!(!quase_no_nivel(NivelEquipe::MudaPatamar, 68, 68, 71));
        assert!(quase_no_nivel(NivelEquipe::Titular, 75, 75, 71), "um pouco acima do nível titular");
    }
}
