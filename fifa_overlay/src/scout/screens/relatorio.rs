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

use imgui::{SelectableFlags, StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, TableRowFlags, Ui};

use super::componentes::{self, badge_qualidade, badge_tier, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::save_repo::nome_posicao;
use crate::scout::state::{Atributo, FaixaAtributo, JogadorEncontrado, Qualidade, RelatorioNaLista, ScoutState};

const LARGURA_NOME: f32 = 210.0;
const LARGURA_NACAO: f32 = 120.0;
const LARGURA_CLUBE: f32 = 150.0;
const LARGURA_NUMERO: f32 = 62.0;
const LARGURA_ATRIBUTO: f32 = 58.0;
const ALTURA_LINHA: f32 = 30.0;

pub const MSG_BAIXA: &str =
    "Relatório de Qualidade baixa: os valores aparecem em faixas largas e só alguns atributos foram observados.";
pub const MSG_SEM_JOGADORES: &str = "O Olheiro não encontrou nenhum jogador com esses filtros.";
const NAO_OBSERVADO: &str = "—";

/// O que o jogador fez na tela neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Voltar,
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
    }
    if let Some(data) = r.gerado_em {
        partes.push(format!("gerado em {}", formatar_data(data)));
    }
    partes.push(super::aviso::texto_jogadores(r.jogadores.len()));
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
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = Acao::Voltar;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    cabecalho(ui, fonts, &item);
    ui.dummy([0.0, theme::ESPACO_2]);

    let r = &item.relatorio;
    if r.qualidade == Qualidade::Baixa {
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::WARNING, MSG_BAIXA));
    }
    if r.jogadores.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_JOGADORES));
        return acao;
    }
    tabela(ui, fonts, &r.jogadores);
    acao
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

/// Tabela com rolagem nos dois eixos; a coluna do nome e o cabeçalho
/// ficam fixos.
fn tabela(ui: &Ui, fonts: Option<&Fonts>, jogadores: &[JogadorEncontrado]) {
    let atributos = colunas_de_atributos(jogadores);
    let colunas = 7 + atributos.len();
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
    let Some(_tabela) = ui.begin_table_with_sizing("##tabela_relatorio", colunas, flags, [0.0, 0.0], 0.0) else {
        return;
    };
    let fixa = |nome: &str, largura: f32| {
        let mut setup = TableColumnSetup::new(nome.to_string());
        setup.flags = TableColumnFlags::WIDTH_FIXED | TableColumnFlags::NO_RESIZE;
        setup.init_width_or_weight = largura;
        setup
    };
    ui.table_setup_column_with(fixa("Nome", LARGURA_NOME));
    ui.table_setup_column_with(fixa("Idade", LARGURA_NUMERO));
    ui.table_setup_column_with(fixa("Pos", LARGURA_NUMERO));
    ui.table_setup_column_with(fixa("Nação", LARGURA_NACAO));
    ui.table_setup_column_with(fixa("Clube", LARGURA_CLUBE));
    ui.table_setup_column_with(fixa("OVR", LARGURA_NUMERO + 10.0));
    ui.table_setup_column_with(fixa("POT", LARGURA_NUMERO + 10.0));
    for a in &atributos {
        ui.table_setup_column_with(fixa(a.sigla(), LARGURA_ATRIBUTO));
    }
    ui.table_setup_scroll_freeze(1, 1);

    // Cabeçalho: siglas com o nome inteiro no tooltip.
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.table_next_row_with_flags(TableRowFlags::HEADERS);
        let nomes: Vec<(String, Option<&str>)> = ["Nome", "Idade", "Pos", "Nação", "Clube", "OVR", "POT"]
            .into_iter()
            .map(|n| (n.to_string(), None))
            .chain(atributos.iter().map(|a| (a.sigla().to_string(), Some(a.nome()))))
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
            ui.selectable_config("##linha")
                .flags(SelectableFlags::SPAN_ALL_COLUMNS | SelectableFlags::ALLOW_ITEM_OVERLAP)
                .size([0.0, ALTURA_LINHA - 4.0])
                .build();
            ui.is_item_hovered() || (ui.is_item_focused() && ui.io().nav_visible)
        };
        let medir_body = |t: &str| com_fonte(ui, fonts.map(|f| f.body), || ui.calc_text_size(t)[0]);
        let (nome, nome_cortado) = truncar(&j.nome, LARGURA_NOME - theme::ESPACO_2, medir_body);
        ui.set_cursor_pos([inicio[0], inicio[1] + (ALTURA_LINHA - 4.0 - ui.text_line_height()) * 0.5]);
        com_fonte(ui, fonts.map(|f| f.body), || ui.text(&nome));

        celula_numero(ui, mono, 1, &j.idade.to_string(), theme::TEXT_PRIMARY);
        celula_texto(ui, fonts, 2, nome_posicao(j.posicao), LARGURA_NUMERO);
        let nacao_cortada = celula_texto(ui, fonts, 3, &j.nacao, LARGURA_NACAO);
        let clube = if j.clube.is_empty() { "Sem clube" } else { j.clube.as_str() };
        let clube_cortado = celula_texto(ui, fonts, 4, clube, LARGURA_CLUBE);
        celula_numero(ui, mono, 5, &formatar_faixa(j.overall), theme::TEXT_PRIMARY);
        celula_numero(ui, mono, 6, &formatar_faixa(j.potencial), theme::FIELD_GREEN);
        for (coluna, a) in atributos.iter().enumerate() {
            match j.atributo(*a) {
                Some(valor) => celula_numero(ui, mono, 7 + coluna, &formatar_faixa(valor), theme::TEXT_PRIMARY),
                None => celula_numero(ui, mono, 7 + coluna, NAO_OBSERVADO, theme::TEXT_DISABLED),
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
    fn low_quality_message_is_factual() {
        assert!(!MSG_BAIXA.contains('!'));
        assert!(!MSG_SEM_JOGADORES.contains('!'));
    }
}
