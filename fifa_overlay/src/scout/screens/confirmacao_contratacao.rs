//! Confirmação de Contratação (Story 1.5, UX-DR9): tela satélite por cima
//! da aba Olheiros que mostra o custo exato e o orçamento resultante
//! ANTES de debitar. A escrita no jogo só acontece no botão confirmar
//! (FR-3); "Cancelar" volta sem tocar em nada.
//!
//! Épico 5: o modal mostra o Olheiro do mercado (nome, nação, mercados,
//! estrelas) e deixa trocar o nome antes de confirmar (teclado).
//!
//! Desenhada como janela própria, por cima do painel (que fica
//! desabilitado e escurecido por baixo), em vez de popup modal do ImGui:
//! um popup aberto sobrevive ao painel fechar com F10 no meio e pode
//! deixar a entrada travada; aqui, sem a satélite na pilha (AD-6), não há
//! janela.

use imgui::{Condition, StyleColor, StyleVar, Ui, WindowFlags};

use super::componentes::{botao, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_milhar, olheiros};
use crate::scout::state::{ErroCompra, PreviaContratacao, ScoutState};

const LARGURA: f32 = 480.0;

/// O que o usuário fez no modal neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Contratou,
    Cancelou,
}

/// Mensagem de cada falha (UX-DR21: factual, valores exatos, sem
/// exclamação; sempre diz se o dinheiro saiu ou não).
pub fn texto_erro(erro: &ErroCompra) -> String {
    match erro {
        ErroCompra::OrcamentoInsuficiente { faltam } => olheiros::texto_faltam(*faltam),
        ErroCompra::OrcamentoMudou { atual } => format!(
            "O orçamento mudou para {} antes da confirmação. Nada foi debitado; confira os valores e confirme de novo.",
            formatar_milhar(*atual)
        ),
        ErroCompra::SemCarreira => "Nenhuma carreira carregada. Nada foi debitado.".to_string(),
        ErroCompra::EstadoNaoSalvavel => {
            "Não é possível salvar o estado do Scout desta carreira. Nada foi debitado.".to_string()
        }
        ErroCompra::EscritaFalhou => "Não foi possível debitar o orçamento. Nada foi contratado.".to_string(),
        ErroCompra::NaoSalvo => {
            "Não foi possível salvar o Olheiro. O débito foi desfeito e nada foi contratado.".to_string()
        }
        ErroCompra::DebitadoSemSalvar { debitado } => format!(
            "Não foi possível salvar o Olheiro, e o débito de {} não pôde ser desfeito. Confira o orçamento no jogo.",
            formatar_milhar(*debitado)
        ),
    }
}

