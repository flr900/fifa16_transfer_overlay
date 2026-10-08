//! Aba Olheiros: os Olheiros contratados, em Cards ou Tabular (escolha
//! salva por carreira), e no fim da lista a entrada "Contratar Olheiro",
//! que abre o mercado de Olheiros da semana numa tela própria
//! (`render_contratacao`).
//!
//! Ativar um Olheiro contratado (clique ou A) leva direto ao que ele pode
//! fazer (2026-10-03, pedido do Felipe): livre → Nova Missão com ele, já
//! com os filtros ideais do foco dele; em Missão → o Relatório dela (ou a
//! aba Missões, se o Relatório ainda não existe); acompanhando os
//! Escolhidos → a aba Escolhidos.
//!
//! Épico 5 (2026-10-04): cada Olheiro tem nome, nação, mercados e estrelas
//! em cinco atributos; o mercado da semana (com filtro por continente e bandeiras) muda com a atratividade do clube; "Demitir" tira um Olheiro da lista.
//! Layout dos cards do `mockups/olheiros.html` (v2), com uma linha a mais
//! para as estrelas. Rótulos sem maiúsculas (DESIGN.md: maiúsculas só nos
//! badges) e valores sem símbolo de moeda.
//!
//! Cada card é UM item navegável (Story 1.6). Os dados vêm de
//! `scout::state` (AD-1); custos e estrelas, de `scout::quality`.

use imgui::{DrawListMut, SelectableFlags, StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, TableRowFlags, Ui};
use uuid::Uuid;

use super::componentes::{self, badge_tier, desenhar_badge, rotulo_com_estrelas, EstiloBotao};
use super::olheiro_card::{self, Rodape};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data, formatar_milhar};
use crate::save_repo::Confederacao;
use crate::scout::minifaces::Rosto;
use crate::scout::quality::{self, Mercado};
use crate::scout::state::{Carga, Densidade, Especializacao, OfertaOlheiro, Olheiro, OlheiroContratado, ScoutState, StatusMissao, Tier};

/// Largura de cada botão do filtro de continente.
const LARGURA_FILTRO: f32 = 150.0;
const ALTURA_LINHA: f32 = 34.0;
const LARGURA_ALTERNADOR: f32 = 110.0;

pub const MSG_NENHUM_CONTRATADO: &str = "Nenhum Olheiro contratado ainda.";
pub const ROTULO_CONTRATAR: &str = "Contratar Olheiro";
const DETALHE_CONTRATAR: &str = "O mercado da semana: cada Olheiro com nome, nação, mercados e estrelas.";
pub const MSG_LENDO_MERCADO: &str = "Lendo o clube e o mercado de Olheiros…";
pub const MSG_FILTRO_VAZIO: &str = "Nenhum Olheiro com mercado nesse continente esta semana. O mercado renova toda semana.";
pub const MSG_MERCADO_VAZIO: &str = "Você já contratou todos os Olheiros desta semana. O mercado renova na semana que vem.";

/// O que o jogador fez na aba neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    /// "Contratar Olheiro": abre o mercado.
    AbrirContratacao,
    /// Ativou um Olheiro contratado (ver `ScoutState::destino_do_olheiro`).
    Ativar(Uuid),
    /// Y (ou o botão direito) num Olheiro contratado: as Opções dele
    /// (cancelar a pesquisa, ajustar o perfil, mudar de região, demitir).
    Opcoes(Uuid),
}

/// O que o jogador fez no mercado neste frame.
#[derive(Debug, Clone, PartialEq)]
pub enum AcaoContratacao {
    Nenhuma,
    Voltar,
    Contratar(OfertaOlheiro),
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

/// Uma linha sobre o que cada foco faz bem (textos do mockup).
pub fn descricao(especializacao: Especializacao) -> &'static str {
    match especializacao {
        Especializacao::CacadorDeJovens => "Foco em potencial, idade baixa.",
        Especializacao::CacadorDeMedalhoes => "Foco em jogadores consagrados, prontos para jogar já.",
        Especializacao::Tatico => "Especialista em fit de atributos e posição.",
        Especializacao::Generalista => "De tudo um pouco: Missões gerais e a Lista de Escolhidos.",
    }
}

/// Sigla curta de cada atributo na linha de estrelas.
pub fn sigla_atributo(especializacao: Especializacao) -> &'static str {
    match especializacao {
        Especializacao::CacadorDeJovens => "JOV",
        Especializacao::CacadorDeMedalhoes => "MED",
        Especializacao::Tatico => "TÁT",
        Especializacao::Generalista => "GEN",
    }
}

