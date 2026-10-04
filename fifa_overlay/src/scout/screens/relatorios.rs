//! Aba Relatórios (Story 2.5): um card por Relatório, do mais novo para o
//! mais antigo, com a Missão, o Olheiro, a Qualidade e o indicador "novo"
//! até ser aberto pela primeira vez. Ativar o card abre o Relatório.

use imgui::Ui;
use uuid::Uuid;

use super::componentes::{self, badge_novo, badge_qualidade, badge_tier, card_com_largura, desenhar_badge, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::scout::state::{RelatorioNaLista, ScoutState};

const ALTURA_CARD: f32 = 76.0;
/// Coluna do botão Arquivar/Restaurar à direita de cada card (Story 2.7).
const LARGURA_ACAO: f32 = 130.0;

pub const MSG_SEM_ARQUIVADOS: &str = "Nenhum Relatório arquivado.";

pub const MSG_SEM_RELATORIOS: &str =
    "Nenhum Relatório ainda. Quando o prazo de uma Missão passar, o Relatório aparece aqui.";

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

/// Desenha a aba; devolve o Relatório a abrir, se algum foi ativado.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Option<Uuid> {
    // Lista principal / "Arquivados" (Story 2.7).
    let arquivados = state.vendo_arquivados();
    if let Some(indice) = componentes::alternador(ui, fonts, &["Ativos", "Arquivados"], usize::from(arquivados), 130.0) {
        state.ver_arquivados(indice == 1);
    }
    ui.dummy([0.0, theme::ESPACO_2]);
    let arquivados = state.vendo_arquivados();
    let lista = state.relatorios(arquivados);
    if lista.is_empty() {
        let msg = if arquivados { MSG_SEM_ARQUIVADOS } else { MSG_SEM_RELATORIOS };
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, msg));
        return None;
    }
    let mut abrir = None;
    for item in &lista {
        if card_relatorio(ui, fonts, item) {
            abrir = Some(item.relatorio.id);
        }
        // Arquivar só para Relatório já aberto; Restaurar no filtro.
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        let y = ui.cursor_pos()[1];
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (ALTURA_CARD - theme::ALVO_MINIMO) * 0.5]);
        let _id = ui.push_id(item.relatorio.id.to_string());
        if arquivados {
            if componentes::botao_com_largura(ui, fonts, "Restaurar", EstiloBotao::Secundario, true, Some(LARGURA_ACAO)) {
                state.restaurar_relatorio(item.relatorio.id);
            }
        } else if ScoutState::pode_arquivar(item) {
            if componentes::botao_com_largura(ui, fonts, "Arquivar", EstiloBotao::Secundario, true, Some(LARGURA_ACAO)) {
                state.arquivar_relatorio(item.relatorio.id);
            }
        } else {
            ui.dummy([LARGURA_ACAO, theme::ALVO_MINIMO]);
        }
        ui.set_cursor_pos([ui.cursor_pos()[0], y + ALTURA_CARD + theme::ESPACO_2]);
    }
    abrir
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

    let olheiro = item.olheiro.as_ref().map_or("Olheiro removido", |o| o.especializacao.nome());
    let linha2 = format!("{olheiro} · {}", detalhe_card(item));
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::TEXT_SECONDARY, &linha2);
    c.ativou
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_repo::Date;
    use crate::scout::state::{Missao, Relatorio, StatusMissao};

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
}