/// Rótulo do botão confirmar: depois de uma falha, "Tentar novamente".
pub fn rotulo_confirmar(previa: &PreviaContratacao) -> &'static str {
    if previa.contratacao.erro.is_some() {
        "Tentar novamente"
    } else {
        "Confirmar contratação"
    }
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some(previa) = state.previa_contratacao() else {
        return Acao::Cancelou;
    };
    let mut acao = Acao::Nenhuma;
    let [largura_tela, altura_tela] = ui.io().display_size;

    let _fundo = ui.push_style_color(StyleColor::WindowBg, theme::BG_PANEL_RAISED);
    let _borda = ui.push_style_color(StyleColor::Border, theme::BORDER_HAIRLINE);
    let _raio = ui.push_style_var(StyleVar::WindowRounding(theme::RAIO_LG));
    let _padding = ui.push_style_var(StyleVar::WindowPadding([theme::ESPACO_5, theme::ESPACO_5]));

    ui.window("Confirmar contratação##modal_contratacao")
        .position([largura_tela * 0.5, altura_tela * 0.5], Condition::Always)
        .position_pivot([0.5, 0.5])
        // Fica por cima do painel e com o foco (teclado/gamepad).
        .focused(true)
        .flags(
            WindowFlags::NO_DECORATION
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_SAVED_SETTINGS
                | WindowFlags::ALWAYS_AUTO_RESIZE,
        )
        .build(|| {
            ui.dummy([LARGURA, 0.0]);
            com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Contratar Olheiro"));
            ui.dummy([0.0, theme::ESPACO_2]);

            let c = &previa.contratacao;
            let o = &c.oferta.olheiro;
            let perfil = o.perfil();
            olheiros::nome_com_badge(ui, fonts, &o.nome, o.tier);
            com_fonte(ui, fonts.map(|f| f.meta), || {
                ui.text_colored(
                    theme::TEXT_SECONDARY,
                    format!("Foco {} ({} estrelas) · {}", perfil.foco().nome(), perfil.principal().texto(), olheiros::descricao(perfil.foco())),
                );
            });
            // nação e mercados, com bandeira
            olheiros::com_nacoes(state, |nacoes| olheiros::origem_no_fluxo(ui, fonts, o, nacoes, true, 3));
            ui.dummy([0.0, theme::ESPACO_1]);
            let pos = ui.cursor_screen_pos();
            let largura = olheiros::estrelas_do_perfil(ui, fonts, &ui.get_window_draw_list(), pos, &perfil);
            ui.dummy([largura, ui.text_line_height()]);
            ui.dummy([0.0, theme::ESPACO_2]);
            // Nome (item 1): começa com o gerado; vazio volta a ele.
            com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, "Nome do Olheiro"));
            let mut nome = c.nome.clone();
            {
                let _largura = ui.push_item_width(LARGURA);
                if com_fonte(ui, fonts.map(|f| f.body), || ui.input_text("##nome_olheiro", &mut nome).build()) {
                    state.definir_nome_da_contratacao(&nome);
                }
            }
            ui.dummy([0.0, theme::ESPACO_3]);
            ui.separator();
            ui.dummy([0.0, theme::ESPACO_2]);

            linha(ui, fonts, "Custo de contratação", &formatar_milhar(c.custo()), theme::TEXT_PRIMARY);
            linha(ui, fonts, "Orçamento atual", &formatar_milhar(previa.orcamento_atual), theme::TEXT_PRIMARY);
            let (texto_apos, cor_apos) = match previa.faltam {
                Some(_) => ("—".to_string(), theme::TEXT_DISABLED),
                None => (formatar_milhar(previa.orcamento_apos), theme::FIELD_GREEN),
            };
            linha(ui, fonts, "Orçamento após a contratação", &texto_apos, cor_apos);
            ui.dummy([0.0, theme::ESPACO_2]);

            // Bloqueio por orçamento e falhas da última tentativa.
            let aviso = match (&previa.faltam, &c.erro) {
                (Some(faltam), _) => Some(olheiros::texto_faltam(*faltam)),
                (None, Some(erro)) => Some(texto_erro(erro)),
                (None, None) => None,
            };
            if let Some(texto) = aviso {
                com_fonte(ui, fonts.map(|f| f.body), || {
                    let _quebra = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + LARGURA);
                    ui.text_colored(theme::DANGER, texto);
                });
                ui.dummy([0.0, theme::ESPACO_2]);
            }

            // Foco inicial (controle/teclado): confirmar, ou cancelar quando
            // não dá para confirmar.
            let habilitado = previa.faltam.is_none();
            if botao(ui, fonts, rotulo_confirmar(&previa), EstiloBotao::Primario, habilitado) {
                acao = if state.confirmar_contratacao() { Acao::Contratou } else { Acao::Nenhuma };
            }
            if habilitado {
                ui.set_item_default_focus();
            }
            ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            if botao(ui, fonts, "Cancelar", EstiloBotao::Secundario, true) {
                acao = Acao::Cancelou;
            }
            if !habilitado {
                ui.set_item_default_focus();
            }
        });
    acao
}

/// Rótulo à esquerda, valor em fonte mono alinhado à direita.
fn linha(ui: &Ui, fonts: Option<&Fonts>, rotulo: &str, valor: &str, cor: [f32; 4]) {
    let inicio = ui.cursor_pos();
    com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, rotulo));
    let fim = ui.cursor_pos();
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    com_fonte(ui, mono, || {
        let largura = ui.calc_text_size(valor)[0];
        ui.set_cursor_pos([inicio[0] + LARGURA - largura, inicio[1]]);
        ui.text_colored(cor, valor);
    });
    ui.set_cursor_pos([inicio[0], fim[1].max(ui.cursor_pos()[1])]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_says_whether_money_left_and_has_no_exclamation() {
        let erros = [
            ErroCompra::OrcamentoInsuficiente { faltam: 2_100_000 },
            ErroCompra::OrcamentoMudou { atual: 60_000_000 },
            ErroCompra::SemCarreira,
            ErroCompra::EstadoNaoSalvavel,
            ErroCompra::EscritaFalhou,
            ErroCompra::NaoSalvo,
            ErroCompra::DebitadoSemSalvar { debitado: 5_800_000 },
        ];
        for erro in &erros {
            let texto = texto_erro(erro);
            assert!(!texto.contains('!'), "{texto}");
        }
        assert_eq!(texto_erro(&erros[0]), "Orçamento insuficiente: faltam 2.100.000.");
        assert!(texto_erro(&erros[1]).contains("60.000.000"));
        assert!(texto_erro(&erros[4]).contains("Nada foi contratado"));
        assert!(texto_erro(&erros[6]).contains("5.800.000"));
    }
}
