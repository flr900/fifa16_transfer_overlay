//! Filtro geográfico (Story 2.9, refeito em 2026-10-03 a pedido do
//! Felipe): **onde o jogador joga** — continente → país → liga —, só com
//! países que têm ligas com clubes no save (ver `save_repo::Liga`).
//!
//! Em níveis, para a tela não ficar poluída (segundo pedido do mesmo
//! dia):
//! - **Continentes** (o filtro rápido): uma linha por continente, com
//!   "Inteiro" (entra/sai da seleção) e, ao lado, "Países e ligas ›", que
//!   desce para escolher só parte dele;
//! - **um continente**: uma linha por país, com "Inteiro" e "Ligas ›", e as
//!   ligas sem país ("Clubes da UEFA") como linhas próprias;
//! - **um país**: uma linha por liga.
//!
//! Cada linha tem as mesmas colunas (nome, botão de escolha, botão de
//! descer, resumo), alinhadas ao centro da linha. O que já está incluído
//! por um nível acima aparece marcado e não muda ao ser ativado (o tooltip
//! diz por quê). B sobe um nível; no topo, volta ao formulário, como
//! "Confirmar". O resumo do rodapé (Qualidade, precisão, custo, prazo)
//! muda ao vivo: quanto mais amplo, menos preciso (FR4).

use imgui::{StyleColor, Ui};

use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, nova_missao};
use crate::scout::quality::AmplitudeGeografica;
use crate::scout::state::{Carga, Confederacao, FiltrosMissao, FocoGeografico, Liga, ScoutState};

const COLUNA_NOME: f32 = 240.0;
const LARGURA_ESCOLHA: f32 = 130.0;
const LARGURA_DESCER: f32 = 200.0;

pub const MSG_TODOS: &str = "Nada escolhido: o Olheiro procura em todas as ligas.";
pub const MSG_LENDO: &str = "Lendo as ligas do save…";
pub const MSG_ERRO: &str = "Não foi possível ler as ligas do save ativo.";
const MSG_DICA: &str = "Filtro rápido por continente. Para escolher só alguns países ou ligas, use \"Países e ligas ›\".";

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

/// Resumo de um continente na lista rápida: "inteiro", "2 escolhidos de
/// 12 países" ou "12 países · 28 ligas".
pub fn resumo_continente(ligas: &[Liga], continente: Confederacao, filtros: &FiltrosMissao) -> String {
    let paises = paises_do_continente(ligas, continente);
    let do_continente: Vec<&Liga> = ligas.iter().filter(|l| l.continente == continente).collect();
    if filtros.continentes.contains(&continente) {
        return format!("inteiro: {} países, {} ligas", paises.len(), do_continente.len());
    }
    let escolhidos = paises.iter().filter(|(p, _)| filtros.paises_dos_clubes.contains(p)).count()
        + do_continente.iter().filter(|l| filtros.ligas.contains(&l.id)).count();
    if escolhidos > 0 {
        format!("{escolhidos} escolhido(s) aqui dentro")
    } else {
        format!("{} países · {} ligas", paises.len(), do_continente.len())
    }
}

/// Resumo de um país: "inteiro", "escolhidas: Premier League" ou "4 ligas".
pub fn resumo_pais(ligas: &[Liga], pais: u16, filtros: &FiltrosMissao, incluido: bool) -> String {
    let do_pais: Vec<&Liga> = ligas.iter().filter(|l| l.pais == Some(pais)).collect();
    if incluido || filtros.paises_dos_clubes.contains(&pais) {
        return format!("inteiro: {} liga(s)", do_pais.len());
    }
    let escolhidas: Vec<String> = do_pais.iter().filter(|l| filtros.ligas.contains(&l.id)).map(|l| nome_curto(l)).collect();
    if escolhidas.is_empty() {
        format!("{} liga(s)", do_pais.len())
    } else {
        format!("escolhidas: {}", escolhidas.join(", "))
    }
}

/// O que foi ativado na tela neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clique {
    Continente(Confederacao),
    Pais(u16),
    Liga(u32),
    Focar(FocoGeografico),
    Subir,
}

