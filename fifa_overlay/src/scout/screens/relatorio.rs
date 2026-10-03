//! Tela do Relatório (Story 2.5, UX-DR14): tela satélite aberta a partir
//! da aba Missões ou Relatórios. Cabeçalho com a Missão, o Olheiro e o
//! badge de Qualidade; abaixo, a visão Tabular — uma linha por jogador:
//! nome → idade → posição → nação → clube → colunas numéricas em Consolas
//! alinhadas dígito a dígito, divisórias horizontais e nenhum card.
//!
//! Nada é inventado: valores aparecem como o Olheiro revelou — faixa
//! ("65–78") ou exato ("72") — e atributo não observado é "—". Texto longo
//! é cortado com "…" e o texto inteiro aparece no tooltip ao passar o
//! mouse ou focar a linha com o controle (NFR5).
//!
//! Épico 3: ativar uma linha (ou card) abre a Ficha do jogador (Story 3.1).
//! Uma Missão com Fit Posicional ganha a coluna/badge "Fit" logo depois da
//! posição (3.5); com Jogador de Referência, a coluna "Sim." de
//! similaridade (3.3). Os dois são calculados pelo que o Olheiro revelou e
//! levam "≈" abaixo da Qualidade Alta.

use imgui::{SelectableFlags, StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, TableRowFlags, Ui};

use super::componentes::{self, badge_fit, badge_qualidade, badge_tier, card_com_largura, desenhar_badge_texto, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::save_repo::nome_posicao;
use crate::scout::minifaces::Rosto;
use crate::scout::state::{
    Atributo, Densidade, FaixaAtributo, JogadorEncontrado, PosicaoAlvo, Qualidade, RelatorioNaLista, ScoutState,
};

const LARGURA_NOME: f32 = 210.0;
const LARGURA_NACAO: f32 = 120.0;
const LARGURA_CLUBE: f32 = 150.0;
const LARGURA_NUMERO: f32 = 62.0;
const LARGURA_ATRIBUTO: f32 = 58.0;
const LARGURA_FIT: f32 = 96.0;
const LARGURA_SIMILARIDADE: f32 = 70.0;
const ALTURA_LINHA: f32 = 30.0;
const LARGURA_CARD: f32 = 380.0;
const ALTURA_CARD: f32 = 128.0;
const LADO_ROSTO: f32 = 96.0;
const LARGURA_ALTERNADOR: f32 = 110.0;
/// Atributos mostrados em cada card (os primeiros que o Olheiro observou).
const ATRIBUTOS_NO_CARD: usize = 3;

pub const MSG_BAIXA: &str =
    "Relatório de Qualidade baixa: os valores aparecem em faixas largas e só alguns atributos foram observados.";
pub const MSG_SEM_JOGADORES: &str = "O Olheiro não encontrou nenhum jogador com esses filtros.";
pub const MSG_PARCIAL: &str = "Relatório parcial: mais jogadores aparecem conforme os dias de carreira passam.";
pub const MSG_PARCIAL_VAZIO: &str = "O Olheiro ainda não enviou nenhum nome. Volte em alguns dias de carreira.";
const NAO_OBSERVADO: &str = "—";

/// O que o jogador fez na tela neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Voltar,
    /// Abrir a Ficha deste jogador (Story 3.1).
    AbrirFicha(u32),
}

/// Similaridade e fit são "≈" abaixo da Qualidade Alta: vêm de faixas
/// largas e de poucos atributos observados (Story 3.3).
pub fn aproximado(qualidade: Qualidade) -> bool {
    qualidade < Qualidade::Alta
}

/// "87%" ou "≈87%"; sem valor, "—".
pub fn texto_percentual(valor: Option<u8>, aproximado: bool) -> String {
    match valor {
        Some(v) if aproximado => format!("≈{v}%"),
        Some(v) => format!("{v}%"),
        None => NAO_OBSERVADO.to_string(),
    }
}

