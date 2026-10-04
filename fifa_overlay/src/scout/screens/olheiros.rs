//! Aba Olheiros: os Olheiros contratados, em Cards ou Tabular (escolha
//! salva por carreira), e no fim da lista a entrada "Contratar Olheiro",
//! que abre as 12 combinações Especialização × Tier à venda (FR-2) numa
//! tela própria (`render_contratacao`).
//!
//! Ativar um Olheiro contratado (clique ou A) leva direto ao que ele pode
//! fazer (2026-10-03, pedido do Felipe): livre → Nova Missão com ele, já
//! com os filtros ideais da Especialização; em Missão → o Relatório dela
//! (ou a aba Missões, se o Relatório ainda não existe).
//!
//! Layout dos cards do `mockups/olheiros.html` (v2): avatar de sigla, nome
//! com badge de Tier em contorno, uma linha de detalhe e, à direita, o
//! status (ou custo e "Contratar", nas ofertas). Rótulos sem maiúsculas
//! (DESIGN.md: maiúsculas só nos badges) e valores sem símbolo de moeda.
//!
//! Cada card é UM item navegável (Story 1.6). Os dados vêm de
//! `scout::state` (AD-1); custos, de `scout::quality`.

use imgui::{DrawListMut, SelectableFlags, StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, TableRowFlags, Ui};
use uuid::Uuid;

use super::componentes::{self, badge_tier, desenhar_badge, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data, formatar_milhar};
use crate::scout::state::{Densidade, Especializacao, OfertaOlheiro, OlheiroContratado, ScoutState, Tier};

const ALTURA_CARD: f32 = 72.0;
const LADO_AVATAR: f32 = 44.0;
const LARGURA_BOTAO: f32 = 132.0;
const RAIO_DOT: f32 = 4.0;
const ALTURA_LINHA: f32 = 34.0;
const LARGURA_ALTERNADOR: f32 = 110.0;

pub const MSG_NENHUM_CONTRATADO: &str = "Nenhum Olheiro contratado ainda.";
pub const ROTULO_CONTRATAR: &str = "Contratar Olheiro";
const DETALHE_CONTRATAR: &str = "4 Especializações × 3 Tiers, com o custo de cada um.";

/// O que o jogador fez na aba neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    /// "Contratar Olheiro": abre a lista de ofertas.
    AbrirContratacao,
    /// Ativou um Olheiro contratado (ver `ScoutState::destino_do_olheiro`).
    Ativar(Uuid),
}

/// O que o jogador fez na lista de ofertas neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoContratacao {
    Nenhuma,
    Voltar,
    Contratar(Especializacao, Tier),
}

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

/// Microcopy do bloqueio por orçamento (UX-DR9/DR21: valor exato, sem
/// exclamação).
pub fn texto_faltam(faltam: i32) -> String {
    format!("Orçamento insuficiente: faltam {}.", formatar_milhar(faltam))
}

/// Status curto: "Disponível" / "Em Missão".
pub fn texto_status(contratado: &OlheiroContratado) -> &'static str {
    if contratado.em_missao {
        "Em Missão"
    } else {
        "Disponível"
    }
}

/// O que o Olheiro está fazendo: "Missão Jovens · pronta em 12/08/2026",
/// "Missão contínua Tática · bloco até 01/09/2026", ou, livre, o convite.
pub fn texto_missao(contratado: &OlheiroContratado) -> String {
    match &contratado.missao {
        Some(m) if m.continua => {
            format!("Missão contínua {} · bloco até {}", super::nova_missao::nome_tipo(m.tipo), formatar_data(m.prazo_estimado))
        }
        Some(m) => format!("Missão {} · pronta em {}", super::nova_missao::nome_tipo(m.tipo), formatar_data(m.prazo_estimado)),
        None => "Livre: ative para encomendar uma Missão com ele.".to_string(),
    }
}

/// "1 Relatório" / "3 Relatórios" / "nenhum Relatório".
pub fn texto_relatorios(n: usize) -> String {
    match n {
        0 => "nenhum Relatório".to_string(),
        1 => "1 Relatório".to_string(),
        n => format!("{n} Relatórios"),
    }
}

/// Desenha a aba Olheiros.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let densidade = state.densidade_olheiros();
    let atual = usize::from(densidade == Densidade::Tabular);
    match componentes::alternador(ui, fonts, &["Cards", "Tabular"], atual, LARGURA_ALTERNADOR) {
        Some(0) => state.definir_densidade_olheiros(Densidade::Cards),
        Some(_) => state.definir_densidade_olheiros(Densidade::Tabular),
        None => {}
    }
    ui.dummy([0.0, theme::ESPACO_2]);
    let contratados = state.olheiros_contratados();
    if contratados.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_NENHUM_CONTRATADO));
        ui.dummy([0.0, theme::ESPACO_1]);
    }
    match densidade {
        Densidade::Cards => cards(ui, fonts, &contratados),
        Densidade::Tabular => tabela(ui, fonts, &contratados),
    }
}