/// Microcopy do bloqueio por orçamento (UX-DR9/DR21: valor exato, sem
/// exclamação).
pub fn texto_faltam(faltam: i32) -> String {
    format!("Orçamento insuficiente: faltam {}.", formatar_milhar(faltam))
}

/// Status curto: "Disponível" / "Em Missão" / "Acompanhando".
pub fn texto_status(contratado: &OlheiroContratado) -> &'static str {
    if contratado.em_missao {
        "Em Missão"
    } else if contratado.acompanhando {
        "Acompanhando"
    } else {
        "Disponível"
    }
}

/// O que o Olheiro está fazendo: "Missão Jovens · pronta em 12/08/2026",
/// "Missão contínua Tática · bloco até 01/09/2026", a Lista de Escolhidos
/// ou, livre, o convite.
pub fn texto_missao(contratado: &OlheiroContratado) -> String {
    match &contratado.missao {
        Some(m) if m.continua && m.tem_contrato() => {
            format!("Missão contínua {} · contrato até {}", super::nova_missao::nome_tipo(m.tipo), formatar_data(m.fim_do_contrato()))
        }
        Some(m) if m.continua => {
            format!("Missão contínua {} · bloco até {}", super::nova_missao::nome_tipo(m.tipo), formatar_data(m.prazo_estimado))
        }
        Some(m) => format!("Missão {} · pronta em {}", super::nova_missao::nome_tipo(m.tipo), formatar_data(m.prazo_estimado)),
        None if contratado.acompanhando => format!(
            "Acompanhando a Lista de Escolhidos: mantém até {} jogadores atualizados.",
            contratado.olheiro.capacidade_acompanhamento()
        ),
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

/// Nome de um mercado ("Brasil", "América do Sul"); `nacao` resolve o nome
/// de um país pelo id.
pub fn nome_mercado(mercado: Mercado, nacao: &dyn Fn(u16) -> Option<String>) -> String {
    match mercado {
        Mercado::Pais { id, .. } => nacao(id).unwrap_or_else(|| format!("país {id}")),
        Mercado::Continente(c) => c.nome().to_string(),
    }
}

/// Nome e bandeira das nações, como as telas de Olheiro os pedem ao estado.
pub struct Nacoes<'a> {
    pub nome: &'a dyn Fn(u16) -> Option<String>,
    pub bandeira: &'a dyn Fn(u16) -> Rosto,
}

/// Monta o `Nacoes` do estado e roda `f` com ele.
pub fn com_nacoes<R>(state: &ScoutState, f: impl FnOnce(&Nacoes<'_>) -> R) -> R {
    let nome = |id: u16| state.nome_da_nacao(id);
    let bandeira = |id: u16| state.bandeira(id);
    f(&Nacoes { nome: &nome, bandeira: &bandeira })
}

/// Um item da linha de origem: uma bandeira (opcional) e um texto.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub bandeira: Option<Rosto>,
    pub texto: String,
    pub cor: [f32; 4],
}

/// A origem do Olheiro em itens: a nação (com bandeira) e os mercados dele
/// (país com bandeira; continente em roxo). `com_nacao`: começar pela nação.
pub fn chips_do_olheiro(olheiro: &Olheiro, nacoes: &Nacoes<'_>, com_nacao: bool) -> Vec<Chip> {
    let mut chips = Vec::new();
    if let (true, Some(n)) = (com_nacao, &olheiro.nacao) {
        chips.push(Chip { bandeira: Some((nacoes.bandeira)(n.id)), texto: n.nome.clone(), cor: theme::TEXT_PRIMARY });
    }
    if olheiro.mercados.is_empty() {
        chips.push(Chip { bandeira: None, texto: "conhece todos os mercados".to_string(), cor: theme::TEXT_SECONDARY });
        return chips;
    }
    chips.push(Chip { bandeira: None, texto: "Mercados:".to_string(), cor: theme::TEXT_SECONDARY });
    for &m in &olheiro.mercados {
        let chip = match m {
            Mercado::Pais { id, .. } => Chip {
                bandeira: Some((nacoes.bandeira)(id)),
                texto: nome_mercado(m, nacoes.nome),
                cor: theme::TEXT_PRIMARY,
            },
            Mercado::Continente(c) => Chip { bandeira: None, texto: format!("{} (todo o continente)", c.nome()), cor: theme::ACCENT_PRIMARY },
        };
        chips.push(chip);
    }
    chips
}

/// Desenha os itens a partir de `pos`, quebrando a linha em `largura_max`
/// (no máximo `max_linhas`; o que não cabe some). Devolve [largura usada,
/// altura usada].
pub fn desenhar_chips(
    ui: &Ui,
    fonts: Option<&Fonts>,
    dl: &DrawListMut<'_>,
    pos: [f32; 2],
    largura_max: f32,
    max_linhas: usize,
    chips: &[Chip],
) -> [f32; 2] {
    let meta = fonts.map(|f| f.meta);
    let altura = com_fonte(ui, meta, || ui.text_line_height());
    let altura_bandeira = (altura - 2.0).max(10.0);
    let (mut x, mut y, mut linha, mut maior) = (0.0f32, 0.0f32, 1usize, 0.0f32);
    for chip in chips {
        let largura_texto = com_fonte(ui, meta, || ui.calc_text_size(&chip.texto)[0]);
        let largura_bandeira = chip.bandeira.map_or(0.0, |_| componentes::largura_da_bandeira(altura_bandeira) + theme::ESPACO_1);
        let largura = largura_bandeira + largura_texto;
        if x > 0.0 && x + largura > largura_max {
            if linha >= max_linhas {
                break;
            }
            linha += 1;
            x = 0.0;
            y += altura + theme::ESPACO_1;
        }
        if let Some(rosto) = chip.bandeira {
            componentes::bandeira(dl, rosto, [pos[0] + x, pos[1] + y + (altura - altura_bandeira) * 0.5], altura_bandeira);
        }
        componentes::texto_em(ui, meta, dl, [pos[0] + x + largura_bandeira, pos[1] + y], chip.cor, &chip.texto);
        x += largura + theme::ESPACO_3;
        maior = maior.max(x - theme::ESPACO_3);
    }
    [maior, y + altura]
}

/// Os itens de origem no fluxo da tela (modal, painel): desenha e reserva o
/// espaço.
pub fn origem_no_fluxo(ui: &Ui, fonts: Option<&Fonts>, olheiro: &Olheiro, nacoes: &Nacoes<'_>, com_nacao: bool, max_linhas: usize) {
    let largura = ui.content_region_avail()[0];
    let pos = ui.cursor_screen_pos();
    let usado = desenhar_chips(ui, fonts, &ui.get_window_draw_list(), pos, largura, max_linhas, &chips_do_olheiro(olheiro, nacoes, com_nacao));
    ui.dummy([usado[0], usado[1]]);
}

/// Nome curto de um continente nos botões do filtro.
pub fn nome_curto(continente: Confederacao) -> &'static str {
    match continente {
        Confederacao::AmericaDoSul => "Am. do Sul",
        Confederacao::AmericaDoNorte => "Am. do Norte",
        outro => outro.nome(),
    }
}

