//! `scout::search` — orquestrador de leitura do Scout (AD-3).
//!
//! Pelo AD-1, só esta camada do domínio conversa com o `save_repo`; as
//! telas falam com `scout::state`, que pede os dados aqui. Dois papéis:
//! - estado da carreira para o cabeçalho e os estados vazios (Story 1.2);
//! - a busca de uma Missão (Story 2.4): `executar_missao` lê TODOS os
//!   jogadores pelo `save_repo` (dados crus, sem filtro), aplica os filtros
//!   da Missão aqui e pede a `scout::quality` as fórmulas de escolha e de
//!   revelação. É pesada (arquivo de ~10 MB, ~39 mil jogadores): só roda
//!   dentro de um `AsyncTask` (AD-4), disparado por `scout::state`.
//!
//! Os filtros de perfil do Épico 3 (Fit Posicional, Jogador de Referência)
//! usam os valores REAIS para decidir quem entra — o Olheiro sabe o que
//! procura —, mas o que o Relatório guarda de similaridade e de fit é
//! recalculado só com o que ele revelou (`revelar`).

use std::collections::HashSet;

use uuid::Uuid;

use super::quality;
use super::state::{
    Atributo, AtributoRevelado, FaixaAtributo, FiltroPe, FiltrosMissao, JogadorEncontrado, JogadorReferencia, Missao, PosicaoAlvo,
    Relatorio,
};
use crate::async_task::AsyncTask;
use crate::save_repo::{self, DadosDoClube, Date, Liga, Nacao, PlayerPool, PlayerRaw, SaveRepoError};

/// Id do quadro "Outros" do mapa: nações dos jogadores que não existem na
/// tabela de nações (nunca somem em silêncio — Story 2.9).
pub const NACAO_OUTROS: u16 = u16::MAX;

/// O que o painel mostra da carreira ativa.
#[derive(Debug, Clone, PartialEq)]
pub struct CareerSnapshot {
    pub orcamento_transferencias: i32,
    pub data_atual: Date,
    /// "Nome Sobrenome" do técnico (`mPrV`), para o jogador reconhecer a carreira.
    pub tecnico: String,
    /// SHA-256 da identidade do save (AD-11): nome do arquivo de estado.
    pub id_save: String,
    /// Data gravada no save que o jogo carregou (ver
    /// `save_repo::read_saved_date`): o Scout desfaz o que foi feito
    /// depois dela ao ativar a carreira.
    pub data_do_save: Date,
    /// Folha salarial semanal disponível (`dqXv.wagebudget` vivo); `None`
    /// se não deu para ler (o limite de salário "do clube" fica sem teto).
    pub folha_salarial: Option<i32>,
}

/// De onde vem o estado da carreira. Em produção é o `save_repo`; nos
/// testes de `scout::state` é uma fonte falsa (sem varrer memória).
pub trait CareerSource: Send + Sync {
    /// Dispara a localização (varredura pesada) no `AsyncTask` (AD-4).
    fn start_locating(&self, task: &AsyncTask<()>) -> bool;
    /// Leitura barata (poucos bytes) do estado vivo da carreira localizada.
    fn read_snapshot(&self) -> Result<CareerSnapshot, SaveRepoError>;
    /// Dispara o sinal barato "parece haver carreira carregada?" (Story 1.7).
    fn start_career_probe(&self, task: &AsyncTask<bool>) -> bool;
    /// Escreve o orçamento vivo se ele ainda for `anterior`; devolve o
    /// valor relido (Story 1.5, ver `save_repo::write_transfer_budget`).
    fn write_transfer_budget(&self, anterior: i32, novo: i32) -> Result<i32, SaveRepoError>;
    /// Todos os jogadores do save ativo (pesado: só em `AsyncTask`).
    fn read_all_players(&self) -> Result<PlayerPool, SaveRepoError>;
    /// Nações do banco estático, para o mapa (lê disco: `AsyncTask`).
    fn read_nations(&self) -> Result<Vec<Nacao>, SaveRepoError>;
    /// Elenco do técnico (lê o save: `AsyncTask`). Por padrão, os jogadores
    /// do clube do técnico em `read_all_players`.
    fn read_squad_players(&self) -> Result<PlayerPool, SaveRepoError> {
        Ok(self.read_all_players()?.elenco())
    }
    /// Ligas com clubes, para o filtro geográfico (lê o save: `AsyncTask`).
    fn read_leagues(&self) -> Result<Vec<Liga>, SaveRepoError> {
        Ok(self.read_all_players()?.ligas)
    }
    /// Prestígio, liga e títulos do clube do técnico, para o mercado de
    /// Olheiros (lê o save: `AsyncTask`). Por padrão, desconhecido.
    fn read_club_profile(&self) -> Result<DadosDoClube, SaveRepoError> {
        Err(SaveRepoError::NaoLocalizado)
    }
}

/// Fonte real: o `save_repo` da Story 1.1.
pub struct SaveRepoSource;

impl CareerSource for SaveRepoSource {
    fn start_locating(&self, task: &AsyncTask<()>) -> bool {
        save_repo::start_locating(task)
    }

    fn start_career_probe(&self, task: &AsyncTask<bool>) -> bool {
        save_repo::start_career_probe(task)
    }

    fn write_transfer_budget(&self, anterior: i32, novo: i32) -> Result<i32, SaveRepoError> {
        save_repo::write_transfer_budget(anterior, novo)
    }

    fn read_all_players(&self) -> Result<PlayerPool, SaveRepoError> {
        save_repo::read_all_players()
    }

    fn read_nations(&self) -> Result<Vec<Nacao>, SaveRepoError> {
        save_repo::read_nations()
    }

    fn read_squad_players(&self) -> Result<PlayerPool, SaveRepoError> {
        save_repo::read_squad_players()
    }

    fn read_leagues(&self) -> Result<Vec<Liga>, SaveRepoError> {
        save_repo::read_leagues()
    }

    fn read_club_profile(&self) -> Result<DadosDoClube, SaveRepoError> {
        save_repo::read_club_profile()
    }

    fn read_snapshot(&self) -> Result<CareerSnapshot, SaveRepoError> {
        let identidade = save_repo::read_career_identity()?;
        Ok(CareerSnapshot {
            orcamento_transferencias: save_repo::read_transfer_budget()?,
            data_atual: save_repo::read_current_date()?,
            tecnico: format!("{} {}", identidade.first_name, identidade.surname)
                .trim()
                .to_string(),
            id_save: identidade.hash(),
            data_do_save: save_repo::read_saved_date()?,
            folha_salarial: save_repo::read_wage_budget().ok(),
        })
    }
}

// ---------------------------------------------------------------------
// Busca de uma Missão (Story 2.4)
// ---------------------------------------------------------------------

/// Roda a Missão: lê os jogadores e escolhe até `quantos` novos (fora de
/// `excluir`, os já encontrados), já na ORDEM DE DESCOBERTA — o Relatório
/// parcial (Story 2.10) revela essa lista aos poucos, então ela é
/// embaralhada (determinística pela Missão) para os melhores não virem
/// sempre primeiro. `hoje` é a data da carreira (idade dos jogadores).
pub fn executar_missao(
    missao: &Missao,
    fonte: &dyn CareerSource,
    hoje: Date,
    excluir: &HashSet<u32>,
    quantos: usize,
) -> Result<Vec<JogadorEncontrado>, SaveRepoError> {
    let inicio = std::time::Instant::now();
    let pool = fonte.read_all_players()?;
    let mut jogadores = escolher_jogadores(missao, &pool, hoje, excluir, quantos);
    let id = missao.id.as_u128();
    jogadores.sort_by_key(|j| quality::semente(id, j.player_id, 3));
    tracing::info!(
        "[scout::search] Missão {}: {} jogadores encontrados ({} ms, render não bloqueado).",
        missao.id,
        jogadores.len(),
        inicio.elapsed().as_millis()
    );
    Ok(jogadores)
}

