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

/// O que o jogo sabe de um jogador: `(nível 0–198, campo "a")`.
pub type Conhecimento = (i32, i32);

/// A partir deste nível o jogo mostra o jogador aberto (Overall, Potencial,
/// atributos e contrato). O Felipe confirmou no Mbappé e no Camarda (178) e
/// em jogadores no 144 e no 162–168 (Haaland, Saka, Bellingham...). No 140
/// o jogo mostra transferência, salário e estimativas dos atributos (um passo
/// antes de abrir), como o Udogie. Entre o 141 e o 143 não se sabe: a
/// Central fica com o lado seguro (parcial).
const NIVEL_ABERTO: i32 = 144;

/// O campo `a` tem os 16 bits de baixo todos ligados (visto no Mbappé, que o
/// tem em 0x10FFFF)? Só o jogo mexe nele: a Central nunca o altera.
pub fn campo_aberto(conhecimento: Conhecimento) -> bool {
    conhecimento.1 & 0xFFFF == 0xFFFF
}

/// O jogador está aberto na tela do jogo? Pelo nível, ou pelo campo `a`.
pub fn atributos_abertos(conhecimento: Conhecimento) -> bool {
    conhecimento.0 >= NIVEL_ABERTO || campo_aberto(conhecimento)
}

/// O que a Central mostra de um jogador pelo que o jogo sabe dele (ou nada,
/// sem registro): como `revelacao_do_nivel`, mas com o jogador aberto no jogo
/// (Overall, Potencial e todos os atributos à mostra) ele vem exato e completo.
pub fn revelacao(conhecimento: Option<Conhecimento>) -> (u8, usize) {
    match conhecimento {
        Some(c) if atributos_abertos(c) => (0, Atributo::TODOS.len()),
        _ => revelacao_do_nivel(conhecimento.map(|c| c.0)),
    }
}

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
    /// O campo `a` dele está aberto (`campo_aberto`).
    pub campo_aberto: bool,
    /// A precisão (±) que o nível dá (`revelacao_do_nivel`).
    pub precisao: u8,
    /// A foto na precisão do nível.
    pub jogador: JogadorEncontrado,
    /// A foto exata e completa, se o jogo pode estar mostrando o jogador
    /// aberto (nível a partir do 144, ou o campo `a` aberto).
    pub exato: Option<JogadorEncontrado>,
}