fn cards(ui: &Ui, fonts: Option<&Fonts>, contratados: &[OlheiroContratado]) -> Acao {
    let mut acao = Acao::Nenhuma;
    for (indice, c) in contratados.iter().enumerate() {
        let lado = Lado::Status { em_missao: c.em_missao, relatorios: c.relatorios };
        let revelar = if indice == 0 { Revelar::Topo } else { Revelar::Nada };
        let detalhe = texto_missao(c);
        if card(ui, fonts, &c.olheiro.id.to_string(), Some((c.olheiro.especializacao, c.olheiro.tier)), &detalhe, &lado, revelar) {
            acao = Acao::Ativar(c.olheiro.id);
        }
    }
    let revelar = if contratados.is_empty() { Revelar::Topo } else { Revelar::Nada };
    if card(ui, fonts, "contratar", None, DETALHE_CONTRATAR, &Lado::Novo, revelar) {
        acao = Acao::AbrirContratacao;
    }
    acao
}

/// Visão Tabular: uma linha por Olheiro e, por último, "Contratar Olheiro".
fn tabela(ui: &Ui, fonts: Option<&Fonts>, contratados: &[OlheiroContratado]) -> Acao {
    let mut acao = Acao::Nenhuma;
    let flags = TableFlags::ROW_BG | TableFlags::BORDERS_INNER_H | TableFlags::SIZING_FIXED_FIT | TableFlags::NO_SAVED_SETTINGS;
    let _c1 = ui.push_style_color(StyleColor::TableRowBg, theme::TRANSPARENTE);
    let _c2 = ui.push_style_color(StyleColor::TableRowBgAlt, theme::LINHA_ALTERNADA);
    let _c3 = ui.push_style_color(StyleColor::TableBorderLight, theme::BORDER_HAIRLINE_SUBTLE);
    let _c4 = ui.push_style_color(StyleColor::TableHeaderBg, theme::BG_PANEL_RAISED);
    let Some(_tabela) = ui.begin_table_with_flags("##tabela_olheiros", 5, flags) else {
        return acao;
    };
    let coluna = |nome: &str, largura: f32| {
        let mut setup = TableColumnSetup::new(nome.to_string());
        setup.flags = if largura > 0.0 { TableColumnFlags::WIDTH_FIXED } else { TableColumnFlags::WIDTH_STRETCH };
        setup.init_width_or_weight = if largura > 0.0 { largura } else { 1.0 };
        setup
    };
    ui.table_setup_column_with(coluna("Olheiro", 230.0));
    ui.table_setup_column_with(coluna("Tier", 90.0));
    ui.table_setup_column_with(coluna("Status", 120.0));
    ui.table_setup_column_with(coluna("Missão", 0.0));
    ui.table_setup_column_with(coluna("Relatórios", 120.0));
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.table_next_row_with_flags(TableRowFlags::HEADERS);
        for (i, nome) in ["Olheiro", "Tier", "Status", "Missão", "Relatórios"].into_iter().enumerate() {
            ui.table_set_column_index(i);
            let _cor = ui.push_style_color(StyleColor::Text, theme::TEXT_SECONDARY);
            ui.table_header(nome);
        }
    });
    for c in contratados {
        let _id = ui.push_id(c.olheiro.id.to_string());
        ui.table_next_row_with_height(TableRowFlags::empty(), ALTURA_LINHA);
        if linha_selecionavel(ui) {
            acao = Acao::Ativar(c.olheiro.id);
        }
        texto_na_celula(ui, fonts.map(|f| f.body), c.olheiro.especializacao.nome(), theme::TEXT_PRIMARY);
        ui.table_set_column_index(1);
        let pos = ui.cursor_screen_pos();
        desenhar_badge(ui, fonts, &ui.get_window_draw_list(), &badge_tier(c.olheiro.tier), [pos[0], pos[1] + 4.0], ALTURA_LINHA - 8.0);
        ui.table_set_column_index(2);
        let cor = if c.em_missao { theme::WARNING } else { theme::FIELD_GREEN };
        texto_na_celula(ui, fonts.map(|f| f.body), texto_status(c), cor);
        ui.table_set_column_index(3);
        texto_na_celula(ui, fonts.map(|f| f.meta), &texto_missao(c), theme::TEXT_SECONDARY);
        ui.table_set_column_index(4);
        texto_na_celula(ui, fonts.map(|f| f.meta), &texto_relatorios(c.relatorios), theme::TEXT_SECONDARY);
    }
    ui.table_next_row_with_height(TableRowFlags::empty(), ALTURA_LINHA);
    if linha_selecionavel(ui) {
        acao = Acao::AbrirContratacao;
    }
    texto_na_celula(ui, fonts.map(|f| f.heading), &format!("+ {ROTULO_CONTRATAR}"), theme::FIELD_GREEN);
    ui.table_set_column_index(3);
    texto_na_celula(ui, fonts.map(|f| f.meta), DETALHE_CONTRATAR, theme::TEXT_SECONDARY);
    acao
}