/// O Olheiro tem algum mercado (país ou continente) neste continente?
pub fn tem_mercado_em(olheiro: &Olheiro, continente: Confederacao) -> bool {
    olheiro.mercados.iter().any(|m| m.continente() == continente)
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
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    com_fonte(ui, fonts.map(|f| f.meta), || {
        let y = ui.cursor_pos()[1];
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text_colored(theme::TEXT_SECONDARY, "Y (ou botão direito) abre as opções do Olheiro: pesquisa, perfil, demissão.");
    });
    ui.dummy([0.0, theme::ESPACO_2]);
    let contratados = state.olheiros_contratados();
    if contratados.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_NENHUM_CONTRATADO));
        ui.dummy([0.0, theme::ESPACO_1]);
    }
    let regioes: std::collections::HashMap<Uuid, String> = contratados
        .iter()
        .filter_map(|c| c.missao.as_ref().map(|m| (c.olheiro.id, state.regiao_da_missao(&m.filtros))))
        .collect();
    let pediu_opcoes = state.opcoes_pedidas();
    com_nacoes(state, |nacoes| match densidade {
        Densidade::Cards => cards(ui, fonts, &contratados, nacoes, &regioes, pediu_opcoes),
        Densidade::Tabular => tabela(ui, fonts, &contratados, pediu_opcoes),
    })
}