/// Lê o elenco e a lista do jogo no `pool`. `lista`: `(jogador, nível de
/// conhecimento)` de cada um na lista do jogo; `niveis`: o conhecimento do
/// jogo sobre quem a Central já conhece; `conhecidos`: quem a Central já tem
/// nos Escolhidos ou não quer de volta.
pub fn montar(
    pool: &PlayerPool,
    hoje: Date,
    lista: &[(u32, Option<Conhecimento>)],
    niveis: &HashMap<u32, Conhecimento>,
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
            let (precisao, atributos) = revelacao(nivel);
            Some((search::fotografar(raw, pool, hoje, precisao, atributos), precisao))
        })
        .collect();
    let mut sincronia: Vec<Sincronia> = niveis
        .iter()
        .filter_map(|(&id, &conhecimento)| {
            let raw = pool.jogadores.iter().find(|j| j.player_id == id && !do_clube(j))?;
            let (precisao, atributos) = revelacao_do_nivel(Some(conhecimento.0));
            Some(Sincronia {
                nivel: conhecimento.0,
                campo_aberto: campo_aberto(conhecimento),
                precisao,
                jogador: search::fotografar(raw, pool, hoje, precisao, atributos),
                exato: atributos_abertos(conhecimento).then(|| search::fotografar(raw, pool, hoje, 0, todos)),
            })
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
    /// Registros "Relatório do jogo" de quem a Central não conhece, tirados
    /// da Base (a v11 os trazia de todo jogador com conhecimento máximo).
    pub limpos: usize,
}

impl Resumo {
    pub fn mudou(&self) -> bool {
        self.sairam + self.importados + self.completados + self.limpos > 0
    }
}

/// Grava o mapeamento: os que saíram do elenco viram ex-jogadores na Base e
/// os da lista do jogo entram nos Escolhidos e na Base. Idempotente.
pub fn aplicar(dados: &mut ScoutStateFile, m: Mapeamento, hoje: Date) -> Resumo {
    let mut resumo = Resumo::default();

    // ---- limpeza: "Relatório do jogo" só vale para quem a Central conhece
    // (um Relatório dos Olheiros ou os Escolhidos); a v11 trazia todo
    // jogador que o jogo marca com conhecimento máximo, vistos ou não
    let conhecidos_da_central: HashSet<u32> = dados
        .relatorios
        .iter()
        .flat_map(|r| r.jogadores.iter().chain(r.da_base.iter()))
        .map(|j| j.player_id)
        .chain(dados.escolhidos.iter().map(|e| e.jogador.player_id))
        .collect();
    let antes = dados.mapeados.len();
    dados.mapeados.retain(|x| x.motivo != MotivoMapeamento::RelatorioDoJogo || conhecidos_da_central.contains(&x.jogador.player_id));
    resumo.limpos = antes - dados.mapeados.len();

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
        let escolhido = dados.escolhidos.iter().find(|e| e.jogador.player_id == id).map(|e| e.importado);
        let mapeado_da_lista = dados.mapeados.iter().any(|x| x.jogador.player_id == id && x.motivo == MotivoMapeamento::ListaDoJogo);
        let mapeado = dados.mapeados.iter().any(|x| x.jogador.player_id == id);
        if escolhido.is_none() && !mapeado && !nos_relatorios.contains(&id) {
            continue; // o jogo marca 198 até em quem o técnico nunca viu
        }
        // O nível que a Central escreve no jogo (198 − 4 × precisão) não pode
        // passar por "o jogo sabe": só vale como nível do jogo para quem veio
        // da lista dele e a Central nunca mexeu. Para os outros, só o campo
        // `a` aberto ou o nível máximo mostram o jogador aberto.
        let so_do_jogo = escolhido == Some(true) || (escolhido.is_none() && mapeado_da_lista);
        let aberto = sincronia.campo_aberto || sincronia.nivel >= NIVEL_COMPLETO || (so_do_jogo && sincronia.nivel >= NIVEL_ABERTO);
        let mostra_contrato = aberto || so_do_jogo;
        let foto = match (&sincronia.exato, aberto) {
            (Some(exato), true) => exato.clone(),
            _ => sincronia.jogador.clone(),
        };
        let precisao = if aberto && sincronia.exato.is_some() { 0 } else { sincronia.precisao };
        let mut mudou = false;
        // Escolhido: o jogo sabe mais do que a Central mostra (um olheiro do
        // FIFA observou antes, ou depois)
        if let Some(e) = dados.escolhidos.iter_mut().find(|e| e.jogador.player_id == id) {
            let esperado = super::quality::nivel_no_jogo(e.precisao, false, false, 0);
            if sincronia.nivel > esperado || (aberto && e.precisao > precisao) {
                // mantém o que a Missão de origem pediu (Fit, referência)
                let fit_alvo = e.jogador.fit_alvo;
                e.jogador = JogadorEncontrado { fit_alvo, ..foto.clone() };
                e.precisao = precisao;
                e.observado_em = hoje;
                mudou = true;
            } else if aberto && e.jogador.atributos.len() < foto.atributos.len() {
                // o jogo mostra todos os atributos e a Central só alguns: completa
                // com os que faltam, sem piorar o que ela já sabe
                for a in &foto.atributos {
                    if !e.jogador.atributos.iter().any(|x| x.atributo == a.atributo) {
                        e.jogador.atributos.push(*a);
                    }
                }
                mudou = true;
            } else if !aberto && e.importado && e.acompanhamento.is_none() && e.precisao < sincronia.precisao {
                // veio da lista do jogo, nenhum Generalista o acompanha e o jogo
                // ainda não o mostra aberto: a Central mostra só o que o jogo mostra
                let fit_alvo = e.jogador.fit_alvo;
                e.jogador = JogadorEncontrado { fit_alvo, ..sincronia.jogador.clone() };
                e.precisao = sincronia.precisao;
                e.observado_em = hoje;
                mudou = true;
            }
        }
        // o registro da Base que veio da lista do jogo mostra o que o jogo mostra
        if !aberto && so_do_jogo {
            for x in dados.mapeados.iter_mut().filter(|x| x.jogador.player_id == id && x.motivo == MotivoMapeamento::ListaDoJogo) {
                if x.jogador.overall.min == x.jogador.overall.max && sincronia.precisao > 0 {
                    x.jogador = JogadorEncontrado { visto_em: Some(hoje), ..sincronia.jogador.clone() };
                    mudou = true;
                }
            }
        }
        // o contrato que o jogo mostra e a Central ainda não tinha (as fotos
        // antigas não o traziam)
        if mostra_contrato && foto.contrato_ate.is_some() {
            if let Some(e) = dados.escolhidos.iter_mut().find(|e| e.jogador.player_id == id) {
                if e.jogador.contrato_ate.is_none() && (aberto || e.importado) {
                    e.jogador.contrato_ate = foto.contrato_ate;
                    mudou = true;
                }
            }
            for x in dados.mapeados.iter_mut().filter(|x| x.jogador.player_id == id && x.motivo != MotivoMapeamento::ExClube) {
                if x.jogador.contrato_ate.is_none() {
                    x.jogador.contrato_ate = foto.contrato_ate;
                    mudou = true;
                }
            }
        }
        // Base: o jogador está aberto no jogo e a Central ainda não o tem
        // completo (os registros de Relatório só têm o que o Olheiro viu)
        if aberto {
            // completo = tudo o que se observa nele (goleiro tem menos atributos) e exato
            let todos = foto.atributos.len();
            let ja_completo = |j: &JogadorEncontrado| j.atributos.len() >= todos && j.overall.min == j.overall.max;
            let relatorio_completo = dados.relatorios.iter().flat_map(|r| r.jogadores.iter()).any(|j| j.player_id == id && ja_completo(j));
            let mut registro = foto;
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

    fn niveis(pares: &[(u32, i32)]) -> HashMap<u32, Conhecimento> {
        pares.iter().map(|&(id, n)| (id, (n, 0))).collect()
    }

    #[test]
    fn a_player_the_game_shows_with_every_attribute_open_gets_them_all_in_the_central() {
        let mut j = jogador(40, 80, 85, 24);
        j.clube_id = Some(5);
        let p = pool(vec![j]);
        let raw = &p.jogadores[0];
        let mut dados = ScoutStateFile::default();
        // um Escolhido importado com o nível 178 (±5, 24 atributos), como o Mbappé
        let (precisao, atributos) = revelacao(Some((130, 0x100002)));
        assert_eq!((precisao, atributos), (PRECISAO_DESCONHECIDO, ATRIBUTOS_DESCONHECIDO), "abaixo do 140 e sem o campo aberto: o básico");
        assert_eq!(revelacao(Some((140, 0x100002))), (15, 14), "140 (o Udogie): valor e salário, não o resto");
        assert_eq!(revelacao(Some((144, 0x100002))), (0, Atributo::TODOS.len()), "144: aberto no jogo, exato e completo");
        assert_eq!(revelacao(Some((162, 0x100002))), (0, Atributo::TODOS.len()), "162: aberto no jogo, exato e completo");
        assert_eq!(revelacao(Some((178, 0x100002))), (0, Atributo::TODOS.len()), "178: aberto no jogo, exato e completo");
        assert_eq!(revelacao(Some((166, 0x10FFFF))), (0, Atributo::TODOS.len()), "campo aberto: exato e completo");
        // veio da lista do jogo (como o Mbappé e o Camarda): a Central nunca escreveu o nível dele
        let mut importado = escolhido(search::fotografar(raw, &p, HOJE, 5, 24), 5);
        importado.importado = true;
        importado.jogador.contrato_ate = None; // as fotos antigas não traziam o contrato
        dados.escolhidos.push(importado);
        // o jogo mostra o jogador aberto (nível 162 em diante)
        let abertos: HashMap<u32, Conhecimento> = [(40, (178, 0x100002))].into_iter().collect();
        let r = aplicar(&mut dados, montar(&p, HOJE, &[], &abertos, &HashSet::new()), HOJE);
        assert!(r.mudou());
        let e = &dados.escolhidos[0];
        assert_eq!(e.jogador.atributos.len(), crate::scout::lista::ATRIBUTOS_DETALHADO, "agora tem todos");
        assert_eq!(e.precisao, 0, "Overall, Potencial e atributos exatos, como o jogo mostra");
        assert_eq!(e.jogador.contrato_ate, Some(2028), "e o contrato");
        assert_eq!((e.jogador.overall.min, e.jogador.overall.max), (80, 80));
        assert_eq!((e.jogador.potencial.min, e.jogador.potencial.max), (85, 85));
        // a Base também o tem completo
        assert!(dados.mapeados.iter().any(|m| m.jogador.player_id == 40 && m.jogador.atributos.len() == e.jogador.atributos.len()));
        assert!(!aplicar(&mut dados, montar(&p, HOJE, &[], &abertos, &HashSet::new()), HOJE).mudou(), "idempotente");
        // abaixo do 144 e sem o campo aberto, nada muda
        let mut outro = ScoutStateFile::default();
        let (p140, a140) = revelacao_do_nivel(Some(140));
        let mut imp = escolhido(search::fotografar(raw, &p, HOJE, p140, a140), p140);
        imp.importado = true;
        outro.escolhidos.push(imp);
        let fechados: HashMap<u32, Conhecimento> = [(40, (140, 0x100002))].into_iter().collect();
        assert!(!aplicar(&mut outro, montar(&p, HOJE, &[], &fechados, &HashSet::new()), HOJE).mudou());
        // quem a Central achou (não importado): o 178 pode ser o que ELA escreveu
        // (±5), então não abre; só o campo `a` aberto abre
        let mut achado = ScoutStateFile::default();
        achado.escolhidos.push(escolhido(search::fotografar(raw, &p, HOJE, 5, 24), 5));
        let r = aplicar(&mut achado, montar(&p, HOJE, &[], &abertos, &HashSet::new()), HOJE);
        assert!(!r.mudou() && achado.escolhidos[0].precisao == 5, "o 178 sozinho não abre quem a Central escreveu");
        let campo: HashMap<u32, Conhecimento> = [(40, (178, 0x10FFFF))].into_iter().collect();
        let r = aplicar(&mut achado, montar(&p, HOJE, &[], &campo, &HashSet::new()), HOJE);
        assert!(r.mudou() && achado.escolhidos[0].precisao == 0, "o campo aberto abre");
    }

    #[test]
    fn a_player_the_game_does_not_show_open_goes_back_to_what_the_game_shows() {
        let mut j = jogador(41, 80, 85, 24);
        j.clube_id = Some(5);
        let p = pool(vec![j]);
        let raw = &p.jogadores[0];
        let exato = || {
            let mut e = escolhido(search::fotografar(raw, &p, HOJE, 0, 33), 0);
            e.importado = true;
            e
        };
        // o Udogie: veio da lista do jogo, ficou exato por engano, o jogo o tem no 140
        let mut dados = ScoutStateFile::default();
        dados.escolhidos.push(exato());
        dados.mapeados.push(JogadorMapeado { jogador: search::fotografar(raw, &p, HOJE, 0, 33), desde: HOJE, motivo: MotivoMapeamento::ListaDoJogo });
        let no_140: HashMap<u32, Conhecimento> = [(41, (140, 0x100002))].into_iter().collect();
        let r = aplicar(&mut dados, montar(&p, HOJE, &[], &no_140, &HashSet::new()), HOJE);
        assert!(r.mudou());
        let e = &dados.escolhidos[0];
        assert_eq!((e.precisao, e.jogador.atributos.len()), (15, 14), "volta ao que o jogo mostra no 140");
        assert!(e.jogador.overall.min < e.jogador.overall.max, "Overall em faixa");
        assert!(dados.mapeados[0].jogador.overall.min < dados.mapeados[0].jogador.overall.max, "e o registro da Base também");
        assert!(!aplicar(&mut dados, montar(&p, HOJE, &[], &no_140, &HashSet::new()), HOJE).mudou(), "idempotente");
        // com um Generalista acompanhando, a precisão é dele: não volta
        let mut acompanhado = ScoutStateFile::default();
        let mut e = exato();
        e.acompanhamento = Some(crate::scout::state::Acompanhamento { inicio: HOJE, precisao_inicial: 15, atributos_iniciais: 14, dias_para_exato: 30, dias: 30 });
        acompanhado.escolhidos.push(e);
        assert!(!aplicar(&mut acompanhado, montar(&p, HOJE, &[], &no_140, &HashSet::new()), HOJE).mudou());
        assert_eq!(acompanhado.escolhidos[0].precisao, 0);
    }

    #[test]
    fn game_report_records_of_players_the_central_never_saw_are_cleaned_up() {
        let mut jogadores: Vec<_> = Vec::new();
        for id in 30..=33 {
            let mut j = jogador(id, 70, 74, 18);
            j.clube_id = Some(5);
            jogadores.push(j);
        }
        let p = pool(jogadores);
        let foto = |id: u32| {
            let raw = p.jogadores.iter().find(|j| j.player_id == id).expect("jogador");
            search::fotografar(raw, &p, HOJE, 0, 33)
        };
        let mapeado = |id: u32, motivo| JogadorMapeado { jogador: foto(id), desde: HOJE, motivo };
        let mut dados = ScoutStateFile::default();
        // 30: ninguém da Central o viu (lixo da v11); 31: está num Relatório; 32: é Escolhido;
        // 33: ex-jogador do clube (outro motivo: não é tocado)
        dados.mapeados = vec![
            mapeado(30, MotivoMapeamento::RelatorioDoJogo),
            mapeado(31, MotivoMapeamento::RelatorioDoJogo),
            mapeado(32, MotivoMapeamento::RelatorioDoJogo),
            mapeado(33, MotivoMapeamento::ExClube),
        ];
        let mut relatorio = crate::scout::state::Relatorio::de_teste(uuid::Uuid::new_v4());
        relatorio.jogadores = vec![foto(31)];
        dados.relatorios.push(relatorio);
        dados.escolhidos.push(escolhido(foto(32), 0));
        let vazio = Mapeamento { clube: 241, elenco: Vec::new(), da_lista: Vec::new(), sincronia: Vec::new() };
        let r = aplicar(&mut dados, vazio.clone(), HOJE);
        assert_eq!(r.limpos, 1);
        let ids: Vec<u32> = dados.mapeados.iter().map(|m| m.jogador.player_id).collect();
        assert_eq!(ids, vec![31, 32, 33]);
        assert!(!aplicar(&mut dados, vazio, HOJE).mudou(), "limpar de novo não muda nada");
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
        let lista = [(10, Some((198, 0))), (11, Some((198, 0))), (12, Some((198, 0))), (13, None), (99, Some((198, 0))), (1, Some((198, 0)))];
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
        let r = aplicar(&mut dados, montar(&p, HOJE, &[(12, Some((198, 0))), (14, Some((198, 0)))], &HashMap::new(), &HashSet::new()), HOJE);
        assert_eq!((r.importados, dados.escolhidos.len()), (1, 1));
        assert_eq!(dados.escolhidos[0].jogador.player_id, 14);
    }
}
