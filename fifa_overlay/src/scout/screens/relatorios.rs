//! Aba Relatórios (Story 2.5; refeita em 2026-10-08): os Relatórios que os
//! Olheiros entregaram, em dois jeitos de ver.
//!
//! - **Por Relatório** (padrão): um card por Relatório, do mais novo para o
//!   mais antigo, com a Missão, o Olheiro, a Qualidade e o indicador "novo"
//!   até ser aberto pela primeira vez. Ativar o card abre o Relatório.
//! - **Por Olheiro**: os mesmos cards, agrupados sob o Olheiro que os
//!   entregou (o que entregou o Relatório mais novo vem primeiro).
//!
//! "Ativos / Arquivados" vale para os dois. A lista de jogadores que os
//! Olheiros já encontraram saiu daqui: é a aba Base do Scout. Aqui, como nas
//! listas de jogadores, trocar de visão só vale com o clique (ou o A).

use imgui::Ui;
use uuid::Uuid;

use super::componentes::{self, badge_novo, badge_qualidade, badge_tier, card_com_largura, desenhar_badge, texto_em, EstiloBotao};
use super::olheiros;
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::scout::state::{Olheiro, RelatorioNaLista, ScoutState, VisaoRelatorios};

const ALTURA_CARD: f32 = 76.0;
/// Coluna do botão Arquivar/Restaurar à direita de cada card (Story 2.7).
const LARGURA_ACAO: f32 = 130.0;
const LARGURA_VISAO: f32 = 150.0;

pub const MSG_SEM_ARQUIVADOS: &str = "Nenhum Relatório arquivado.";

pub const MSG_SEM_RELATORIOS: &str =
    "Nenhum Relatório ainda. Quando o prazo de uma Missão passar, o Relatório aparece aqui.";

/// O que o jogador fez na aba neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    /// Abrir o Relatório (ativou um card).
    AbrirRelatorio(Uuid),
}

/// Linha de detalhes do card: "17 jogadores · gerado em 03/07/2026 · Rápida"
/// (ou "parcial: 9 de 17 jogadores" com a Missão ainda rodando).
pub fn detalhe_card(item: &RelatorioNaLista) -> String {
    let n = item.relatorio.jogadores.len();
    let mut partes = vec![if item.parcial {
        format!("parcial: {n} de {} jogadores", item.previstos)
    } else {
        super::aviso::texto_jogadores(n)
    }];
    if let Some(data) = item.relatorio.gerado_em {
        partes.push(format!("gerado em {}", formatar_data(data)));
    }
    if let Some(m) = &item.missao {
        partes.push(super::missoes::nome_modo(m.modo_busca).to_string());
    }
    partes.join(" · ")
}

/// Os Relatórios agrupados por Olheiro, na ordem em que cada Olheiro
/// aparece pela primeira vez (a lista chega do mais novo para o mais
/// antigo, então vem primeiro quem entregou o Relatório mais recente).
/// Relatório de Olheiro que já não existe fica num grupo sem nome.
pub fn agrupar_por_olheiro(lista: &[RelatorioNaLista]) -> Vec<(Option<&Olheiro>, Vec<&RelatorioNaLista>)> {
    let mut grupos: Vec<(Option<&Olheiro>, Vec<&RelatorioNaLista>)> = Vec::new();
    for item in lista {
        let id = item.olheiro.as_ref().map(|o| o.id);
        match grupos.iter_mut().find(|(o, _)| o.map(|o| o.id) == id) {
            Some((_, itens)) => itens.push(item),
            None => grupos.push((item.olheiro.as_ref(), vec![item])),
        }
    }
    grupos
}

