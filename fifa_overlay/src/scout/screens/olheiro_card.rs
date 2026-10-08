//! Card vertical do Olheiro (2026-10-08, pedido do Felipe): os cards da aba
//! Olheiros e do mercado deixam de ser faixas largas e viram uma grade, vários
//! por linha, cada um com tudo do Olheiro de cima para baixo — quem é (bandeira,
//! nome, nação, foco, nível), as estrelas dos cinco atributos, os mercados
//! com bandeira, as habilidades como selos e, no rodapé, o que ele está
//! fazendo (ou o custo e o "Contratar").
//!
//! Cada card é UM item navegável: A ativa; Y (ou o botão direito do mouse)
//! pede as Opções do Olheiro. Tudo é desenhado pelo draw list (nenhum outro
//! item), para não haver dois alvos de foco sobrepostos.

use imgui::{DrawListMut, MouseButton, Ui};

use super::componentes::{self, badge_tier, desenhar_badge, desenhar_estrelas, texto_em, EstiloBadge};
use super::olheiros::{chips_do_olheiro, desenhar_chips, sigla, texto_estrelas, Nacoes};
use super::relatorio::truncar;
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_milhar};
use crate::scout::quality::Estrelas;
use crate::scout::state::{Especializacao, OfertaOlheiro, Olheiro};

pub const LARGURA: f32 = 340.0;
pub const ALTURA: f32 = 332.0;
const ALTURA_NOVO: f32 = 120.0;
const LADO_AVATAR: f32 = 40.0;
const ALTURA_RODAPE: f32 = 58.0;
const LARGURA_BOTAO: f32 = 124.0;
const RAIO_DOT: f32 = 4.0;

/// O que o rodapé do card mostra.
pub enum Rodape<'a> {
    /// Olheiro contratado: o status (ponto colorido) e o que ele faz agora,
    /// em até duas linhas.
    Situacao { status: &'static str, cor: [f32; 4], linhas: Vec<String> },
    /// Olheiro do mercado: o custo e o botão "Contratar".
    Oferta(&'a OfertaOlheiro),
}

/// O que o jogador fez no card neste frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resultado {
    /// Clique ou A.
    pub ativou: bool,
    /// O card tem o foco do controle (para o Y valer para ele).
    pub focado: bool,
    /// Botão direito do mouse (as Opções, para quem usa mouse).
    pub opcoes: bool,
}

/// Quantos cards cabem por linha na largura disponível.
pub fn por_linha(disponivel: f32) -> usize {
    (((disponivel + theme::ESPACO_3) / (LARGURA + theme::ESPACO_3)).floor() as usize).max(1)
}