/// "VOL ≈96%" (posição-alvo e força do fit).
pub fn texto_fit(alvo: PosicaoAlvo, forca: Option<u8>, aproximado: bool) -> String {
    format!("{} {}", alvo.sigla(), texto_percentual(forca, aproximado))
}

/// O que a Missão pediu de perfil (decide as colunas extras).
#[derive(Debug, Clone, PartialEq)]
pub struct PerfilPedido {
    pub alvo: Option<PosicaoAlvo>,
    /// Nome do Jogador de Referência.
    pub referencia: Option<String>,
    pub aproximado: bool,
}

impl PerfilPedido {
    pub fn de(item: &RelatorioNaLista) -> PerfilPedido {
        let filtros = item.missao.as_ref().map(|m| &m.filtros);
        PerfilPedido {
            alvo: filtros.and_then(|f| f.fit_posicional),
            referencia: filtros.and_then(|f| f.referencia.as_ref()).map(|r| r.nome.clone()),
            aproximado: aproximado(item.relatorio.qualidade),
        }
    }
}

/// "72" quando exato, "65–78" quando faixa.
pub fn formatar_faixa(faixa: FaixaAtributo) -> String {
    if faixa.min == faixa.max {
        faixa.min.to_string()
    } else {
        format!("{}–{}", faixa.min, faixa.max)
    }
}

/// Colunas de atributo da tabela: todo atributo revelado em pelo menos um
/// jogador, na ordem fixa de `Atributo::TODOS` (ritmo → drible → chute →
/// passe → defesa → físico → goleiro).
pub fn colunas_de_atributos(jogadores: &[JogadorEncontrado]) -> Vec<Atributo> {
    Atributo::TODOS
        .into_iter()
        .filter(|a| jogadores.iter().any(|j| j.atributo(*a).is_some()))
        .collect()
}

/// Jogadores do melhor para o pior pelo meio da faixa de Overall (empate:
/// Potencial, depois nome).
pub fn ordenar(jogadores: &[JogadorEncontrado]) -> Vec<&JogadorEncontrado> {
    let meio = |f: FaixaAtributo| u16::from(f.min) + u16::from(f.max);
    let mut lista: Vec<&JogadorEncontrado> = jogadores.iter().collect();
    lista.sort_by(|a, b| {
        meio(b.overall)
            .cmp(&meio(a.overall))
            .then(meio(b.potencial).cmp(&meio(a.potencial)))
            .then(a.nome.cmp(&b.nome))
    });
    lista
}

/// Corta `texto` para caber em `largura` (medida por `medir`), terminando
/// em "…". Devolve o texto e se cortou.
pub fn truncar(texto: &str, largura: f32, medir: impl Fn(&str) -> f32) -> (String, bool) {
    if medir(texto) <= largura {
        return (texto.to_string(), false);
    }
    let mut corte: String = texto.to_string();
    while !corte.is_empty() {
        corte.pop();
        let candidato = format!("{}…", corte.trim_end());
        if medir(&candidato) <= largura {
            return (candidato, true);
        }
    }
    ("…".to_string(), true)
}

/// Título da tela: "Relatório · Missão Jovens".
pub fn titulo(item: &RelatorioNaLista) -> String {
    match &item.missao {
        Some(m) => format!("Relatório · Missão {}", super::nova_missao::nome_tipo(m.tipo)),
        None => "Relatório".to_string(),
    }
}

