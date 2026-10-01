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

use std::collections::HashSet;

use uuid::Uuid;

use super::quality;
use super::state::{Atributo, AtributoRevelado, JogadorEncontrado, Missao, Relatorio};
use crate::async_task::AsyncTask;
use crate::save_repo::{self, Date, Nacao, PlayerPool, PlayerRaw, SaveRepoError};

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

    fn read_snapshot(&self) -> Result<CareerSnapshot, SaveRepoError> {
        let identidade = save_repo::read_career_identity()?;
        Ok(CareerSnapshot {
            orcamento_transferencias: save_repo::read_transfer_budget()?,
            data_atual: save_repo::read_current_date()?,
            tecnico: format!("{} {}", identidade.first_name, identidade.surname)
                .trim()
                .to_string(),
            id_save: identidade.hash(),
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

/// O jogador passa nos filtros da Missão (todos combinados com E)?
pub fn passa_nos_filtros(missao: &Missao, pool: &PlayerPool, jogador: &PlayerRaw) -> bool {
    let filtros = &missao.filtros;
    let no_mercado = !jogador.resto_do_mundo
        && jogador.clube_id.is_some()
        && jogador.clube_id.map(i64::from) != Some(pool.clube_usuario);
    no_mercado
        && (filtros.overall.min..=filtros.overall.max).contains(&jogador.overall)
        && (filtros.potencial.min..=filtros.potencial.max).contains(&jogador.potencial)
        && filtros.atributo_dominante.is_none_or(|a| tem_dominante(jogador, a))
        && do_pais(&filtros.paises, pool, jogador)
}

/// Filtro geográfico (Story 2.9): lista vazia = todos os países; "Outros"
/// pega as nações que não estão no mapa.
pub fn do_pais(paises: &[u16], pool: &PlayerPool, jogador: &PlayerRaw) -> bool {
    if paises.is_empty() || paises.contains(&jogador.nacionalidade) {
        return true;
    }
    paises.contains(&NACAO_OUTROS) && !pool.nacoes.iter().any(|n| n.id == jogador.nacionalidade)
}

/// O atributo está entre os maiores do jogador (Story 2.8)? Conta só os
/// atributos da função dele: os de goleiro só para goleiros.
pub fn tem_dominante(jogador: &PlayerRaw, atributo: Atributo) -> bool {
    let goleiro = save_repo::funcao_da_posicao(jogador.posicao) == save_repo::Funcao::Goleiro;
    let valores: Vec<(Atributo, u8)> =
        Atributo::TODOS.iter().filter(|a| goleiro || !a.goleiro()).map(|&a| (a, jogador.atributo(a))).collect();
    quality::eh_dominante(&valores, atributo)
}

/// Escolhe até `quantos` candidatos (fora de `excluir`) e revela cada um
/// conforme a Qualidade da Missão. Determinístico para a mesma Missão.
pub fn escolher_jogadores(
    missao: &Missao,
    pool: &PlayerPool,
    hoje: Date,
    excluir: &HashSet<u32>,
    quantos: usize,
) -> Vec<JogadorEncontrado> {
    let id = missao.id.as_u128();
    let qualidade = missao.estimativa.qualidade;
    let mut candidatos: Vec<(u32, &PlayerRaw)> = pool
        .jogadores
        .iter()
        .filter(|j| !excluir.contains(&j.player_id) && passa_nos_filtros(missao, pool, j))
        .map(|j| {
            let dominante = missao.filtros.atributo_dominante.map(|a| j.atributo(a));
            let relevancia = quality::relevancia(missao.tipo, j.overall, j.potencial, dominante);
            (quality::nota_de_escolha(relevancia, qualidade, quality::semente(id, j.player_id, 0)), j)
        })
        .collect();
    // maior nota primeiro; empate pelo id (ordem estável e determinística)
    candidatos.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.player_id.cmp(&b.1.player_id)));
    candidatos.into_iter().take(quantos).map(|(_, j)| revelar(missao, pool, hoje, j)).collect()
}

/// O que o Relatório mostra de um jogador: faixas de Overall/Potencial e
/// os `atributos_revelados` primeiros atributos que o Olheiro observou.
pub fn revelar(missao: &Missao, pool: &PlayerPool, hoje: Date, jogador: &PlayerRaw) -> JogadorEncontrado {
    let id = missao.id.as_u128();
    let pid = jogador.player_id;
    let precisao = missao.estimativa.precisao_mais_menos;
    let funcao = save_repo::funcao_da_posicao(jogador.posicao);
    let atributos = quality::ordem_de_observacao(funcao, missao.filtros.atributo_dominante)
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
    JogadorEncontrado {
        player_id: pid,
        nome: jogador.nome.clone(),
        idade: jogador.idade(hoje),
        posicao: jogador.posicao,
        nacao_id: jogador.nacionalidade,
        nacao: nacao.map(|n| n.nome.clone()).unwrap_or_else(|| "Outros".to_string()),
        clube: jogador.clube.clone(),
        overall: quality::faixa_revelada(jogador.overall, precisao, quality::semente(id, pid, 1)),
        potencial: quality::faixa_revelada(jogador.potencial, precisao, quality::semente(id, pid, 2)),
        atributos,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::save_repo::jogadores::TOTAL_ATRIBUTOS;
    use crate::save_repo::{Confederacao, Nacao};
    use crate::scout::state::{FaixaAtributo, StatusMissao};

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
        }
    }

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
        let ids: Vec<u32> = p.jogadores.iter().filter(|j| passa_nos_filtros(&m, &p, j)).map(|j| j.player_id).collect();
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
        let mut defensor = jogador(9, 70, 75, 4);
        defensor.atributos = [60; TOTAL_ATRIBUTOS];
        defensor.atributos[Atributo::Marcacao.indice()] = 85;
        defensor.atributos[Atributo::Drible.indice()] = 40;
        let p = pool(vec![driblador(1, 80, 70), driblador(2, 92, 72), driblador(3, 95, 60), defensor]);
        let mut m = missao_com((65, 80), (50, 99));
        m.filtros.atributo_dominante = Some(Atributo::Drible);
        let ids: Vec<u32> = p.jogadores.iter().filter(|j| passa_nos_filtros(&m, &p, j)).map(|j| j.player_id).collect();
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
        let ids = |m: &Missao| -> Vec<u32> { p.jogadores.iter().filter(|j| passa_nos_filtros(m, &p, j)).map(|j| j.player_id).collect() };
        assert_eq!(ids(&m), vec![1, 2, 3], "nenhum país = todos");
        m.filtros.paises = vec![54];
        assert_eq!(ids(&m), vec![1]);
        m.filtros.paises = vec![54, NACAO_OUTROS];
        assert_eq!(ids(&m), vec![1, 3], "Outros pega quem não tem quadro no mapa");
    }
}