/// Linha inteira selecionável (mouse e controle), a partir da coluna 0;
/// deixa o cursor na coluna 0 para o texto.
fn linha_selecionavel(ui: &Ui) -> bool {
    ui.table_set_column_index(0);
    let inicio = ui.cursor_pos();
    let _c = ui.push_style_color(StyleColor::Header, theme::ACCENT_PRIMARY_DIM);
    let _h = ui.push_style_color(StyleColor::HeaderHovered, theme::ACCENT_PRIMARY_DIM);
    let ativou = ui
        .selectable_config("##linha")
        .flags(SelectableFlags::SPAN_ALL_COLUMNS | SelectableFlags::ALLOW_ITEM_OVERLAP)
        .size([0.0, ALTURA_LINHA - 4.0])
        .build();
    ui.set_cursor_pos(inicio);
    ativou
}

fn texto_na_celula(ui: &Ui, fonte: Option<imgui::FontId>, texto: &str, cor: [f32; 4]) {
    com_fonte(ui, fonte, || {
        let [x, y] = ui.cursor_pos();
        ui.set_cursor_pos([x, y + ((ALTURA_LINHA - 4.0 - ui.text_line_height()) * 0.5).max(0.0)]);
        ui.text_colored(cor, texto);
    });
}

/// Tela "Contratar Olheiro": as 12 ofertas, cada uma com o custo; ativar
/// uma (com orçamento) abre a Confirmação de Contratação (Story 1.5).
pub fn render_contratacao(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState) -> AcaoContratacao {
    let mut acao = AcaoContratacao::Nenhuma;
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = AcaoContratacao::Voltar;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text(ROTULO_CONTRATAR));
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.text_colored(theme::TEXT_SECONDARY, "O custo sai do orçamento de transferências, só depois da confirmação.")
    });
    ui.dummy([0.0, theme::ESPACO_2]);
    for (indice, oferta) in state.olheiros_disponiveis().into_iter().enumerate() {
        let chave = format!("oferta_{:?}_{:?}", oferta.especializacao, oferta.tier);
        let revelar = if indice == 0 { Revelar::Topo } else { Revelar::Nada };
        let especializacao = Some((oferta.especializacao, oferta.tier));
        if card(ui, fonts, &chave, especializacao, descricao(oferta.especializacao), &Lado::Contratar(oferta), revelar) {
            acao = AcaoContratacao::Contratar(oferta.especializacao, oferta.tier);
        }
    }
    acao
}

/// Nome da Especialização com o badge do Tier ao lado (também usado na
/// Confirmação de Contratação).
pub fn nome_com_badge(ui: &Ui, fonts: Option<&Fonts>, especializacao: Especializacao, tier: Tier) {
    let altura_nome = com_fonte(ui, fonts.map(|f| f.heading), || {
        ui.text(especializacao.nome());
        ui.calc_text_size(especializacao.nome())[1]
    });
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    super::componentes::badge_no_fluxo(ui, fonts, &badge_tier(tier), altura_nome);
}

/// O que vai no lado direito do card.
enum Lado {
    Contratar(OfertaOlheiro),
    Status { em_missao: bool, relatorios: usize },
    /// A entrada "Contratar Olheiro" no fim da lista.
    Novo,
}

/// O que mostrar ACIMA do card quando ele recebe o foco do controle.
#[derive(Clone, Copy, PartialEq)]
enum Revelar {
    /// Só o card (o ImGui já rola para ele).
    Nada,
    /// Primeiro card da tela: rola até o topo (título e estado vazio).
    Topo,
}