/// Linha de detalhes sob o título.
pub fn detalhe(item: &RelatorioNaLista) -> String {
    let r = &item.relatorio;
    let mut partes = Vec::new();
    if let Some(o) = &item.olheiro {
        partes.push(format!("{} ({})", o.especializacao.nome(), o.tier.nome()));
    }
    if let Some(m) = &item.missao {
        partes.push(super::missoes::nome_modo(m.modo_busca).to_string());
        if let Some(a) = m.filtros.atributo_dominante {
            partes.push(format!("foco em {}", a.nome()));
        }
        if let Some(alvo) = m.filtros.fit_posicional {
            partes.push(format!("fit em {}", alvo.nome()));
        }
        if let Some(r) = &m.filtros.referencia {
            partes.push(format!("parecidos com {}", r.nome));
        }
    }
    if let Some(data) = r.gerado_em {
        partes.push(format!("gerado em {}", formatar_data(data)));
    }
    partes.push(if item.parcial {
        format!("parcial: {} de {} jogadores até agora", r.jogadores.len(), item.previstos)
    } else {
        super::aviso::texto_jogadores(r.jogadores.len())
    });
    partes.push(if r.precisao_mais_menos == 0 {
        "valores exatos".to_string()
    } else {
        format!("precisão de ±{}", r.precisao_mais_menos)
    });
    partes.join(" · ")
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some(item) = state.relatorio_aberto() else {
        return Acao::Voltar;
    };
    let mut acao = Acao::Nenhuma;
    let inicio = ui.cursor_pos();
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = Acao::Voltar;
    }
    // Arquivar daqui também (o Relatório já está aberto): volta à lista,
    // onde ele passa para "Arquivados" (Story 2.7).
    if ScoutState::pode_arquivar(&item) {
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        if componentes::botao(ui, fonts, "Arquivar", EstiloBotao::Secundario, true) && state.arquivar_relatorio(item.relatorio.id) {
            acao = Acao::Voltar;
        }
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    cabecalho(ui, fonts, &item);
    // Tabular / Cards sempre visível no topo, à direita (Story 2.6).
    let fim = ui.cursor_pos();
    let largura_total = LARGURA_ALTERNADOR * 2.0 + theme::ESPACO_1;
    ui.set_cursor_pos([inicio[0] + ui.content_region_avail()[0] - largura_total, inicio[1]]);
    alternador_densidade(ui, fonts, state);
    ui.set_cursor_pos(fim);
    ui.dummy([0.0, theme::ESPACO_2]);

    let r = &item.relatorio;
    if r.qualidade == Qualidade::Baixa {
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::WARNING, MSG_BAIXA));
    }
    if item.parcial {
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, MSG_PARCIAL));
    }
    if r.jogadores.is_empty() {
        let msg = if item.parcial { MSG_PARCIAL_VAZIO } else { MSG_SEM_JOGADORES };
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, msg));
        return acao;
    }
    let perfil = PerfilPedido::de(&item);
    let ficha = match state.densidade() {
        Densidade::Tabular => tabela(ui, fonts, &r.jogadores, &perfil),
        Densidade::Cards => cards(ui, fonts, state, &r.jogadores, &perfil),
    };
    match ficha {
        Some(player_id) if acao == Acao::Nenhuma => Acao::AbrirFicha(player_id),
        _ => acao,
    }
}

/// Botões Tabular / Cards; a escolha fica salva em `ui_prefs`.
pub fn alternador_densidade(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) {
    let atual = match state.densidade() {
        Densidade::Tabular => 0,
        Densidade::Cards => 1,
    };
    match componentes::alternador(ui, fonts, &["Tabular", "Cards"], atual, LARGURA_ALTERNADOR) {
        Some(0) => state.definir_densidade(Densidade::Tabular),
        Some(_) => state.definir_densidade(Densidade::Cards),
        None => {}
    }
}

/// Visão Cards: grade de cards com o rosto, nome, idade, posição, nação,
/// clube, Overall/Potencial e os primeiros atributos observados. Devolve o
/// jogador cujo card foi ativado (abre a Ficha).
fn cards(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState, jogadores: &[JogadorEncontrado], perfil: &PerfilPedido) -> Option<u32> {
    let mut ativado = None;
    ui.child_window("##cards_relatorio").size([0.0, 0.0]).border(false).flags(super::flags_conteudo()).build(|| {
        let disponivel = ui.content_region_avail()[0];
        let por_linha = (((disponivel + theme::ESPACO_3) / (LARGURA_CARD + theme::ESPACO_3)).floor() as usize).max(1);
        for (indice, j) in ordenar(jogadores).into_iter().enumerate() {
            if indice % por_linha != 0 {
                ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            }
            if card_jogador(ui, fonts, state, j, perfil) {
                ativado = Some(j.player_id);
            }
            if indice % por_linha == por_linha - 1 {
                ui.dummy([0.0, theme::ESPACO_1]);
            }
        }
    });
    ativado
}