/// O que cada Olheiro faz, em até duas linhas para o rodapé do card:
/// a Missão (tipo, onde, contrato ou prazo), o acompanhamento dos Escolhidos
/// ou o convite para encomendar uma Missão.
pub fn texto_situacao(c: &OlheiroContratado, regiao: &str) -> Vec<String> {
    if let Some(m) = &c.missao {
        let natureza = if m.continua { "Missão contínua" } else { "Missão" };
        let onde = if regiao.is_empty() { String::new() } else { format!(" · {regiao}") };
        let prazo = if m.status == StatusMissao::EmExecucao {
            "Gerando o Relatório…".to_string()
        } else if m.tem_contrato() {
            format!("Contrato até {}", formatar_data(m.fim_do_contrato()))
        } else if m.continua {
            format!("Bloco até {}", formatar_data(m.prazo_estimado))
        } else {
            format!("Pronta em {}", formatar_data(m.prazo_estimado))
        };
        return vec![format!("{natureza} {}{onde}", super::nova_missao::nome_tipo(m.tipo)), prazo];
    }
    if c.acompanhando {
        return vec![
            "Acompanha os Escolhidos".to_string(),
            format!("Mantém até {} jogadores atualizados", c.olheiro.capacidade_acompanhamento()),
        ];
    }
    vec!["Livre: A encomenda uma Missão".to_string(), texto_relatorios(c.relatorios)]
}

/// Os Olheiros em grade, mais "Contratar Olheiro" no fim. Y no card em
/// foco (ou o botão direito) pede as Opções dele.
fn cards(
    ui: &Ui,
    fonts: Option<&Fonts>,
    contratados: &[OlheiroContratado],
    nacoes: &Nacoes<'_>,
    regioes: &std::collections::HashMap<Uuid, String>,
    pediu_opcoes: bool,
) -> Acao {
    let mut acao = Acao::Nenhuma;
    let por_linha = olheiro_card::por_linha(ui.content_region_avail()[0]);
    let total = contratados.len() + 1;
    for indice in 0..total {
        if indice % por_linha != 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_3);
        }
        let resultado = match contratados.get(indice) {
            Some(c) => {
                let regiao = regioes.get(&c.olheiro.id).map_or("", String::as_str);
                let rodape = Rodape::Situacao { status: texto_status(c), cor: cor_status(c), linhas: texto_situacao(c, regiao) };
                let r = olheiro_card::desenhar(ui, fonts, &c.olheiro.id.to_string(), &c.olheiro, nacoes, &rodape);
                if r.ativou {
                    acao = Acao::Ativar(c.olheiro.id);
                }
                if r.opcoes || (r.focado && pediu_opcoes) {
                    acao = Acao::Opcoes(c.olheiro.id);
                }
                r
            }
            None => {
                let r = olheiro_card::desenhar_novo(ui, fonts, "contratar", ROTULO_CONTRATAR, DETALHE_CONTRATAR);
                if r.ativou {
                    acao = Acao::AbrirContratacao;
                }
                r
            }
        };
        // o primeiro card em foco: rola até o topo (o alternador não é card)
        if resultado.focado && indice == 0 && ui.scroll_y() > 0.0 {
            ui.set_scroll_y(0.0);
        }
        if indice % por_linha == por_linha - 1 {
            ui.dummy([0.0, theme::ESPACO_1]);
        }
    }
    acao
}

/// Verde livre, dourado em Missão, roxo acompanhando.
fn cor_status(c: &OlheiroContratado) -> [f32; 4] {
    if c.em_missao {
        theme::WARNING
    } else if c.acompanhando {
        theme::ACCENT_PRIMARY
    } else {
        theme::FIELD_GREEN
    }
}

