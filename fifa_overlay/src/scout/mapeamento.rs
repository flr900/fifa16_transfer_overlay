//! Jogadores que o scout mapeou sem uma Missão (2026-10-08, pedido do Felipe):
//!
//! - **Ex-jogadores do clube.** Cada leitura do elenco guarda uma foto de
//!   cada jogador (com os valores reais: é do técnico). Quando um deles
//!   deixa o clube — vendido, emprestado, dispensado —, a foto vira um
//!   registro da Base do Scout, com os atributos que ele tinha ao sair. Com o
//!   tempo o jogador muda e a foto fica para trás, como qualquer observação
//!   antiga. Enquanto está no clube, ele NÃO aparece na Base.
//! - **A lista de escolhidos do jogo.** O que o técnico põe na lista do
//!   próprio FIFA entra nos Escolhidos e na Base da Central, com o que o
//!   jogo já sabe dele (o nível de conhecimento decide a precisão). Quem o
//!   técnico tirou dos Escolhidos da Central não volta sozinho.
//! - **O que o jogo já sabe de quem a Central conhece.** O FIFA marca
//!   conhecimento máximo (198) até em quem o técnico nunca viu; por isso só
//!   vale para quem a Central já tem. Se o jogo sabe mais de um Escolhido do
//!   que a Central (um olheiro do FIFA observou antes, ou depois), o
//!   Escolhido sobe para o nível do jogo. Se o jogo conhece por inteiro um
//!   jogador que um Olheiro da Central encontrou (ou um Escolhido), ele passa
//!   a valer completo e exato na Base. Quem o jogo conhece mas a Central não,
//!   não entra.
//!
//! A leitura (`montar`) e a aplicação (`aplicar`) são puras e testáveis; o
//! `ScoutState` só liga uma à outra, em background.

use std::collections::{HashMap, HashSet};

use super::persistence::ScoutStateFile;
use super::search;
use super::state::{Escolhido, JogadorEncontrado, JogadorMapeado, MotivoMapeamento};
use crate::save_repo::{Atributo, Date, PlayerPool};

/// Nível de conhecimento do jogo que mostra todos os atributos.
const NIVEL_COMPLETO: i32 = 198;
/// A partir daqui o jogo mostra valor, salário e estimativas melhores.
const NIVEL_REVELADO: i32 = 140;
/// Precisão (±) e atributos de quem o jogo mal conhece.
const PRECISAO_DESCONHECIDO: u8 = 20;
const ATRIBUTOS_DESCONHECIDO: usize = 6;

/// O que a Central mostra de um jogador pelo nível de conhecimento que o jogo
/// tem dele: `(precisão ±, quantos atributos)`. É o inverso de
/// `quality::nivel_no_jogo` (`198 − 4 × precisão`); sem registro, o básico.
pub fn revelacao_do_nivel(nivel: Option<i32>) -> (u8, usize) {
    match nivel {
        Some(n) if n >= NIVEL_COMPLETO => (0, Atributo::TODOS.len()),
        Some(n) if n >= NIVEL_REVELADO => {
            let precisao = u8::try_from((NIVEL_COMPLETO - n + 3) / 4).unwrap_or(PRECISAO_DESCONHECIDO);
            (precisao.max(1), if n >= 170 { 24 } else { 14 })
        }
        _ => (PRECISAO_DESCONHECIDO, ATRIBUTOS_DESCONHECIDO),
    }
}

/// O que a leitura do jogo trouxe, pronto para gravar.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapeamento {
    /// O clube do técnico na leitura.
    pub clube: i64,
    /// Foto exata de cada jogador do elenco agora.
    pub elenco: Vec<JogadorEncontrado>,
    /// Jogadores da lista do jogo que a Central ainda não conhece, com a
    /// precisão (±) que o conhecimento do jogo dá.
    pub da_lista: Vec<(JogadorEncontrado, u8)>,
    /// Quem a Central conhece e o jogo também, como o jogo o mostra: a foto
    /// na precisão do nível de conhecimento dele.
    pub sincronia: Vec<Sincronia>,
}

