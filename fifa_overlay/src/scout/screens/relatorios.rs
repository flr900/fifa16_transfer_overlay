//! Aba Relatórios (Story 2.5): um card por Relatório, do mais novo para o
//! mais antigo, com a Missão, o Olheiro, a Qualidade e o indicador "novo"
//! até ser aberto pela primeira vez. Ativar o card abre o Relatório.

use imgui::Ui;
use uuid::Uuid;

use super::componentes::{badge_novo, badge_qualidade, badge_tier, card, desenhar_badge, texto_em};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::scout::state::{RelatorioNaLista, ScoutState};

const ALTURA_CARD: f32 = 76.0;

pub const MSG_SEM_RELATORIOS: &str =
    "Nenhum Relatório ainda. Quando o prazo de uma Missão passar, o Relatório aparece aqui.";

/// Linha de detalhes do card: "17 jogadores · gerado em 03/07/2026 · Rápida".
pub fn detalhe_card(item: &RelatorioNaLista) -> String {
    let mut partes = vec![super::aviso::texto_jogadores(item.relatorio.jogadores.len())];
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
    // Tabular / Cards sempre visível no topo (Story 2.6): vale para os
    // Relatórios abertos a partir daqui.
    super::relatorio::alternador_densidade(ui, fonts, state);
    ui.dummy([0.0, theme::ESPACO_2]);
    let lista = state.relatorios(false);
    if lista.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_RELATORIOS));
        return None;
    }
    let mut abrir = None;
    for item in &lista {
        if card_relatorio(ui, fonts, item) {
            abrir = Some(item.relatorio.id);
        }
    }
    abrir
}

/// Card de um Relatório; `true` = ativado (clique ou A).
pub fn card_relatorio(ui: &Ui, fonts: Option<&Fonts>, item: &RelatorioNaLista) -> bool {
    let c = card(ui, &item.relatorio.id.to_string(), ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let dl = ui.get_window_draw_list();
    let x = c.min[0] + theme::ESPACO_4;
    let mut y = c.min[1] + theme::ESPACO_3;

    let titulo = super::relatorio::titulo(item);
    let [largura, altura] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &titulo);
    let mut xb = x + largura + theme::ESPACO_2;
    if let Some(o) = &item.olheiro {
        xb += desenhar_badge(ui, fonts, &dl, &badge_tier(o.tier), [xb, y], altura)[0] + theme::ESPACO_2;
    }
    if !item.relatorio.aberto {
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
        let item = RelatorioNaLista { relatorio, missao: Some(missao), olheiro: None };
        assert_eq!(detalhe_card(&item), "nenhum jogador · gerado em 03/07/2026 · Rápida");
        assert!(!MSG_SEM_RELATORIOS.contains('!'));
    }
}
