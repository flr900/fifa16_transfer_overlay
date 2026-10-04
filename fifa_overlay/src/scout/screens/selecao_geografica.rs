//! Filtro geográfico (Story 2.9, refeito em 2026-10-03 a pedido do
//! Felipe): tela cheia sobre o formulário Nova Missão, com tudo numa tela
//! só — continente → país → ligas — em vez do mapa de nacionalidades.
//!
//! O filtro é **onde o jogador joga**: só aparecem países que têm ligas
//! com clubes no save, e as ligas de cada um (ver `save_repo::Liga`). Cada
//! nível é um botão que entra/sai da seleção, sem tecla modificadora:
//! "Europa inteira", "Todas" (o país inteiro) ou uma liga. O que já está
//! incluído por um nível acima aparece marcado e não muda ao ser ativado
//! (o tooltip diz por quê). "Confirmar" ou B volta ao formulário; o resumo
//! do rodapé (Qualidade, precisão, custo, prazo) muda ao vivo: quanto
//! mais amplo, menos preciso (FR4).

use imgui::{StyleColor, Ui};

use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, nova_missao};
use crate::scout::quality::AmplitudeGeografica;
use crate::scout::state::{Carga, Confederacao, FiltrosMissao, Liga, ScoutState};

const LARGURA_PAIS: f32 = 190.0;
const RECUO: f32 = 16.0;

pub const MSG_TODOS: &str = "Nada escolhido: o Olheiro procura em todas as ligas.";
pub const MSG_LENDO: &str = "Lendo as ligas do save…";
pub const MSG_ERRO: &str = "Não foi possível ler as ligas do save ativo.";

/// Nome da amplitude para o jogador.
pub fn nome_amplitude(amplitude: AmplitudeGeografica) -> &'static str {
    match amplitude {
        AmplitudeGeografica::Pais => "um país",
        AmplitudeGeografica::VariosPaises => "vários países",
        AmplitudeGeografica::Continente => "um continente inteiro",
        AmplitudeGeografica::Mundo => "mundo",
    }
}

/// Nome da liga sem o país na frente ("England Premier League" →
/// "Premier League").
pub fn nome_curto(liga: &Liga) -> String {
    let prefixo = format!("{} ", liga.pais_nome);
    match liga.nome.strip_prefix(&prefixo) {
        Some(resto) if !liga.pais_nome.is_empty() && !resto.is_empty() => resto.to_string(),
        _ => liga.nome.clone(),
    }
}

/// Resumo da seleção para a linha do formulário: "O mundo todo",
/// "Europa inteira", "Brazil, Premier League (England)", "… e mais 3".
pub fn resumo_geografia(filtros: &FiltrosMissao, ligas: &[Liga]) -> String {
    let mut partes: Vec<String> = filtros.continentes.iter().map(|c| format!("{} inteira", c.nome())).collect();
    partes.extend(filtros.paises_dos_clubes.iter().map(|p| {
        ligas.iter().find(|l| l.pais == Some(*p)).map_or_else(|| format!("País {p}"), |l| l.pais_nome.clone())
    }));
    partes.extend(filtros.ligas.iter().map(|id| match ligas.iter().find(|l| l.id == *id) {
        Some(l) if !l.pais_nome.is_empty() => format!("{} ({})", nome_curto(l), l.pais_nome),
        Some(l) => l.nome.clone(),
        None => format!("Liga {id}"),
    }));
    match partes.as_slice() {
        [] => "O mundo todo".to_string(),
        [um] => um.clone(),
        [a, b] => format!("{a}, {b}"),
        [a, b, resto @ ..] => format!("{a}, {b} e mais {}", resto.len()),
    }
}

/// O que foi ativado na tela neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clique {
    Continente(Confederacao),
    Pais(u16),
    Liga(u32),
}