/// O que o jogo sabe de um jogador que a Central conhece.
#[derive(Debug, Clone, PartialEq)]
pub struct Sincronia {
    /// Nível de conhecimento do jogo (0–198).
    pub nivel: i32,
    /// A precisão (±) que esse nível dá (`revelacao_do_nivel`).
    pub precisao: u8,
    pub jogador: JogadorEncontrado,
}

/// Lê o elenco e a lista do jogo no `pool`. `lista`: `(jogador, nível de
/// conhecimento)` de cada um na lista do jogo; `niveis`: o conhecimento do
/// jogo sobre quem a Central já conhece; `conhecidos`: quem a Central já tem
/// nos Escolhidos ou não quer de volta.
pub fn montar(
    pool: &PlayerPool,
    hoje: Date,
    lista: &[(u32, Option<i32>)],
    niveis: &HashMap<u32, i32>,
    conhecidos: &HashSet<u32>,
) -> Mapeamento {
    let do_clube = |j: &crate::save_repo::PlayerRaw| !j.resto_do_mundo && j.clube_id.map(i64::from) == Some(pool.clube_usuario);
    let todos = Atributo::TODOS.len();
    let elenco = pool
        .jogadores
        .iter()
        .filter(|j| do_clube(j))
        .map(|j| search::fotografar(j, pool, hoje, 0, todos))
        .collect();
    let da_lista = lista
        .iter()
        .filter(|(id, _)| !conhecidos.contains(id))
        .filter_map(|&(id, nivel)| {
            let raw = pool.jogadores.iter().find(|j| j.player_id == id && !do_clube(j))?;
            let (precisao, atributos) = revelacao_do_nivel(nivel);
            Some((search::fotografar(raw, pool, hoje, precisao, atributos), precisao))
        })
        .collect();
    let mut sincronia: Vec<Sincronia> = niveis
        .iter()
        .filter_map(|(&id, &nivel)| {
            let raw = pool.jogadores.iter().find(|j| j.player_id == id && !do_clube(j))?;
            let (precisao, atributos) = revelacao_do_nivel(Some(nivel));
            Some(Sincronia { nivel, precisao, jogador: search::fotografar(raw, pool, hoje, precisao, atributos) })
        })
        .collect();
    sincronia.sort_by_key(|s| s.jogador.player_id);
    Mapeamento { clube: pool.clube_usuario, elenco, da_lista, sincronia }
}

/// O que `aplicar` mudou (para o log).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resumo {
    pub sairam: usize,
    pub importados: usize,
    /// Escolhidos que subiram para o nível do jogo e jogadores que ficaram
    /// completos na Base.
    pub completados: usize,
}

impl Resumo {
    pub fn mudou(&self) -> bool {
        self.sairam + self.importados + self.completados > 0
    }
}