/// Visão Tabular: uma linha por Olheiro e, por último, "Contratar Olheiro".
fn tabela(ui: &Ui, fonts: Option<&Fonts>, contratados: &[OlheiroContratado], pediu_opcoes: bool) -> Acao {
    let mut acao = Acao::Nenhuma;
    let flags = TableFlags::ROW_BG | TableFlags::BORDERS_INNER_H | TableFlags::SIZING_FIXED_FIT | TableFlags::NO_SAVED_SETTINGS;
    let _c1 = ui.push_style_color(StyleColor::TableRowBg, theme::TRANSPARENTE);
    let _c2 = ui.push_style_color(StyleColor::TableRowBgAlt, theme::LINHA_ALTERNADA);
    let _c3 = ui.push_style_color(StyleColor::TableBorderLight, theme::BORDER_HAIRLINE_SUBTLE);
    let _c4 = ui.push_style_color(StyleColor::TableHeaderBg, theme::BG_PANEL_RAISED);
    let Some(_tabela) = ui.begin_table_with_flags("##tabela_olheiros", 6, flags) else {
        return acao;
    };
    let coluna = |nome: &str, largura: f32| {
        let mut setup = TableColumnSetup::new(nome.to_string());
        setup.flags = if largura > 0.0 { TableColumnFlags::WIDTH_FIXED } else { TableColumnFlags::WIDTH_STRETCH };
        setup.init_width_or_weight = if largura > 0.0 { largura } else { 1.0 };
        setup
    };
    let nomes = ["Olheiro", "Foco", "Tier", "Status", "Missão", "Relatórios"];
    for (nome, largura) in nomes.into_iter().zip([200.0, 210.0, 70.0, 120.0, 0.0, 110.0]) {
        ui.table_setup_column_with(coluna(nome, largura));
    }
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.table_next_row_with_flags(TableRowFlags::HEADERS);
        for (i, nome) in nomes.into_iter().enumerate() {
            ui.table_set_column_index(i);
            let _cor = ui.push_style_color(StyleColor::Text, theme::TEXT_SECONDARY);
            ui.table_header(nome);
        }
    });
    for c in contratados {
        let _id = ui.push_id(c.olheiro.id.to_string());
        ui.table_next_row_with_height(TableRowFlags::empty(), ALTURA_LINHA);
        let (ativou, focada) = linha_selecionavel(ui);
        if ativou {
            acao = Acao::Ativar(c.olheiro.id);
        }
        if focada && pediu_opcoes {
            acao = Acao::Opcoes(c.olheiro.id);
        }
        texto_na_celula(ui, fonts.map(|f| f.body), &c.olheiro.nome_exibicao(), theme::TEXT_PRIMARY);
        ui.table_set_column_index(1);
        let perfil = c.olheiro.perfil();
        let foco = format!("{} ({} estrelas)", perfil.foco().nome(), perfil.principal().texto());
        texto_na_celula(ui, fonts.map(|f| f.meta), &foco, theme::TEXT_SECONDARY);
        ui.table_set_column_index(2);
        let pos = ui.cursor_screen_pos();
        desenhar_badge(ui, fonts, &ui.get_window_draw_list(), &badge_tier(c.olheiro.tier), [pos[0], pos[1] + 4.0], ALTURA_LINHA - 8.0);
        ui.table_set_column_index(3);
        texto_na_celula(ui, fonts.map(|f| f.body), texto_status(c), cor_status(c));
        ui.table_set_column_index(4);
        texto_na_celula(ui, fonts.map(|f| f.meta), &texto_missao(c), theme::TEXT_SECONDARY);
        ui.table_set_column_index(5);
        texto_na_celula(ui, fonts.map(|f| f.meta), &texto_relatorios(c.relatorios), theme::TEXT_SECONDARY);
    }
    ui.table_next_row_with_height(TableRowFlags::empty(), ALTURA_LINHA);
    if linha_selecionavel(ui).0 {
        acao = Acao::AbrirContratacao;
    }
    texto_na_celula(ui, fonts.map(|f| f.heading), &format!("+ {ROTULO_CONTRATAR}"), theme::FIELD_GREEN);
    ui.table_set_column_index(4);
    texto_na_celula(ui, fonts.map(|f| f.meta), DETALHE_CONTRATAR, theme::TEXT_SECONDARY);
    acao
}

/// Linha inteira selecionável (mouse e controle), a partir da coluna 0;
/// deixa o cursor na coluna 0 para o texto.
fn linha_selecionavel(ui: &Ui) -> (bool, bool) {
    ui.table_set_column_index(0);
    let inicio = ui.cursor_pos();
    let _c = ui.push_style_color(StyleColor::Header, theme::ACCENT_PRIMARY_DIM);
    let _h = ui.push_style_color(StyleColor::HeaderHovered, theme::ACCENT_PRIMARY_DIM);
    let ativou = ui
        .selectable_config("##linha")
        .flags(SelectableFlags::SPAN_ALL_COLUMNS | SelectableFlags::ALLOW_ITEM_OVERLAP)
        .size([0.0, ALTURA_LINHA - 4.0])
        .build();
    let focada = ui.is_item_focused() && ui.io().nav_visible;
    ui.set_cursor_pos(inicio);
    (ativou, focada)
}