/// Desenha o painel; `true` = voltar ao formulário.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> bool {
    let Some(previa) = state.previa_missao() else {
        return true;
    };
    let filtros = previa.rascunho.filtros.clone();
    let foco = state.foco_geografico();
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
        ui.dummy([0.0, theme::ESPACO_2]);

        match &ligas {
            Carga::Carregando => com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_LENDO)),
            Carga::Erro => {
                com_fonte(ui, fonts.map(|f| f.body), || ui.text(MSG_ERRO));
                if componentes::botao(ui, fonts, "Tentar novamente", EstiloBotao::Primario, true) {
                    reler = true;
                }
            }
            Carga::Pronto(ligas) => {
                let rolagem = state.rolagem();
                ui.child_window("##niveis").size([0.0, 0.0]).border(false).flags(super::flags_conteudo()).build(|| {
        super::rolar_com_analogico(ui, rolagem);
                    clique = match foco {
                        FocoGeografico::Continentes => nivel_continentes(ui, fonts, ligas, &filtros),
                        FocoGeografico::Continente(c) => nivel_continente(ui, fonts, ligas, &filtros, c),
                        FocoGeografico::Pais(c, p) => nivel_pais(ui, fonts, ligas, &filtros, c, p),
                    };
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
        Some(Clique::Focar(f)) => state.focar_geografia(f),
        Some(Clique::Subir) => {
            state.subir_foco_geografico();
        }
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

/// Lista rápida: um continente por linha.
fn nivel_continentes(ui: &Ui, fonts: Option<&Fonts>, ligas: &[Liga], filtros: &FiltrosMissao) -> Option<Clique> {
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, MSG_DICA));
    ui.dummy([0.0, theme::ESPACO_1]);
    let mut clique = None;
    for continente in Confederacao::TODAS {
        if !ligas.iter().any(|l| l.continente == continente) {
            continue;
        }
        let _id = ui.push_id(continente.nome());
        let escolha = Escolha { texto: "Inteiro", escolhido: filtros.continentes.contains(&continente), incluido_por: None };
        let (escolheu, desceu) =
            linha(ui, fonts, continente.nome(), escolha, Some("Países e ligas ›"), &resumo_continente(ligas, continente, filtros));
        if escolheu {
            clique = Some(Clique::Continente(continente));
        }
        if desceu {
            clique = Some(Clique::Focar(FocoGeografico::Continente(continente)));
        }
    }
    clique
}

/// Um continente: o continente inteiro, os países e as ligas sem país.
fn nivel_continente(ui: &Ui, fonts: Option<&Fonts>, ligas: &[Liga], filtros: &FiltrosMissao, continente: Confederacao) -> Option<Clique> {
    let mut clique = cabecalho_nivel(ui, fonts, "‹ Continentes", continente.nome());
    let inteiro = filtros.continentes.contains(&continente);
    let escolha = Escolha { texto: "Inteiro", escolhido: inteiro, incluido_por: None };
    if linha(ui, fonts, &format!("{} inteira", continente.nome()), escolha, None, "").0 {
        clique = Some(Clique::Continente(continente));
    }
    separador(ui);
    let por = format!("{} inteira", continente.nome());
    let incluido_por = inteiro.then_some(por.as_str());
    for (pais, nome) in paises_do_continente(ligas, continente) {
        let _id = ui.push_id_usize(usize::from(pais));
        let escolha = Escolha { texto: "Inteiro", escolhido: inteiro || filtros.paises_dos_clubes.contains(&pais), incluido_por };
        let resumo = resumo_pais(ligas, pais, filtros, inteiro);
        let (escolheu, desceu) = linha(ui, fonts, &nome, escolha, Some("Ligas ›"), &resumo);
        if escolheu {
            clique = Some(Clique::Pais(pais));
        }
        if desceu {
            clique = Some(Clique::Focar(FocoGeografico::Pais(continente, pais)));
        }
    }
    for liga in ligas.iter().filter(|l| l.continente == continente && l.pais.is_none()) {
        let _id = ui.push_id_usize(liga.id as usize);
        let escolha = Escolha { texto: "Incluir", escolhido: inteiro || filtros.ligas.contains(&liga.id), incluido_por };
        if linha(ui, fonts, &liga.nome, escolha, None, &format!("{} clubes, sem país definido", liga.clubes)).0 {
            clique = Some(Clique::Liga(liga.id));
        }
    }
    clique
}

/// Um país: o país inteiro e as ligas dele.
fn nivel_pais(
    ui: &Ui,
    fonts: Option<&Fonts>,
    ligas: &[Liga],
    filtros: &FiltrosMissao,
    continente: Confederacao,
    pais: u16,
) -> Option<Clique> {
    let nome = ligas.iter().find(|l| l.pais == Some(pais)).map_or_else(String::new, |l| l.pais_nome.clone());
    let voltar = format!("‹ {}", continente.nome());
    let mut clique = cabecalho_nivel(ui, fonts, &voltar, &nome);
    let por_continente = filtros.continentes.contains(&continente).then(|| format!("{} inteira", continente.nome()));
    let pais_todo = filtros.paises_dos_clubes.contains(&pais);
    let escolha = Escolha { texto: "Inteiro", escolhido: pais_todo || por_continente.is_some(), incluido_por: por_continente.as_deref() };
    if linha(ui, fonts, &format!("{nome} inteiro"), escolha, None, "").0 {
        clique = Some(Clique::Pais(pais));
    }
    separador(ui);
    let por_pais = format!("{nome} inteiro");
    let incluido_por = por_continente.as_deref().or(pais_todo.then_some(por_pais.as_str()));
    for liga in ligas.iter().filter(|l| l.pais == Some(pais)) {
        let _id = ui.push_id_usize(liga.id as usize);
        let escolha = Escolha { texto: "Incluir", escolhido: incluido_por.is_some() || filtros.ligas.contains(&liga.id), incluido_por };
        let detalhe = format!("divisão {} · {} clubes", liga.nivel, liga.clubes);
        if linha(ui, fonts, &nome_curto(liga), escolha, None, &detalhe).0 {
            clique = Some(Clique::Liga(liga.id));
        }
    }
    clique
}

/// "‹ Voltar" de nível + título do nível.
fn cabecalho_nivel(ui: &Ui, fonts: Option<&Fonts>, voltar: &str, titulo: &str) -> Option<Clique> {
    let clicou = componentes::botao(ui, fonts, voltar, EstiloBotao::Secundario, true);
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    let y = ui.cursor_pos()[1];
    com_fonte(ui, fonts.map(|f| f.heading), || {
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text(titulo);
    });
    ui.dummy([0.0, theme::ESPACO_1]);
    clicou.then_some(Clique::Subir)
}

fn separador(ui: &Ui) {
    let _c = ui.push_style_color(StyleColor::Separator, theme::BORDER_HAIRLINE_SUBTLE);
    ui.separator();
    ui.dummy([0.0, theme::ESPACO_1]);
}

/// O botão de escolha de uma linha.
struct Escolha<'a> {
    texto: &'a str,
    escolhido: bool,
    /// Já incluído por um nível acima (aparece marcado; o tooltip diz qual).
    incluido_por: Option<&'a str>,
}