/// Grava o mapeamento: os que saíram do elenco viram ex-jogadores na Base e
/// os da lista do jogo entram nos Escolhidos e na Base. Idempotente.
pub fn aplicar(dados: &mut ScoutStateFile, m: Mapeamento, hoje: Date) -> Resumo {
    let mut resumo = Resumo::default();

    // ---- o elenco: uma leitura vazia é leitura ruim, não "todo mundo saiu"
    if !m.elenco.is_empty() {
        let mesmo_clube = dados.elenco_clube == Some(m.clube);
        let agora: HashSet<u32> = m.elenco.iter().map(|j| j.player_id).collect();
        if mesmo_clube {
            let sairam: Vec<JogadorEncontrado> = dados.elenco.iter().filter(|j| !agora.contains(&j.player_id)).cloned().collect();
            for mut jogador in sairam {
                jogador.visto_em = Some(hoje);
                dados.mapeados.retain(|x| x.jogador.player_id != jogador.player_id);
                dados.mapeados.push(JogadorMapeado { jogador, desde: hoje, motivo: MotivoMapeamento::ExClube });
                resumo.sairam += 1;
            }
            // quem voltou ao elenco deixa de ser ex-jogador
            dados.mapeados.retain(|x| !(x.motivo == MotivoMapeamento::ExClube && agora.contains(&x.jogador.player_id)));
        }
        if !mesmo_clube || dados.elenco != m.elenco {
            dados.elenco = m.elenco;
            dados.elenco_clube = Some(m.clube);
        }
    }

    // ---- a lista do jogo
    for (jogador, precisao) in m.da_lista {
        let id = jogador.player_id;
        let ja_tem = dados.escolhidos.iter().any(|e| e.jogador.player_id == id) || dados.importacao_ignorada.contains(&id);
        if ja_tem {
            continue;
        }
        let mut registro = jogador.clone();
        registro.visto_em = Some(hoje);
        if !dados.mapeados.iter().any(|x| x.jogador.player_id == id) {
            dados.mapeados.push(JogadorMapeado { jogador: registro, desde: hoje, motivo: MotivoMapeamento::ListaDoJogo });
        }
        dados.escolhidos.push(Escolhido {
            jogador,
            adicionado_em: hoje,
            observado_em: hoje,
            precisao,
            prioridade: false,
            acompanhamento: None,
            alvo: None,
            referencia: None,
            relatorio_id: None,
            no_jogo: false,
            importado: true,
        });
        resumo.importados += 1;
    }

    // ---- o que o jogo sabe de quem a Central conhece
    let nos_relatorios: HashSet<u32> =
        dados.relatorios.iter().flat_map(|r| r.jogadores.iter().chain(r.da_base.iter())).map(|j| j.player_id).collect();
    for sincronia in m.sincronia {
        let id = sincronia.jogador.player_id;
        let escolhido = dados.escolhidos.iter().any(|e| e.jogador.player_id == id);
        let mapeado = dados.mapeados.iter().any(|x| x.jogador.player_id == id);
        if !(escolhido || mapeado || nos_relatorios.contains(&id)) {
            continue; // o jogo marca 198 até em quem o técnico nunca viu
        }
        let mut mudou = false;
        // Escolhido: o jogo sabe mais do que a Central mostra (um olheiro do
        // FIFA observou antes, ou depois)
        if let Some(e) = dados.escolhidos.iter_mut().find(|e| e.jogador.player_id == id) {
            let esperado = super::quality::nivel_no_jogo(e.precisao, false, false, 0);
            if sincronia.nivel > esperado {
                // mantém o que a Missão de origem pediu (Fit, referência)
                let fit_alvo = e.jogador.fit_alvo;
                e.jogador = JogadorEncontrado { fit_alvo, ..sincronia.jogador.clone() };
                e.precisao = sincronia.precisao;
                e.observado_em = hoje;
                mudou = true;
            }
        }
        // Base: o jogo o conhece por inteiro e a Central ainda não o tem
        // completo (os registros de Relatório só têm o que o Olheiro viu)
        if sincronia.nivel >= NIVEL_COMPLETO {
            let ja_completo = |j: &JogadorEncontrado| j.atributos.len() >= crate::scout::lista::ATRIBUTOS_DETALHADO && j.overall.min == j.overall.max;
            let relatorio_completo = dados.relatorios.iter().flat_map(|r| r.jogadores.iter()).any(|j| j.player_id == id && ja_completo(j));
            let mut registro = sincronia.jogador;
            registro.visto_em = Some(hoje);
            match dados.mapeados.iter_mut().find(|x| x.jogador.player_id == id && x.motivo != MotivoMapeamento::ExClube) {
                Some(existente) => {
                    if !ja_completo(&existente.jogador) || existente.jogador.overall != registro.overall {
                        existente.jogador = registro;
                        existente.desde = hoje;
                        mudou = true;
                    }
                }
                None if !relatorio_completo => {
                    dados.mapeados.push(JogadorMapeado { jogador: registro, desde: hoje, motivo: MotivoMapeamento::RelatorioDoJogo });
                    mudou = true;
                }
                None => {}
            }
        }
        if mudou {
            resumo.completados += 1;
        }
    }
    resumo
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::search::tests::{jogador, pool};

    fn do_clube(id: u32, clube: u32) -> crate::save_repo::PlayerRaw {
        let mut j = jogador(id, 70, 74, 18);
        j.clube_id = Some(clube);
        j
    }

    fn pool_do_clube(ids: &[u32]) -> PlayerPool {
        pool(ids.iter().map(|&id| do_clube(id, 241)).collect())
    }

    const HOJE: Date = Date(20260801);

    #[test]
    fn the_game_knowledge_level_sets_how_much_the_central_shows() {
        assert_eq!(revelacao_do_nivel(Some(198)), (0, Atributo::TODOS.len()), "198 = tudo, exato");
        assert_eq!(revelacao_do_nivel(Some(170)), (7, 24));
        assert_eq!(revelacao_do_nivel(Some(158)), (10, 14));
        assert_eq!(revelacao_do_nivel(Some(140)), (15, 14));
        assert_eq!(revelacao_do_nivel(Some(60)), (PRECISAO_DESCONHECIDO, ATRIBUTOS_DESCONHECIDO));
        assert_eq!(revelacao_do_nivel(None), (PRECISAO_DESCONHECIDO, ATRIBUTOS_DESCONHECIDO), "sem registro: o básico");
        // o inverso de nivel_no_jogo: a precisão dá o nível de volta
        for precisao in [2u8, 5, 10, 14] {
            let nivel = crate::scout::quality::nivel_no_jogo(precisao, false, false, 0);
            assert_eq!(revelacao_do_nivel(Some(nivel)).0, precisao);
        }
    }

    #[test]
    fn a_player_leaving_the_club_becomes_a_scout_base_record_and_nobody_else_does() {
        let mut dados = ScoutStateFile::default();
        // 1ª leitura: só guarda as fotos
        let r = aplicar(&mut dados, montar(&pool_do_clube(&[1, 2, 3]), HOJE, &[], &HashMap::new(), &HashSet::new()), HOJE);
        assert!(!r.mudou() && dados.mapeados.is_empty(), "quem está no clube não vai para a Base");
        assert_eq!(dados.elenco.len(), 3);
        assert_eq!(dados.elenco[0].atributos.len(), crate::scout::lista::ATRIBUTOS_DETALHADO, "a foto tem tudo, exato");
        assert_eq!(dados.elenco[0].overall.min, dados.elenco[0].overall.max);
        // o 2 é vendido
        let depois = Date(20260901);
        let r = aplicar(&mut dados, montar(&pool_do_clube(&[1, 3]), depois, &[], &HashMap::new(), &HashSet::new()), depois);
        assert_eq!(r.sairam, 1);
        assert_eq!(dados.mapeados.len(), 1);
        let ex = &dados.mapeados[0];
        assert_eq!((ex.jogador.player_id, ex.motivo, ex.desde), (2, MotivoMapeamento::ExClube, depois));
        assert_eq!(ex.jogador.visto_em, Some(depois));
        assert_eq!(dados.elenco.iter().map(|j| j.player_id).collect::<Vec<_>>(), vec![1, 3]);
        // ler de novo não repete
        let r = aplicar(&mut dados, montar(&pool_do_clube(&[1, 3]), depois, &[], &HashMap::new(), &HashSet::new()), depois);
        assert!(!r.mudou() && dados.mapeados.len() == 1);
        // ele volta ao clube: deixa de ser ex-jogador
        aplicar(&mut dados, montar(&pool_do_clube(&[1, 2, 3]), depois, &[], &HashMap::new(), &HashSet::new()), depois);
        assert!(dados.mapeados.is_empty(), "no clube de novo, fora da Base");
    }

    #[test]
    fn a_bad_squad_read_or_another_club_never_empties_the_squad_into_the_base() {
        let mut dados = ScoutStateFile::default();
        aplicar(&mut dados, montar(&pool_do_clube(&[1, 2, 3]), HOJE, &[], &HashMap::new(), &HashSet::new()), HOJE);
        // leitura vazia: ninguém "saiu"
        let vazio = Mapeamento { clube: 241, elenco: Vec::new(), da_lista: Vec::new(), sincronia: Vec::new() };
        assert!(!aplicar(&mut dados, vazio, HOJE).mudou());
        assert!(dados.mapeados.is_empty() && dados.elenco.len() == 3);
        // o técnico mudou de clube: recomeça, sem mapear o elenco antigo
        let mut outro = pool(vec![do_clube(7, 99), do_clube(8, 99)]);
        outro.clube_usuario = 99;
        let r = aplicar(&mut dados, montar(&outro, HOJE, &[], &HashMap::new(), &HashSet::new()), HOJE);
        assert!(!r.mudou() && dados.mapeados.is_empty());
        assert_eq!((dados.elenco_clube, dados.elenco.len()), (Some(99), 2));
    }

    fn escolhido(jogador: JogadorEncontrado, precisao: u8) -> Escolhido {
        Escolhido {
            jogador,
            adicionado_em: HOJE,
            observado_em: HOJE,
            precisao,
            prioridade: false,
            acompanhamento: None,
            alvo: None,
            referencia: None,
            relatorio_id: None,
            no_jogo: false,
            importado: false,
        }
    }

    fn niveis(pares: &[(u32, i32)]) -> HashMap<u32, i32> {
        pares.iter().copied().collect()
    }

    #[test]
    fn the_game_knowledge_only_counts_for_players_the_central_already_has() {
        let mut jogadores: Vec<_> = (1..=2).map(|id| do_clube(id, 241)).collect();
        for id in 20..=24 {
            let mut j = jogador(id, 70, 74, 18);
            j.clube_id = Some(5);
            jogadores.push(j);
        }
        let p = pool(jogadores);
        let raso = |id: u32, precisao: u8, atributos: usize| {
            let raw = p.jogadores.iter().find(|j| j.player_id == id).expect("jogador");
            search::fotografar(raw, &p, HOJE, precisao, atributos)
        };
        let mut dados = ScoutStateFile::default();
        // 20: Escolhido vago; 22: Escolhido com ±5 (a Central o mapeia a 178); 23: Escolhido, jogo sabe menos
        dados.escolhidos.push(escolhido(raso(20, 12, 6), 12));
        dados.escolhidos.push(escolhido(raso(22, 5, 20), 5));
        dados.escolhidos.push(escolhido(raso(23, 5, 20), 5));
        // 21: um Olheiro da Central o encontrou (está num Relatório), com poucos atributos
        let mut relatorio = crate::scout::state::Relatorio::de_teste(uuid::Uuid::new_v4());
        relatorio.jogadores = vec![raso(21, 6, 12)];
        dados.relatorios.push(relatorio);
        // o jogo marca 198 em todos, até em quem ninguém da Central viu (24) e no clube (1)
        let depois = Date(20260901);
        let pares = niveis(&[(20, 198), (21, 198), (22, 190), (23, 170), (24, 198), (1, 198)]);
        let r = aplicar(&mut dados, montar(&p, depois, &[], &pares, &HashSet::new()), depois);
        let por_id = |id: u32| dados.escolhidos.iter().find(|e| e.jogador.player_id == id).expect("escolhido");
        assert_eq!((por_id(20).precisao, por_id(20).jogador.atributos.len()), (0, crate::scout::lista::ATRIBUTOS_DETALHADO), "o jogo sabe tudo: o Escolhido fica completo");
        assert_eq!(por_id(20).observado_em, depois);
        assert_eq!(por_id(22).precisao, 2, "190 no jogo: ±2");
        assert_eq!(por_id(23).precisao, 5, "o jogo sabe menos do que a Central: fica como está");
        let na_base: Vec<(u32, MotivoMapeamento)> = dados.mapeados.iter().map(|m| (m.jogador.player_id, m.motivo)).collect();
        assert_eq!(
            na_base,
            vec![(20, MotivoMapeamento::RelatorioDoJogo), (21, MotivoMapeamento::RelatorioDoJogo)],
            "completos na Base só os que a Central conhece; o 24 e o do clube ficam de fora"
        );
        assert_eq!(dados.escolhidos.len(), 3, "ninguém vira Escolhido por isso");
        assert_eq!(r.completados, 3, "20, 21 e 22");
        // ler de novo, igual: nada muda
        assert!(!aplicar(&mut dados, montar(&p, depois, &[], &pares, &HashSet::new()), depois).mudou());
        // o próprio nível que a Central escreve no jogo não a "atualiza": ±5 vira 178 e volta igual
        let mut dados = ScoutStateFile::default();
        dados.escolhidos.push(escolhido(raso(22, 5, 20), 5));
        let esperado = crate::scout::quality::nivel_no_jogo(5, false, false, 0);
        assert!(!aplicar(&mut dados, montar(&p, depois, &[], &niveis(&[(22, esperado)]), &HashSet::new()), depois).mudou());
    }

    #[test]
    fn the_games_shortlist_comes_into_the_escolhidos_and_the_base_once() {
        let mut jogadores: Vec<_> = (1..=3).map(|id| do_clube(id, 241)).collect();
        for id in 10..=14 {
            let mut j = jogador(id, 70, 74, 18);
            j.clube_id = Some(5);
            jogadores.push(j);
        }
        let p = pool(jogadores);
        let mut dados = ScoutStateFile::default();
        // 10 já é Escolhido, 11 foi tirado de propósito, 99 não existe, 1 é do clube
        let conhecidos: HashSet<u32> = [10, 11].into_iter().collect();
        let lista = [(10, Some(198)), (11, Some(198)), (12, Some(198)), (13, None), (99, Some(198)), (1, Some(198))];
        let r = aplicar(&mut dados, montar(&p, HOJE, &lista, &HashMap::new(), &conhecidos), HOJE);
        assert_eq!(r.importados, 2);
        let ids: Vec<u32> = dados.escolhidos.iter().map(|e| e.jogador.player_id).collect();
        assert_eq!(ids, vec![12, 13]);
        let exato = &dados.escolhidos[0];
        assert!(exato.importado && !exato.no_jogo && exato.relatorio_id.is_none());
        assert_eq!((exato.precisao, exato.jogador.atributos.len()), (0, crate::scout::lista::ATRIBUTOS_DETALHADO), "198: tudo, exato");
        let basico = &dados.escolhidos[1];
        assert_eq!((basico.precisao, basico.jogador.atributos.len()), (PRECISAO_DESCONHECIDO, ATRIBUTOS_DESCONHECIDO));
        assert_eq!(dados.mapeados.iter().map(|m| (m.jogador.player_id, m.motivo)).collect::<Vec<_>>(), vec![(12, MotivoMapeamento::ListaDoJogo), (13, MotivoMapeamento::ListaDoJogo)]);
        // de novo, com os dois já nos Escolhidos: nada muda
        let conhecidos: HashSet<u32> = [10, 11, 12, 13].into_iter().collect();
        assert!(!aplicar(&mut dados, montar(&p, HOJE, &lista, &HashMap::new(), &conhecidos), HOJE).mudou());
        // quem foi tirado dos Escolhidos não volta (a lista ignorada vale também na aplicação)
        dados.escolhidos.clear();
        dados.importacao_ignorada.push(12);
        let r = aplicar(&mut dados, montar(&p, HOJE, &[(12, Some(198)), (14, Some(198))], &HashMap::new(), &HashSet::new()), HOJE);
        assert_eq!((r.importados, dados.escolhidos.len()), (1, 1));
        assert_eq!(dados.escolhidos[0].jogador.player_id, 14);
    }
}
