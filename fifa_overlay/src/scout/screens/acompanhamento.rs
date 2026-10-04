//! Olheiros do acompanhamento (Épico 6): tela satélite da aba Escolhidos
//! onde o técnico designa (ou libera) Generalistas para manter a Lista de
//! Escolhidos atualizada. Mais de um pode ser designado; cada um mantém
//! duas vagas por estrela de Generalista. Designado, ele não aceita
//! Missão; em Missão, não pode ser designado.
//!
//! Aqui foco não é escolha: ativar um card (clique ou A) designa ou libera.

use imgui::Ui;

use super::componentes::{self, badge_tier, card, desenhar_badge, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, olheiros};
use crate::scout::state::{OlheiroContratado, ScoutState};

const ALTURA_CARD: f32 = 64.0;

pub const MSG_REGRAS: &str = "Só Generalistas acompanham a Lista de Escolhidos. Cada um mantém 2 jogadores por estrela de Generalista; designado, ele não aceita Missão.";
pub const MSG_SEM_OLHEIROS: &str = "Nenhum Olheiro contratado. Contrate um Generalista na aba Olheiros.";

/// O que o jogador fez neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Voltar,
}

/// O que ativar o card faz (ou por que não faz nada).
pub fn situacao(c: &OlheiroContratado) -> (&'static str, bool) {
    if c.acompanhando {
        ("Liberar", true)
    } else if c.em_missao {
        ("Em Missão", false)
    } else if !c.olheiro.pode_acompanhar() {
        ("Não é Generalista", false)
    } else {
        ("Designar", true)
    }
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let mut acao = Acao::Nenhuma;
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = Acao::Voltar;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Olheiros do acompanhamento"));
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_wrapped(MSG_REGRAS));
    let resumo = state.resumo_acompanhamento();
    com_fonte(ui, fonts.map(|f| f.body), || ui.text(super::escolhidos::texto_resumo(&resumo)));
    ui.dummy([0.0, theme::ESPACO_2]);

    let contratados = state.olheiros_contratados();
    if contratados.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_OLHEIROS));
    }
    let mut alternar = None;
    for c in &contratados {
        if linha(ui, fonts, c) {
            alternar = Some((c.olheiro.id, !c.acompanhando));
        }
    }
    if let Some((id, designar)) = alternar {
        state.designar_acompanhamento(id, designar);
    }
    acao
}

fn linha(ui: &Ui, fonts: Option<&Fonts>, c: &OlheiroContratado) -> bool {
    let c_card = card(ui, &format!("acomp_{}", c.olheiro.id), ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let dl = ui.get_window_draw_list();
    let (rotulo, possivel) = situacao(c);
    let cor = if possivel { theme::TEXT_PRIMARY } else { theme::TEXT_DISABLED };
    let x = c_card.min[0] + theme::ESPACO_4;
    let y = c_card.min[1] + theme::ESPACO_2;
    let [w, h] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], cor, &c.olheiro.nome_exibicao());
    desenhar_badge(ui, fonts, &dl, &badge_tier(c.olheiro.tier), [x + w + theme::ESPACO_2, y], h);
    let perfil = c.olheiro.perfil();
    let detalhe = if c.olheiro.pode_acompanhar() {
        format!(
            "Generalista {} estrelas · mantém {} jogadores atualizados{}",
            perfil.generalista.texto(),
            c.olheiro.capacidade_acompanhamento(),
            if c.acompanhando { " · designado" } else { "" }
        )
    } else {
        format!("Foco {} · {}", perfil.foco().nome(), olheiros::texto_missao(c))
    };
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y + h + theme::ESPACO_1], theme::TEXT_SECONDARY, &detalhe);
    let cor_rotulo = match (c.acompanhando, possivel) {
        (true, _) => theme::WARNING,
        (false, true) => theme::FIELD_GREEN,
        (false, false) => theme::TEXT_DISABLED,
    };
    let largura = com_fonte(ui, fonts.map(|f| f.body), || ui.calc_text_size(rotulo)[0]);
    let y_rotulo = (c_card.min[1] + c_card.max[1]) * 0.5 - h * 0.5;
    texto_em(ui, fonts.map(|f| f.body), &dl, [c_card.max[0] - theme::ESPACO_4 - largura, y_rotulo], cor_rotulo, rotulo);
    c_card.ativou && possivel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{Especializacao, Olheiro, Tier};

    fn contratado(especializacao: Especializacao) -> OlheiroContratado {
        let olheiro = Olheiro { especializacao, tier: Tier::Experiente, ..Default::default() };
        OlheiroContratado { olheiro, em_missao: false, acompanhando: false, missao: None, relatorio_atual: None, relatorios: 0 }
    }

    #[test]
    fn only_free_generalists_can_be_designated() {
        assert_eq!(situacao(&contratado(Especializacao::Generalista)), ("Designar", true));
        assert_eq!(situacao(&contratado(Especializacao::Tatico)), ("Não é Generalista", false));
        let ocupado = OlheiroContratado { em_missao: true, ..contratado(Especializacao::Generalista) };
        assert_eq!(situacao(&ocupado), ("Em Missão", false));
        let designado = OlheiroContratado { acompanhando: true, ..contratado(Especializacao::Generalista) };
        assert_eq!(situacao(&designado), ("Liberar", true));
        assert!(!MSG_REGRAS.contains('!'));
    }
}
