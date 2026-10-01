//! Aba Missões (Story 2.2): o botão "Nova Missão" e a lista das Missões
//! encomendadas nesta carreira. Progresso com barra e "pronto em ~N dias"
//! chega na Story 2.3; aqui cada Missão mostra Olheiro, Modo, Qualidade
//! estimada e prazo.

use imgui::Ui;

use super::componentes::{self, badge_qualidade, badge_tier, card, desenhar_badge, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::scout::state::{ModoBusca, ScoutState, StatusMissao};

const ALTURA_CARD: f32 = 64.0;

pub const MSG_SEM_MISSOES: &str = "Nenhuma Missão encomendada ainda.";

/// Rótulo do Modo de Busca.
pub fn nome_modo(modo: ModoBusca) -> &'static str {
    match modo {
        ModoBusca::Rapida => "Rápida",
        ModoBusca::Completa => "Completa",
    }
}

/// Rótulo do status de uma Missão nesta fase (a 2.3/2.4 detalham).
pub fn nome_status(status: StatusMissao) -> &'static str {
    match status {
        StatusMissao::Pendente => "Em andamento",
        StatusMissao::EmExecucao => "Gerando relatório",
        StatusMissao::Concluida => "Concluída",
    }
}

/// Desenha a aba; `true` = "Nova Missão" pressionado.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState) -> bool {
    let nova = componentes::botao(ui, fonts, "Nova Missão", EstiloBotao::Primario, true);
    ui.dummy([0.0, theme::ESPACO_3]);

    let missoes = state.missoes();
    if missoes.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_MISSOES));
        return nova;
    }
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text_colored(theme::TEXT_SECONDARY, "Missões"));
    for (missao, olheiro) in &missoes {
        let c = card(ui, &missao.id.to_string(), ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
        let dl = ui.get_window_draw_list();
        let x = c.min[0] + theme::ESPACO_4;
        let y = c.min[1] + theme::ESPACO_3;

        // Linha 1: Olheiro + Tier, Qualidade estimada à direita.
        let nome = olheiro.as_ref().map_or("Olheiro removido", |o| o.especializacao.nome());
        let [largura_nome, altura_nome] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, nome);
        if let Some(o) = olheiro {
            desenhar_badge(ui, fonts, &dl, &badge_tier(o.tier), [x + largura_nome + theme::ESPACO_2, y], altura_nome);
        }
        let qualidade = badge_qualidade(missao.estimativa.qualidade);
        let largura_badge = com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(qualidade.texto)[0]) + theme::ESPACO_2 * 2.0;
        desenhar_badge(ui, fonts, &dl, &qualidade, [c.max[0] - theme::ESPACO_4 - largura_badge, y], altura_nome);

        // Linha 2: Modo · status · prazo.
        let detalhe = format!(
            "{} · {} · prazo {}",
            nome_modo(missao.modo_busca),
            nome_status(missao.status),
            formatar_data(missao.prazo_estimado)
        );
        texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y + altura_nome + theme::ESPACO_1], theme::TEXT_SECONDARY, &detalhe);
    }
    nova
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_portuguese_and_without_exclamation() {
        assert_eq!(nome_modo(ModoBusca::Rapida), "Rápida");
        assert_eq!(nome_modo(ModoBusca::Completa), "Completa");
        assert_eq!(nome_status(StatusMissao::Pendente), "Em andamento");
        assert_eq!(MSG_SEM_MISSOES, "Nenhuma Missão encomendada ainda.");
    }
}