/// Linhas de estrelas do card: o atributo, as estrelas e a cor.
pub fn linhas_de_estrelas(o: &Olheiro) -> Vec<(&'static str, Estrelas, [f32; 4])> {
    let perfil = o.perfil();
    let foco = perfil.foco();
    let mut linhas: Vec<(&'static str, Estrelas, [f32; 4])> = Especializacao::TODAS
        .iter()
        .map(|&e| (e.nome(), perfil.atributo(e), if e == foco { theme::ACCENT_PRIMARY } else { theme::TEXT_SECONDARY }))
        .collect();
    linhas.push(("Rede de contatos", perfil.rede, theme::FIELD_GREEN));
    linhas
}

/// Desenha o card de um Olheiro no cursor. `habilidades`: os selos dele.
#[allow(clippy::too_many_arguments)]
pub fn desenhar(
    ui: &Ui,
    fonts: Option<&Fonts>,
    chave: &str,
    olheiro: &Olheiro,
    nacoes: &Nacoes<'_>,
    habilidades: &[EstiloBadge],
    rodape: &Rodape<'_>,
) -> Resultado {
    let _id = ui.push_id(chave);
    let min = ui.cursor_screen_pos();
    let max = [min[0] + LARGURA, min[1] + ALTURA];
    let ativou = ui.invisible_button("##card", [LARGURA, ALTURA]);
    let hover = ui.is_item_hovered();
    let focado = ui.is_item_focused() && ui.io().nav_visible;
    let opcoes = ui.is_item_clicked_with_button(MouseButton::Right);

    let dl = ui.get_window_draw_list();
    fundo(&dl, min, max, hover || focado, matches!(rodape, Rodape::Situacao { .. }));

    let meta = fonts.map(|f| f.meta);
    let x = min[0] + theme::ESPACO_3;
    let direita = max[0] - theme::ESPACO_3;
    let largura_util = direita - x;
    let mut y = min[1] + theme::ESPACO_3;

    // ---- quem é: avatar, bandeira + nome, nação · foco, nível
    let a_min = [x, y];
    let a_max = [x + LADO_AVATAR, y + LADO_AVATAR];
    dl.add_rect(a_min, a_max, [1.0, 1.0, 1.0, 0.05]).filled(true).rounding(theme::RAIO_MD).build();
    dl.add_rect(a_min, a_max, theme::BORDER_HAIRLINE).rounding(theme::RAIO_MD).build();
    texto_centralizado(ui, fonts.map(|f| f.heading), &dl, sigla(olheiro.perfil().foco()), a_min, a_max, theme::ACCENT_PRIMARY);

    let x_texto = a_max[0] + theme::ESPACO_2;
    let altura_nome = com_fonte(ui, fonts.map(|f| f.heading), || ui.text_line_height());
    let largura_badge = com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(badge_tier(olheiro.tier).texto)[0]) + theme::ESPACO_2 * 3.0;
    let mut x_nome = x_texto;
    if let Some(n) = &olheiro.nacao {
        let altura_bandeira = (altura_nome * 0.72).round();
        x_nome += componentes::bandeira(&dl, (nacoes.bandeira)(n.id), [x_texto, y + (altura_nome - altura_bandeira) * 0.5], altura_bandeira) + theme::ESPACO_2;
    }
    let medir_heading = |t: &str| com_fonte(ui, fonts.map(|f| f.heading), || ui.calc_text_size(t)[0]);
    let (nome, _) = truncar(&olheiro.nome_exibicao(), direita - largura_badge - x_nome, medir_heading);
    texto_em(ui, fonts.map(|f| f.heading), &dl, [x_nome, y], theme::TEXT_PRIMARY, &nome);
    desenhar_badge(ui, fonts, &dl, &badge_tier(olheiro.tier), [direita - largura_badge + theme::ESPACO_2, y], altura_nome);
    let nacao = olheiro.nacao.as_ref().map_or("Sem nação", |n| n.nome.as_str());
    let origem = format!("{nacao} · {}", olheiro.perfil().foco().nome());
    let medir_meta = |t: &str| com_fonte(ui, meta, || ui.calc_text_size(t)[0]);
    let (origem, _) = truncar(&origem, direita - x_texto, medir_meta);
    texto_em(ui, meta, &dl, [x_texto, y + altura_nome + theme::ESPACO_1], theme::TEXT_SECONDARY, &origem);
    y = a_max[1] + theme::ESPACO_2;
    y = divisor(&dl, x, direita, y);

    // ---- as estrelas
    let altura_linha = com_fonte(ui, meta, || ui.text_line_height()) + theme::ESPACO_1;
    let lado_estrela = (altura_linha * 0.62).max(8.0);
    let largura_estrelas = (lado_estrela + 2.0) * 5.0 - 2.0;
    for (nome, estrelas, cor) in linhas_de_estrelas(olheiro) {
        texto_em(ui, meta, &dl, [x, y], cor, nome);
        desenhar_estrelas(&dl, [direita - largura_estrelas, y + (altura_linha - lado_estrela) * 0.5 - 1.0], estrelas, lado_estrela, cor);
        y += altura_linha;
    }
    y = divisor(&dl, x, direita, y + theme::ESPACO_1);

    // ---- os mercados (com bandeira)
    let usado = desenhar_chips(ui, fonts, &dl, [x, y], largura_util, 2, &chips_do_olheiro(olheiro, nacoes, false));
    y += usado[1].max(altura_linha * 2.0 - theme::ESPACO_1) + theme::ESPACO_1;
    y = divisor(&dl, x, direita, y);

    // ---- as habilidades
    if habilidades.is_empty() {
        texto_em(ui, meta, &dl, [x, y], theme::TEXT_DISABLED, "Sem habilidades");
    } else {
        let mut xb = x;
        for badge in habilidades {
            let tamanho = desenhar_badge(ui, fonts, &dl, badge, [xb, y], altura_linha);
            xb += tamanho[0] + theme::ESPACO_2;
        }
    }

    // ---- o rodapé, encostado embaixo
    let y_rodape = max[1] - theme::ESPACO_3 - ALTURA_RODAPE;
    divisor(&dl, x, direita, y_rodape - theme::ESPACO_1);
    match rodape {
        Rodape::Situacao { status, cor, linhas } => situacao(ui, fonts, &dl, [x, y_rodape], direita, status, *cor, linhas),
        Rodape::Oferta(oferta) => oferta_no_rodape(ui, fonts, &dl, [x, y_rodape], direita, oferta, hover),
    }
    if hover || focado {
        ui.tooltip_text(texto_estrelas(&olheiro.perfil()));
    }
    let ativou = match rodape {
        Rodape::Oferta(oferta) => ativou && oferta.faltam.is_none(),
        Rodape::Situacao { .. } => ativou,
    };
    Resultado { ativou, focado, opcoes }
}

