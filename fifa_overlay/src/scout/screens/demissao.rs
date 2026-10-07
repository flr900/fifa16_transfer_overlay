//! Aviso "Demitir Olheiro?" (2026-10-05): aparece por cima da aba Olheiros
//! depois do "Demitir" num Olheiro. A demissão só vale no botão de confirmar;
//! o foco inicial fica em "Cancelar" (B também cancela, em
//! `Scout::aplicar_controle`). Desenhada como janela própria, como o aviso
//! "Sair da Nova Missão?": sem a janela na tela, não há popup preso.

use imgui::{Condition, StyleColor, StyleVar, Ui, WindowFlags};

use super::componentes::{botao, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, olheiros};
use crate::scout::state::{OlheiroContratado, ScoutState};

const LARGURA: f32 = 480.0;

/// O que o jogador fez no aviso neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Demitiu,
    Cancelou,
}

/// O que a demissão faz e não faz (factual, sem exclamação).
pub fn texto_aviso(c: &OlheiroContratado) -> String {
    let nome = c.olheiro.nome_exibicao();
    let mut texto = format!(
        "{nome} sai da sua equipe. O que foi pago na contratação não volta. As Missões e os Relatórios dele continuam no histórico."
    );
    if c.acompanhando {
        let vagas = c.olheiro.capacidade_acompanhamento();
        texto.push_str(&format!(" Ele deixa de acompanhar a Lista de Escolhidos: o acompanhamento perde {vagas} vagas."));
    }
    texto
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some(contratado) = state.demissao_pendente() else {
        return Acao::Cancelou;
    };
    let mut acao = Acao::Nenhuma;
    let [largura_tela, altura_tela] = ui.io().display_size;
    let _fundo = ui.push_style_color(StyleColor::WindowBg, theme::BG_PANEL_RAISED);
    let _borda = ui.push_style_color(StyleColor::Border, theme::BORDER_HAIRLINE);
    let _raio = ui.push_style_var(StyleVar::WindowRounding(theme::RAIO_LG));
    let _padding = ui.push_style_var(StyleVar::WindowPadding([theme::ESPACO_5, theme::ESPACO_5]));
    ui.window("Demitir Olheiro##aviso_demissao")
        .position([largura_tela * 0.5, altura_tela * 0.5], Condition::Always)
        .position_pivot([0.5, 0.5])
        .focused(true)
        .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_SAVED_SETTINGS | WindowFlags::ALWAYS_AUTO_RESIZE)
        .build(|| {
            ui.dummy([LARGURA, 0.0]);
            com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Demitir Olheiro?"));
            ui.dummy([0.0, theme::ESPACO_2]);
            olheiros::nome_com_badge(ui, fonts, &contratado.olheiro.nome_exibicao(), contratado.olheiro.tier);
            olheiros::com_nacoes(state, |nacoes| {
                olheiros::origem_no_fluxo(ui, fonts, &contratado.olheiro, nacoes, true, 2);
            });
            ui.dummy([0.0, theme::ESPACO_2]);
            com_fonte(ui, fonts.map(|f| f.body), || {
                let _quebra = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + LARGURA);
                ui.text(texto_aviso(&contratado));
            });
            ui.dummy([0.0, theme::ESPACO_3]);
            // Foco inicial no lado seguro: cancelar.
            if botao(ui, fonts, "Cancelar", EstiloBotao::Primario, true) {
                acao = Acao::Cancelou;
            }
            ui.set_item_default_focus();
            ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            if botao(ui, fonts, "Confirmar demissão", EstiloBotao::Secundario, true) {
                acao = if state.confirmar_demissao() { Acao::Demitiu } else { Acao::Nenhuma };
            }
        });
    acao
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{Especializacao, Olheiro, Tier};

    #[test]
    fn the_warning_says_what_is_lost_and_what_stays() {
        let olheiro = Olheiro { nome: "Zé".to_string(), especializacao: Especializacao::Generalista, tier: Tier::Experiente, ..Default::default() };
        let livre = OlheiroContratado { olheiro: olheiro.clone(), em_missao: false, acompanhando: false, missao: None, relatorio_atual: None, relatorios: 0 };
        let texto = texto_aviso(&livre);
        assert!(texto.contains("não volta") && texto.contains("continuam no histórico"), "{texto}");
        assert!(!texto.contains("Escolhidos"));
        let acompanhando = OlheiroContratado { acompanhando: true, ..livre };
        let texto = texto_aviso(&acompanhando);
        assert!(texto.contains("Lista de Escolhidos") && texto.contains("vagas"), "{texto}");
        assert!(!texto.contains('!'));
    }
}