/// Card de um jogador; `true` = ativado.
fn card_jogador(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState, j: &JogadorEncontrado, perfil: &PerfilPedido) -> bool {
    let c = card_com_largura(ui, &j.player_id.to_string(), LARGURA_CARD, ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let ativo = ui.is_item_hovered() || (ui.is_item_focused() && ui.io().nav_visible);
    // Rosto só para cards visíveis: a lista pode ser longa (carga preguiçosa).
    let visivel = ui.is_rect_visible(c.min, c.max);
    let dl = ui.get_window_draw_list();

    let r_min = [c.min[0] + theme::ESPACO_3, c.min[1] + (ALTURA_CARD - LADO_ROSTO) * 0.5];
    let r_max = [r_min[0] + LADO_ROSTO, r_min[1] + LADO_ROSTO];
    dl.add_rect(r_min, r_max, theme::BG_BASE).filled(true).rounding(theme::RAIO_MD).build();
    match if visivel { state.rosto(j.player_id) } else { Rosto::Carregando } {
        Rosto::Pronto(textura) => dl.add_image(textura, r_min, r_max).build(),
        Rosto::Ausente => silhueta(&dl, r_min, LADO_ROSTO),
        Rosto::Carregando => {}
    }

    let x = r_max[0] + theme::ESPACO_3;
    let largura_texto = c.max[0] - theme::ESPACO_3 - x;
    let mut y = c.min[1] + theme::ESPACO_3;
    let medir = |fonte: Option<imgui::FontId>| move |t: &str| com_fonte(ui, fonte, || ui.calc_text_size(t)[0]);

    let (nome, nome_cortado) = truncar(&j.nome, largura_texto, medir(fonts.map(|f| f.heading)));
    y += texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &nome)[1];

    let clube = if j.clube.is_empty() { "Sem clube" } else { j.clube.as_str() };
    // Fit Posicional: badge extra na linha da posição nativa (Story 3.5).
    let fit = perfil.alvo.map(|alvo| format!("FIT {}", texto_fit(alvo, j.fit, perfil.aproximado)));
    let largura_fit = fit.as_ref().map_or(0.0, |t| {
        com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(t)[0]) + theme::ESPACO_2 * 3.0
    });
    let linha2 = format!("{} anos · {} · {}", j.idade, nome_posicao(j.posicao), j.nacao);
    let (linha2, cortou2) = truncar(&linha2, largura_texto - largura_fit, medir(fonts.map(|f| f.meta)));
    let [w2, h2] = texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::TEXT_SECONDARY, &linha2);
    if let Some(texto) = &fit {
        desenhar_badge_texto(ui, fonts, &dl, &badge_fit(), texto, [x + w2 + theme::ESPACO_2, y], h2);
    }
    y += h2;
    let (clube_visivel, cortou3) = truncar(clube, largura_texto, medir(fonts.map(|f| f.meta)));
    y += texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::TEXT_SECONDARY, &clube_visivel)[1] + theme::ESPACO_1;

    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let ovr = format!("OVR {}", formatar_faixa(j.overall));
    let [w_ovr, h_ovr] = texto_em(ui, mono, &dl, [x, y], theme::TEXT_PRIMARY, &ovr);
    let x_pot = x + w_ovr + theme::ESPACO_3;
    let [w_pot, _] = texto_em(ui, mono, &dl, [x_pot, y], theme::FIELD_GREEN, &format!("POT {}", formatar_faixa(j.potencial)));
    if perfil.referencia.is_some() {
        let sim = format!("SIM {}", texto_percentual(j.similaridade, perfil.aproximado));
        texto_em(ui, mono, &dl, [x_pot + w_pot + theme::ESPACO_3, y], theme::ACCENT_PRIMARY, &sim);
    }
    y += h_ovr;
    let chave: Vec<String> =
        j.atributos.iter().take(ATRIBUTOS_NO_CARD).map(|a| format!("{} {}", a.atributo.sigla(), formatar_faixa(a.valor))).collect();
    if !chave.is_empty() {
        let (linha, _) = truncar(&chave.join(" · "), largura_texto, medir(mono));
        texto_em(ui, mono, &dl, [x, y], theme::TEXT_SECONDARY, &linha);
    }

    if ativo && (nome_cortado || cortou2 || cortou3) {
        ui.tooltip(|| {
            com_fonte(ui, fonts.map(|f| f.body), || ui.text(&j.nome));
            com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, format!("{} · {clube}", j.nacao)));
        });
    }
    if let (true, Some(nome)) = (ativo, &perfil.referencia) {
        ui.tooltip_text(format!("Similaridade com {nome}: {}", texto_percentual(j.similaridade, perfil.aproximado)));
    }
    c.ativou
}