/// Relatório novo, vazio, de uma Missão.
pub fn relatorio_vazio(missao: &Missao, hoje: Date) -> Relatorio {
    Relatorio {
        id: Uuid::new_v4(),
        missao_id: missao.id,
        gerado_em: Some(hoje),
        qualidade: missao.estimativa.qualidade,
        precisao_mais_menos: missao.estimativa.precisao_mais_menos,
        jogadores: Vec::new(),
        aberto: false,
        arquivado: false,
        vistos: 0,
        notificados: 0,
    }
}

/// Titulares do elenco do técnico (régua do `NivelEquipe`).
pub fn nivel_do_elenco(pool: &PlayerPool) -> quality::NivelElenco {
    let elenco: Vec<(u8, u8)> = pool
        .jogadores
        .iter()
        .filter(|j| j.clube_id.map(i64::from) == Some(pool.clube_usuario))
        .map(|j| (j.posicao, j.overall))
        .collect();
    quality::NivelElenco::de(&elenco)
}

/// Perfil de posição com que o jogador é comparado ao elenco: o alvo do
/// Fit Posicional, se houver; senão o da posição dele.
fn perfil_comparado(missao: &Missao, jogador: &PlayerRaw) -> quality::Perfil {
    missao.filtros.fit_posicional.map_or_else(|| quality::perfil_da_posicao(jogador.posicao), |alvo| alvo.perfil())
}

/// Valor estimado do jogador pelos números reais (teto de gastos).
pub fn valor_real(jogador: &PlayerRaw, hoje: Date) -> i64 {
    let goleiro = save_repo::funcao_da_posicao(jogador.posicao) == save_repo::Funcao::Goleiro;
    quality::valor_estimado(jogador.overall, jogador.potencial, jogador.idade(hoje), goleiro)
}

/// O jogador passa nos filtros da Missão (todos combinados com E)? `hoje`
/// é a data da carreira (idade e anos de contrato). Calcula a régua do
/// elenco a cada chamada: na busca, use `passa_nos_filtros_com`.
#[cfg(test)]
pub fn passa_nos_filtros(missao: &Missao, pool: &PlayerPool, jogador: &PlayerRaw, hoje: Date) -> bool {
    passa_nos_filtros_com(missao, pool, jogador, hoje, &nivel_do_elenco(pool))
}

/// `passa_nos_filtros` com a régua do elenco já calculada.
pub fn passa_nos_filtros_com(
    missao: &Missao,
    pool: &PlayerPool,
    jogador: &PlayerRaw,
    hoje: Date,
    elenco: &quality::NivelElenco,
) -> bool {
    let filtros = &missao.filtros;
    let no_mercado = !jogador.resto_do_mundo
        && jogador.clube_id.is_some()
        && jogador.clube_id.map(i64::from) != Some(pool.clube_usuario);
    let contrato = anos_de_contrato(jogador.contrato_ate, hoje);
    no_mercado
        && (filtros.overall.min..=filtros.overall.max).contains(&jogador.overall)
        && (filtros.potencial.min..=filtros.potencial.max).contains(&jogador.potencial)
        && (filtros.idade.min..=filtros.idade.max).contains(&jogador.idade(hoje))
        && (filtros.contrato.min..=filtros.contrato.max).contains(&contrato)
        && tem_dominantes(jogador, &filtros.atributos_dominantes)
        && (filtros.ritmo_ataque.is_empty() || filtros.ritmo_ataque.contains(&jogador.ritmo_ataque))
        && (filtros.ritmo_defesa.is_empty() || filtros.ritmo_defesa.contains(&jogador.ritmo_defesa))
        && (filtros.estrelas_drible.min..=filtros.estrelas_drible.max).contains(&jogador.estrelas_drible)
        && filtros.pe.is_none_or(|pe| do_pe(jogador, pe))
        && filtros.teto_valor.is_none_or(|teto| valor_real(jogador, hoje) <= teto)
        && filtros.teto_salario.is_none_or(|teto| quality::salario_estimado(jogador.overall) <= teto)
        && (filtros.posicoes.is_empty() || filtros.posicoes.contains(&quality::perfil_da_posicao(jogador.posicao)))
        && filtros.nivel_elenco.is_none_or(|nivel| {
            quality::no_nivel(nivel, jogador.overall, jogador.potencial, elenco.titular(perfil_comparado(missao, jogador)))
        })
        && do_pais(&filtros.paises, pool, jogador)
        && da_geografia(filtros, pool, jogador)
        && filtros.fit_posicional.is_none_or(|alvo| serve_no_alvo(jogador, alvo))
        && filtros.referencia.as_ref().is_none_or(|r| similaridade_real(jogador, r).is_some_and(|s| s >= quality::LIMIAR_SIMILARIDADE))
}

/// Valores reais de um jogador para as fórmulas de perfil.
fn valores(jogador: &PlayerRaw) -> impl Fn(Atributo) -> Option<f32> + '_ {
    move |a| Some(f32::from(jogador.atributo(a)))
}

/// Filtro de pé: direito/esquerdo pelo preferido; ambidestro pelo pé
/// fraco (`quality::PE_FRACO_AMBIDESTRO`).
pub fn do_pe(jogador: &PlayerRaw, pe: FiltroPe) -> bool {
    match pe {
        FiltroPe::Direito => jogador.pe == save_repo::Pe::Direito,
        FiltroPe::Esquerdo => jogador.pe == save_repo::Pe::Esquerdo,
        FiltroPe::Ambidestro => jogador.pe_fraco >= quality::PE_FRACO_AMBIDESTRO,
    }
}

/// Fit Posicional (Story 3.4): fora das posições excluídas do alvo (a
/// nativa e as mudanças triviais, `PosicaoAlvo::posicoes_excluidas`) e
/// força do fit ≥ `quality::LIMIAR_FIT`.
pub fn serve_no_alvo(jogador: &PlayerRaw, alvo: PosicaoAlvo) -> bool {
    !alvo.posicoes_excluidas().contains(&jogador.posicao)
        && quality::forca_fit(alvo, jogador.posicao, valores(jogador)).is_some_and(|f| f >= quality::LIMIAR_FIT)
}

/// Similaridade real com a referência (Story 3.3).
pub fn similaridade_real(jogador: &PlayerRaw, referencia: &JogadorReferencia) -> Option<u8> {
    quality::similaridade(valores(jogador), |a| referencia.atributo(a).map(f32::from), referencia.goleiro())
}

/// Notas dos critérios de perfil pedidos na Missão, para a relevância.
fn notas_de_perfil(missao: &Missao, jogador: &PlayerRaw) -> Vec<u8> {
    let filtros = &missao.filtros;
    // vários atributos dominantes contam como UM critério: a média deles
    let dominante = (!filtros.atributos_dominantes.is_empty()).then(|| {
        let soma: u32 = filtros.atributos_dominantes.iter().map(|&a| u32::from(jogador.atributo(a))).sum();
        u8::try_from(soma / filtros.atributos_dominantes.len() as u32).unwrap_or(u8::MAX)
    });
    let similar = filtros.referencia.as_ref().and_then(|r| similaridade_real(jogador, r));
    let no_alvo = filtros
        .fit_posicional
        .and_then(|alvo| quality::nota_no_perfil(alvo.perfil(), valores(jogador)))
        .map(|nota| nota.round().clamp(0.0, 99.0) as u8);
    [dominante, similar, no_alvo].into_iter().flatten().collect()
}

