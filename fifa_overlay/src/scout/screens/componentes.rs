//! Componentes de tela reaproveitados (DESIGN.md → Components): botões
//! primário/secundário, badges em contorno (Tier e Qualidade) e o card
//! selecionável. Juntos aqui desde a Story 2.2, quando o formulário Nova
//! Missão passou a precisar dos mesmos pedaços da aba Olheiros e do modal
//! de contratação.
//!
//! Regras que valem para todos (UX-DR5/DR6/DR19/DR20):
//! - alvo de clique ≥ 32 px;
//! - hover e foco de controle/teclado com a mesma borda roxa de 2 px;
//! - badge sempre com texto, em contorno + preenchimento tênue.

use imgui::{DrawListMut, FontId, StyleColor, StyleVar, Ui};

use super::theme::{self, Fonts};
use super::{com_fonte, contorno_hover, ESPESSURA_FOCO};
use crate::scout::state::{Qualidade, Tier};

// ---------------------------------------------------------------------
// Botões
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstiloBotao {
    /// Verde-campo sólido, texto escuro: a ação principal da tela.
    Primario,
    /// Contorno sutil, fundo transparente: cancelar, voltar.
    Secundario,
    /// Opção escolhida de um grupo (ex.: Modo de Busca): roxo tênue +
    /// contorno roxo.
    Selecionado,
}

/// Botão do tema (≥ 32 px). Desabilitado: cinza e inerte (devolve
/// `false`), mas continua recebendo foco para o jogador chegar nele.
pub fn botao(ui: &Ui, fonts: Option<&Fonts>, rotulo: &str, estilo: EstiloBotao, habilitado: bool) -> bool {
    botao_com_largura(ui, fonts, rotulo, estilo, habilitado, None)
}

pub fn botao_com_largura(
    ui: &Ui,
    fonts: Option<&Fonts>,
    rotulo: &str,
    estilo: EstiloBotao,
    habilitado: bool,
    largura: Option<f32>,
) -> bool {
    let (fundo, texto, borda, cor_borda) = match (estilo, habilitado) {
        (_, false) => (theme::BOTAO_DESABILITADO, theme::TEXT_DISABLED, 0.0, theme::BORDER_HAIRLINE),
        (EstiloBotao::Primario, true) => (theme::FIELD_GREEN, theme::BG_BASE, 0.0, theme::BORDER_HAIRLINE),
        (EstiloBotao::Secundario, true) => (theme::TRANSPARENTE, theme::TEXT_PRIMARY, 1.0, theme::BORDER_HAIRLINE),
        (EstiloBotao::Selecionado, true) => (theme::ACCENT_PRIMARY_DIM, theme::TEXT_PRIMARY, 1.0, theme::ACCENT_PRIMARY),
    };
    let realce = match (estilo, habilitado) {
        (EstiloBotao::Secundario, true) => theme::ACCENT_PRIMARY_DIM,
        _ => fundo,
    };
    let _c1 = ui.push_style_color(StyleColor::Button, fundo);
    let _c2 = ui.push_style_color(StyleColor::ButtonHovered, realce);
    let _c3 = ui.push_style_color(StyleColor::ButtonActive, realce);
    let _c4 = ui.push_style_color(StyleColor::Text, texto);
    let _c5 = ui.push_style_color(StyleColor::Border, cor_borda);
    let _b = ui.push_style_var(StyleVar::FrameBorderSize(borda));
    let clicou = com_fonte(ui, fonts.map(|f| f.heading), || {
        let largura = largura.unwrap_or_else(|| ui.calc_text_size(rotulo)[0] + theme::ESPACO_5 * 2.0);
        ui.button_with_size(rotulo, [largura, theme::ALVO_MINIMO])
    });
    contorno_hover(ui, theme::RAIO_PADRAO);
    clicou && habilitado
}

// ---------------------------------------------------------------------
// Badges (Tier, Qualidade)
// ---------------------------------------------------------------------

/// Texto e cores de um badge: `(texto, contorno, preenchimento, cor do texto)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EstiloBadge {
    pub texto: &'static str,
    pub contorno: [f32; 4],
    pub fundo: [f32; 4],
    pub cor_texto: [f32; 4],
}

