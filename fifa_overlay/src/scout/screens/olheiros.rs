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
//! Cada card é UM item navegável (Story 1.6): mouse e controle focam o
//! card inteiro, inclusive os contratados; numa oferta, ativar o card (clique
//! ou A) é "Contratar".
//!
//! Os dados vêm de `scout::state` (AD-1); custos, de `scout::quality`.
//! "Contratar" devolve o pedido para `screens::render_painel`, que abre a
//! Confirmação de Contratação (Story 1.5).

use imgui::{DrawListMut, Ui};

use super::theme::{self, Fonts};
use super::{com_fonte, formatar_milhar};
use crate::scout::state::{Especializacao, OfertaOlheiro, OlheiroContratado, ScoutState, Tier};

const ALTURA_CARD: f32 = 72.0;
const LADO_AVATAR: f32 = 44.0;
const LARGURA_BOTAO: f32 = 132.0;
const RAIO_DOT: f32 = 4.0;
/// Altura reservada ao rótulo de seção acima do primeiro card dela.
const ALTURA_ROTULO: f32 = 40.0;

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

/// Desenha a aba; devolve a combinação cujo "Contratar" foi clicado.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState) -> Option<(Especializacao, Tier)> {
    let mut pedido = None;
    rotulo_secao(ui, fonts, "Contratados");
    let contratados = state.olheiros_contratados();
    if contratados.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_NENHUM_CONTRATADO));
    }
    for (indice, OlheiroContratado { olheiro, em_missao }) in contratados.iter().enumerate() {
        let lado = Lado::Status { em_missao: *em_missao };
        let revelar = if indice == 0 { Revelar::Topo } else { Revelar::Nada };
        // Contratado: foca, mas ainda não tem ação (Missões chegam no Épico 2).
        card(ui, fonts, &olheiro.id.to_string(), olheiro.especializacao, olheiro.tier, &lado, revelar);
    }

    ui.dummy([0.0, theme::ESPACO_3]);
    rotulo_secao(ui, fonts, "Disponíveis para contratação");
    for (indice, oferta) in state.olheiros_disponiveis().into_iter().enumerate() {
        let chave = format!("oferta_{:?}_{:?}", oferta.especializacao, oferta.tier);
        let revelar = match (indice, contratados.is_empty()) {
            (0, true) => Revelar::Topo,
            (0, false) => Revelar::Rotulo,
            _ => Revelar::Nada,
        };
        if card(ui, fonts, &chave, oferta.especializacao, oferta.tier, &Lado::Contratar(oferta), revelar) {
            pedido = Some((oferta.especializacao, oferta.tier));
        }
    }
    pedido
}

/// Nome da Especialização com o badge do Tier ao lado (também usado na
/// Confirmação de Contratação).
pub fn nome_com_badge(ui: &Ui, fonts: Option<&Fonts>, especializacao: Especializacao, tier: Tier) {
    let altura_nome = com_fonte(ui, fonts.map(|f| f.heading), || {
        ui.text(especializacao.nome());
        ui.calc_text_size(especializacao.nome())[1]
    });
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    let pos = ui.cursor_screen_pos();
    let dl = ui.get_window_draw_list();
    let tamanho = desenhar_badge(ui, fonts, &dl, tier, pos, altura_nome);
    drop(dl);
    ui.dummy([tamanho[0], altura_nome.max(tamanho[1])]);
}

/// Rótulo de seção ("Contratados" / "Disponíveis…"). Não é item
/// navegável: quem garante que ele aparece ao voltar com o controle é o
/// `Revelar` do primeiro card abaixo dele.
fn rotulo_secao(ui: &Ui, fonts: Option<&Fonts>, texto: &str) {
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text_colored(theme::TEXT_SECONDARY, texto));
}

/// O que mostrar ACIMA do card quando ele recebe o foco do controle.
#[derive(Clone, Copy, PartialEq)]
enum Revelar {
    /// Só o card (o ImGui já rola para ele).
    Nada,
    /// Primeiro card da aba: rola até o topo (título e estado vazio).
    Topo,
    /// Primeiro card de uma seção: também o rótulo da seção, logo acima.
    Rotulo,
}