/// Desenha a aba.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    // visão (por Relatório / por Olheiro) e filtro Ativos / Arquivados (Story 2.7)
    let visao = state.visao_dos_relatorios();
    let atual = usize::from(visao == VisaoRelatorios::PorOlheiro);
    match componentes::alternador_por_clique(ui, fonts, &["Por Relatório", "Por Olheiro"], atual, LARGURA_VISAO) {
        Some(0) => state.definir_visao_dos_relatorios(VisaoRelatorios::PorRelatorio),
        Some(_) => state.definir_visao_dos_relatorios(VisaoRelatorios::PorOlheiro),
        None => {}
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_5);
    let arquivados = state.vendo_arquivados();
    if let Some(indice) = componentes::alternador_por_clique(ui, fonts, &["Ativos", "Arquivados"], usize::from(arquivados), 130.0) {
        state.ver_arquivados(indice == 1);
    }
    ui.dummy([0.0, theme::ESPACO_2]);
    let arquivados = state.vendo_arquivados();
    let lista = state.relatorios(arquivados);
    if lista.is_empty() {
        let msg = if arquivados { MSG_SEM_ARQUIVADOS } else { MSG_SEM_RELATORIOS };
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, msg));
        return Acao::Nenhuma;
    }
    let mut acao = Acao::Nenhuma;
    // Arquivar/Restaurar mexe no estado: é feito depois de desenhar
    let mut mudar: Option<(Uuid, bool)> = None;
    // o foco vai para o primeiro card
    let mut foco = state.tomar_foco_no_principal();
    match state.visao_dos_relatorios() {
        VisaoRelatorios::PorRelatorio => {
            let itens: Vec<&RelatorioNaLista> = lista.iter().collect();
            lista_de_cards(ui, fonts, &itens, arquivados, &mut acao, &mut mudar, &mut foco);
        }
        VisaoRelatorios::PorOlheiro => {
            olheiros::com_nacoes(state, |nacoes| {
                for (olheiro, itens) in agrupar_por_olheiro(&lista) {
                    cabecalho_do_olheiro(ui, fonts, olheiro, &itens, nacoes);
                    lista_de_cards(ui, fonts, &itens, arquivados, &mut acao, &mut mudar, &mut foco);
                    ui.dummy([0.0, theme::ESPACO_2]);
                }
            });
        }
    }
    match mudar {
        Some((id, true)) => drop(state.restaurar_relatorio(id)),
        Some((id, false)) => drop(state.arquivar_relatorio(id)),
        None => {}
    }
    acao
}

/// O Olheiro que encabeça um grupo: bandeira, nome, nível e quantos
/// Relatórios (e quantos novos) ele tem.
fn cabecalho_do_olheiro(
    ui: &Ui,
    fonts: Option<&Fonts>,
    olheiro: Option<&Olheiro>,
    itens: &[&RelatorioNaLista],
    nacoes: &olheiros::Nacoes<'_>,
) {
    let dl = ui.get_window_draw_list();
    let pos = ui.cursor_screen_pos();
    let altura = com_fonte(ui, fonts.map(|f| f.heading), || ui.text_line_height());
    let mut x = pos[0];
    if let Some(n) = olheiro.and_then(|o| o.nacao.as_ref()) {
        let altura_bandeira = (altura * 0.72).round();
        x += componentes::bandeira(&dl, (nacoes.bandeira)(n.id), [x, pos[1] + (altura - altura_bandeira) * 0.5], altura_bandeira) + theme::ESPACO_2;
    }
    let nome = olheiro.map_or_else(|| "Olheiro removido".to_string(), Olheiro::nome_exibicao);
    let [w, _] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, pos[1]], theme::TEXT_PRIMARY, &nome);
    x += w + theme::ESPACO_2;
    if let Some(o) = olheiro {
        x += desenhar_badge(ui, fonts, &dl, &badge_tier(o.tier), [x, pos[1]], altura)[0] + theme::ESPACO_3;
    }
    let novos = itens.iter().filter(|i| i.novo).count();
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, pos[1] + 2.0], theme::TEXT_SECONDARY, &texto_do_grupo(itens.len(), novos));
    ui.dummy([0.0, altura + theme::ESPACO_2]);
}

/// "3 Relatórios · 1 novo".
pub fn texto_do_grupo(relatorios: usize, novos: usize) -> String {
    let r = if relatorios == 1 { "Relatório" } else { "Relatórios" };
    match novos {
        0 => format!("{relatorios} {r}"),
        1 => format!("{relatorios} {r} · 1 novo"),
        n => format!("{relatorios} {r} · {n} novos"),
    }
}

/// Os cards de Relatório, um embaixo do outro, cada um com o botão
/// Arquivar/Restaurar ao lado.
fn lista_de_cards(
    ui: &Ui,
    fonts: Option<&Fonts>,
    itens: &[&RelatorioNaLista],
    arquivados: bool,
    acao: &mut Acao,
    mudar: &mut Option<(Uuid, bool)>,
    foco: &mut bool,
) {
    for item in itens {
        if std::mem::take(foco) {
            componentes::focar_proximo_item();
        }
        if card_relatorio(ui, fonts, item) {
            *acao = Acao::AbrirRelatorio(item.relatorio.id);
        }
        // Arquivar só para Relatório já aberto; Restaurar no filtro.
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        let y = ui.cursor_pos()[1];
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (ALTURA_CARD - theme::ALVO_MINIMO) * 0.5]);
        let _id = ui.push_id(item.relatorio.id.to_string());
        if arquivados {
            if componentes::botao_com_largura(ui, fonts, "Restaurar", EstiloBotao::Secundario, true, Some(LARGURA_ACAO)) {
                *mudar = Some((item.relatorio.id, true));
            }
        } else if ScoutState::pode_arquivar(item) {
            if componentes::botao_com_largura(ui, fonts, "Arquivar", EstiloBotao::Secundario, true, Some(LARGURA_ACAO)) {
                *mudar = Some((item.relatorio.id, false));
            }
        } else {
            ui.dummy([LARGURA_ACAO, theme::ALVO_MINIMO]);
        }
        ui.set_cursor_pos([ui.cursor_pos()[0], y + ALTURA_CARD + theme::ESPACO_2]);
    }
}

