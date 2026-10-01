//! Aba Olheiros (Story 1.4): os Olheiros contratados e as 12 combinações
//! Especialização × Tier à venda, cada uma com o custo (FR-2).
//!
//! Layout do `mockups/olheiros.html` (v2): card com avatar de sigla, nome
//! com badge de Tier em contorno, descrição curta e, à direita, custo e
//! "Contratar" (ou o status, para os contratados). Diferenças de
//! propósito: rótulos de seção e botões sem maiúsculas (DESIGN.md:
//! maiúsculas só nos badges) e valores sem símbolo de moeda (a moeda do
//! jogo não está mapeada — mesma decisão do cabeçalho na Story 1.2).
//!
//! Os dados vêm de `scout::state` (AD-1); custos, de `scout::quality`.
//! A confirmação de contratação é a Story 1.5: por enquanto o botão só
//! registra o pedido no log.

use imgui::{DrawListMut, ItemHoveredFlags, StyleColor, Ui};

use super::theme::{self, Fonts};
use super::{com_fonte, formatar_milhar};
use crate::scout::state::{Especializacao, OfertaOlheiro, OlheiroContratado, ScoutState, Tier};

const ALTURA_CARD: f32 = 72.0;
const LADO_AVATAR: f32 = 44.0;
const LARGURA_BOTAO: f32 = 132.0;
const RAIO_DOT: f32 = 4.0;

pub const MSG_NENHUM_CONTRATADO: &str = "Nenhum Olheiro contratado ainda.";

/// Sigla do avatar (mockup: CJ, CM, T, G).
pub fn sigla(especializacao: Especializacao) -> &'static str {
    match especializacao {
        Especializacao::CacadorDeJovens => "CJ",
        Especializacao::CacadorDeMedalhoes => "CM",
        Especializacao::Tatico => "T",
        Especializacao::Generalista => "G",
    }
}

/// Uma linha sobre o que cada Especialização faz bem (textos do mockup).
pub fn descricao(especializacao: Especializacao) -> &'static str {
    match especializacao {
        Especializacao::CacadorDeJovens => "Foco em potencial, idade baixa.",
        Especializacao::CacadorDeMedalhoes => "Foco em jogadores consagrados, prontos para jogar já.",
        Especializacao::Tatico => "Especialista em fit de atributos e posição.",
        Especializacao::Generalista => "Sem especialização: barato, relatórios rasos.",
    }
}

/// Texto do badge de Tier (sempre junto da cor — NFR5).
pub fn texto_badge(tier: Tier) -> &'static str {
    match tier {
        Tier::Junior => "JR",
        Tier::Experiente => "EXP",
        Tier::Elite => "ELITE",
    }
}

/// `(contorno, preenchimento, texto)` do badge: contorno + preenchimento
/// tênue, nunca sólido (DESIGN.md → tier-badge-*).
pub fn cores_badge(tier: Tier) -> ([f32; 4], [f32; 4], [f32; 4]) {
    match tier {
        Tier::Junior => (theme::TIER_JUNIOR, theme::TRANSPARENTE, theme::TIER_JUNIOR_TEXTO),
        Tier::Experiente => (theme::TIER_EXPERIENTE, theme::ACCENT_PRIMARY_DIM, theme::TIER_EXPERIENTE),
        Tier::Elite => (theme::TIER_ELITE, theme::TRANSPARENTE, theme::TIER_ELITE),
    }
}

/// Microcopy do bloqueio por orçamento (UX-DR9/DR21: valor exato, sem
/// exclamação).
pub fn texto_faltam(faltam: i32) -> String {
    format!("Orçamento insuficiente: faltam {}.", formatar_milhar(faltam))
}

/// O que vai no lado direito do card.
enum Lado {
    Contratar(OfertaOlheiro),
    Status { em_missao: bool },
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState) {
    rotulo_secao(ui, fonts, "Contratados");
    let contratados = state.olheiros_contratados();
    if contratados.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_NENHUM_CONTRATADO));
    }
    for (indice, OlheiroContratado { olheiro, em_missao }) in contratados.iter().enumerate() {
        let lado = Lado::Status { em_missao: *em_missao };
        card(ui, fonts, &format!("contratado{indice}"), olheiro.especializacao, olheiro.tier, &lado);
    }

    ui.dummy([0.0, theme::ESPACO_3]);
    rotulo_secao(ui, fonts, "Disponíveis para contratação");
    for oferta in state.olheiros_disponiveis() {
        let chave = format!("oferta_{:?}_{:?}", oferta.especializacao, oferta.tier);
        if card(ui, fonts, &chave, oferta.especializacao, oferta.tier, &Lado::Contratar(oferta)) {
            tracing::info!(
                "[scout::olheiros] Contratar {} {:?} ({}) pedido; a confirmação chega na Story 1.5.",
                oferta.especializacao.nome(),
                oferta.tier,
                oferta.custo
            );
        }
    }
}