fn texto_na_celula(ui: &Ui, fonte: Option<imgui::FontId>, texto: &str, cor: [f32; 4]) {
    com_fonte(ui, fonte, || {
        let [x, y] = ui.cursor_pos();
        ui.set_cursor_pos([x, y + ((ALTURA_LINHA - 4.0 - ui.text_line_height()) * 0.5).max(0.0)]);
        ui.text_colored(cor, texto);
    });
}

/// "Atratividade do clube: Alta (prestígio 20/20 nacional, 20/20
/// internacional, 1ª divisão, 21 títulos)".
pub fn texto_atratividade(atratividade: u8, clube: Option<&quality::PerfilClube>) -> String {
    let nivel = quality::nome_atratividade(atratividade);
    match clube {
        Some(c) => format!(
            "Atratividade do clube: {nivel} (prestígio {}/20 nacional e {}/20 internacional, {}ª divisão, {} {}).",
            c.prestigio_nacional,
            c.prestigio_internacional,
            c.nivel_liga.max(1),
            c.titulos,
            if c.titulos == 1 { "título" } else { "títulos" }
        ),
        None => format!("Atratividade do clube: {nivel} (o save não deu o prestígio do clube)."),
    }
}

/// Tela "Contratar Olheiro": o mercado da semana, cada Olheiro com o custo,
/// do mais raro ao mais comum, com uma linha de filtros por continente (um
/// Olheiro aparece no continente de qualquer um dos mercados dele); ativar
/// um (com orçamento) abre a Confirmação de Contratação (Story 1.5).
pub fn render_contratacao(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> AcaoContratacao {
    let mut acao = AcaoContratacao::Nenhuma;
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = AcaoContratacao::Voltar;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text(ROTULO_CONTRATAR));
    let mercado = match state.mercado_de_olheiros() {
        Carga::Pronto(m) => m,
        Carga::Carregando | Carga::Erro => {
            com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_LENDO_MERCADO));
            return acao;
        }
    };
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.text_colored(theme::TEXT_SECONDARY, texto_atratividade(mercado.atratividade, mercado.clube.as_ref()));
        ui.text_colored(
            theme::TEXT_SECONDARY,
            format!(
                "O mercado muda toda semana (próximo em {}): cada continente tem de 3 a 10 Olheiros, mais onde há mais ligas relevantes; clubes mais atrativos veem mais Elites. O custo sai do orçamento de transferências, só depois da confirmação.",
                formatar_data(mercado.renova_em)
            ),
        );
    });
    ui.dummy([0.0, theme::ESPACO_2]);
    let filtro = linha_de_filtros(ui, fonts, state, &mercado.ofertas);
    ui.dummy([0.0, theme::ESPACO_2]);

    let visiveis: Vec<&OfertaOlheiro> =
        mercado.ofertas.iter().filter(|o| filtro.is_none_or(|c| tem_mercado_em(&o.olheiro, c))).collect();
    if mercado.ofertas.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_MERCADO_VAZIO));
    } else if visiveis.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_FILTRO_VAZIO));
    }
    com_nacoes(state, |nacoes| {
        let por_linha = olheiro_card::por_linha(ui.content_region_avail()[0]);
        for (indice, oferta) in visiveis.iter().enumerate() {
            if indice % por_linha != 0 {
                ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            }
            let r = olheiro_card::desenhar(ui, fonts, &oferta.id.to_string(), &oferta.olheiro, nacoes, &Rodape::Oferta(oferta));
            if r.ativou {
                acao = AcaoContratacao::Contratar((*oferta).clone());
            }
            if indice % por_linha == por_linha - 1 {
                ui.dummy([0.0, theme::ESPACO_1]);
            }
        }
    });
    acao
}

