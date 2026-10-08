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
use crate::scout::minifaces::Rosto;
use crate::scout::quality::{Estrelas, Frescor};
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

/// O selo de uma habilidade de Olheiro (2026-10-08): contorno roxo, texto
/// curto (FIT, REFERÊNCIA, ...).
pub fn badge_habilidade(habilidade: crate::scout::quality::Habilidade) -> EstiloBadge {
    EstiloBadge {
        texto: habilidade.selo(),
        contorno: theme::ACCENT_PRIMARY,
        fundo: theme::ACCENT_PRIMARY_DIM,
        cor_texto: theme::ACCENT_PRIMARY,
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

/// "ESCOLHIDO" em verde-campo: o jogador está na Lista de Escolhidos.
pub fn badge_escolhido() -> EstiloBadge {
    EstiloBadge { texto: "ESCOLHIDO", contorno: theme::FIELD_GREEN, fundo: theme::FIELD_GREEN_DIM, cor_texto: theme::FIELD_GREEN }
}

/// "BASE" em roxo: o jogador veio da Base do Scout (o clube já o tinha
/// mapeado), não da pesquisa do Olheiro da Missão.
pub fn badge_base() -> EstiloBadge {
    EstiloBadge { texto: "BASE", contorno: theme::ACCENT_PRIMARY, fundo: theme::ACCENT_PRIMARY_DIM, cor_texto: theme::ACCENT_PRIMARY }
}

/// Estado de um Escolhido: ACOMPANHADO (roxo), ATUALIZADO (verde),
/// ENVELHECENDO/DESATUALIZADO (dourado), VENCIDO (vermelho). Sempre com
/// texto (NFR5).
pub fn badge_frescor(frescor: Frescor, acompanhado: bool) -> EstiloBadge {
    let (texto, cor, fundo) = match (acompanhado, frescor) {
        (true, _) => ("ACOMPANHADO", theme::ACCENT_PRIMARY, theme::ACCENT_PRIMARY_DIM),
        (false, Frescor::Atualizado) => ("ATUALIZADO", theme::FIELD_GREEN, theme::FIELD_GREEN_DIM),
        (false, Frescor::Envelhecendo { .. }) => ("ENVELHECENDO", theme::WARNING, theme::TRANSPARENTE),
        (false, Frescor::Desatualizado { .. }) => ("DESATUALIZADO", theme::WARNING, theme::TRANSPARENTE),
        (false, Frescor::Vencido) => ("VENCIDO", theme::DANGER, theme::TRANSPARENTE),
    };
    EstiloBadge { texto, contorno: cor, fundo, cor_texto: cor }
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

/// O próximo item navegável que for desenhado recebe o foco do controle
/// (botões não são "clicados": só focados).
pub fn focar_proximo_item() {
    // SAFETY: chamada simples do ImGui 1.89, com um quadro em andamento.
    unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
}

/// Seletor de dois ou mais botões (ex.: Tabular / Cards): o escolhido em
/// roxo. Só muda com o clique (ou o A em cima do botão): passar o foco por
/// cima não escolhe nada (2026-10-08, pedido do Felipe para as telas de
/// lista). Devolve o índice ativado neste frame.
pub fn alternador_por_clique(ui: &Ui, fonts: Option<&Fonts>, opcoes: &[&str], escolhida: usize, largura: f32) -> Option<usize> {
    let mut ativada = None;
    for (indice, rotulo) in opcoes.iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_1);
        }
        let estilo = if indice == escolhida { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        if botao_com_largura(ui, fonts, rotulo, estilo, true, Some(largura)) && indice != escolhida {
            ativada = Some(indice);
        }
    }
    ativada
}

// ---------------------------------------------------------------------
// Estrelas (Épico 5)
// ---------------------------------------------------------------------

/// Os 10 vértices de uma estrela de 5 pontas (pontas e vales alternados),
/// começando pela ponta de cima.
pub fn vertices_estrela(centro: [f32; 2], raio: f32) -> [[f32; 2]; 10] {
    let mut v = [[0.0; 2]; 10];
    for (i, p) in v.iter_mut().enumerate() {
        let r = if i % 2 == 0 { raio } else { raio * 0.45 };
        let angulo = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        *p = [centro[0] + r * angulo.cos(), centro[1] + r * angulo.sin()];
    }
    v
}

/// Uma estrela cheia: 5 triângulos das pontas + o pentágono do meio (o
/// draw list só preenche polígonos convexos).
fn estrela_cheia(dl: &DrawListMut<'_>, centro: [f32; 2], raio: f32, cor: [f32; 4]) {
    let v = vertices_estrela(centro, raio);
    let vale = |i: usize| v[(i + 10) % 10];
    for ponta in (0..10).step_by(2) {
        dl.add_triangle(vale(ponta + 9), v[ponta], vale(ponta + 1), cor).filled(true).build();
    }
    let pentagono: Vec<[f32; 2]> = (1..10).step_by(2).map(|i| v[i]).collect();
    dl.add_polyline(pentagono, cor).filled(true).build();
}

/// Cinco estrelas a partir de `pos` (canto superior esquerdo), cada uma
/// com `lado` px: cheias, meia e vazias conforme `estrelas`. Devolve a
/// largura ocupada. Sem glifo de estrela na fonte do overlay, elas são
/// desenhadas.
pub fn desenhar_estrelas(dl: &DrawListMut<'_>, pos: [f32; 2], estrelas: Estrelas, lado: f32, cor: [f32; 4]) -> f32 {
    let raio = lado * 0.5;
    let passo = lado + 2.0;
    let apagada = [cor[0], cor[1], cor[2], cor[3] * 0.22];
    let meias = estrelas.meias();
    for i in 0..5u8 {
        let centro = [pos[0] + raio + f32::from(i) * passo, pos[1] + raio];
        estrela_cheia(dl, centro, raio, apagada);
        let cheias = meias / 2;
        if i < cheias {
            estrela_cheia(dl, centro, raio, cor);
        } else if i == cheias && meias % 2 == 1 {
            // meia estrela: só a metade esquerda
            dl.with_clip_rect_intersect([centro[0] - raio - 1.0, centro[1] - raio - 1.0], [centro[0], centro[1] + raio + 1.0], || {
                estrela_cheia(dl, centro, raio, cor);
            });
        }
    }
    passo * 5.0 - 2.0
}

/// "Rótulo ★★★½☆" pelo draw list: o rótulo em texto secundário e as
/// estrelas ao lado, centradas na linha. Devolve a largura ocupada.
pub fn rotulo_com_estrelas(
    ui: &Ui,
    fonts: Option<&Fonts>,
    dl: &DrawListMut<'_>,
    pos: [f32; 2],
    rotulo: &str,
    estrelas: Estrelas,
    cor: [f32; 4],
) -> f32 {
    let [w, h] = texto_em(ui, fonts.map(|f| f.meta), dl, pos, theme::TEXT_SECONDARY, rotulo);
    let lado = (h * 0.8).max(8.0);
    let x = pos[0] + w + theme::ESPACO_1;
    let largura = desenhar_estrelas(dl, [x, pos[1] + (h - lado) * 0.5], estrelas, lado, cor);
    w + theme::ESPACO_1 + largura
}

// ---------------------------------------------------------------------
// Bandeira de nação (2026-10-05)
// ---------------------------------------------------------------------

/// Largura ÷ altura da bandeira (o ícone do jogo é ~1,6 : 1).
const PROPORCAO_BANDEIRA: f32 = 1.6;

/// Largura de uma bandeira de `altura` px.
pub fn largura_da_bandeira(altura: f32) -> f32 {
    (altura * PROPORCAO_BANDEIRA).round()
}

/// A bandeira pelo draw list, com o canto de cima à esquerda em `pos`.
/// Enquanto a imagem não chega (ou se o jogo não a tem), um retângulo
/// neutro com a mesma borda marca o lugar. Devolve a largura ocupada.
pub fn bandeira(dl: &DrawListMut<'_>, rosto: Rosto, pos: [f32; 2], altura: f32) -> f32 {
    let largura = largura_da_bandeira(altura);
    let max = [pos[0] + largura, pos[1] + altura];
    match rosto {
        Rosto::Pronto(textura) => dl.add_image_rounded(textura, pos, max, 2.0).build(),
        Rosto::Carregando | Rosto::Ausente => dl.add_rect(pos, max, [1.0, 1.0, 1.0, 0.08]).filled(true).rounding(2.0).build(),
    }
    dl.add_rect(pos, max, theme::BORDER_HAIRLINE).rounding(2.0).build();
    largura
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

    #[test]
    fn every_freshness_badge_says_it_in_words() {
        let estados = [Frescor::Atualizado, Frescor::Envelhecendo { extra: 2 }, Frescor::Desatualizado { extra: 6 }, Frescor::Vencido];
        let textos: Vec<&str> = estados.iter().map(|&f| badge_frescor(f, false).texto).collect();
        assert_eq!(textos, ["ATUALIZADO", "ENVELHECENDO", "DESATUALIZADO", "VENCIDO"]);
        assert_eq!(badge_frescor(Frescor::Vencido, true).texto, "ACOMPANHADO");
        assert_eq!(badge_escolhido().texto, "ESCOLHIDO");
    }

    #[test]
    fn a_star_alternates_tips_and_valleys_starting_at_the_top() {
        let v = vertices_estrela([0.0, 0.0], 10.0);
        assert!((v[0][0]).abs() < 1e-4 && (v[0][1] + 10.0).abs() < 1e-4, "ponta de cima");
        for (i, p) in v.iter().enumerate() {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            let esperado = if i % 2 == 0 { 10.0 } else { 4.5 };
            assert!((r - esperado).abs() < 1e-3, "{i}: {r}");
        }
    }
}