/// Desenha um card como UM item navegável (o card inteiro): mouse e
/// controle focam o card, e o ImGui rola para mostrá-lo por completo.
/// Devolve `true` se um card de oferta foi ativado (clique ou A) com
/// orçamento suficiente. Tudo dentro do card é desenhado pelo draw list
/// (nenhum outro item), para não haver dois alvos de foco sobrepostos.
fn card(
    ui: &Ui,
    fonts: Option<&Fonts>,
    chave: &str,
    especializacao: Especializacao,
    tier: Tier,
    lado: &Lado,
    revelar: Revelar,
) -> bool {
    let _id = ui.push_id(chave);
    let largura = ui.content_region_avail()[0];
    let min = ui.cursor_screen_pos();
    let max = [min[0] + largura, min[1] + ALTURA_CARD];

    let ativou = ui.invisible_button("##card", [largura, ALTURA_CARD]);
    let hover = ui.is_item_hovered();
    let focado = ui.is_item_focused() && ui.io().nav_visible;
    if focado {
        revelar_acima(ui, min[1], revelar);
    }

    let dl = ui.get_window_draw_list();
    // Fundo e borda: hover e foco têm a MESMA borda roxa de 2 px (UX-DR19).
    let (borda, espessura) = if hover || focado {
        (theme::ACCENT_PRIMARY, super::ESPESSURA_FOCO)
    } else if matches!(lado, Lado::Status { .. }) {
        (theme::BORDER_HAIRLINE, 1.0)
    } else {
        (theme::BORDER_HAIRLINE_SUBTLE, 1.0)
    };
    dl.add_rect(min, max, theme::BG_PANEL_RAISED).filled(true).rounding(theme::RAIO_MD).build();
    dl.add_rect(min, max, borda).rounding(theme::RAIO_MD).thickness(espessura).build();

    // Avatar com a sigla.
    let a_min = [min[0] + theme::ESPACO_4, min[1] + (ALTURA_CARD - LADO_AVATAR) * 0.5];
    let a_max = [a_min[0] + LADO_AVATAR, a_min[1] + LADO_AVATAR];
    dl.add_rect(a_min, a_max, [1.0, 1.0, 1.0, 0.05]).filled(true).rounding(theme::RAIO_MD).build();
    dl.add_rect(a_min, a_max, theme::BORDER_HAIRLINE).rounding(theme::RAIO_MD).build();
    texto_centralizado(ui, fonts.map(|f| f.heading), &dl, sigla(especializacao), a_min, a_max, theme::ACCENT_PRIMARY);

    // Nome + badge; descrição embaixo.
    let x_texto = a_max[0] + theme::ESPACO_3;
    let y_nome = min[1] + theme::ESPACO_3;
    let [largura_nome, altura_nome] = com_fonte(ui, fonts.map(|f| f.heading), || {
        dl.add_text([x_texto, y_nome], theme::TEXT_PRIMARY, especializacao.nome());
        ui.calc_text_size(especializacao.nome())
    });
    desenhar_badge(ui, fonts, &dl, tier, [x_texto + largura_nome + theme::ESPACO_2, y_nome], altura_nome);
    com_fonte(ui, fonts.map(|f| f.meta), || {
        let y = y_nome + altura_nome + theme::ESPACO_1;
        dl.add_text([x_texto, y], theme::TEXT_SECONDARY, descricao(especializacao));
    });

    // Lado direito.
    let direita = max[0] - theme::ESPACO_4;
    let mut pediu = false;
    match lado {
        Lado::Contratar(oferta) => {
            let b_min = [direita - LARGURA_BOTAO, min[1] + (ALTURA_CARD - theme::ALVO_MINIMO) * 0.5];
            let b_max = [direita, b_min[1] + theme::ALVO_MINIMO];
            custo(ui, fonts, &dl, oferta, b_min[0] - theme::ESPACO_3, min[1]);
            let habilitado = oferta.faltam.is_none();
            let (fundo, texto) = if habilitado {
                (theme::FIELD_GREEN, theme::BG_BASE)
            } else {
                (theme::BOTAO_DESABILITADO, theme::TEXT_DISABLED)
            };
            dl.add_rect(b_min, b_max, fundo).filled(true).rounding(theme::RAIO_PADRAO).build();
            texto_centralizado(ui, fonts.map(|f| f.heading), &dl, "Contratar", b_min, b_max, texto);
            pediu = ativou && habilitado;
            if let (Some(faltam), true) = (oferta.faltam, hover) {
                ui.tooltip_text(texto_faltam(faltam));
            }
        }
        Lado::Status { em_missao } => status(ui, fonts, &dl, *em_missao, direita, min[1]),
    }
    pediu
}