fn rotulo_secao(ui: &Ui, fonts: Option<&Fonts>, texto: &str) {
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text_colored(theme::TEXT_SECONDARY, texto));
}

/// Desenha um card e devolve `true` se "Contratar" foi clicado (só com
/// orçamento suficiente). Fundo e borda são desenhados DEPOIS do conteúdo
/// (canal 0 do draw list), para a borda já saber se há hover ou foco.
fn card(ui: &Ui, fonts: Option<&Fonts>, chave: &str, especializacao: Especializacao, tier: Tier, lado: &Lado) -> bool {
    let _id = ui.push_id(chave);
    let largura = ui.content_region_avail()[0];
    let min = ui.cursor_screen_pos();
    let max = [min[0] + largura, min[1] + ALTURA_CARD];
    let mut clicou = false;
    let mut focado = false;

    let dl = ui.get_window_draw_list();
    dl.channels_split(2, |canais| {
        canais.set_current(1);

        // Avatar com a sigla.
        let a_min = [min[0] + theme::ESPACO_4, min[1] + (ALTURA_CARD - LADO_AVATAR) * 0.5];
        let a_max = [a_min[0] + LADO_AVATAR, a_min[1] + LADO_AVATAR];
        dl.add_rect(a_min, a_max, [1.0, 1.0, 1.0, 0.05]).filled(true).rounding(theme::RAIO_MD).build();
        dl.add_rect(a_min, a_max, theme::BORDER_HAIRLINE).rounding(theme::RAIO_MD).build();
        com_fonte(ui, fonts.map(|f| f.heading), || {
            let texto = sigla(especializacao);
            let [w, h] = ui.calc_text_size(texto);
            let pos = [a_min[0] + (LADO_AVATAR - w) * 0.5, a_min[1] + (LADO_AVATAR - h) * 0.5];
            dl.add_text(pos, theme::ACCENT_PRIMARY, texto);
        });

        // Nome + badge, descrição embaixo.
        let x_texto = a_max[0] + theme::ESPACO_3;
        ui.set_cursor_screen_pos([x_texto, min[1] + theme::ESPACO_3]);
        let altura_nome = com_fonte(ui, fonts.map(|f| f.heading), || {
            ui.text(especializacao.nome());
            ui.calc_text_size(especializacao.nome())[1]
        });
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        badge_tier(ui, fonts, &dl, tier, altura_nome);
        ui.set_cursor_screen_pos([x_texto, ui.cursor_screen_pos()[1]]);
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, descricao(especializacao)));

        // Lado direito.
        let direita = max[0] - theme::ESPACO_4;
        match lado {
            Lado::Contratar(oferta) => {
                let x_botao = direita - LARGURA_BOTAO;
                custo(ui, fonts, oferta, x_botao - theme::ESPACO_3, min[1]);
                ui.set_cursor_screen_pos([x_botao, min[1] + (ALTURA_CARD - theme::ALVO_MINIMO) * 0.5]);
                let habilitado = oferta.faltam.is_none();
                clicou = botao_contratar(ui, fonts, habilitado) && habilitado;
                focado = ui.is_item_focused();
                if let Some(faltam) = oferta.faltam {
                    if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                        ui.tooltip_text(texto_faltam(faltam));
                    }
                }
            }
            Lado::Status { em_missao } => status(ui, fonts, &dl, *em_missao, direita, min[1]),
        }

        // Fundo e borda por baixo do conteúdo.
        canais.set_current(0);
        let hover = ui.is_window_hovered() && ui.is_mouse_hovering_rect(min, max);
        let borda = if hover || focado {
            theme::ACCENT_PRIMARY
        } else if matches!(lado, Lado::Status { .. }) {
            theme::BORDER_HAIRLINE
        } else {
            theme::BORDER_HAIRLINE_SUBTLE
        };
        dl.add_rect(min, max, theme::BG_PANEL_RAISED).filled(true).rounding(theme::RAIO_MD).build();
        dl.add_rect(min, max, borda).rounding(theme::RAIO_MD).build();
    });
    drop(dl);

    // Um item do tamanho do card fixa a área ocupada (scroll e próximo card).
    ui.set_cursor_screen_pos(min);
    ui.dummy([largura, ALTURA_CARD]);
    clicou
}

/// Badge em contorno + preenchimento tênue, com o texto do Tier,
/// centralizado na altura do nome ao lado (`altura_linha`).
fn badge_tier(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, tier: Tier, altura_linha: f32) {
    let (contorno, fundo, cor_texto) = cores_badge(tier);
    let texto = texto_badge(tier);
    com_fonte(ui, fonts.map(|f| f.badge), || {
        let [w, h] = ui.calc_text_size(texto);
        let padding = [theme::ESPACO_2, 2.0];
        let tamanho = [w + padding[0] * 2.0, h + padding[1] * 2.0];
        let pos = ui.cursor_screen_pos();
        let b_min = [pos[0], pos[1] + (altura_linha - tamanho[1]).max(0.0) * 0.5];
        let b_max = [b_min[0] + tamanho[0], b_min[1] + tamanho[1]];
        dl.add_rect(b_min, b_max, fundo).filled(true).rounding(theme::RAIO_SM).build();
        dl.add_rect(b_min, b_max, contorno).rounding(theme::RAIO_SM).build();
        dl.add_text([b_min[0] + padding[0], b_min[1] + padding[1]], cor_texto, texto);
        ui.dummy([tamanho[0], altura_linha.max(tamanho[1])]);
    });
}