/// "Todos" e um botão por continente que tem Olheiros esta semana, com a
/// quantidade (escolha única: foco = escolha). Devolve o filtro em vigor.
fn linha_de_filtros(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, ofertas: &[OfertaOlheiro]) -> Option<Confederacao> {
    let atual = state.filtro_continente();
    let mut opcoes: Vec<(Option<Confederacao>, usize)> = vec![(None, ofertas.len())];
    for c in Confederacao::TODAS {
        let n = ofertas.iter().filter(|o| tem_mercado_em(&o.olheiro, c)).count();
        if n > 0 && c != Confederacao::Outras {
            opcoes.push((Some(c), n));
        }
    }
    // um filtro que sumiu da semana (sem Olheiros) volta para "Todos"
    let atual = if opcoes.iter().any(|(c, _)| *c == atual) { atual } else { None };
    let largura = ui.content_region_avail()[0];
    let mut x = 0.0;
    let mut escolhido = atual;
    for (indice, (continente, n)) in opcoes.iter().enumerate() {
        if indice > 0 {
            if x + theme::ESPACO_2 + LARGURA_FILTRO <= largura {
                ui.same_line_with_spacing(0.0, theme::ESPACO_2);
                x += theme::ESPACO_2;
            } else {
                x = 0.0;
            }
        }
        let nome = continente.map_or("Todos", nome_curto);
        // o id fica só no nome: a contagem muda ao contratar sem perder o foco
        let rotulo = format!("{nome} · {n}##filtro_{nome}");
        let estilo = if *continente == atual { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        let clicou = componentes::botao_com_largura(ui, fonts, &rotulo, estilo, true, Some(LARGURA_FILTRO));
        x += LARGURA_FILTRO;
        let focado = componentes::focado_pelo_controle(ui);
        if focado && ui.scroll_y() > 0.0 {
            ui.set_scroll_y(0.0);
        }
        if (clicou || focado) && *continente != atual {
            escolhido = *continente;
        }
    }
    if escolhido != state.filtro_continente() {
        state.definir_filtro_continente(escolhido);
    }
    escolhido
}

/// Nome do Olheiro com o badge do Tier ao lado (também usado na
/// Confirmação de Contratação e na Nova Missão).
pub fn nome_com_badge(ui: &Ui, fonts: Option<&Fonts>, nome: &str, tier: Tier) {
    let altura_nome = com_fonte(ui, fonts.map(|f| f.heading), || {
        ui.text(nome);
        ui.calc_text_size(nome)[1]
    });
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    super::componentes::badge_no_fluxo(ui, fonts, &badge_tier(tier), altura_nome);
}

/// As cinco linhas de estrelas ("JOV ★★★★☆  MED ★★☆☆☆ … REDE ★★★☆☆") pelo
/// draw list, a partir de `pos`; o foco em roxo, a Rede em verde. Devolve a
/// largura ocupada.
pub fn estrelas_do_perfil(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, pos: [f32; 2], perfil: &quality::PerfilOlheiro) -> f32 {
    let foco = perfil.foco();
    let mut x = pos[0];
    for e in Especializacao::TODAS {
        let cor = if e == foco { theme::ACCENT_PRIMARY } else { theme::TEXT_SECONDARY };
        x += rotulo_com_estrelas(ui, fonts, dl, [x, pos[1]], sigla_atributo(e), perfil.atributo(e), cor) + theme::ESPACO_3;
    }
    x += rotulo_com_estrelas(ui, fonts, dl, [x, pos[1]], "REDE", perfil.rede, theme::FIELD_GREEN);
    x - pos[0]
}

/// Texto do tooltip de estrelas: os valores por extenso.
pub fn texto_estrelas(perfil: &quality::PerfilOlheiro) -> String {
    let partes: Vec<String> = Especializacao::TODAS.iter().map(|&e| format!("{} {}", e.nome(), perfil.atributo(e).texto())).collect();
    format!("{} · Rede de contatos {} (estrelas de 0 a 5)", partes.join(" · "), perfil.rede.texto())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{Missao, NacaoOlheiro, StatusMissao};
    use crate::save_repo::Confederacao;

    #[test]
    fn each_especializacao_has_its_own_initials_and_description() {
        let siglas: Vec<_> = Especializacao::TODAS.map(sigla).to_vec();
        assert_eq!(siglas, ["CJ", "CM", "T", "G"]);
        let mut descricoes: Vec<_> = Especializacao::TODAS.map(descricao).to_vec();
        descricoes.dedup();
        assert_eq!(descricoes.len(), 4);
        let mut siglas_atributo: Vec<_> = Especializacao::TODAS.map(sigla_atributo).to_vec();
        siglas_atributo.dedup();
        assert_eq!(siglas_atributo.len(), 4);
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
        assert!(!MSG_MERCADO_VAZIO.contains('!') && !MSG_LENDO_MERCADO.contains('!'));
    }

    fn contratado(olheiro: Olheiro) -> OlheiroContratado {
        OlheiroContratado { olheiro, em_missao: false, acompanhando: false, missao: None, relatorio_atual: None, relatorios: 2, rescisao: None }
    }

    #[test]
    fn a_hired_olheiro_says_what_he_is_doing() {
        let olheiro = Olheiro { id: Uuid::new_v4(), especializacao: Especializacao::Tatico, tier: Tier::Elite, ..Default::default() };
        let livre = contratado(olheiro.clone());
        assert_eq!(texto_status(&livre), "Disponível");
        assert!(texto_missao(&livre).starts_with("Livre"));
        let missao = Missao::de_teste(olheiro.id, StatusMissao::Pendente);
        let ocupado = OlheiroContratado { em_missao: true, missao: Some(missao), ..livre.clone() };
        assert_eq!(texto_status(&ocupado), "Em Missão");
        assert_eq!(texto_missao(&ocupado), "Missão Geral · pronta em 15/07/2026");
        let generalista = Olheiro { especializacao: Especializacao::Generalista, tier: Tier::Experiente, ..olheiro };
        let acompanhando = OlheiroContratado { acompanhando: true, ..contratado(generalista) };
        assert_eq!(texto_status(&acompanhando), "Acompanhando");
        assert!(texto_missao(&acompanhando).contains("até 7 jogadores"), "{}", texto_missao(&acompanhando));
    }

    #[test]
    fn an_olheiro_belongs_to_every_continent_where_he_has_a_market() {
        let olheiro = Olheiro {
            mercados: vec![
                Mercado::Pais { id: 54, continente: Confederacao::AmericaDoSul },
                Mercado::Pais { id: 155, continente: Confederacao::Asia },
                Mercado::Continente(Confederacao::Africa),
            ],
            ..Default::default()
        };
        assert!(tem_mercado_em(&olheiro, Confederacao::AmericaDoSul));
        assert!(tem_mercado_em(&olheiro, Confederacao::Asia), "um país do continente basta");
        assert!(tem_mercado_em(&olheiro, Confederacao::Africa), "o continente inteiro também");
        assert!(!tem_mercado_em(&olheiro, Confederacao::Europa));
        assert!(!tem_mercado_em(&Olheiro::default(), Confederacao::Europa));
        for c in Confederacao::TODAS {
            assert!(nome_curto(c).chars().count() <= 14, "{}", nome_curto(c));
        }
    }

    #[test]
    fn the_origin_chips_carry_the_flags_of_the_nation_and_the_country_markets() {
        let olheiro = Olheiro {
            nacao: Some(NacaoOlheiro { id: 54, nome: "Brasil".to_string(), continente: Confederacao::AmericaDoSul }),
            mercados: vec![Mercado::Pais { id: 54, continente: Confederacao::AmericaDoSul }, Mercado::Continente(Confederacao::AmericaDoSul)],
            ..Default::default()
        };
        let nome = |id: u16| (id == 54).then(|| "Brasil".to_string());
        let bandeira = |_: u16| Rosto::Ausente;
        let nacoes = Nacoes { nome: &nome, bandeira: &bandeira };
        let chips = chips_do_olheiro(&olheiro, &nacoes, true);
        let textos: Vec<&str> = chips.iter().map(|c| c.texto.as_str()).collect();
        assert_eq!(textos, ["Brasil", "Mercados:", "Brasil", "América do Sul (todo o continente)"]);
        assert!(chips[0].bandeira.is_some() && chips[2].bandeira.is_some(), "país: com bandeira");
        assert!(chips[1].bandeira.is_none() && chips[3].bandeira.is_none(), "rótulo e continente: sem");
        assert_eq!(chips_do_olheiro(&olheiro, &nacoes, false).len(), 3, "sem a nação");
        let antigo = chips_do_olheiro(&Olheiro::default(), &nacoes, true);
        assert_eq!(antigo.len(), 1);
        assert!(antigo[0].texto.contains("todos os mercados"));
    }

    #[test]
    fn the_attractiveness_and_stars_texts_read_naturally() {
        let clube = quality::PerfilClube {
            prestigio_nacional: 20,
            prestigio_internacional: 20,
            nivel_liga: 1,
            titulos: 21,
            pais: Some(45),
            continente: Confederacao::Europa,
        };
        assert_eq!(
            texto_atratividade(quality::atratividade(&clube), Some(&clube)),
            "Atratividade do clube: Alta (prestígio 20/20 nacional e 20/20 internacional, 1ª divisão, 21 títulos)."
        );
        let estrelas = texto_estrelas(&quality::PerfilOlheiro::v1(Especializacao::Tatico, Tier::Elite));
        assert!(estrelas.contains("Tático 4,5") && estrelas.contains("Rede de contatos 4,5"), "{estrelas}");
    }
}