/// Anos de contrato que faltam em `hoje`: contratos terminam no fim da
/// temporada (30/06), então a temporada que vai de julho a junho termina em
/// `ano + 1` a partir de julho. 0 = termina nesta temporada; nunca passa de
/// `quality::CONTRATO_MAIOR` ("isso ou mais").
pub fn anos_de_contrato(contrato_ate: u16, hoje: Date) -> u8 {
    let fim_da_temporada = if hoje.month() >= 7 { hoje.year() + 1 } else { hoje.year() };
    let anos = (i32::from(contrato_ate) - fim_da_temporada).clamp(0, i32::from(quality::CONTRATO_MAIOR));
    u8::try_from(anos).unwrap_or(0)
}

/// Onde o jogador joga (2026-10-03): sem nada escolhido, qualquer lugar;
/// senão a liga do clube dele precisa estar numa liga, país ou continente
/// escolhido. Jogador sem liga conhecida só passa sem esse filtro.
pub fn da_geografia(filtros: &FiltrosMissao, pool: &PlayerPool, jogador: &PlayerRaw) -> bool {
    if !filtros.tem_geografia() {
        return true;
    }
    let Some(liga) = jogador.liga_id.and_then(|id| pool.ligas.iter().find(|l| l.id == id)) else {
        return false;
    };
    filtros.ligas.contains(&liga.id)
        || liga.pais.is_some_and(|p| filtros.paises_dos_clubes.contains(&p))
        || filtros.continentes.contains(&liga.continente)
}

/// Filtro legado de nacionalidade (Story 2.9; a tela não o usa mais):
/// lista vazia = todos os países; "Outros" pega as nações que não estão no
/// mapa.
pub fn do_pais(paises: &[u16], pool: &PlayerPool, jogador: &PlayerRaw) -> bool {
    if paises.is_empty() || paises.contains(&jogador.nacionalidade) {
        return true;
    }
    paises.contains(&NACAO_OUTROS) && !pool.nacoes.iter().any(|n| n.id == jogador.nacionalidade)
}

/// Todos os atributos pedidos estão entre os maiores do jogador (Story
/// 2.8; vários desde 2026-10-03, ver `quality::top_para`)? Conta só os
/// atributos da função dele: os de goleiro só para goleiros. Nenhum
/// pedido = passa.
pub fn tem_dominantes(jogador: &PlayerRaw, atributos: &[Atributo]) -> bool {
    if atributos.is_empty() {
        return true;
    }
    let goleiro = save_repo::funcao_da_posicao(jogador.posicao) == save_repo::Funcao::Goleiro;
    let valores: Vec<(Atributo, u8)> =
        Atributo::TODOS.iter().filter(|a| goleiro || !a.goleiro()).map(|&a| (a, jogador.atributo(a))).collect();
    let top = quality::top_para(atributos.len());
    atributos.iter().all(|&a| quality::eh_dominante(&valores, a, top))
}

/// A Missão com os filtros "quase" dos falsos positivos (Épico 5, item 7):
/// faixas numéricas com folga e sem o nível no elenco (que tem a folga
/// própria, `quality::quase_no_nivel`). Geografia, posições e perfil
/// continuam valendo: o Olheiro erra por pouco, não de lugar.
fn missao_folgada(missao: &Missao) -> Missao {
    let mut m = missao.clone();
    let f = &mut m.filtros;
    let folga = |x: &mut FaixaAtributo, d: u8, menor: u8, maior: u8| {
        x.min = x.min.saturating_sub(d).max(menor);
        x.max = x.max.saturating_add(d).min(maior);
    };
    folga(&mut f.overall, quality::FOLGA_OVERALL, 1, 99);
    folga(&mut f.potencial, quality::FOLGA_OVERALL, 1, 99);
    folga(&mut f.idade, quality::FOLGA_IDADE, 0, 99);
    folga(&mut f.contrato, 1, 0, quality::CONTRATO_MAIOR);
    f.teto_valor = f.teto_valor.map(|t| t * quality::FOLGA_TETO_PCT / 100);
    f.teto_salario = f.teto_salario.map(|t| t * quality::FOLGA_TETO_PCT / 100);
    f.nivel_elenco = None;
    m
}

/// Escolhe até `quantos` candidatos (fora de `excluir`) e revela cada um
/// conforme a Qualidade da Missão. Determinístico para a mesma Missão.
/// Abaixo da Qualidade Alta, `quality::falsos_positivos` das vagas vão para
/// jogadores que quase passam no filtro (marcados só no arquivo); sem
/// "quase" suficientes, as vagas ficam com os que passam.
pub fn escolher_jogadores(
    missao: &Missao,
    pool: &PlayerPool,
    hoje: Date,
    excluir: &HashSet<u32>,
    quantos: usize,
) -> Vec<JogadorEncontrado> {
    let id = missao.id.as_u128();
    let qualidade = missao.estimativa.qualidade;
    let elenco = nivel_do_elenco(pool);
    let folgada = missao_folgada(missao);
    let querem_falsos = quality::falsos_positivos(qualidade, quantos) > 0;
    let nota = |j: &PlayerRaw| {
        let relevancia = quality::relevancia(missao.tipo, j.overall, j.potencial, &notas_de_perfil(missao, j));
        quality::nota_de_escolha(relevancia, qualidade, quality::semente(id, j.player_id, 0))
    };
    let mut candidatos: Vec<(u32, &PlayerRaw)> = Vec::new();
    let mut quase: Vec<(u32, &PlayerRaw)> = Vec::new();
    for j in pool.jogadores.iter().filter(|j| !excluir.contains(&j.player_id)) {
        if passa_nos_filtros_com(missao, pool, j, hoje, &elenco) {
            candidatos.push((nota(j), j));
        } else if querem_falsos
            && passa_nos_filtros_com(&folgada, pool, j, hoje, &elenco)
            && missao.filtros.nivel_elenco.is_none_or(|nivel| {
                quality::quase_no_nivel(nivel, j.overall, j.potencial, elenco.titular(perfil_comparado(missao, j)))
            })
        {
            quase.push((nota(j), j));
        }
    }
    // maior nota primeiro; empate pelo id (ordem estável e determinística)
    let ordem = |a: &(u32, &PlayerRaw), b: &(u32, &PlayerRaw)| b.0.cmp(&a.0).then(a.1.player_id.cmp(&b.1.player_id));
    candidatos.sort_by(ordem);
    quase.sort_by(ordem);
    let falsos = quality::falsos_positivos(qualidade, quantos).min(quase.len());
    let verdadeiros = quantos.saturating_sub(falsos);
    candidatos
        .iter()
        .take(verdadeiros)
        .map(|(_, j)| (*j, false))
        .chain(quase.iter().take(falsos).map(|(_, j)| (*j, true)))
        .map(|(j, falso)| {
            let mut encontrado = revelar(missao, pool, hoje, j);
            encontrado.titular_elenco = Some(elenco.titular(perfil_comparado(missao, j)));
            encontrado.falso_positivo = falso;
            encontrado
        })
        .collect()
}

