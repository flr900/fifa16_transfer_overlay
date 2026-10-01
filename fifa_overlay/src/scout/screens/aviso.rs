//! Banner do canto superior direito (Story 1.7): avisa, com o painel
//! FECHADO, que a Central de Scout foi injetada e em que pé está a
//! carreira (carregando / pronta / falhou).
//!
//! Não pode atrapalhar o jogo: janela sem foco, sem entrada de mouse ou
//! teclado (`NO_INPUTS`), sem decoração e sem animação — aparece e some
//! (EXPERIENCE.md: nada de transições longas). O tempo na tela é regra do
//! `scout::state` (`DURACAO_AVISO`); aqui é só o desenho. Cor nunca é o
//! único indicador: a barra lateral colorida acompanha sempre um título.

use imgui::{Condition, StyleColor, StyleVar, Ui, WindowFlags};

use super::theme::{self, Fonts};
use super::nova_missao::nome_tipo;
use super::{com_fonte, formatar_data};
use crate::scout::state::TipoAviso;

/// Distância do banner até as bordas da tela.
const MARGEM: f32 = 24.0;
/// Largura da barra colorida à esquerda do texto.
const LARGURA_BARRA: f32 = 4.0;

/// Título, detalhe e cor do banner para cada aviso.
pub fn textos(aviso: &TipoAviso) -> (String, String, [f32; 4]) {
    match aviso {
        TipoAviso::Injetado => (
            "Central de Scout ativa".to_string(),
            "F10 abre o painel.".to_string(),
            theme::ACCENT_PRIMARY,
        ),
        TipoAviso::Carregando => (
            "Carregando carreira…".to_string(),
            "Lendo orçamento e data do save ativo.".to_string(),
            theme::ACCENT_PRIMARY,
        ),
        TipoAviso::Pronta(carreira) => (
            "Carreira pronta".to_string(),
            format!("{} · {} · F10 abre o painel.", carreira.tecnico, formatar_data(carreira.data_atual)),
            theme::FIELD_GREEN,
        ),
        TipoAviso::Falhou => (
            "Não foi possível carregar a carreira.".to_string(),
            "F10 abre o painel para tentar de novo.".to_string(),
            theme::DANGER,
        ),
        TipoAviso::RelatorioPronto { tipo, jogadores } => (
            "Relatório pronto".to_string(),
            format!("Missão {}: {} · F10 abre o painel.", nome_tipo(*tipo), texto_jogadores(*jogadores)),
            theme::FIELD_GREEN,
        ),
        TipoAviso::BuscaFalhou => (
            "A busca de uma Missão falhou.".to_string(),
            "Ela roda de novo quando o painel abrir.".to_string(),
            theme::DANGER,
        ),
    }
}

/// "1 jogador" / "12 jogadores" / "nenhum jogador".
pub fn texto_jogadores(n: usize) -> String {
    match n {
        0 => "nenhum jogador".to_string(),
        1 => "1 jogador".to_string(),
        n => format!("{n} jogadores"),
    }
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, aviso: &TipoAviso) {
    let (titulo, detalhe, cor) = textos(aviso);
    let [largura_tela, _] = ui.io().display_size;

    let _fundo = ui.push_style_color(StyleColor::WindowBg, theme::BG_PANEL_RAISED);
    let _borda = ui.push_style_color(StyleColor::Border, theme::BORDER_HAIRLINE);
    let _raio = ui.push_style_var(StyleVar::WindowRounding(theme::RAIO_MD));
    let _espessura = ui.push_style_var(StyleVar::WindowBorderSize(1.0));
    let _padding = ui.push_style_var(StyleVar::WindowPadding([theme::ESPACO_4, theme::ESPACO_3]));

    ui.window("##aviso_scout")
        .position([largura_tela - MARGEM, MARGEM], Condition::Always)
        .position_pivot([1.0, 0.0])
        .flags(
            WindowFlags::NO_DECORATION
                | WindowFlags::NO_INPUTS
                | WindowFlags::NO_NAV
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_FOCUS_ON_APPEARING
                | WindowFlags::NO_SAVED_SETTINGS
                | WindowFlags::ALWAYS_AUTO_RESIZE,
        )
        .build(|| {
            let inicio = ui.cursor_screen_pos();
            ui.indent_by(LARGURA_BARRA + theme::ESPACO_3);
            ui.group(|| {
                com_fonte(ui, fonts.map(|f| f.heading), || ui.text(&titulo));
                com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, &detalhe));
            });
            ui.unindent_by(LARGURA_BARRA + theme::ESPACO_3);
            let fim = ui.item_rect_max();
            ui.get_window_draw_list()
                .add_rect(inicio, [inicio[0] + LARGURA_BARRA, fim[1]], cor)
                .filled(true)
                .rounding(theme::RAIO_SM / 2.0)
                .build();
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_repo::Date;
    use crate::scout::search::CareerSnapshot;

    #[test]
    fn texts_are_factual_portuguese_without_exclamation() {
        let pronta = TipoAviso::Pronta(CareerSnapshot {
            orcamento_transferencias: 1,
            data_atual: Date(20260701),
            tecnico: "Senhor Manager".to_string(),
            id_save: String::new(),
        });
        assert_eq!(textos(&pronta).1, "Senhor Manager · 01/07/2026 · F10 abre o painel.");
        assert_eq!(textos(&TipoAviso::Injetado).0, "Central de Scout ativa");
        let relatorio = TipoAviso::RelatorioPronto { tipo: crate::scout::quality::TipoMissao::Jovens, jogadores: 12 };
        assert_eq!(textos(&relatorio).1, "Missão Jovens: 12 jogadores · F10 abre o painel.");
        for aviso in [TipoAviso::Injetado, TipoAviso::Carregando, pronta, TipoAviso::Falhou, relatorio, TipoAviso::BuscaFalhou] {
            let (titulo, detalhe, _) = textos(&aviso);
            assert!(!titulo.contains('!') && !detalhe.contains('!'), "{titulo} / {detalhe}");
        }
    }

    #[test]
    fn each_state_has_its_own_colour() {
        assert_eq!(textos(&TipoAviso::Carregando).2, theme::ACCENT_PRIMARY);
        assert_eq!(textos(&TipoAviso::Falhou).2, theme::DANGER);
    }
}