/// O card "+ Contratar Olheiro" (a entrada para o mercado).
pub fn desenhar_novo(ui: &Ui, fonts: Option<&Fonts>, chave: &str, titulo: &str, detalhe: &str) -> Resultado {
    let _id = ui.push_id(chave);
    let min = ui.cursor_screen_pos();
    let max = [min[0] + LARGURA, min[1] + ALTURA_NOVO];
    let ativou = ui.invisible_button("##card", [LARGURA, ALTURA_NOVO]);
    let focado = ui.is_item_focused() && ui.io().nav_visible;
    let dl = ui.get_window_draw_list();
    fundo(&dl, min, max, ui.is_item_hovered() || focado, false);
    let medir = |t: &str| com_fonte(ui, fonts.map(|f| f.meta), || ui.calc_text_size(t)[0]);
    let x = min[0] + theme::ESPACO_3;
    let largura = LARGURA - theme::ESPACO_3 * 2.0;
    let h = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, min[1] + theme::ESPACO_3], theme::FIELD_GREEN, &format!("+ {titulo}"))[1];
    let (detalhe, _) = truncar(detalhe, largura, medir);
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, min[1] + theme::ESPACO_3 + h + theme::ESPACO_1], theme::TEXT_SECONDARY, &detalhe);
    Resultado { ativou, focado, opcoes: false }
}

/// Fundo e borda: hover e foco têm a MESMA borda roxa de 2 px (UX-DR19).
fn fundo(dl: &DrawListMut<'_>, min: [f32; 2], max: [f32; 2], destaque: bool, contratado: bool) {
    let (borda, espessura) = if destaque {
        (theme::ACCENT_PRIMARY, super::ESPESSURA_FOCO)
    } else if contratado {
        (theme::BORDER_HAIRLINE, 1.0)
    } else {
        (theme::BORDER_HAIRLINE_SUBTLE, 1.0)
    };
    dl.add_rect(min, max, theme::BG_PANEL_RAISED).filled(true).rounding(theme::RAIO_MD).build();
    dl.add_rect(min, max, borda).rounding(theme::RAIO_MD).thickness(espessura).build();
}

/// Linha fina entre os blocos; devolve o `y` de depois dela.
fn divisor(dl: &DrawListMut<'_>, x0: f32, x1: f32, y: f32) -> f32 {
    dl.add_line([x0, y], [x1, y], theme::BORDER_HAIRLINE_SUBTLE).build();
    y + theme::ESPACO_2
}

fn texto_centralizado(ui: &Ui, fonte: Option<imgui::FontId>, dl: &DrawListMut<'_>, texto: &str, r_min: [f32; 2], r_max: [f32; 2], cor: [f32; 4]) {
    com_fonte(ui, fonte, || {
        let [w, h] = ui.calc_text_size(texto);
        dl.add_text([r_min[0] + (r_max[0] - r_min[0] - w) * 0.5, r_min[1] + (r_max[1] - r_min[1] - h) * 0.5], cor, texto);
    });
}