/// Nova observação de um jogador da Lista de Escolhidos pelo acompanhamento
/// (Épico 6): faixas com a `precisao` de agora, a partir dos valores atuais
/// do save, e os `atributos` primeiros na ordem de observação. Bio, clube e
/// contrato também se atualizam (o Olheiro acompanha o jogador). Nunca
/// guarda o valor real além do que a precisão revela.
pub fn reobservar(
    anterior: &JogadorEncontrado,
    jogador: &PlayerRaw,
    pool: &PlayerPool,
    hoje: Date,
    precisao: u8,
    atributos: usize,
    alvo: Option<PosicaoAlvo>,
    referencia: Option<&JogadorReferencia>,
) -> JogadorEncontrado {
    let pid = jogador.player_id;
    let base = (u128::from(pid) << 64) | 0xE5C0;
    let funcao = save_repo::funcao_da_posicao(jogador.posicao);
    // o que já tinha sido observado (os atributos pedidos na Missão vêm
    // primeiro lá) continua na frente; depois, a ordem da função
    let ja_vistos: Vec<Atributo> = anterior.atributos.iter().map(|a| a.atributo).collect();
    let mut ordem = ja_vistos.clone();
    ordem.extend(quality::ordem_de_observacao(funcao, &ja_vistos, alvo).into_iter().filter(|a| !ja_vistos.contains(a)));
    let atributos = ordem
        .into_iter()
        .take(atributos)
        .map(|atributo| {
            let canal = 100 + u32::try_from(atributo.indice()).unwrap_or(0);
            AtributoRevelado {
                atributo,
                valor: quality::faixa_revelada(jogador.atributo(atributo), precisao, quality::semente(base, pid, canal)),
            }
        })
        .collect();
    let nacao = pool.nacoes.iter().find(|n| n.id == jogador.nacionalidade);
    let mut novo = JogadorEncontrado {
        player_id: pid,
        nome: jogador.nome.clone(),
        idade: jogador.idade(hoje),
        posicao: jogador.posicao,
        nacao_id: jogador.nacionalidade,
        nacao: nacao.map(|n| n.nome.clone()).unwrap_or_else(|| anterior.nacao.clone()),
        clube: jogador.clube.clone(),
        contrato_ate: jogador.clube_id.map(|_| jogador.contrato_ate),
        observacao: Default::default(),
        overall: quality::faixa_revelada(jogador.overall, precisao, quality::semente(base, pid, 1)),
        potencial: quality::faixa_revelada(jogador.potencial, precisao, quality::semente(base, pid, 2)),
        atributos,
        pe: Some(jogador.pe),
        similaridade: None,
        fit: None,
        variacao_overall: None,
        ritmo_ataque: Some(jogador.ritmo_ataque),
        ritmo_defesa: Some(jogador.ritmo_defesa),
        estrelas_drible: Some(jogador.estrelas_drible),
        pe_fraco: Some(jogador.pe_fraco),
        titular_elenco: anterior.titular_elenco,
        falso_positivo: anterior.falso_positivo,
    };
    let visto = |a: Atributo| novo.valor_visto(a);
    let fit = alvo.and_then(|alvo| quality::forca_fit(alvo, jogador.posicao, visto));
    let variacao = alvo.and_then(|alvo| quality::variacao_overall(alvo, jogador.posicao, visto));
    let similaridade = referencia.and_then(|r| quality::similaridade(visto, |a| r.atributo(a).map(f32::from), r.goleiro()));
    novo.fit = fit;
    novo.variacao_overall = variacao;
    novo.similaridade = similaridade;
    novo
}