/// "custo" + valor em fonte mono, alinhados à direita de `x_direita`; sem
/// orçamento, a segunda linha diz quanto falta (vermelho + texto).
fn custo(ui: &Ui, fonts: Option<&Fonts>, oferta: &OfertaOlheiro, x_direita: f32, y_card: f32) {
    let valor = formatar_milhar(oferta.custo);
    let (rotulo, cor_rotulo) = match oferta.faltam {
        Some(faltam) => (format!("faltam {}", formatar_milhar(faltam)), theme::DANGER),
        None => ("custo".to_string(), theme::TEXT_SECONDARY),
    };
    let largura_rotulo = com_fonte(ui, fonts.map(|f| f.meta), || ui.calc_text_size(&rotulo)[0]);
    let largura_valor = com_fonte(ui, fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body)), || ui.calc_text_size(&valor)[0]);
    ui.set_cursor_screen_pos([x_direita - largura_rotulo, y_card + theme::ESPACO_3]);
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(cor_rotulo, &rotulo));
    ui.set_cursor_screen_pos([x_direita - largura_valor, ui.cursor_screen_pos()[1]]);
    com_fonte(ui, fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body)), || ui.text(&valor));
}

/// Botão primário verde (≥ 32 px, UX-DR20); sem orçamento, cinza e inerte.
fn botao_contratar(ui: &Ui, fonts: Option<&Fonts>, habilitado: bool) -> bool {
    let (fundo, hover, texto) = if habilitado {
        (theme::FIELD_GREEN, theme::FIELD_GREEN, theme::BG_BASE)
    } else {
        (theme::BOTAO_DESABILITADO, theme::BOTAO_DESABILITADO, theme::TEXT_DISABLED)
    };
    let _c1 = ui.push_style_color(StyleColor::Button, fundo);
    let _c2 = ui.push_style_color(StyleColor::ButtonHovered, hover);
    let _c3 = ui.push_style_color(StyleColor::ButtonActive, hover);
    let _c4 = ui.push_style_color(StyleColor::Text, texto);
    com_fonte(ui, fonts.map(|f| f.heading), || ui.button_with_size("Contratar", [LARGURA_BOTAO, theme::ALVO_MINIMO]))
}

/// Ponto + "Disponível" (verde) ou "Em Missão" (dourado): cor e texto.
fn status(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, em_missao: bool, x_direita: f32, y_card: f32) {
    let (texto, cor) = if em_missao { ("Em Missão", theme::WARNING) } else { ("Disponível", theme::FIELD_GREEN) };
    com_fonte(ui, fonts.map(|f| f.body), || {
        let [w, h] = ui.calc_text_size(texto);
        let y = y_card + (ALTURA_CARD - h) * 0.5;
        ui.set_cursor_screen_pos([x_direita - w, y]);
        ui.text_colored(cor, texto);
        let centro = [x_direita - w - theme::ESPACO_2 - RAIO_DOT, y + h * 0.5];
        dl.add_circle(centro, RAIO_DOT, cor).filled(true).build();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_always_have_text_and_follow_the_three_step_colour_scale() {
        assert_eq!(Tier::TODOS.map(texto_badge), ["JR", "EXP", "ELITE"]);
        assert_eq!(cores_badge(Tier::Junior).0, theme::TIER_JUNIOR);
        assert_eq!(cores_badge(Tier::Experiente).0, theme::TIER_EXPERIENTE);
        assert_eq!(cores_badge(Tier::Elite).0, theme::TIER_ELITE);
        // contorno + preenchimento tênue, nunca sólido
        for tier in Tier::TODOS {
            assert!(cores_badge(tier).1[3] <= 0.15, "{tier:?}");
        }
    }

    #[test]
    fn each_especializacao_has_its_own_initials_and_description() {
        let siglas: Vec<_> = Especializacao::TODAS.map(sigla).to_vec();
        assert_eq!(siglas, ["CJ", "CM", "T", "G"]);
        let mut descricoes: Vec<_> = Especializacao::TODAS.map(descricao).to_vec();
        descricoes.dedup();
        assert_eq!(descricoes.len(), 4);
    }

    #[test]
    fn microcopy_is_exact_and_without_exclamation() {
        assert_eq!(MSG_NENHUM_CONTRATADO, "Nenhum Olheiro contratado ainda.");
        assert_eq!(texto_faltam(2_100_000), "Orçamento insuficiente: faltam 2.100.000.");
        for e in Especializacao::TODAS {
            assert!(!descricao(e).contains('!'));
        }
    }
}
