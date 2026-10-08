//! Aviso "Aprofundar agora?" (2026-10-08): aparece por cima da Ficha de um
//! Escolhido. Diz o que custa e quanto encurta o acompanhamento; o dinheiro
//! só sai no botão de confirmar. O foco inicial fica em "Cancelar" (B também
//! cancela, em `Scout::aplicar_controle`). Janela própria, como o aviso
//! "Demitir Olheiro?": sem a janela na tela, não há popup preso.

use imgui::{Condition, StyleColor, StyleVar, Ui, WindowFlags};

use super::componentes::{botao, EstiloBotao};
use super::relatorio::formatar_dinheiro;
use super::theme::{self, Fonts};
use super::{com_fonte, olheiros};
use crate::scout::state::{ErroCompra, PreviaAprofundamento, ScoutState};

const LARGURA: f32 = 520.0;

/// O que o jogador fez no aviso neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Confirmou,
    Cancelou,
}

/// O que o aprofundamento faz e custa (factual, sem exclamação).
pub fn texto_aviso(p: &PreviaAprofundamento) -> String {
    let faltando = match (p.precisao, p.faltam) {
        (0, 0) => String::new(),
        (precisao, 0) => format!(" Hoje ele está em ±{precisao}."),
        (0, faltam) => format!(" Faltam {faltam} atributos a observar."),
        (precisao, faltam) => format!(" Hoje ele está em ±{precisao}, com {faltam} atributos a observar."),
    };
    format!(
        "{} se dedica a {} até os valores ficarem exatos: cerca de {} dias de carreira, em vez de {} pelo acompanhamento normal.{faltando} Custa {} e ocupa uma vaga do acompanhamento até acabar.",
        p.generalista,
        p.nome,
        p.dias,
        p.dias_normal,
        formatar_dinheiro(i64::from(p.custo))
    )
}

/// Por que a confirmação não valeu (nada ficou gravado, salvo o último caso).
pub fn texto_erro(erro: &ErroCompra) -> String {
    match erro {
        ErroCompra::OrcamentoInsuficiente { faltam } => olheiros::texto_faltam(*faltam),
        ErroCompra::OrcamentoMudou { atual } => format!(
            "O orçamento mudou para {} desde que o aviso abriu. Nada foi debitado: confira o valor e confirme de novo.",
            formatar_dinheiro(i64::from(*atual))
        ),
        ErroCompra::SemCarreira => "Nenhuma carreira carregada. Nada foi debitado.".to_string(),
        ErroCompra::EstadoNaoSalvavel => "O arquivo de estado da Central não pode ser gravado. Nada foi debitado.".to_string(),
        ErroCompra::EscritaFalhou => "Não foi possível debitar o orçamento. Nada foi aprofundado.".to_string(),
        ErroCompra::NaoSalvo => "Não foi possível guardar o aprofundamento; o débito foi desfeito.".to_string(),
        ErroCompra::DebitadoSemSalvar { debitado } => format!(
            "Atenção: {} saíram do orçamento e o aprofundamento não foi guardado, e não deu para desfazer o débito.",
            formatar_dinheiro(i64::from(*debitado))
        ),
    }
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some(previa) = state.aprofundamento_pendente() else {
        return Acao::Cancelou;
    };
    let mut acao = Acao::Nenhuma;
    let [largura_tela, altura_tela] = ui.io().display_size;
    let _fundo = ui.push_style_color(StyleColor::WindowBg, theme::BG_PANEL_RAISED);
    let _borda = ui.push_style_color(StyleColor::Border, theme::BORDER_HAIRLINE);
    let _raio = ui.push_style_var(StyleVar::WindowRounding(theme::RAIO_LG));
    let _padding = ui.push_style_var(StyleVar::WindowPadding([theme::ESPACO_5, theme::ESPACO_5]));
    ui.window("Aprofundar agora##aviso_aprofundamento")
        .position([largura_tela * 0.5, altura_tela * 0.5], Condition::Always)
        .position_pivot([0.5, 0.5])
        .focused(true)
        .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_SAVED_SETTINGS | WindowFlags::ALWAYS_AUTO_RESIZE)
        .build(|| {
            ui.dummy([LARGURA, 0.0]);
            com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Aprofundar agora?"));
            ui.dummy([0.0, theme::ESPACO_2]);
            com_fonte(ui, fonts.map(|f| f.body), || {
                let _quebra = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + LARGURA);
                ui.text(texto_aviso(&previa));
            });
            if let Some(orcamento) = previa.orcamento {
                ui.dummy([0.0, theme::ESPACO_2]);
                com_fonte(ui, fonts.map(|f| f.meta), || {
                    ui.text_colored(theme::TEXT_SECONDARY, format!("Orçamento de transferências: {}", formatar_dinheiro(i64::from(orcamento))));
                });
            }
            if let Some(erro) = &previa.erro {
                ui.dummy([0.0, theme::ESPACO_2]);
                com_fonte(ui, fonts.map(|f| f.body), || {
                    let _quebra = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + LARGURA);
                    ui.text_colored(theme::DANGER, texto_erro(erro));
                });
            }
            ui.dummy([0.0, theme::ESPACO_3]);
            // Foco inicial no lado seguro: cancelar.
            if botao(ui, fonts, "Cancelar", EstiloBotao::Primario, true) {
                acao = Acao::Cancelou;
            }
            ui.set_item_default_focus();
            ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            let rotulo = format!("Aprofundar por {}", formatar_dinheiro(i64::from(previa.custo)));
            if botao(ui, fonts, &rotulo, EstiloBotao::Secundario, true) && state.confirmar_aprofundamento() {
                acao = Acao::Confirmou;
            }
        });
    acao
}

#[cfg(test)]
mod tests {
    use super::*;

    fn previa() -> PreviaAprofundamento {
        PreviaAprofundamento {
            player_id: 9,
            nome: "Kostoulas".to_string(),
            generalista: "Zé".to_string(),
            precisao: 14,
            faltam: 22,
            dias_normal: 25,
            dias: 10,
            percentual: 40,
            custo: 160_000,
            orcamento: Some(5_000_000),
            erro: None,
        }
    }

    #[test]
    fn the_warning_says_who_how_long_what_it_costs_and_that_it_takes_a_vacancy() {
        let texto = texto_aviso(&previa());
        for trecho in ["Zé", "Kostoulas", "10 dias", "25", "±14", "22 atributos", "160 mil", "vaga"] {
            assert!(texto.contains(trecho), "falta '{trecho}' em: {texto}");
        }
        assert!(!texto.contains('!'));
    }

    #[test]
    fn every_failure_has_a_plain_message_and_only_the_last_one_admits_the_money_left() {
        let erros = [
            ErroCompra::OrcamentoInsuficiente { faltam: 50_000 },
            ErroCompra::OrcamentoMudou { atual: 1_000_000 },
            ErroCompra::SemCarreira,
            ErroCompra::EstadoNaoSalvavel,
            ErroCompra::EscritaFalhou,
            ErroCompra::NaoSalvo,
            ErroCompra::DebitadoSemSalvar { debitado: 160_000 },
        ];
        for erro in &erros {
            let texto = texto_erro(erro);
            assert!(!texto.is_empty() && !texto.contains('!'), "{texto}");
            assert_eq!(texto.contains("saíram do orçamento"), matches!(erro, ErroCompra::DebitadoSemSalvar { .. }), "{texto}");
        }
    }
}