/// O que o Relatório mostra de um jogador: faixas de Overall/Potencial e
/// os `atributos_revelados` primeiros atributos que o Olheiro observou.
pub fn revelar(missao: &Missao, pool: &PlayerPool, hoje: Date, jogador: &PlayerRaw) -> JogadorEncontrado {
    let id = missao.id.as_u128();
    let pid = jogador.player_id;
    let precisao = missao.estimativa.precisao_mais_menos;
    let funcao = save_repo::funcao_da_posicao(jogador.posicao);
    let filtros = &missao.filtros;
    let atributos = quality::ordem_de_observacao(funcao, &filtros.atributos_dominantes, filtros.fit_posicional)
        .into_iter()
        .take(usize::from(missao.estimativa.atributos_revelados))
        .map(|atributo| {
            let canal = 100 + u32::try_from(atributo.indice()).unwrap_or(0);
            AtributoRevelado {
                atributo,
                valor: quality::faixa_revelada(jogador.atributo(atributo), precisao, quality::semente(id, pid, canal)),
            }
        })
        .collect();
    let nacao = pool.nacoes.iter().find(|n| n.id == jogador.nacionalidade);
    let mut encontrado = JogadorEncontrado {
        player_id: pid,
        nome: jogador.nome.clone(),
        idade: jogador.idade(hoje),
        posicao: jogador.posicao,
        nacao_id: jogador.nacionalidade,
        nacao: nacao.map(|n| n.nome.clone()).unwrap_or_else(|| "Outros".to_string()),
        clube: jogador.clube.clone(),
        contrato_ate: jogador.clube_id.map(|_| jogador.contrato_ate),
        observacao: Default::default(),
        overall: quality::faixa_revelada(jogador.overall, precisao, quality::semente(id, pid, 1)),
        potencial: quality::faixa_revelada(jogador.potencial, precisao, quality::semente(id, pid, 2)),
        atributos,
        pe: Some(jogador.pe),
        similaridade: None,
        fit: None,
        variacao_overall: None,
        ritmo_ataque: Some(jogador.ritmo_ataque),
        ritmo_defesa: Some(jogador.ritmo_defesa),
        estrelas_drible: Some(jogador.estrelas_drible),
        pe_fraco: Some(jogador.pe_fraco),
        titular_elenco: None,
        falso_positivo: false,
    };
    // Similaridade e fit "pelo que o Olheiro viu" (nunca o valor real).
    let visto = |a: Atributo| encontrado.valor_visto(a);
    let similaridade = filtros
        .referencia
        .as_ref()
        .and_then(|r| quality::similaridade(visto, |a| r.atributo(a).map(f32::from), r.goleiro()));
    let fit = filtros.fit_posicional.and_then(|alvo| quality::forca_fit(alvo, jogador.posicao, visto));
    let variacao = filtros.fit_posicional.and_then(|alvo| quality::variacao_overall(alvo, jogador.posicao, visto));
    encontrado.similaridade = similaridade;
    encontrado.fit = fit;
    encontrado.variacao_overall = variacao;
    encontrado
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::save_repo::jogadores::TOTAL_ATRIBUTOS;
    use crate::save_repo::{Confederacao, Nacao};
    use crate::scout::state::{FaixaAtributo, NivelEquipe, StatusMissao};

    /// Jogador sintético com todos os atributos em `nivel`.
    pub fn jogador(player_id: u32, overall: u8, potencial: u8, posicao: u8) -> PlayerRaw {
        PlayerRaw {
            player_id,
            nome: format!("Jogador {player_id}"),
            nascimento: Date(20100101),
            posicao,
            nacionalidade: 54,
            overall,
            potencial,
            atributos: [overall; TOTAL_ATRIBUTOS],
            clube_id: Some(10),
            clube: "Clube".to_string(),
            resto_do_mundo: false,
            pe: crate::save_repo::Pe::Direito,
            liga_id: Some(13),
            contrato_ate: 2028,
            ritmo_ataque: crate::save_repo::RitmoTrabalho::Medio,
            ritmo_defesa: crate::save_repo::RitmoTrabalho::Medio,
            estrelas_drible: 3,
            pe_fraco: 3,
        }
    }

    pub fn pool(jogadores: Vec<PlayerRaw>) -> PlayerPool {
        PlayerPool {
            jogadores,
            nacoes: vec![
                Nacao { id: 52, nome: "Argentina".to_string(), iso: "AR".to_string(), confederacao: Confederacao::AmericaDoSul },
                Nacao { id: 54, nome: "Brazil".to_string(), iso: "BR".to_string(), confederacao: Confederacao::AmericaDoSul },
            ],
            clube_usuario: 241,
            ligas: vec![
                liga(13, Some(14), Confederacao::Europa),
                liga(53, Some(45), Confederacao::Europa),
                liga(7, Some(54), Confederacao::AmericaDoSul),
                liga(77, None, Confederacao::Europa),
            ],
        }
    }

    pub fn liga(id: u32, pais: Option<u16>, continente: Confederacao) -> Liga {
        Liga { id, nome: format!("Liga {id}"), pais, pais_nome: String::new(), continente, nivel: 1, clubes: 20 }
    }

    /// Data de carreira dos testes (o `jogador` sintético nasceu em 2010).
    const HOJE: Date = Date(20260801);

    fn missao_com(overall: (u8, u8), potencial: (u8, u8)) -> Missao {
        let mut m = Missao::de_teste(Uuid::new_v4(), StatusMissao::EmExecucao);
        m.filtros.overall = FaixaAtributo { min: overall.0, max: overall.1 };
        m.filtros.potencial = FaixaAtributo { min: potencial.0, max: potencial.1 };
        m
    }

    #[test]
    fn filters_combine_overall_and_potencial_and_skip_the_users_club_and_rest_of_world() {
        let mut meu = jogador(1, 70, 80, 24);
        meu.clube_id = Some(241);
        let mut generico = jogador(2, 70, 80, 24);
        generico.resto_do_mundo = true;
        let mut sem_clube = jogador(3, 70, 80, 24);
        sem_clube.clube_id = None;
        let p = pool(vec![meu, generico, sem_clube, jogador(4, 70, 80, 24), jogador(5, 69, 80, 24), jogador(6, 70, 90, 24)]);
        let m = missao_com((70, 75), (75, 85));
        let ids: Vec<u32> = p.jogadores.iter().filter(|j| passa_nos_filtros(&m, &p, j, HOJE)).map(|j| j.player_id).collect();
        assert_eq!(ids, vec![4]);
    }

    #[test]
    fn the_report_respects_the_target_count_quality_and_never_reveals_exact_values_it_should_not() {
        let jogadores = (1..=200).map(|i| jogador(i, 60 + (i % 30) as u8, 70 + (i % 25) as u8, (i % 27) as u8)).collect();
        let p = pool(jogadores);
        let m = missao_com((50, 99), (50, 99));
        let lista = escolher_jogadores(&m, &p, Date(20260801), &HashSet::new(), usize::from(m.estimativa.alvo_jogadores));
        assert_eq!(lista.len(), usize::from(m.estimativa.alvo_jogadores));
        let precisao = m.estimativa.precisao_mais_menos;
        for j in &lista {
            let real = p.jogadores.iter().find(|r| r.player_id == j.player_id).expect("jogador do pool");
            assert!(j.overall.min <= real.overall && real.overall <= j.overall.max);
            assert!(j.overall.max - j.overall.min <= 2 * precisao);
            assert_eq!(j.atributos.len(), usize::from(m.estimativa.atributos_revelados));
            for a in &j.atributos {
                let v = real.atributo(a.atributo);
                assert!(a.valor.min <= v && v <= a.valor.max);
            }
            assert_eq!(j.nacao, "Brazil");
        }
        // determinístico
        let de_novo = escolher_jogadores(&m, &p, Date(20260801), &HashSet::new(), usize::from(m.estimativa.alvo_jogadores));
        assert_eq!(lista, de_novo);
    }

    #[test]
    fn excluded_players_never_come_back_and_a_small_pool_gives_fewer_players() {
        let p = pool((1..=5).map(|i| jogador(i, 70, 80, 24)).collect());
        let m = missao_com((50, 99), (50, 99));
        let excluir: HashSet<u32> = [1, 2].into_iter().collect();
        let lista = escolher_jogadores(&m, &p, Date(20260801), &excluir, 30);
        let ids: HashSet<u32> = lista.iter().map(|j| j.player_id).collect();
        assert_eq!(ids, [3, 4, 5].into_iter().collect());
    }

    #[test]
    fn search_module_receives_no_filter_from_save_repo_and_passes_none_to_it() {
        // AD-3: `save_repo::read_all_players` não recebe critério de filtro.
        let fonte = include_str!("search.rs");
        let codigo = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        assert!(codigo.contains("fonte.read_all_players()?"));
    }

    #[test]
    fn the_dominant_attribute_filter_combines_with_the_ranges_and_ranks_by_it() {
        let driblador = |id: u32, drible: u8, overall: u8| {
            let mut j = jogador(id, overall, overall + 5, 24);
            j.atributos = [50; TOTAL_ATRIBUTOS];
            j.atributos[Atributo::Drible.indice()] = drible;
            j
        };
        let mut defensor = jogador(9, 70, 75, 5);
        defensor.atributos = [60; TOTAL_ATRIBUTOS];
        defensor.atributos[Atributo::Marcacao.indice()] = 85;
        defensor.atributos[Atributo::Drible.indice()] = 40;
        let p = pool(vec![driblador(1, 80, 70), driblador(2, 92, 72), driblador(3, 95, 60), defensor]);
        let mut m = missao_com((65, 80), (50, 99));
        m.filtros.atributos_dominantes = vec![Atributo::Drible];
        let ids: Vec<u32> = p.jogadores.iter().filter(|j| passa_nos_filtros(&m, &p, j, HOJE)).map(|j| j.player_id).collect();
        assert_eq!(ids, vec![1, 2], "o 3 sai pelo Overall, o 9 não dribla");
        m.estimativa.qualidade = crate::scout::state::Qualidade::Alta;
        let lista = escolher_jogadores(&m, &p, Date(20260801), &HashSet::new(), 2);
        assert_eq!(lista.first().map(|j| j.player_id), Some(2), "o melhor driblador primeiro");
        assert_eq!(lista[0].atributos.first().map(|a| a.atributo), Some(Atributo::Drible), "observado primeiro");
    }

    #[test]
    fn the_country_filter_keeps_only_the_chosen_nations_and_others_is_never_dropped() {
        let mut argentino = jogador(2, 70, 75, 24);
        argentino.nacionalidade = 52;
        let mut sem_mapa = jogador(3, 70, 75, 24);
        sem_mapa.nacionalidade = 999;
        let p = pool(vec![jogador(1, 70, 75, 24), argentino, sem_mapa]);
        let mut m = missao_com((50, 99), (50, 99));
        let ids = |m: &Missao| -> Vec<u32> { p.jogadores.iter().filter(|j| passa_nos_filtros(m, &p, j, HOJE)).map(|j| j.player_id).collect() };
        assert_eq!(ids(&m), vec![1, 2, 3], "nenhum país = todos");
        m.filtros.paises = vec![54];
        assert_eq!(ids(&m), vec![1]);
        m.filtros.paises = vec![54, NACAO_OUTROS];
        assert_eq!(ids(&m), vec![1, 3], "Outros pega quem não tem quadro no mapa");
    }

    // -----------------------------------------------------------------
    // Épico 3
    // -----------------------------------------------------------------

    /// Meia-atacante (18) com passe/drible altos e defesa em `defesa`.
    fn meia(id: u32, defesa: u8) -> PlayerRaw {
        use Atributo::*;
        let mut j = jogador(id, 75, 78, 18);
        j.atributos = [50; TOTAL_ATRIBUTOS];
        for a in [PasseCurto, PasseLongo, Visao, ControleDeBola, Drible, PosicionamentoOfensivo, Agilidade, Reacao] {
            j.atributos[a.indice()] = 82;
        }
        for a in [Interceptacao, DesarmeEmPe, Marcacao, Carrinho, Agressividade, Folego, Forca] {
            j.atributos[a.indice()] = defesa;
        }
        j
    }

    #[test]
    fn fit_keeps_other_positions_whose_profile_serves_the_target() {
        let mut volante_nato = meia(3, 82);
        volante_nato.posicao = 10; // já é VOL: não é um "fit"
        let p = pool(vec![meia(1, 80), meia(2, 40), volante_nato]);
        let mut m = missao_com((50, 99), (50, 99));
        m.filtros.fit_posicional = Some(PosicaoAlvo::Volante);
        let ids: Vec<u32> = p.jogadores.iter().filter(|j| passa_nos_filtros(&m, &p, j, HOJE)).map(|j| j.player_id).collect();
        assert_eq!(ids, vec![1], "o 2 não defende; o 3 já é volante");
        // combina (E) com as faixas
        m.filtros.overall = FaixaAtributo { min: 80, max: 99 };
        assert!(p.jogadores.iter().all(|j| !passa_nos_filtros(&m, &p, j, HOJE)));
    }

    #[test]
    fn the_report_stores_fit_and_similarity_from_what_was_revealed_only() {
        let meu_meia = meia(50, 80);
        let referencia = JogadorReferencia {
            player_id: 50,
            nome: "Meu Meia".to_string(),
            posicao: 18,
            atributos: meu_meia.atributos.to_vec(),
        };
        let mut gemeo = meia(1, 80);
        gemeo.pe = crate::save_repo::Pe::Esquerdo;
        let p = pool(vec![gemeo, meia(2, 40), jogador(3, 75, 78, 5)]);
        let mut m = missao_com((50, 99), (50, 99));
        m.filtros.referencia = Some(referencia.clone());
        m.filtros.fit_posicional = Some(PosicaoAlvo::Volante);
        let ids: Vec<u32> = p.jogadores.iter().filter(|j| passa_nos_filtros(&m, &p, j, HOJE)).map(|j| j.player_id).collect();
        assert_eq!(ids, vec![1], "referência E fit, juntos");
        assert_eq!(similaridade_real(&p.jogadores[0], &referencia), Some(100));

        let lista = escolher_jogadores(&m, &p, Date(20260801), &HashSet::new(), 5);
        let j = lista.first().expect("um jogador");
        assert_eq!(j.pe, Some(crate::save_repo::Pe::Esquerdo));
        // Qualidade baixa da Missão de teste: poucos atributos, faixas largas
        assert!(j.atributos.len() < 28);
        let visto = j.similaridade.expect("similaridade pelo revelado");
        assert!(visto <= 100);
        let fit = j.fit.expect("fit pelo revelado");
        assert!(fit > 0);
        assert!(j.variacao_overall.is_some(), "estimativa de Overall na posição-alvo");
        assert_eq!((j.estrelas_drible, j.pe_fraco), (Some(3), Some(3)));
        // os atributos do alvo vêm logo no começo da observação
        assert_eq!(j.atributos.first().map(|a| a.atributo), Some(Atributo::PasseCurto));
    }

    #[test]
    fn similar_profiles_rank_first() {
        let referencia = meia(50, 80);
        let r = JogadorReferencia { player_id: 50, nome: "R".to_string(), posicao: 18, atributos: referencia.atributos.to_vec() };
        let mut quase = meia(2, 74);
        quase.atributos[Atributo::Visao.indice()] = 76;
        let p = pool(vec![quase, meia(1, 80)]);
        let mut m = missao_com((50, 99), (50, 99));
        m.filtros.referencia = Some(r);
        m.estimativa.qualidade = crate::scout::state::Qualidade::Alta;
        let ordem: Vec<u32> = {
            let mut c: Vec<(u8, u32)> = p
                .jogadores
                .iter()
                .filter(|j| passa_nos_filtros(&m, &p, j, HOJE))
                .map(|j| (quality::relevancia(m.tipo, j.overall, j.potencial, &notas_de_perfil(&m, j)), j.player_id))
                .collect();
            c.sort_by(|a, b| b.cmp(a));
            c.into_iter().map(|(_, id)| id).collect()
        };
        assert_eq!(ordem, vec![1, 2], "o gêmeo antes do parecido");
    }

    /// Calibração dos limiares com o save versionado (rodar à mão:
    /// `cargo test --release calibracao -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn calibracao_dos_limiares_no_save_real() {
        use crate::save_repo::jogadores::{ler_de_arquivos, pasta_do_jogo};
        let save = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../save_backups/717036e3_20260913_172921/DATA");
        let banco = pasta_do_jogo().join("data/db/fifa_ng_db.db");
        let (jogadores, _) = ler_de_arquivos(&save, &banco).expect("save legível");
        let ativos: Vec<&PlayerRaw> = jogadores.iter().filter(|j| j.overall >= 60).collect();
        for alvo in PosicaoAlvo::TODAS {
            let fits: Vec<u8> = ativos
                .iter()
                .filter(|j| !alvo.posicoes_excluidas().contains(&j.posicao) && j.posicao != 0)
                .filter_map(|j| quality::forca_fit(alvo, j.posicao, valores(j)))
                .collect();
            let passam = fits.iter().filter(|&&f| f >= quality::LIMIAR_FIT).count();
            let em = |l: u8| fits.iter().filter(|&&f| f >= l).count();
            eprintln!("{:<18} {passam:>5} de {:>5} passam (≥{}) · ≥95 {} · ≥98 {} · 100 {}", alvo.nome(), fits.len(), quality::LIMIAR_FIT, em(95), em(98), em(100));
        }
        let mut melhores: Vec<&PlayerRaw> = ativos.clone();
        melhores.sort_by_key(|j| std::cmp::Reverse(j.overall));
        let referencias: Vec<&PlayerRaw> = [0u8, 4, 8, 16, 24].iter().filter_map(|&p| melhores.iter().find(|j| j.posicao == p).copied()).collect();
        for r in referencias {
            let nome = r.nome.as_str();
            let referencia = JogadorReferencia { player_id: r.player_id, nome: r.nome.clone(), posicao: r.posicao, atributos: r.atributos.to_vec() };
            let mut notas: Vec<(u8, &str)> = ativos
                .iter()
                .filter(|j| j.player_id != r.player_id)
                .filter_map(|j| Some((similaridade_real(j, &referencia)?, j.nome.as_str())))
                .collect();
            notas.sort_by(|a, b| b.cmp(a));
            let passam = notas.iter().filter(|(s, _)| *s >= quality::LIMIAR_SIMILARIDADE).count();
            eprintln!("{nome}: {passam} passam (≥{}); top 5 {:?}", quality::LIMIAR_SIMILARIDADE, notas.iter().take(5).collect::<Vec<_>>());
        }
    }

    // -----------------------------------------------------------------
    // Ajustes de 2026-10-03: idade, contrato, geografia por liga, vários
    // atributos dominantes
    // -----------------------------------------------------------------

    #[test]
    fn age_and_contract_ranges_filter_on_the_career_date() {
        let mut novo = jogador(1, 70, 75, 24);
        novo.nascimento = Date(20080101); // 18 anos em 01/08/2026
        let mut veterano = jogador(2, 70, 75, 24);
        veterano.nascimento = Date(19940101); // 32
        veterano.contrato_ate = 2027; // termina nesta temporada (26/27)
        let p = pool(vec![novo, veterano]);
        let mut m = missao_com((50, 99), (50, 99));
        let ids = |m: &Missao| -> Vec<u32> { p.jogadores.iter().filter(|j| passa_nos_filtros(m, &p, j, HOJE)).map(|j| j.player_id).collect() };
        assert_eq!(ids(&m), vec![1, 2], "padrão não restringe");
        m.filtros.idade = FaixaAtributo { min: 15, max: 21 };
        assert_eq!(ids(&m), vec![1]);
        m.filtros.idade = FaixaAtributo { min: 15, max: 45 };
        m.filtros.contrato = FaixaAtributo { min: 0, max: 0 };
        assert_eq!(ids(&m), vec![2], "só quem fica livre no fim da temporada");
    }

    #[test]
    fn contract_years_count_from_the_end_of_the_current_season() {
        assert_eq!(anos_de_contrato(2027, Date(20260801)), 0, "temporada 26/27");
        assert_eq!(anos_de_contrato(2027, Date(20270301)), 0);
        assert_eq!(anos_de_contrato(2028, Date(20260801)), 1);
        assert_eq!(anos_de_contrato(2026, Date(20260801)), 0, "vencido conta como 0");
        assert_eq!(anos_de_contrato(2040, Date(20260801)), quality::CONTRATO_MAIOR, "5 ou mais");
    }

    #[test]
    fn geography_matches_league_country_or_continent_of_the_club() {
        let mut ingles = jogador(1, 70, 75, 24);
        ingles.liga_id = Some(13);
        let mut espanhol = jogador(2, 70, 75, 24);
        espanhol.liga_id = Some(53);
        let mut brasileiro = jogador(3, 70, 75, 24);
        brasileiro.liga_id = Some(7);
        let mut sem_liga = jogador(4, 70, 75, 24);
        sem_liga.liga_id = None;
        let mut da_uefa = jogador(5, 70, 75, 24);
        da_uefa.liga_id = Some(77);
        let p = pool(vec![ingles, espanhol, brasileiro, sem_liga, da_uefa]);
        let mut m = missao_com((50, 99), (50, 99));
        let ids = |m: &Missao| -> Vec<u32> { p.jogadores.iter().filter(|j| passa_nos_filtros(m, &p, j, HOJE)).map(|j| j.player_id).collect() };
        assert_eq!(ids(&m), vec![1, 2, 3, 4, 5], "sem geografia = todos");
        m.filtros.ligas = vec![13];
        assert_eq!(ids(&m), vec![1]);
        m.filtros.paises_dos_clubes = vec![54];
        assert_eq!(ids(&m), vec![1, 3], "liga OU país");
        m.filtros = FiltrosMissao { continentes: vec![Confederacao::Europa], ..m.filtros.clone() };
        m.filtros.ligas.clear();
        m.filtros.paises_dos_clubes.clear();
        assert_eq!(ids(&m), vec![1, 2, 5], "continente inclui clubes da UEFA");
    }

    #[test]
    fn several_dominant_attributes_must_all_be_near_the_top() {
        let rapido_driblador = {
            let mut j = jogador(1, 75, 78, 24);
            j.atributos = [50; TOTAL_ATRIBUTOS];
            j.atributos[Atributo::Velocidade.indice()] = 90;
            j.atributos[Atributo::Aceleracao.indice()] = 89;
            j.atributos[Atributo::Finalizacao.indice()] = 88;
            j.atributos[Atributo::Drible.indice()] = 87; // 4º maior
            j
        };
        let p = pool(vec![rapido_driblador]);
        let mut m = missao_com((50, 99), (50, 99));
        m.filtros.atributos_dominantes = vec![Atributo::Drible];
        assert!(!passa_nos_filtros(&m, &p, &p.jogadores[0], HOJE), "sozinho, top 3");
        m.filtros.atributos_dominantes = vec![Atributo::Velocidade, Atributo::Drible];
        assert!(passa_nos_filtros(&m, &p, &p.jogadores[0], HOJE), "dois pedidos, top 4");
        m.filtros.atributos_dominantes = vec![Atributo::Velocidade, Atributo::Marcacao];
        assert!(!passa_nos_filtros(&m, &p, &p.jogadores[0], HOJE), "todos precisam passar");
        // os pedidos são observados primeiro
        m.estimativa.qualidade = crate::scout::state::Qualidade::Alta;
        m.filtros.atributos_dominantes = vec![Atributo::Velocidade, Atributo::Drible];
        let lista = escolher_jogadores(&m, &p, HOJE, &HashSet::new(), 1);
        let primeiros: Vec<Atributo> = lista[0].atributos.iter().take(2).map(|a| a.atributo).collect();
        assert_eq!(primeiros, [Atributo::Velocidade, Atributo::Drible]);
    }

    #[test]
    fn work_rate_dribble_stars_and_foot_filter_together() {
        use crate::save_repo::{Pe, RitmoTrabalho};
        let mut box_to_box = jogador(1, 70, 75, 14);
        box_to_box.ritmo_ataque = RitmoTrabalho::Alto;
        box_to_box.ritmo_defesa = RitmoTrabalho::Alto;
        box_to_box.estrelas_drible = 4;
        box_to_box.pe = Pe::Esquerdo;
        box_to_box.pe_fraco = 2;
        let mut ambidestro = jogador(2, 70, 75, 14);
        ambidestro.ritmo_defesa = RitmoTrabalho::Baixo;
        ambidestro.estrelas_drible = 2;
        ambidestro.pe_fraco = 5;
        let p = pool(vec![box_to_box, ambidestro]);
        let mut m = missao_com((50, 99), (50, 99));
        let ids = |m: &Missao| -> Vec<u32> { p.jogadores.iter().filter(|j| passa_nos_filtros(m, &p, j, HOJE)).map(|j| j.player_id).collect() };
        assert_eq!(ids(&m), vec![1, 2], "padrão não restringe");
        m.filtros.ritmo_defesa = vec![RitmoTrabalho::Alto, RitmoTrabalho::Medio];
        assert_eq!(ids(&m), vec![1]);
        m.filtros.ritmo_defesa.clear();
        m.filtros.estrelas_drible = FaixaAtributo { min: 4, max: 5 };
        assert_eq!(ids(&m), vec![1]);
        m.filtros.estrelas_drible = FaixaAtributo { min: 1, max: 5 };
        m.filtros.pe = Some(FiltroPe::Ambidestro);
        assert_eq!(ids(&m), vec![2], "pé fraco 4+ estrelas");
        m.filtros.pe = Some(FiltroPe::Esquerdo);
        assert_eq!(ids(&m), vec![1]);
    }

    #[test]
    fn fit_never_brings_trivial_moves_like_a_left_back_for_right_back() {
        let lateral = |id: u32, posicao: u8| {
            let mut j = jogador(id, 75, 78, posicao);
            j.atributos = [80; TOTAL_ATRIBUTOS];
            j
        };
        // perfis idênticos: força 100 para qualquer alvo
        let p = pool(vec![lateral(1, 7), lateral(2, 8), lateral(3, 5), lateral(4, 10)]);
        let mut m = missao_com((50, 99), (50, 99));
        m.filtros.fit_posicional = Some(PosicaoAlvo::LateralDireito);
        let ids: Vec<u32> = p.jogadores.iter().filter(|j| passa_nos_filtros(&m, &p, j, HOJE)).map(|j| j.player_id).collect();
        assert_eq!(ids, vec![4], "LE, ALE e zagueiro ficam de fora; o volante pode virar lateral");
    }

    #[test]
    fn the_spending_cap_and_the_team_level_filter_by_default() {
        // elenco do técnico (clube 241): centroavante 71
        let mut meu = jogador(1, 71, 71, 25);
        meu.clube_id = Some(241);
        meu.nascimento = Date(19980101);
        let craque = {
            let mut j = jogador(2, 90, 92, 25);
            j.nascimento = Date(20000101);
            j
        };
        let reforco = {
            let mut j = jogador(3, 74, 76, 25);
            j.nascimento = Date(20000101);
            j
        };
        let reserva = {
            let mut j = jogador(4, 66, 68, 25);
            j.nascimento = Date(20000101);
            j
        };
        let p = pool(vec![meu, craque, reforco, reserva]);
        let mut m = missao_com((40, 99), (40, 99));
        let ids = |m: &Missao| -> Vec<u32> { p.jogadores.iter().filter(|j| passa_nos_filtros(m, &p, j, HOJE)).map(|j| j.player_id).collect() };
        m.filtros.nivel_elenco = Some(NivelEquipe::MudaPatamar);
        assert_eq!(ids(&m), vec![2, 3], "74+ muda o patamar de um time com titular 71");
        // orçamento de 15 M: o craque (≈ 100 M) fica de fora
        m.filtros.teto_valor = Some(15_000_000);
        assert_eq!(ids(&m), vec![3]);
        m.filtros.nivel_elenco = Some(NivelEquipe::Banco);
        assert_eq!(ids(&m), vec![4]);
        // e o Relatório guarda o titular usado na comparação
        m.estimativa.qualidade = crate::scout::state::Qualidade::Alta;
        let lista = escolher_jogadores(&m, &p, HOJE, &HashSet::new(), 5);
        assert_eq!(lista.first().and_then(|j| j.titular_elenco), Some(71));
    }

    #[test]
    fn wage_cap_and_position_groups_filter_too() {
        let p = pool(vec![jogador(1, 88, 88, 25), jogador(2, 70, 72, 25), jogador(3, 70, 72, 5)]);
        let mut m = missao_com((40, 99), (40, 99));
        let ids = |m: &Missao| -> Vec<u32> { p.jogadores.iter().filter(|j| passa_nos_filtros(m, &p, j, HOJE)).map(|j| j.player_id).collect() };
        m.filtros.teto_salario = Some(165_000);
        assert_eq!(ids(&m), vec![2, 3], "Overall 88 pede mais que a folha");
        m.filtros.posicoes = vec![crate::scout::state::Perfil::Centroavante];
        assert_eq!(ids(&m), vec![2]);
    }

    // -----------------------------------------------------------------
    // Épicos 5 e 6: falsos positivos e reobservação dos Escolhidos
    // -----------------------------------------------------------------

    #[test]
    fn low_quality_reports_bring_some_near_misses_flagged_only_in_the_file() {
        // 20 que passam (Overall 70) e 20 "quase" (Overall 67, filtro 68+)
        let jogadores = (1..=20).map(|i| jogador(i, 70, 75, 24)).chain((21..=40).map(|i| jogador(i, 67, 75, 24))).collect();
        let p = pool(jogadores);
        let mut m = missao_com((68, 99), (50, 99));
        m.estimativa.qualidade = crate::scout::state::Qualidade::Baixa;
        let lista = escolher_jogadores(&m, &p, HOJE, &HashSet::new(), 8);
        assert_eq!(lista.len(), 8);
        let falsos: Vec<&JogadorEncontrado> = lista.iter().filter(|j| j.falso_positivo).collect();
        assert_eq!(falsos.len(), quality::falsos_positivos(m.estimativa.qualidade, 8));
        assert!(falsos.iter().all(|j| j.player_id > 20), "os falsos são os de 67");
        assert!(lista.iter().filter(|j| !j.falso_positivo).all(|j| j.player_id <= 20));
        // a faixa revelada continua contendo o valor real
        for j in &falsos {
            assert!(j.overall.min <= 67 && 67 <= j.overall.max);
        }
        // Qualidade Alta: nenhum
        m.estimativa.qualidade = crate::scout::state::Qualidade::Alta;
        assert!(escolher_jogadores(&m, &p, HOJE, &HashSet::new(), 8).iter().all(|j| !j.falso_positivo));
        // longe demais (Overall 60) nunca é "quase"
        let longe = pool((1..=10).map(|i| jogador(i, 70, 75, 24)).chain((11..=20).map(|i| jogador(i, 60, 75, 24))).collect());
        m.estimativa.qualidade = crate::scout::state::Qualidade::Baixa;
        assert!(escolher_jogadores(&m, &longe, HOJE, &HashSet::new(), 8).iter().all(|j| !j.falso_positivo));
    }

    #[test]
    fn reobserving_reveals_current_values_within_the_new_precision() {
        let mut real = jogador(9, 72, 80, 25);
        real.atributos[Atributo::Finalizacao.indice()] = 85;
        let p = pool(vec![real.clone()]);
        let m = missao_com((50, 99), (50, 99));
        let mut antes = revelar(&m, &p, HOJE, &real);
        antes.atributos.truncate(3);
        antes.falso_positivo = true;
        let depois = reobservar(&antes, &real, &p, Date(20270101), 2, 20, None, None);
        let ja_vistos: Vec<Atributo> = antes.atributos.iter().map(|a| a.atributo).collect();
        let primeiros: Vec<Atributo> = depois.atributos.iter().take(3).map(|a| a.atributo).collect();
        assert_eq!(primeiros, ja_vistos, "o que já tinha sido observado continua na frente");
        assert_eq!(depois.atributos.len(), 20);
        assert!(depois.overall.max - depois.overall.min <= 4);
        assert!(depois.overall.min <= 72 && 72 <= depois.overall.max);
        for a in &depois.atributos {
            let v = real.atributo(a.atributo);
            assert!(a.valor.min <= v && v <= a.valor.max);
        }
        assert!(depois.falso_positivo, "a marca segue com o jogador");
        let referencia = JogadorReferencia { player_id: 1, nome: "R".to_string(), posicao: 25, atributos: real.atributos.to_vec() };
        let exato = reobservar(&antes, &real, &p, Date(20270101), 0, 28, Some(PosicaoAlvo::SegundoAtacante), Some(&referencia));
        assert_eq!(exato.similaridade, Some(100), "similaridade recalculada com a referência da Missão");
        assert_eq!((exato.overall.min, exato.overall.max), (72, 72));
        assert_eq!(exato.atributos.len(), 28);
        assert!(exato.fit.is_some(), "fit recalculado com o alvo da Missão de origem");
    }
}