/// Silhueta neutra (cabeça + ombros) para jogador sem rosto no jogo.
pub(super) fn silhueta(dl: &imgui::DrawListMut<'_>, min: [f32; 2], lado: f32) {
    let cor = theme::TEXT_DISABLED;
    let centro = [min[0] + lado * 0.5, min[1] + lado * 0.38];
    dl.add_circle(centro, lado * 0.18, cor).filled(true).build();
    dl.add_rect([min[0] + lado * 0.2, min[1] + lado * 0.62], [min[0] + lado * 0.8, min[1] + lado * 0.92], cor)
        .filled(true)
        .rounding(lado * 0.2)
        .build();
}

fn cabecalho(ui: &Ui, fonts: Option<&Fonts>, item: &RelatorioNaLista) {
    let inicio = ui.cursor_pos();
    let altura_titulo = com_fonte(ui, fonts.map(|f| f.heading), || {
        let titulo = titulo(item);
        ui.set_cursor_pos([inicio[0], inicio[1] + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text(&titulo);
        ui.text_line_height()
    });
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    componentes::badge_no_fluxo(ui, fonts, &badge_qualidade(item.relatorio.qualidade), altura_titulo);
    if let Some(o) = &item.olheiro {
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        componentes::badge_no_fluxo(ui, fonts, &badge_tier(o.tier), altura_titulo);
    }
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, detalhe(item)));
}

/// Colunas fixas da tabela, na ordem: as de perfil (Fit, Sim.) entram logo
/// depois da posição quando a Missão as pediu (Story 3.5).
pub fn colunas_fixas(perfil: &PerfilPedido) -> Vec<&'static str> {
    let mut colunas = vec!["Nome", "Idade", "Pos"];
    if perfil.alvo.is_some() {
        colunas.push("Fit");
    }
    if perfil.referencia.is_some() {
        colunas.push("Sim.");
    }
    colunas.extend(["Nação", "Clube", "OVR", "POT"]);
    colunas
}

