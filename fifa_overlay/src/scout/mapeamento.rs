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
//!
//! A leitura (`montar`) e a aplicação (`aplicar`) são puras e testáveis; o
//! `ScoutState` só liga uma à outra, em background.

use std::collections::HashSet;

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
}

/// Lê o elenco e a lista do jogo no `pool`. `lista`: `(jogador, nível de
/// conhecimento)` de cada um na lista do jogo; `conhecidos`: quem a Central
/// já tem nos Escolhidos ou não quer de volta.
pub fn montar(pool: &PlayerPool, hoje: Date, lista: &[(u32, Option<i32>)], conhecidos: &HashSet<u32>) -> Mapeamento {
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
    Mapeamento { clube: pool.clube_usuario, elenco, da_lista }
}

/// O que `aplicar` mudou (para o log).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resumo {
    pub sairam: usize,
    pub importados: usize,
}

impl Resumo {
    pub fn mudou(&self) -> bool {
        self.sairam + self.importados > 0
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
        let r = aplicar(&mut dados, montar(&pool_do_clube(&[1, 2, 3]), HOJE, &[], &HashSet::new()), HOJE);
        assert!(!r.mudou() && dados.mapeados.is_empty(), "quem está no clube não vai para a Base");
        assert_eq!(dados.elenco.len(), 3);
        assert_eq!(dados.elenco[0].atributos.len(), crate::scout::lista::ATRIBUTOS_DETALHADO, "a foto tem tudo, exato");
        assert_eq!(dados.elenco[0].overall.min, dados.elenco[0].overall.max);
        // o 2 é vendido
        let depois = Date(20260901);
        let r = aplicar(&mut dados, montar(&pool_do_clube(&[1, 3]), depois, &[], &HashSet::new()), depois);
        assert_eq!(r.sairam, 1);
        assert_eq!(dados.mapeados.len(), 1);
        let ex = &dados.mapeados[0];
        assert_eq!((ex.jogador.player_id, ex.motivo, ex.desde), (2, MotivoMapeamento::ExClube, depois));
        assert_eq!(ex.jogador.visto_em, Some(depois));
        assert_eq!(dados.elenco.iter().map(|j| j.player_id).collect::<Vec<_>>(), vec![1, 3]);
        // ler de novo não repete
        let r = aplicar(&mut dados, montar(&pool_do_clube(&[1, 3]), depois, &[], &HashSet::new()), depois);
        assert!(!r.mudou() && dados.mapeados.len() == 1);
        // ele volta ao clube: deixa de ser ex-jogador
        aplicar(&mut dados, montar(&pool_do_clube(&[1, 2, 3]), depois, &[], &HashSet::new()), depois);
        assert!(dados.mapeados.is_empty(), "no clube de novo, fora da Base");
    }

    #[test]
    fn a_bad_squad_read_or_another_club_never_empties_the_squad_into_the_base() {
        let mut dados = ScoutStateFile::default();
        aplicar(&mut dados, montar(&pool_do_clube(&[1, 2, 3]), HOJE, &[], &HashSet::new()), HOJE);
        // leitura vazia: ninguém "saiu"
        let vazio = Mapeamento { clube: 241, elenco: Vec::new(), da_lista: Vec::new() };
        assert!(!aplicar(&mut dados, vazio, HOJE).mudou());
        assert!(dados.mapeados.is_empty() && dados.elenco.len() == 3);
        // o técnico mudou de clube: recomeça, sem mapear o elenco antigo
        let mut outro = pool(vec![do_clube(7, 99), do_clube(8, 99)]);
        outro.clube_usuario = 99;
        let r = aplicar(&mut dados, montar(&outro, HOJE, &[], &HashSet::new()), HOJE);
        assert!(!r.mudou() && dados.mapeados.is_empty());
        assert_eq!((dados.elenco_clube, dados.elenco.len()), (Some(99), 2));
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
        let r = aplicar(&mut dados, montar(&p, HOJE, &lista, &conhecidos), HOJE);
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
        assert!(!aplicar(&mut dados, montar(&p, HOJE, &lista, &conhecidos), HOJE).mudou());
        // quem foi tirado dos Escolhidos não volta (a lista ignorada vale também na aplicação)
        dados.escolhidos.clear();
        dados.importacao_ignorada.push(12);
        let r = aplicar(&mut dados, montar(&p, HOJE, &[(12, Some(198)), (14, Some(198))], &HashSet::new()), HOJE);
        assert_eq!((r.importados, dados.escolhidos.len()), (1, 1));
        assert_eq!(dados.escolhidos[0].jogador.player_id, 14);
    }
}
