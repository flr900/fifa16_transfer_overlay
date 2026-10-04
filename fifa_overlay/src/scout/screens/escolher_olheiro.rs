//! Nova Missão, passo 1 (2026-10-03, pedido do Felipe): primeiro o Olheiro,
//! depois os filtros. Uma lista dos contratados; os "Em Missão" aparecem,
//! apagados e inertes. Ativar um livre (clique ou A) abre o formulário com
//! os filtros ideais da Especialização dele; B / "Voltar" volta à aba.
//!
//! Aqui foco não é escolha: escolher leva para outra tela, então mover o
//! foco com o D-pad só destaca o card.

use imgui::Ui;
use uuid::Uuid;

use super::componentes::{self, badge_tier, card, desenhar_badge, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, olheiros};
use crate::scout::state::{OlheiroContratado, ScoutState};

const ALTURA_CARD: f32 = 64.0;

pub const MSG_SEM_OLHEIROS: &str = "Nenhum Olheiro contratado. Contrate um na aba Olheiros para encomendar uma Missão.";
pub const MSG_TODOS_OCUPADOS: &str =
    "Todos os Olheiros estão em Missão ou acompanhando os Escolhidos. Contrate outro na aba Olheiros ou espere um terminar.";

/// O que o jogador fez neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Voltar,
    Escolheu(Uuid),
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState) -> Acao {
    let mut acao = Acao::Nenhuma;
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = Acao::Voltar;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Nova Missão · escolha o Olheiro"));
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.text_colored(theme::TEXT_SECONDARY, "Os filtros já abrem com o que combina com a Especialização dele.")
    });
    ui.dummy([0.0, theme::ESPACO_2]);

    let contratados = state.olheiros_contratados();
    let aviso = if contratados.is_empty() {
        Some(MSG_SEM_OLHEIROS)
    } else if contratados.iter().all(OlheiroContratado::ocupado) {
        Some(MSG_TODOS_OCUPADOS)
    } else {
        None
    };
    if let Some(texto) = aviso {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, texto));
        ui.dummy([0.0, theme::ESPACO_2]);
    }
    let nacao = |id: u16| state.nome_da_nacao(id);
    for c in &contratados {
        if linha(ui, fonts, c, &nacao) && !c.ocupado() {
            acao = Acao::Escolheu(c.olheiro.id);
        }
    }
    acao
}

fn linha(ui: &Ui, fonts: Option<&Fonts>, c: &OlheiroContratado, nacao: &dyn Fn(u16) -> Option<String>) -> bool {
    let c_card = card(ui, &c.olheiro.id.to_string(), ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let dl = ui.get_window_draw_list();
    let ocupado = c.ocupado();
    let cor = if ocupado { theme::TEXT_DISABLED } else { theme::TEXT_PRIMARY };
    let x = c_card.min[0] + theme::ESPACO_4;
    let y = c_card.min[1] + theme::ESPACO_2;
    let nome = c.olheiro.nome_exibicao();
    let [w, h] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], cor, &nome);
    let badge = desenhar_badge(ui, fonts, &dl, &badge_tier(c.olheiro.tier), [x + w + theme::ESPACO_2, y], h);
    let perfil = c.olheiro.perfil();
    let foco = format!("{} · {} estrelas", perfil.foco().nome(), perfil.principal().texto());
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x + w + badge[0] + theme::ESPACO_4, y + 2.0], theme::TEXT_SECONDARY, &foco);
    let detalhe = if ocupado { olheiros::texto_missao(c) } else { olheiros::texto_origem(&c.olheiro, nacao) };
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y + h + theme::ESPACO_1], theme::TEXT_SECONDARY, &detalhe);
    let (status, cor_status) = if c.em_missao {
        ("Em Missão", theme::WARNING)
    } else if c.acompanhando {
        ("Acompanhando", theme::ACCENT_PRIMARY)
    } else {
        ("Escolher", theme::FIELD_GREEN)
    };
    let largura = com_fonte(ui, fonts.map(|f| f.body), || ui.calc_text_size(status)[0]);
    let y_status = (c_card.min[1] + c_card.max[1]) * 0.5 - h * 0.5;
    texto_em(ui, fonts.map(|f| f.body), &dl, [c_card.max[0] - theme::ESPACO_4 - largura, y_status], cor_status, status);
    c_card.ativou
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_states_point_to_the_olheiros_tab() {
        assert!(MSG_SEM_OLHEIROS.contains("aba Olheiros"));
        assert!(MSG_TODOS_OCUPADOS.contains("aba Olheiros"));
        assert!(!MSG_SEM_OLHEIROS.contains('!') && !MSG_TODOS_OCUPADOS.contains('!'));
    }
}