/// Uma linha com colunas fixas — nome | escolha | descer | resumo —, tudo
/// centrado na altura da linha. Devolve (escolheu, desceu).
fn linha(ui: &Ui, fonts: Option<&Fonts>, nome: &str, escolha: Escolha<'_>, descer: Option<&str>, resumo: &str) -> (bool, bool) {
    let [x0, y0] = ui.cursor_pos();
    let centro = |altura: f32| y0 + ((theme::ALVO_MINIMO - altura) * 0.5).max(0.0);
    com_fonte(ui, fonts.map(|f| f.body), || {
        ui.set_cursor_pos([x0, centro(ui.text_line_height())]);
        ui.text(nome);
    });

    ui.set_cursor_pos([x0 + COLUNA_NOME, y0]);
    let estilo = if escolha.escolhido { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
    let escolheu = componentes::botao_com_largura(ui, fonts, escolha.texto, estilo, true, Some(LARGURA_ESCOLHA));
    if let (Some(por), true) = (escolha.incluido_por, ui.is_item_hovered()) {
        ui.tooltip_text(format!("Já incluído: {por}"));
    }
    let mut x = x0 + COLUNA_NOME + LARGURA_ESCOLHA + theme::ESPACO_2;
    let mut desceu = false;
    if let Some(texto) = descer {
        ui.set_cursor_pos([x, y0]);
        desceu = componentes::botao_com_largura(ui, fonts, texto, EstiloBotao::Secundario, true, Some(LARGURA_DESCER));
        x += LARGURA_DESCER + theme::ESPACO_2;
    }
    if !resumo.is_empty() {
        com_fonte(ui, fonts.map(|f| f.meta), || {
            ui.set_cursor_pos([x + theme::ESPACO_2, centro(ui.text_line_height())]);
            ui.text_colored(theme::TEXT_SECONDARY, resumo);
        });
    }
    ui.set_cursor_pos([x0, y0 + theme::ALVO_MINIMO + theme::ESPACO_2]);
    ui.dummy([0.0, 0.0]);
    (escolheu && escolha.incluido_por.is_none(), desceu)
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

    #[test]
    fn level_summaries_say_what_is_chosen_inside() {
        let l = ligas();
        let mut f = FiltrosMissao::default();
        assert_eq!(resumo_continente(&l, Confederacao::Europa, &f), "2 países · 4 ligas");
        assert_eq!(resumo_pais(&l, 14, &f, false), "2 liga(s)");
        f.ligas = vec![13];
        assert_eq!(resumo_continente(&l, Confederacao::Europa, &f), "1 escolhido(s) aqui dentro");
        assert_eq!(resumo_pais(&l, 14, &f, false), "escolhidas: Premier League");
        f.continentes = vec![Confederacao::Europa];
        assert_eq!(resumo_continente(&l, Confederacao::Europa, &f), "inteiro: 2 países, 4 ligas");
        assert_eq!(resumo_pais(&l, 14, &f, true), "inteiro: 2 liga(s)");
    }
}