/// Desenha o painel; `true` = voltar ao formulário.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> bool {
    let Some(previa) = state.previa_missao() else {
        return true;
    };
    let filtros = previa.rascunho.filtros.clone();
    let mut voltar = false;
    let mut limpar = false;
    let mut reler = false;
    let mut clique = None;
    let ligas = state.listar_ligas();

    let altura = (ui.content_region_avail()[1] - nova_missao::ALTURA_RESUMO).max(160.0);
    ui.child_window("##selecao_geografica").size([0.0, altura]).border(false).flags(super::flags_conteudo()).build(|| {
        if componentes::botao(ui, fonts, "Confirmar", EstiloBotao::Primario, true) {
            voltar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        if componentes::botao(ui, fonts, "Limpar", EstiloBotao::Secundario, filtros.tem_geografia()) {
            limpar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Filtro geográfico"));
        let linha = match &ligas {
            Carga::Pronto(ligas) if filtros.tem_geografia() => format!(
                "Onde o jogador joga: {} · amplitude: {}",
                resumo_geografia(&filtros, ligas),
                nome_amplitude(previa.amplitude)
            ),
            _ => MSG_TODOS.to_string(),
        };
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, &linha));
        ui.dummy([0.0, theme::ESPACO_1]);

        match &ligas {
            Carga::Carregando => com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_LENDO)),
            Carga::Erro => {
                com_fonte(ui, fonts.map(|f| f.body), || ui.text(MSG_ERRO));
                if componentes::botao(ui, fonts, "Tentar novamente", EstiloBotao::Primario, true) {
                    reler = true;
                }
            }
            Carga::Pronto(ligas) => {
                ui.child_window("##arvore").size([0.0, 0.0]).border(false).flags(super::flags_conteudo()).build(|| {
                    clique = arvore(ui, fonts, ligas, &filtros);
                });
            }
        }
    });

    {
        let _c = ui.push_style_color(StyleColor::Separator, theme::BORDER_HAIRLINE);
        ui.separator();
    }
    ui.dummy([0.0, theme::ESPACO_1]);
    nova_missao::resumo(ui, fonts, &previa);

    match clique {
        Some(Clique::Continente(c)) => state.alternar_continente_da_missao(c),
        Some(Clique::Pais(p)) => state.alternar_pais_do_clube_da_missao(p),
        Some(Clique::Liga(id)) => state.alternar_liga_da_missao(id),
        None => {}
    }
    if limpar {
        state.limpar_geografia_da_missao();
    }
    if reler {
        state.reler_ligas();
    }
    voltar
}

/// Países de um continente (com o nome), em ordem alfabética.
pub fn paises_do_continente(ligas: &[Liga], continente: Confederacao) -> Vec<(u16, String)> {
    let mut paises: Vec<(u16, String)> = ligas
        .iter()
        .filter(|l| l.continente == continente)
        .filter_map(|l| Some((l.pais?, l.pais_nome.clone())))
        .collect();
    paises.sort_by(|a, b| a.1.cmp(&b.1));
    paises.dedup();
    paises
}