/// Tabela com rolagem nos dois eixos; a coluna do nome e o cabeçalho
/// ficam fixos. Devolve o jogador cuja linha foi ativada (abre a Ficha).
fn tabela(ui: &Ui, fonts: Option<&Fonts>, jogadores: &[JogadorEncontrado], perfil: &PerfilPedido) -> Option<u32> {
    let atributos = colunas_de_atributos(jogadores);
    let fixas = colunas_fixas(perfil);
    let colunas = fixas.len() + atributos.len();
    let flags = TableFlags::SCROLL_X
        | TableFlags::SCROLL_Y
        | TableFlags::ROW_BG
        | TableFlags::BORDERS_INNER_H
        | TableFlags::SIZING_FIXED_FIT
        | TableFlags::NO_SAVED_SETTINGS
        | TableFlags::PAD_OUTER_X;
    let _c1 = ui.push_style_color(StyleColor::TableRowBg, theme::TRANSPARENTE);
    let _c2 = ui.push_style_color(StyleColor::TableRowBgAlt, theme::LINHA_ALTERNADA);
    let _c3 = ui.push_style_color(StyleColor::TableBorderLight, theme::BORDER_HAIRLINE_SUBTLE);
    let _c4 = ui.push_style_color(StyleColor::TableHeaderBg, theme::BG_PANEL_RAISED);
    let _tabela = ui.begin_table_with_sizing("##tabela_relatorio", colunas, flags, [0.0, 0.0], 0.0)?;
    let fixa = |nome: &str, largura: f32| {
        let mut setup = TableColumnSetup::new(nome.to_string());
        setup.flags = TableColumnFlags::WIDTH_FIXED | TableColumnFlags::NO_RESIZE;
        setup.init_width_or_weight = largura;
        setup
    };
    for &nome in &fixas {
        let largura = match nome {
            "Nome" => LARGURA_NOME,
            "Nação" => LARGURA_NACAO,
            "Clube" => LARGURA_CLUBE,
            "OVR" | "POT" => LARGURA_NUMERO + 10.0,
            "Fit" => LARGURA_FIT,
            "Sim." => LARGURA_SIMILARIDADE,
            _ => LARGURA_NUMERO,
        };
        ui.table_setup_column_with(fixa(nome, largura));
    }
    for a in &atributos {
        ui.table_setup_column_with(fixa(a.sigla(), LARGURA_ATRIBUTO));
    }
    ui.table_setup_scroll_freeze(1, 1);

    // Cabeçalho: siglas com o nome inteiro no tooltip.
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.table_next_row_with_flags(TableRowFlags::HEADERS);
        let explicar = |n: &str| match n {
            "Fit" => perfil.alvo.map(|a| format!("Fit Posicional: força do perfil dele como {}", a.nome())),
            "Sim." => perfil.referencia.as_ref().map(|r| format!("Similaridade com {r}")),
            _ => None,
        };
        let nomes: Vec<(String, Option<String>)> = fixas
            .iter()
            .map(|n| (n.to_string(), explicar(n)))
            .chain(atributos.iter().map(|a| (a.sigla().to_string(), Some(a.nome().to_string()))))
            .collect();
        for (indice, (rotulo, completo)) in nomes.iter().enumerate() {
            ui.table_set_column_index(indice);
            let _cor = ui.push_style_color(StyleColor::Text, theme::TEXT_SECONDARY);
            ui.table_header(rotulo);
            if let (Some(nome), true) = (completo, ui.is_item_hovered()) {
                ui.tooltip_text(nome);
            }
        }
    });

    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let mut ativado = None;
    for (indice, j) in ordenar(jogadores).into_iter().enumerate() {
        let _id = ui.push_id_usize(indice);
        ui.table_next_row_with_height(TableRowFlags::empty(), ALTURA_LINHA);

        // Coluna do nome: a linha inteira é um item focável (mouse e
        // controle), para o tooltip aparecer também com o foco.
        ui.table_set_column_index(0);
        let inicio = ui.cursor_pos();
        let linha_ativa = {
            let _c = ui.push_style_color(StyleColor::Header, theme::ACCENT_PRIMARY_DIM);
            let _h = ui.push_style_color(StyleColor::HeaderHovered, theme::ACCENT_PRIMARY_DIM);
            let ativou = ui
                .selectable_config("##linha")
                .flags(SelectableFlags::SPAN_ALL_COLUMNS | SelectableFlags::ALLOW_ITEM_OVERLAP)
                .size([0.0, ALTURA_LINHA - 4.0])
                .build();
            if ativou {
                ativado = Some(j.player_id);
            }
            ui.is_item_hovered() || (ui.is_item_focused() && ui.io().nav_visible)
        };
        let medir_body = |t: &str| com_fonte(ui, fonts.map(|f| f.body), || ui.calc_text_size(t)[0]);
        let (nome, nome_cortado) = truncar(&j.nome, LARGURA_NOME - theme::ESPACO_2, medir_body);
        ui.set_cursor_pos([inicio[0], inicio[1] + (ALTURA_LINHA - 4.0 - ui.text_line_height()) * 0.5]);
        com_fonte(ui, fonts.map(|f| f.body), || ui.text(&nome));

        celula_numero(ui, mono, 1, &j.idade.to_string(), theme::TEXT_PRIMARY);
        celula_texto(ui, fonts, 2, nome_posicao(j.posicao), LARGURA_NUMERO);
        let mut coluna = 3;
        if let Some(alvo) = perfil.alvo {
            celula_numero(ui, mono, coluna, &texto_fit(alvo, j.fit, perfil.aproximado), theme::ACCENT_PRIMARY);
            coluna += 1;
        }
        if perfil.referencia.is_some() {
            celula_numero(ui, mono, coluna, &texto_percentual(j.similaridade, perfil.aproximado), theme::ACCENT_PRIMARY);
            coluna += 1;
        }
        let nacao_cortada = celula_texto(ui, fonts, coluna, &j.nacao, LARGURA_NACAO);
        let clube = if j.clube.is_empty() { "Sem clube" } else { j.clube.as_str() };
        let clube_cortado = celula_texto(ui, fonts, coluna + 1, clube, LARGURA_CLUBE);
        celula_numero(ui, mono, coluna + 2, &formatar_faixa(j.overall), theme::TEXT_PRIMARY);
        celula_numero(ui, mono, coluna + 3, &formatar_faixa(j.potencial), theme::FIELD_GREEN);
        let primeira = coluna + 4;
        for (indice, a) in atributos.iter().enumerate() {
            match j.atributo(*a) {
                Some(valor) => celula_numero(ui, mono, primeira + indice, &formatar_faixa(valor), theme::TEXT_PRIMARY),
                None => celula_numero(ui, mono, primeira + indice, NAO_OBSERVADO, theme::TEXT_DISABLED),
            }
        }

        if linha_ativa && (nome_cortado || nacao_cortada || clube_cortado) {
            ui.tooltip(|| {
                com_fonte(ui, fonts.map(|f| f.body), || ui.text(&j.nome));
                com_fonte(ui, fonts.map(|f| f.meta), || {
                    ui.text_colored(theme::TEXT_SECONDARY, format!("{} · {}", j.nacao, clube));
                });
            });
        }
    }
    ativado
}