/// Card de um Relatório; `true` = ativado (clique ou A).
pub fn card_relatorio(ui: &Ui, fonts: Option<&Fonts>, item: &RelatorioNaLista) -> bool {
    let largura = (ui.content_region_avail()[0] - LARGURA_ACAO - theme::ESPACO_2).max(200.0);
    let c = card_com_largura(ui, &item.relatorio.id.to_string(), largura, ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let dl = ui.get_window_draw_list();
    let x = c.min[0] + theme::ESPACO_4;
    let mut y = c.min[1] + theme::ESPACO_3;

    let titulo = super::relatorio::titulo(item);
    let [largura, altura] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &titulo);
    let mut xb = x + largura + theme::ESPACO_2;
    if let Some(o) = &item.olheiro {
        xb += desenhar_badge(ui, fonts, &dl, &badge_tier(o.tier), [xb, y], altura)[0] + theme::ESPACO_2;
    }
    if item.novo {
        desenhar_badge(ui, fonts, &dl, &badge_novo(), [xb, y], altura);
    }
    let qualidade = badge_qualidade(item.relatorio.qualidade);
    let largura_badge =
        com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(qualidade.texto)[0]) + theme::ESPACO_2 * 2.0;
    desenhar_badge(ui, fonts, &dl, &qualidade, [c.max[0] - theme::ESPACO_4 - largura_badge, y], altura);
    y += altura + theme::ESPACO_1;

    let olheiro = item.olheiro.as_ref().map_or_else(|| "Olheiro removido".to_string(), |o| o.nome_exibicao());
    let linha2 = format!("{olheiro} · {}", detalhe_card(item));
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::TEXT_SECONDARY, &linha2);
    c.ativou
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_repo::Date;
    use crate::scout::state::{Missao, Relatorio, StatusMissao};

    fn item(olheiro: Option<Olheiro>, novo: bool) -> RelatorioNaLista {
        let missao = Missao::de_teste(olheiro.as_ref().map_or_else(Uuid::new_v4, |o| o.id), StatusMissao::Concluida);
        RelatorioNaLista { relatorio: Relatorio::de_teste(missao.id), missao: Some(missao), olheiro, previstos: 0, parcial: false, novo }
    }

    #[test]
    fn card_details_and_empty_state() {
        let missao = Missao::de_teste(Uuid::new_v4(), StatusMissao::Concluida);
        let mut relatorio = Relatorio::de_teste(missao.id);
        relatorio.gerado_em = Some(Date(20260703));
        let item = RelatorioNaLista { relatorio, missao: Some(missao), olheiro: None, previstos: 17, parcial: false, novo: false };
        assert_eq!(detalhe_card(&item), "nenhum jogador · gerado em 03/07/2026 · Rápida");
        let parcial = RelatorioNaLista { parcial: true, ..item };
        assert_eq!(detalhe_card(&parcial), "parcial: 0 de 17 jogadores · gerado em 03/07/2026 · Rápida");
        assert!(!MSG_SEM_RELATORIOS.contains('!'));
        assert_eq!(MSG_SEM_ARQUIVADOS, "Nenhum Relatório arquivado.");
    }

    #[test]
    fn the_reports_are_grouped_under_the_olheiro_who_delivered_them_newest_first() {
        let ana = Olheiro { id: Uuid::new_v4(), nome: "Ana".to_string(), ..Olheiro::default() };
        let beto = Olheiro { id: Uuid::new_v4(), nome: "Beto".to_string(), ..Olheiro::default() };
        // a lista chega do mais novo para o mais antigo
        let lista = vec![
            item(Some(beto.clone()), true),
            item(Some(ana.clone()), false),
            item(Some(beto.clone()), false),
            item(None, false),
            item(Some(ana.clone()), false),
        ];
        let grupos = agrupar_por_olheiro(&lista);
        let nomes: Vec<Option<&str>> = grupos.iter().map(|(o, _)| o.map(|o| o.nome.as_str())).collect();
        assert_eq!(nomes, [Some("Beto"), Some("Ana"), None], "quem entregou o mais novo vem primeiro; sem Olheiro por último");
        assert_eq!(grupos.iter().map(|(_, itens)| itens.len()).collect::<Vec<_>>(), [2, 2, 1]);
    }

    #[test]
    fn the_group_text_counts_reports_and_new_ones() {
        assert_eq!(texto_do_grupo(1, 0), "1 Relatório");
        assert_eq!(texto_do_grupo(3, 1), "3 Relatórios · 1 novo");
        assert_eq!(texto_do_grupo(4, 2), "4 Relatórios · 2 novos");
    }
}