/// Ponto + status e, embaixo, o que ele faz; "Y  Opções" à direita.
#[allow(clippy::too_many_arguments)]
fn situacao(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, pos: [f32; 2], direita: f32, status: &str, cor: [f32; 4], linhas: &[String]) {
    let meta = fonts.map(|f| f.meta);
    let h = com_fonte(ui, fonts.map(|f| f.body), || {
        let [w, h] = ui.calc_text_size(status);
        dl.add_circle([pos[0] + RAIO_DOT, pos[1] + h * 0.5], RAIO_DOT, cor).filled(true).build();
        dl.add_text([pos[0] + RAIO_DOT * 2.0 + theme::ESPACO_2, pos[1]], cor, status);
        let _ = w;
        h
    });
    com_fonte(ui, meta, || {
        let dica = "Y  Opções";
        let w = ui.calc_text_size(dica)[0];
        dl.add_text([direita - w, pos[1] + (h - ui.text_line_height()) * 0.5], theme::TEXT_SECONDARY, dica);
    });
    let medir = |t: &str| com_fonte(ui, meta, || ui.calc_text_size(t)[0]);
    let mut y = pos[1] + h + theme::ESPACO_1;
    for linha in linhas.iter().take(2) {
        let (texto, _) = truncar(linha, direita - pos[0], medir);
        y += texto_em(ui, meta, dl, [pos[0], y], theme::TEXT_SECONDARY, &texto)[1];
    }
}

/// "custo" + valor em fonte mono e o botão "Contratar" (ou quanto falta).
fn oferta_no_rodape(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, pos: [f32; 2], direita: f32, oferta: &OfertaOlheiro, hover: bool) {
    let (rotulo, cor_rotulo) = match oferta.faltam {
        Some(faltam) => (format!("faltam {}", formatar_milhar(faltam)), theme::DANGER),
        None => ("custo".to_string(), theme::TEXT_SECONDARY),
    };
    let altura_rotulo = texto_em(ui, fonts.map(|f| f.meta), dl, pos, cor_rotulo, &rotulo)[1];
    texto_em(
        ui,
        fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body)),
        dl,
        [pos[0], pos[1] + altura_rotulo + theme::ESPACO_1],
        theme::TEXT_PRIMARY,
        &formatar_milhar(oferta.custo),
    );
    let b_min = [direita - LARGURA_BOTAO, pos[1] + (ALTURA_RODAPE - theme::ALVO_MINIMO) * 0.5];
    let b_max = [direita, b_min[1] + theme::ALVO_MINIMO];
    let habilitado = oferta.faltam.is_none();
    let (fundo, texto) = if habilitado { (theme::FIELD_GREEN, theme::BG_BASE) } else { (theme::BOTAO_DESABILITADO, theme::TEXT_DISABLED) };
    dl.add_rect(b_min, b_max, fundo).filled(true).rounding(theme::RAIO_PADRAO).build();
    texto_centralizado(ui, fonts.map(|f| f.heading), dl, "Contratar", b_min, b_max, texto);
    if let (Some(faltam), true) = (oferta.faltam, hover) {
        ui.tooltip_text(super::olheiros::texto_faltam(faltam));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::quality::PerfilOlheiro;
    use crate::scout::state::Tier;

    #[test]
    fn the_card_lists_the_four_attributes_and_the_network_with_the_focus_highlighted() {
        let o = Olheiro { perfil: Some(PerfilOlheiro::v1(Especializacao::Tatico, Tier::Elite)), especializacao: Especializacao::Tatico, tier: Tier::Elite, ..Default::default() };
        let linhas = linhas_de_estrelas(&o);
        assert_eq!(linhas.len(), 5);
        assert_eq!(linhas[4].0, "Rede de contatos");
        let destacadas: Vec<&str> = linhas.iter().filter(|(_, _, c)| *c == theme::ACCENT_PRIMARY).map(|(n, _, _)| *n).collect();
        assert_eq!(destacadas, vec![Especializacao::Tatico.nome()], "só o foco em roxo");
    }

    #[test]
    fn at_least_one_card_per_row_and_more_when_the_panel_is_wide() {
        assert_eq!(por_linha(100.0), 1);
        assert_eq!(por_linha(LARGURA), 1);
        assert_eq!(por_linha(LARGURA * 2.0 + theme::ESPACO_3), 2);
        assert!(por_linha(1700.0) >= 4, "num painel de 2560×1080 cabem 4");
    }
}