/// Desenha um card como UM item navegável (o card inteiro): mouse e
/// controle focam o card, e o ImGui rola para mostrá-lo por completo.
/// `olheiro`: Especialização e Tier (`None` na entrada "Contratar
/// Olheiro"). Devolve `true` se o card foi ativado (clique ou A) — numa
/// oferta, só com orçamento suficiente. Tudo dentro do card é desenhado
/// pelo draw list (nenhum outro item), para não haver dois alvos de foco
/// sobrepostos.
fn card(
    ui: &Ui,
    fonts: Option<&Fonts>,
    chave: &str,
    olheiro: Option<(Especializacao, Tier)>,
    detalhe: &str,
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
        revelar_acima(ui, revelar);
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

    // Avatar com a sigla (ou "+").
    let a_min = [min[0] + theme::ESPACO_4, min[1] + (ALTURA_CARD - LADO_AVATAR) * 0.5];
    let a_max = [a_min[0] + LADO_AVATAR, a_min[1] + LADO_AVATAR];
    dl.add_rect(a_min, a_max, [1.0, 1.0, 1.0, 0.05]).filled(true).rounding(theme::RAIO_MD).build();
    dl.add_rect(a_min, a_max, theme::BORDER_HAIRLINE).rounding(theme::RAIO_MD).build();
    let (avatar, cor_avatar) = match olheiro {
        Some((e, _)) => (sigla(e), theme::ACCENT_PRIMARY),
        None => ("+", theme::FIELD_GREEN),
    };
    texto_centralizado(ui, fonts.map(|f| f.heading), &dl, avatar, a_min, a_max, cor_avatar);

    // Nome + badge; detalhe embaixo.
    let x_texto = a_max[0] + theme::ESPACO_3;
    let y_nome = min[1] + theme::ESPACO_3;
    let nome = olheiro.map_or(ROTULO_CONTRATAR, |(e, _)| e.nome());
    let [largura_nome, altura_nome] = com_fonte(ui, fonts.map(|f| f.heading), || {
        dl.add_text([x_texto, y_nome], theme::TEXT_PRIMARY, nome);
        ui.calc_text_size(nome)
    });
    if let Some((_, tier)) = olheiro {
        desenhar_badge(ui, fonts, &dl, &badge_tier(tier), [x_texto + largura_nome + theme::ESPACO_2, y_nome], altura_nome);
    }
    com_fonte(ui, fonts.map(|f| f.meta), || {
        let y = y_nome + altura_nome + theme::ESPACO_1;
        dl.add_text([x_texto, y], theme::TEXT_SECONDARY, detalhe);
    });

    // Lado direito.
    let direita = max[0] - theme::ESPACO_4;
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
            if let (Some(faltam), true) = (oferta.faltam, hover) {
                ui.tooltip_text(texto_faltam(faltam));
            }
            ativou && habilitado
        }
        Lado::Status { em_missao, relatorios } => {
            status(ui, fonts, &dl, *em_missao, *relatorios, direita, min[1]);
            ativou
        }
        Lado::Novo => ativou,
    }
}

/// Com o foco do controle no primeiro card, rola até o topo (o alternador
/// e o estado vazio, que ficam acima, não são o card). O ImGui sozinho só
/// garante o próprio card visível.
fn revelar_acima(ui: &Ui, revelar: Revelar) {
    if revelar == Revelar::Topo && ui.scroll_y() > 0.0 {
        ui.set_scroll_y(0.0);
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

/// Ponto + "Disponível" (verde) ou "Em Missão" (dourado): cor e texto; e
/// quantos Relatórios ele já entregou, embaixo.
fn status(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, em_missao: bool, relatorios: usize, x_direita: f32, y_card: f32) {
    let (texto, cor) = if em_missao { ("Em Missão", theme::WARNING) } else { ("Disponível", theme::FIELD_GREEN) };
    let y = y_card + theme::ESPACO_3;
    let h = com_fonte(ui, fonts.map(|f| f.body), || {
        let [w, h] = ui.calc_text_size(texto);
        dl.add_text([x_direita - w, y], cor, texto);
        let centro = [x_direita - w - theme::ESPACO_2 - RAIO_DOT, y + h * 0.5];
        dl.add_circle(centro, RAIO_DOT, cor).filled(true).build();
        h
    });
    com_fonte(ui, fonts.map(|f| f.meta), || {
        let texto = texto_relatorios(relatorios);
        let w = ui.calc_text_size(&texto)[0];
        dl.add_text([x_direita - w, y + h + theme::ESPACO_1], theme::TEXT_SECONDARY, &texto);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{Missao, Olheiro, StatusMissao};

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
        assert_eq!(texto_relatorios(0), "nenhum Relatório");
        assert_eq!(texto_relatorios(1), "1 Relatório");
        assert_eq!(texto_relatorios(3), "3 Relatórios");
    }

    #[test]
    fn a_hired_olheiro_says_what_he_is_doing() {
        let olheiro = Olheiro { id: Uuid::new_v4(), especializacao: Especializacao::Tatico, tier: Tier::Elite };
        let livre = OlheiroContratado { olheiro: olheiro.clone(), em_missao: false, missao: None, relatorio_atual: None, relatorios: 2 };
        assert_eq!(texto_status(&livre), "Disponível");
        assert!(texto_missao(&livre).starts_with("Livre"));
        let missao = Missao::de_teste(olheiro.id, StatusMissao::Pendente);
        let ocupado = OlheiroContratado { em_missao: true, missao: Some(missao), ..livre };
        assert_eq!(texto_status(&ocupado), "Em Missão");
        assert_eq!(texto_missao(&ocupado), "Missão Geral · pronta em 15/07/2026");
    }
}