/// JR / EXP / ELITE em cinza / roxo / dourado (DESIGN.md → tier-badge-*).
/// O texto do Júnior é um tom acima do contorno (mockup v2): `#5c6b5f`
/// sobre o painel escuro quase some.
pub fn badge_tier(tier: Tier) -> EstiloBadge {
    match tier {
        Tier::Junior => EstiloBadge {
            texto: "JR",
            contorno: theme::TIER_JUNIOR,
            fundo: theme::TRANSPARENTE,
            cor_texto: theme::TIER_JUNIOR_TEXTO,
        },
        Tier::Experiente => EstiloBadge {
            texto: "EXP",
            contorno: theme::TIER_EXPERIENTE,
            fundo: theme::ACCENT_PRIMARY_DIM,
            cor_texto: theme::TIER_EXPERIENTE,
        },
        Tier::Elite => {
            EstiloBadge { texto: "ELITE", contorno: theme::TIER_ELITE, fundo: theme::TRANSPARENTE, cor_texto: theme::TIER_ELITE }
        }
    }
}

/// BAIXA / MÉDIA / ALTA na mesma escala de cores dos Tiers
/// (DESIGN.md → quality-low/-medium/-high).
pub fn badge_qualidade(qualidade: Qualidade) -> EstiloBadge {
    let base = badge_tier(match qualidade {
        Qualidade::Baixa => Tier::Junior,
        Qualidade::Media => Tier::Experiente,
        Qualidade::Alta => Tier::Elite,
    });
    let texto = match qualidade {
        Qualidade::Baixa => "BAIXA",
        Qualidade::Media => "MÉDIA",
        Qualidade::Alta => "ALTA",
    };
    EstiloBadge { texto, ..base }
}

/// "NOVO" em verde-campo: Relatório ainda não aberto (UX-DR13).
pub fn badge_novo() -> EstiloBadge {
    EstiloBadge { texto: "NOVO", contorno: theme::FIELD_GREEN, fundo: theme::FIELD_GREEN_DIM, cor_texto: theme::FIELD_GREEN }
}

/// Fit Posicional ("FIT VOL 96%", Story 3.5) em roxo: o texto é montado
/// na hora, com `desenhar_badge_texto`.
pub fn badge_fit() -> EstiloBadge {
    EstiloBadge {
        texto: "FIT",
        contorno: theme::ACCENT_PRIMARY,
        fundo: theme::ACCENT_PRIMARY_DIM,
        cor_texto: theme::ACCENT_PRIMARY,
    }
}

/// Desenha o badge em `pos`, centralizado na altura `altura_linha`, pelo
/// draw list (sem criar item). Devolve o tamanho ocupado.
pub fn desenhar_badge(
    ui: &Ui,
    fonts: Option<&Fonts>,
    dl: &DrawListMut<'_>,
    estilo: &EstiloBadge,
    pos: [f32; 2],
    altura_linha: f32,
) -> [f32; 2] {
    desenhar_badge_texto(ui, fonts, dl, estilo, estilo.texto, pos, altura_linha)
}

/// Como `desenhar_badge`, com um texto montado na hora nas cores de `estilo`.
pub fn desenhar_badge_texto(
    ui: &Ui,
    fonts: Option<&Fonts>,
    dl: &DrawListMut<'_>,
    estilo: &EstiloBadge,
    texto: &str,
    pos: [f32; 2],
    altura_linha: f32,
) -> [f32; 2] {
    com_fonte(ui, fonts.map(|f| f.badge), || {
        let [w, h] = ui.calc_text_size(texto);
        let padding = [theme::ESPACO_2, 2.0];
        let tamanho = [w + padding[0] * 2.0, h + padding[1] * 2.0];
        let b_min = [pos[0], pos[1] + (altura_linha - tamanho[1]).max(0.0) * 0.5];
        let b_max = [b_min[0] + tamanho[0], b_min[1] + tamanho[1]];
        dl.add_rect(b_min, b_max, estilo.fundo).filled(true).rounding(theme::RAIO_SM).build();
        dl.add_rect(b_min, b_max, estilo.contorno).rounding(theme::RAIO_SM).build();
        dl.add_text([b_min[0] + padding[0], b_min[1] + padding[1]], estilo.cor_texto, texto);
        tamanho
    })
}

/// Badge no fluxo do layout (ocupa espaço como um item comum), alinhado
/// a uma linha de altura `altura_linha`.
pub fn badge_no_fluxo(ui: &Ui, fonts: Option<&Fonts>, estilo: &EstiloBadge, altura_linha: f32) {
    let pos = ui.cursor_screen_pos();
    let dl = ui.get_window_draw_list();
    let tamanho = desenhar_badge(ui, fonts, &dl, estilo, pos, altura_linha);
    drop(dl);
    ui.dummy([tamanho[0], altura_linha.max(tamanho[1])]);
}