/// Continente → países → ligas. Devolve o que foi ativado.
fn arvore(ui: &Ui, fonts: Option<&Fonts>, ligas: &[Liga], filtros: &FiltrosMissao) -> Option<Clique> {
    let mut clique = None;
    for continente in Confederacao::TODAS {
        let do_continente: Vec<&Liga> = ligas.iter().filter(|l| l.continente == continente).collect();
        if do_continente.is_empty() {
            continue;
        }
        let _id = ui.push_id(continente.nome());
        let continente_todo = filtros.continentes.contains(&continente);
        ui.dummy([0.0, theme::ESPACO_2]);
        let inicio = ui.cursor_pos();
        com_fonte(ui, fonts.map(|f| f.heading), || {
            ui.set_cursor_pos([inicio[0], inicio[1] + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
            ui.text(continente.nome());
        });
        ui.same_line_with_spacing(inicio[0] + LARGURA_PAIS, 0.0);
        ui.set_cursor_pos([ui.cursor_pos()[0], inicio[1]]);
        let rotulo = format!("{} inteira", continente.nome());
        if chip(ui, fonts, &rotulo, continente_todo, None) {
            clique = Some(Clique::Continente(continente));
        }

        for (pais, nome) in paises_do_continente(ligas, continente) {
            let _p = ui.push_id_usize(usize::from(pais));
            let pais_todo = filtros.paises_dos_clubes.contains(&pais);
            let incluido_por = continente_todo.then(|| format!("{} inteira", continente.nome()));
            let ligas_do_pais: Vec<&Liga> = do_continente.iter().copied().filter(|l| l.pais == Some(pais)).collect();
            if let Some(c) = linha_de_pais(ui, fonts, &nome, pais, pais_todo, incluido_por.as_deref(), &ligas_do_pais, filtros) {
                clique = Some(c);
            }
        }
        let sem_pais: Vec<&Liga> = do_continente.iter().copied().filter(|l| l.pais.is_none()).collect();
        if !sem_pais.is_empty() {
            let incluido_por = continente_todo.then(|| format!("{} inteira", continente.nome()));
            if let Some(c) = linha_de_ligas(ui, fonts, "Outros clubes", &sem_pais, incluido_por.as_deref(), filtros) {
                clique = Some(c);
            }
        }
    }
    clique
}

/// Uma linha de país: nome, "Todas" (o país inteiro) e as ligas.
#[allow(clippy::too_many_arguments)]
fn linha_de_pais(
    ui: &Ui,
    fonts: Option<&Fonts>,
    nome: &str,
    pais: u16,
    pais_todo: bool,
    incluido_por: Option<&str>,
    ligas: &[&Liga],
    filtros: &FiltrosMissao,
) -> Option<Clique> {
    let mut clique = None;
    let inicio = ui.cursor_pos();
    rotulo_de_linha(ui, fonts, nome, inicio);
    ui.same_line_with_spacing(inicio[0] + LARGURA_PAIS, 0.0);
    ui.set_cursor_pos([ui.cursor_pos()[0], inicio[1]]);
    if chip(ui, fonts, "Todas", pais_todo || incluido_por.is_some(), incluido_por) {
        clique = Some(Clique::Pais(pais));
    }
    let nome_pais = format!("{nome} inteiro");
    let incluida_por = incluido_por.or(pais_todo.then_some(nome_pais.as_str()));
    if let Some(c) = chips_de_ligas(ui, fonts, ligas, incluida_por, filtros, inicio[0] + LARGURA_PAIS) {
        clique = Some(c);
    }
    clique
}

/// Uma linha só de ligas (as "Outros clubes" de um continente).
fn linha_de_ligas(ui: &Ui, fonts: Option<&Fonts>, nome: &str, ligas: &[&Liga], incluida_por: Option<&str>, filtros: &FiltrosMissao) -> Option<Clique> {
    let inicio = ui.cursor_pos();
    rotulo_de_linha(ui, fonts, nome, inicio);
    // âncora sem largura: o primeiro chip entra com o mesmo espaço dos outros
    ui.set_cursor_pos([inicio[0] + LARGURA_PAIS - theme::ESPACO_2, inicio[1]]);
    ui.dummy([0.0, theme::ALVO_MINIMO]);
    chips_de_ligas(ui, fonts, ligas, incluida_por, filtros, inicio[0] + LARGURA_PAIS)
}

fn rotulo_de_linha(ui: &Ui, fonts: Option<&Fonts>, nome: &str, inicio: [f32; 2]) {
    com_fonte(ui, fonts.map(|f| f.body), || {
        ui.set_cursor_pos([inicio[0] + RECUO, inicio[1] + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text_colored(theme::TEXT_SECONDARY, nome);
    });
}

/// Os chips das ligas na mesma linha, quebrando para a linha de baixo
/// (alinhados em `x_recuo`) quando não cabem.
fn chips_de_ligas(
    ui: &Ui,
    fonts: Option<&Fonts>,
    ligas: &[&Liga],
    incluida_por: Option<&str>,
    filtros: &FiltrosMissao,
    x_recuo: f32,
) -> Option<Clique> {
    let mut clique = None;
    let direita = ui.window_content_region_max()[0];
    for liga in ligas {
        let nome = nome_curto(liga);
        let largura = largura_do_chip(ui, fonts, &nome);
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        if ui.cursor_pos()[0] + largura > direita {
            ui.new_line();
            ui.set_cursor_pos([x_recuo, ui.cursor_pos()[1]]);
        }
        let _id = ui.push_id_usize(liga.id as usize);
        let escolhida = incluida_por.is_some() || filtros.ligas.contains(&liga.id);
        if chip(ui, fonts, &nome, escolhida, incluida_por) {
            clique = Some(Clique::Liga(liga.id));
        }
        if ui.is_item_hovered() && incluida_por.is_none() {
            ui.tooltip_text(format!("{} · {} clubes · divisão {}", liga.nome, liga.clubes, liga.nivel));
        }
    }
    clique
}

fn largura_do_chip(ui: &Ui, fonts: Option<&Fonts>, texto: &str) -> f32 {
    com_fonte(ui, fonts.map(|f| f.heading), || ui.calc_text_size(texto)[0]) + theme::ESPACO_4 * 2.0
}

/// Botão que entra/sai da seleção. `incluido_por`: já está dentro de um
/// nível acima (aparece marcado; o tooltip diz qual).
fn chip(ui: &Ui, fonts: Option<&Fonts>, texto: &str, escolhido: bool, incluido_por: Option<&str>) -> bool {
    let estilo = if escolhido { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
    let largura = largura_do_chip(ui, fonts, texto);
    let ativou = componentes::botao_com_largura(ui, fonts, texto, estilo, true, Some(largura));
    if let (Some(por), true) = (incluido_por, ui.is_item_hovered()) {
        ui.tooltip_text(format!("Já incluído: {por}"));
    }
    ativou
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liga(id: u32, nome: &str, pais: Option<(u16, &str)>, continente: Confederacao) -> Liga {
        Liga {
            id,
            nome: nome.to_string(),
            pais: pais.map(|p| p.0),
            pais_nome: pais.map(|p| p.1.to_string()).unwrap_or_default(),
            continente,
            nivel: 1,
            clubes: 20,
        }
    }

    fn ligas() -> Vec<Liga> {
        vec![
            liga(13, "England Premier League", Some((14, "England")), Confederacao::Europa),
            liga(14, "England FL Championship", Some((14, "England")), Confederacao::Europa),
            liga(53, "Spain LaLiga EA Sports", Some((45, "Spain")), Confederacao::Europa),
            liga(77, "Clubes da UEFA", None, Confederacao::Europa),
            liga(7, "Brasileirão", Some((54, "Brazil")), Confederacao::AmericaDoSul),
        ]
    }

    #[test]
    fn league_names_drop_the_country_prefix() {
        let l = ligas();
        assert_eq!(nome_curto(&l[0]), "Premier League");
        assert_eq!(nome_curto(&l[4]), "Brasileirão");
        assert_eq!(nome_curto(&l[3]), "Clubes da UEFA");
    }

    #[test]
    fn countries_come_only_from_leagues_with_clubs() {
        let l = ligas();
        assert_eq!(paises_do_continente(&l, Confederacao::Europa), vec![(14, "England".to_string()), (45, "Spain".to_string())]);
        assert!(paises_do_continente(&l, Confederacao::Africa).is_empty());
    }

    #[test]
    fn the_form_row_summarises_continents_countries_and_leagues() {
        let l = ligas();
        let mut f = FiltrosMissao::default();
        assert_eq!(resumo_geografia(&f, &l), "O mundo todo");
        f.continentes = vec![Confederacao::Europa];
        assert_eq!(resumo_geografia(&f, &l), "Europa inteira");
        f.paises_dos_clubes = vec![54];
        f.ligas = vec![13, 77];
        assert_eq!(resumo_geografia(&f, &l), "Europa inteira, Brazil e mais 2");
        f.continentes.clear();
        f.paises_dos_clubes.clear();
        f.ligas = vec![13];
        assert_eq!(resumo_geografia(&f, &l), "Premier League (England)");
        assert!(!MSG_TODOS.contains('!') && !MSG_ERRO.contains('!'));
    }
}