/// Com o foco do controle num card cujo conteúdo de cima (título ou
/// rótulo de seção, que não são itens) ficou escondido, rola até ele. O
/// ImGui sozinho só garante o próprio card visível.
fn revelar_acima(ui: &Ui, topo_card: f32, revelar: Revelar) {
    let topo_visivel = ui.window_pos()[1];
    match revelar {
        Revelar::Nada => {}
        Revelar::Topo => {
            if ui.scroll_y() > 0.0 {
                ui.set_scroll_y(0.0);
            }
        }
        Revelar::Rotulo => {
            let alvo = topo_card - ALTURA_ROTULO;
            if alvo < topo_visivel {
                ui.set_scroll_y((ui.scroll_y() - (topo_visivel - alvo)).max(0.0));
            }
        }
    }
}

/// Texto centralizado num retângulo (fonte opcional do tema).
fn texto_centralizado(
    ui: &Ui,
    fonte: Option<imgui::FontId>,
    dl: &DrawListMut<'_>,
    texto: &str,
    r_min: [f32; 2],
    r_max: [f32; 2],
    cor: [f32; 4],
) {
    com_fonte(ui, fonte, || {
        let [w, h] = ui.calc_text_size(texto);
        let pos = [r_min[0] + (r_max[0] - r_min[0] - w) * 0.5, r_min[1] + (r_max[1] - r_min[1] - h) * 0.5];
        dl.add_text(pos, cor, texto);
    });
}

/// Badge em contorno + preenchimento tênue, com o texto do Tier, na
/// posição `pos`, centralizado na altura `altura_linha`. Devolve a largura.
fn desenhar_badge(
    ui: &Ui,
    fonts: Option<&Fonts>,
    dl: &DrawListMut<'_>,
    tier: Tier,
    pos: [f32; 2],
    altura_linha: f32,
) -> [f32; 2] {
    let (contorno, fundo, cor_texto) = cores_badge(tier);
    let texto = texto_badge(tier);
    com_fonte(ui, fonts.map(|f| f.badge), || {
        let [w, h] = ui.calc_text_size(texto);
        let padding = [theme::ESPACO_2, 2.0];
        let tamanho = [w + padding[0] * 2.0, h + padding[1] * 2.0];
        let b_min = [pos[0], pos[1] + (altura_linha - tamanho[1]).max(0.0) * 0.5];
        let b_max = [b_min[0] + tamanho[0], b_min[1] + tamanho[1]];
        dl.add_rect(b_min, b_max, fundo).filled(true).rounding(theme::RAIO_SM).build();
        dl.add_rect(b_min, b_max, contorno).rounding(theme::RAIO_SM).build();
        dl.add_text([b_min[0] + padding[0], b_min[1] + padding[1]], cor_texto, texto);
        tamanho
    })
}

/// "custo" + valor em fonte mono, alinhados à direita de `x_direita`; sem
/// orçamento, a primeira linha diz quanto falta (vermelho + texto).
fn custo(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, oferta: &OfertaOlheiro, x_direita: f32, y_card: f32) {
    let valor = formatar_milhar(oferta.custo);
    let (rotulo, cor_rotulo) = match oferta.faltam {
        Some(faltam) => (format!("faltam {}", formatar_milhar(faltam)), theme::DANGER),
        None => ("custo".to_string(), theme::TEXT_SECONDARY),
    };
    let y_rotulo = y_card + theme::ESPACO_3;
    let altura_rotulo = com_fonte(ui, fonts.map(|f| f.meta), || {
        let [w, h] = ui.calc_text_size(&rotulo);
        dl.add_text([x_direita - w, y_rotulo], cor_rotulo, &rotulo);
        h
    });
    com_fonte(ui, fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body)), || {
        let w = ui.calc_text_size(&valor)[0];
        dl.add_text([x_direita - w, y_rotulo + altura_rotulo + theme::ESPACO_1], theme::TEXT_PRIMARY, &valor);
    });
}

/// Ponto + "Disponível" (verde) ou "Em Missão" (dourado): cor e texto.
fn status(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, em_missao: bool, x_direita: f32, y_card: f32) {
    let (texto, cor) = if em_missao { ("Em Missão", theme::WARNING) } else { ("Disponível", theme::FIELD_GREEN) };
    com_fonte(ui, fonts.map(|f| f.body), || {
        let [w, h] = ui.calc_text_size(texto);
        let y = y_card + (ALTURA_CARD - h) * 0.5;
        dl.add_text([x_direita - w, y], cor, texto);
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