// ---------------------------------------------------------------------
// Card selecionável
// ---------------------------------------------------------------------

/// Resultado de um card desenhado por `card`.
pub struct Card {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub ativou: bool,
}

/// Um card do tamanho da largura disponível × `altura`, que é UM item
/// navegável (mouse e controle focam o card inteiro, e o ImGui rola para
/// mostrá-lo por completo — Story 1.6). Desenha fundo e borda; o conteúdo
/// é desenhado pelo chamador com o draw list, dentro de `min..max`.
/// `borda_repouso`: cor da borda sem hover/foco.
pub fn card(ui: &Ui, chave: &str, altura: f32, borda_repouso: [f32; 4]) -> Card {
    let largura = ui.content_region_avail()[0];
    card_com_largura(ui, chave, largura, altura, borda_repouso)
}

/// Card de largura fixa (grade da visão Cards, Story 2.6).
pub fn card_com_largura(ui: &Ui, chave: &str, largura: f32, altura: f32, borda_repouso: [f32; 4]) -> Card {
    let _id = ui.push_id(chave);
    let min = ui.cursor_screen_pos();
    let max = [min[0] + largura, min[1] + altura];
    let ativou = ui.invisible_button("##card", [largura, altura]);
    let hover = ui.is_item_hovered();
    let focado = ui.is_item_focused() && ui.io().nav_visible;
    let (borda, espessura) = if hover || focado { (theme::ACCENT_PRIMARY, ESPESSURA_FOCO) } else { (borda_repouso, 1.0) };
    let dl = ui.get_window_draw_list();
    dl.add_rect(min, max, theme::BG_PANEL_RAISED).filled(true).rounding(theme::RAIO_MD).build();
    dl.add_rect(min, max, borda).rounding(theme::RAIO_MD).thickness(espessura).build();
    Card { min, max, ativou }
}

/// O item anterior tem o foco do CONTROLE/teclado (não só o hover do
/// mouse). Nos grupos de opção, foco = escolha: o foco nunca fica numa
/// opção enquanto outra aparece escolhida (Felipe, 2026-10-01).
pub fn focado_pelo_controle(ui: &Ui) -> bool {
    ui.is_item_focused() && ui.io().nav_visible
}

/// Seletor de dois ou mais botões (ex.: Tabular / Cards): o escolhido em
/// roxo. Devolve o índice clicado — ou focado pelo controle — neste frame.
pub fn alternador(ui: &Ui, fonts: Option<&Fonts>, opcoes: &[&str], escolhida: usize, largura: f32) -> Option<usize> {
    let mut clicada = None;
    for (indice, rotulo) in opcoes.iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_1);
        }
        let estilo = if indice == escolhida { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        let clicou = botao_com_largura(ui, fonts, rotulo, estilo, true, Some(largura));
        if (clicou || focado_pelo_controle(ui)) && indice != escolhida {
            clicada = Some(indice);
        }
    }
    clicada
}

/// Texto pelo draw list numa fonte do tema; devolve o tamanho.
pub fn texto_em(
    ui: &Ui,
    fonte: Option<FontId>,
    dl: &DrawListMut<'_>,
    pos: [f32; 2],
    cor: [f32; 4],
    texto: &str,
) -> [f32; 2] {
    com_fonte(ui, fonte, || {
        dl.add_text(pos, cor, texto);
        ui.calc_text_size(texto)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_always_have_text_and_use_outline_with_tenuous_fill() {
        assert_eq!(Tier::TODOS.map(|t| badge_tier(t).texto), ["JR", "EXP", "ELITE"]);
        let qualidades = [Qualidade::Baixa, Qualidade::Media, Qualidade::Alta];
        assert_eq!(qualidades.map(|q| badge_qualidade(q).texto), ["BAIXA", "MÉDIA", "ALTA"]);
        // mesma escala de cores: cinza / roxo / dourado
        assert_eq!(badge_qualidade(Qualidade::Alta).contorno, theme::TIER_ELITE);
        assert_eq!(badge_qualidade(Qualidade::Baixa).contorno, theme::TIER_JUNIOR);
        for estilo in Tier::TODOS.map(badge_tier).into_iter().chain(qualidades.map(badge_qualidade)) {
            assert!(estilo.fundo[3] <= 0.15, "{}", estilo.texto);
        }
    }
}