/// Texto numa célula, cortado com "…" se não couber. Devolve se cortou.
fn celula_texto(ui: &Ui, fonts: Option<&Fonts>, coluna: usize, texto: &str, largura: f32) -> bool {
    ui.table_set_column_index(coluna);
    com_fonte(ui, fonts.map(|f| f.body), || {
        let (visivel, cortou) = truncar(texto, largura - theme::ESPACO_2, |t| ui.calc_text_size(t)[0]);
        centralizar_na_linha(ui);
        ui.text_colored(theme::TEXT_SECONDARY, &visivel);
        cortou
    })
}

/// Número em Consolas, alinhado à direita (dígito a dígito entre linhas).
fn celula_numero(ui: &Ui, mono: Option<imgui::FontId>, coluna: usize, texto: &str, cor: [f32; 4]) {
    ui.table_set_column_index(coluna);
    com_fonte(ui, mono, || {
        let largura = ui.calc_text_size(texto)[0];
        let livre = ui.content_region_avail()[0];
        centralizar_na_linha(ui);
        let [x, y] = ui.cursor_pos();
        ui.set_cursor_pos([x + (livre - largura - theme::ESPACO_1).max(0.0), y]);
        ui.text_colored(cor, texto);
    });
}

/// Desce o cursor para o texto ficar no meio da linha de `ALTURA_LINHA`.
fn centralizar_na_linha(ui: &Ui) {
    let [x, y] = ui.cursor_pos();
    ui.set_cursor_pos([x, y + ((ALTURA_LINHA - 4.0 - ui.text_line_height()) * 0.5).max(0.0)]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::AtributoRevelado;

    fn faixa(min: u8, max: u8) -> FaixaAtributo {
        FaixaAtributo { min, max }
    }

    fn jogador(nome: &str, overall: (u8, u8), atributos: &[Atributo]) -> JogadorEncontrado {
        JogadorEncontrado {
            player_id: 1,
            nome: nome.to_string(),
            idade: 20,
            posicao: 24,
            nacao_id: 54,
            nacao: "Brazil".to_string(),
            clube: "Clube".to_string(),
            overall: faixa(overall.0, overall.1),
            potencial: faixa(80, 84),
            atributos: atributos.iter().map(|&a| AtributoRevelado { atributo: a, valor: faixa(70, 72) }).collect(),
            pe: None,
            similaridade: None,
            fit: None,
        }
    }

    #[test]
    fn values_show_ranges_or_exact_never_invented() {
        assert_eq!(formatar_faixa(faixa(72, 72)), "72");
        assert_eq!(formatar_faixa(faixa(65, 78)), "65–78");
        let j = jogador("A", (70, 74), &[Atributo::Finalizacao]);
        assert_eq!(j.atributo(Atributo::Drible), None, "não observado não vira número");
    }

    #[test]
    fn columns_are_the_union_of_revealed_attributes_in_fixed_order() {
        let a = jogador("A", (70, 74), &[Atributo::Finalizacao, Atributo::Velocidade]);
        let b = jogador("B", (70, 74), &[Atributo::Marcacao, Atributo::Finalizacao]);
        assert_eq!(
            colunas_de_atributos(&[a, b]),
            vec![Atributo::Velocidade, Atributo::Finalizacao, Atributo::Marcacao]
        );
    }

    #[test]
    fn rows_go_from_best_to_worst_overall() {
        let lista = [jogador("Baixo", (60, 64), &[]), jogador("Alto", (80, 84), &[]), jogador("Meio", (70, 74), &[])];
        let nomes: Vec<&str> = ordenar(&lista).iter().map(|j| j.nome.as_str()).collect();
        assert_eq!(nomes, ["Alto", "Meio", "Baixo"]);
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        let medir = |t: &str| t.chars().count() as f32 * 10.0;
        assert_eq!(truncar("Curto", 100.0, medir), ("Curto".to_string(), false));
        let (cortado, cortou) = truncar("Gonçalo Filipe de Oliveira", 100.0, medir);
        assert!(cortou);
        assert!(cortado.ends_with('…'));
        assert!(medir(&cortado) <= 100.0);
        assert_eq!(truncar("abc", 5.0, medir), ("…".to_string(), true));
    }

    #[test]
    fn profile_columns_follow_the_position_and_only_when_asked() {
        let nada = PerfilPedido { alvo: None, referencia: None, aproximado: false };
        assert_eq!(colunas_fixas(&nada), ["Nome", "Idade", "Pos", "Nação", "Clube", "OVR", "POT"]);
        let fit = PerfilPedido { alvo: Some(PosicaoAlvo::Volante), ..nada.clone() };
        assert_eq!(colunas_fixas(&fit), ["Nome", "Idade", "Pos", "Fit", "Nação", "Clube", "OVR", "POT"]);
        let os_dois = PerfilPedido { referencia: Some("Fulano".to_string()), ..fit };
        assert_eq!(colunas_fixas(&os_dois)[3..5], ["Fit", "Sim."]);
    }

    #[test]
    fn percentages_are_marked_approximate_below_high_quality() {
        assert!(aproximado(Qualidade::Baixa) && aproximado(Qualidade::Media) && !aproximado(Qualidade::Alta));
        assert_eq!(texto_percentual(Some(87), true), "≈87%");
        assert_eq!(texto_percentual(Some(87), false), "87%");
        assert_eq!(texto_percentual(None, true), "—");
        assert_eq!(texto_fit(PosicaoAlvo::Volante, Some(96), false), "VOL 96%");
    }

    #[test]
    fn low_quality_message_is_factual() {
        assert!(!MSG_BAIXA.contains('!'));
        assert!(!MSG_SEM_JOGADORES.contains('!'));
    }
}
