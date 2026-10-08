//! O que as telas de jogadores têm em comum (2026-10-08, pedido do Felipe):
//! Escolhidos, Relatórios (por jogador), Base do Scout e o Relatório aberto.
//!
//! - **Barra** acima da lista: visão Cards / Tabular, "Filtros · Y", a
//!   ordem (coluna e sentido) e a linha de posição — Todos, Detalhados, Gol,
//!   Zag, Mei, Ata — com quantos jogadores há em cada (escolha única: foco =
//!   escolha, como nos outros grupos).
//! - **Visão Tabular**: uma linha por jogador com o que decide uma
//!   contratação — time, país, valor, salário, contrato —, além de nome,
//!   idade, altura, posição, Overall e Potencial. O cabeçalho de cada coluna
//!   é um item de foco: com o controle, A ordena por ela (e A de novo
//!   inverte); com o mouse, clicar faz o mesmo.
//! - **Painel de filtros (Y)**: Overall, Potencial, idade, pé, ritmos e
//!   posições, numa janela por cima da tela; B fecha.
//!
//! A lógica (grupos, filtros, ordenação) fica em `scout::lista`; a tela de
//! cada lista monta os itens, desenha os próprios cards e usa isto para o
//! resto.

use imgui::{Condition, SelectableFlags, StyleColor, StyleVar, TableColumnFlags, TableColumnSetup, TableFlags, TableRowFlags, Ui, WindowFlags};

use super::componentes::{self, EstiloBotao};
use super::relatorio::{contrato_a_vencer, formatar_contrato, formatar_dinheiro, formatar_faixa};
use super::theme::{self, Fonts};
use super::{com_fonte, nova_missao};
use crate::save_repo::{nome_posicao, Date, Pe, RitmoTrabalho};
use crate::scout::lista::{self, Coluna, FiltrosLista, ItemLista, ListaId, Ordenacao};
use crate::scout::quality::Perfil;
use crate::scout::state::{Densidade, FaixaAtributo, JogadorEncontrado, ScoutState};

const ALTURA_LINHA: f32 = 34.0;
const LARGURA_MODO: f32 = 110.0;
const LARGURA_GRUPO: f32 = 150.0;
const LARGURA_PAINEL: f32 = 640.0;
const LARGURA_ROTULO: f32 = 150.0;
const LARGURA_VALOR: f32 = 44.0;
const LARGURA_OPCAO: f32 = 110.0;
const LARGURA_PERFIL: f32 = 128.0;

pub const MSG_NENHUM_NO_FILTRO: &str = "Nenhum jogador passa nos filtros. Aperte Y para mudar os filtros.";

/// Nome da lista no título do painel de filtros.
pub fn nome_da_lista(id: ListaId) -> &'static str {
    match id {
        ListaId::Escolhidos => "Escolhidos",
        ListaId::Relatorios => "Relatórios",
        ListaId::Base => "Base do Scout",
        ListaId::RelatorioAberto => "Relatório",
    }
}

/// As colunas da visão Tabular de cada lista.
pub fn colunas(id: ListaId) -> Vec<Coluna> {
    let mut colunas = vec![
        Coluna::Nome,
        Coluna::Idade,
        Coluna::Altura,
        Coluna::Posicao,
        Coluna::Time,
        Coluna::Pais,
        Coluna::Overall,
        Coluna::Potencial,
        Coluna::Valor,
        Coluna::Salario,
        Coluna::Contrato,
    ];
    if id != ListaId::RelatorioAberto {
        colunas.push(Coluna::Origem);
    }
    colunas
}

/// O título da última coluna: nos Escolhidos é a situação da observação; nas
/// outras, quem viu o jogador.
pub fn titulo_da_coluna(id: ListaId, coluna: Coluna) -> &'static str {
    match (id, coluna) {
        (ListaId::Escolhidos, Coluna::Origem) => "Situação",
        _ => coluna.titulo(),
    }
}

/// O valor de transferência como a tela o mostra: o exato do jogo, se ainda
/// vale, senão a estimativa.
pub fn valor_do_jogador(state: &ScoutState, j: &JogadorEncontrado) -> i64 {
    state.valor_exato(j.player_id).map_or_else(|| state.valor_estimado(j), i64::from)
}

// ---------------------------------------------------------------------
// Barra
// ---------------------------------------------------------------------

/// Desenha a barra acima da lista. `itens` são TODOS os jogadores da lista
/// (os números dos botões de posição saem deles, com os filtros do painel
/// valendo). O Y abre o painel de filtros desta lista.
pub fn barra(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, id: ListaId, itens: &[ItemLista<'_>]) {
    if state.opcoes_pedidas() {
        state.abrir_painel_de_filtros(id);
    }
    let _id = ui.push_id(format!("lista_{id:?}"));
    let modo = state.modo_da_lista(id);
    match componentes::alternador(ui, fonts, &["Cards", "Tabular"], usize::from(modo == Densidade::Tabular), LARGURA_MODO) {
        Some(0) => state.definir_modo_da_lista(id, Densidade::Cards),
        Some(_) => state.definir_modo_da_lista(id, Densidade::Tabular),
        None => {}
    }

    let filtros = state.filtros_da_lista(id);
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    let ativos = filtros.ativos();
    let rotulo = if ativos > 0 { format!("Filtros ({ativos})  ·  Y##filtros") } else { "Filtros  ·  Y##filtros".to_string() };
    let estilo = if ativos > 0 { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
    if componentes::botao(ui, fonts, &rotulo, estilo, true) {
        state.abrir_painel_de_filtros(id);
    }

    // a ordem: a coluna (A passa para a próxima) e o sentido
    let ordenacao = state.ordenacao_da_lista(id);
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    let rotulo = format!("Ordem: {}##ordem", titulo_da_coluna(id, ordenacao.coluna));
    if componentes::botao(ui, fonts, &rotulo, EstiloBotao::Secundario, true) {
        state.definir_ordenacao_da_lista(id, Ordenacao { coluna: proxima_coluna(id, ordenacao.coluna), ..ordenacao });
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    let sentido = if ordenacao.decrescente { "Maior primeiro" } else { "Menor primeiro" };
    if componentes::botao(ui, fonts, &format!("{sentido}##sentido"), EstiloBotao::Secundario, true) {
        state.definir_ordenacao_da_lista(id, Ordenacao { decrescente: !ordenacao.decrescente, ..ordenacao });
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Inverte a ordem. Na visão Tabular, A no cabeçalho de uma coluna ordena por ela.");
    }

    // a posição: escolha única, foco = escolha
    ui.dummy([0.0, theme::ESPACO_1]);
    let contagens = lista::contagem_por_grupo(itens, &filtros);
    for (indice, (grupo, quantos)) in contagens.into_iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        }
        let atual = filtros.grupo == grupo;
        let estilo = if atual { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        // o id é só a sigla: a contagem muda sem o foco se perder
        let rotulo = format!("{} · {quantos}##grupo_{}", grupo.sigla(), grupo.sigla());
        let clicou = componentes::botao_com_largura(ui, fonts, &rotulo, estilo, true, Some(LARGURA_GRUPO));
        if (clicou || componentes::focado_pelo_controle(ui)) && !atual {
            state.mutar_filtros_da_lista(id, |f| f.grupo = grupo);
        }
        if ui.is_item_hovered() {
            ui.tooltip_text(grupo.nome());
        }
    }
    ui.dummy([0.0, theme::ESPACO_2]);
}

/// A coluna seguinte do ciclo do botão "Ordem" (as colunas da lista).
fn proxima_coluna(id: ListaId, atual: Coluna) -> Coluna {
    let todas = colunas(id);
    let i = todas.iter().position(|c| *c == atual).unwrap_or(0);
    todas.get((i + 1) % todas.len()).copied().unwrap_or(atual)
}

/// Aplica os filtros e a ordenação da lista.
pub fn preparar<'a>(state: &ScoutState, id: ListaId, itens: Vec<ItemLista<'a>>) -> Vec<ItemLista<'a>> {
    let filtros = state.filtros_da_lista(id);
    let mut visiveis: Vec<ItemLista<'a>> = itens.into_iter().filter(|i| filtros.passa(i)).collect();
    let valor = |j: &JogadorEncontrado| valor_do_jogador(state, j);
    lista::ordenar(&mut visiveis, state.ordenacao_da_lista(id), &valor);
    visiveis
}

// ---------------------------------------------------------------------
// Visão Tabular
// ---------------------------------------------------------------------

/// A tabela. Devolve o jogador cuja linha foi ativada (clique ou A).
pub fn tabela(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, id: ListaId, itens: &[ItemLista<'_>]) -> Option<u32> {
    let mut ativado = None;
    let ordenacao = state.ordenacao_da_lista(id);
    let hoje = state.data_da_carreira();
    let todas = colunas(id);

    let flags = TableFlags::BORDERS_INNER_H | TableFlags::SIZING_FIXED_FIT | TableFlags::NO_SAVED_SETTINGS;
    let _c1 = ui.push_style_color(StyleColor::TableBorderLight, theme::BORDER_HAIRLINE_SUBTLE);
    let _c2 = ui.push_style_color(StyleColor::TableHeaderBg, theme::BG_PANEL_RAISED);
    let Some(_tabela) = ui.begin_table_with_flags(format!("##tabela_{id:?}"), todas.len(), flags) else {
        return None;
    };
    for coluna in &todas {
        ui.table_setup_column_with(configurar(*coluna));
    }

    // cabeçalho: cada título é um item de foco que ordena
    let mut nova_ordem = None;
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.table_next_row_with_flags(TableRowFlags::HEADERS);
        for (i, coluna) in todas.iter().enumerate() {
            ui.table_set_column_index(i);
            let _id = ui.push_id_usize(i);
            let escolhida = ordenacao.coluna == *coluna;
            let cor = if escolhida { theme::ACCENT_PRIMARY } else { theme::TEXT_SECONDARY };
            let _cor = ui.push_style_color(StyleColor::Text, cor);
            let _realce = ui.push_style_color(StyleColor::HeaderHovered, theme::ACCENT_PRIMARY_DIM);
            let _ativo = ui.push_style_color(StyleColor::Header, theme::ACCENT_PRIMARY_DIM);
            let clicou = ui
                .selectable_config(format!("{}##cabecalho", titulo_da_coluna(id, *coluna)))
                .size([0.0, ALTURA_LINHA - 8.0])
                .build();
            if escolhida {
                seta_de_ordem(ui, ordenacao.decrescente);
            }
            if clicou {
                nova_ordem = Some(ordenacao.alternar(*coluna));
            }
        }
    });

    // linhas: só as visíveis (a Base pode ter milhares)
    let clipper = imgui::ListClipper::new(i32::try_from(itens.len()).unwrap_or(i32::MAX)).items_height(ALTURA_LINHA).begin(ui);
    for indice in clipper.iter() {
        let Some(item) = usize::try_from(indice).ok().and_then(|i| itens.get(i)) else {
            continue;
        };
        let j = item.jogador;
        let _id = ui.push_id_usize(j.player_id as usize);
        ui.table_next_row_with_height(TableRowFlags::empty(), ALTURA_LINHA);
        if linha_selecionavel(ui) {
            ativado = Some(j.player_id);
        }
        for (i, coluna) in todas.iter().enumerate() {
            ui.table_set_column_index(i);
            let (texto, cor) = celula(state, item, *coluna, hoje);
            let fonte = if matches!(coluna, Coluna::Nome) { fonts.map(|f| f.body) } else { fonts.map(|f| f.meta) };
            texto_na_celula(ui, fonte, &texto, cor);
        }
    }
    if let Some(ordem) = nova_ordem {
        state.definir_ordenacao_da_lista(id, ordem);
    }
    ativado
}

/// Largura e peso de cada coluna: os números fixos, os textos esticam.
fn configurar(coluna: Coluna) -> TableColumnSetup<String> {
    let (nome, largura, estica) = match coluna {
        Coluna::Nome => ("Jogador", 3.0, true),
        Coluna::Idade => ("Idade", 60.0, false),
        Coluna::Altura => ("Altura", 84.0, false),
        Coluna::Posicao => ("Pos.", 58.0, false),
        Coluna::Time => ("Time", 2.5, true),
        Coluna::Pais => ("País", 1.6, true),
        Coluna::Overall => ("OVR", 80.0, false),
        Coluna::Potencial => ("POT", 80.0, false),
        Coluna::Valor => ("Valor", 104.0, false),
        Coluna::Salario => ("Salário", 120.0, false),
        Coluna::Contrato => ("Contrato", 138.0, false),
        Coluna::Origem => ("Origem", 3.0, true),
    };
    let mut setup = TableColumnSetup::new(nome.to_string());
    setup.flags = if estica { TableColumnFlags::WIDTH_STRETCH } else { TableColumnFlags::WIDTH_FIXED };
    setup.init_width_or_weight = largura;
    setup
}

/// O texto e a cor de uma célula.
pub fn celula(state: &ScoutState, item: &ItemLista<'_>, coluna: Coluna, hoje: Option<Date>) -> (String, [f32; 4]) {
    let j = item.jogador;
    let normal = theme::TEXT_PRIMARY;
    let suave = theme::TEXT_SECONDARY;
    match coluna {
        Coluna::Nome => (j.nome.clone(), normal),
        Coluna::Idade => (j.idade.to_string(), normal),
        Coluna::Altura => (lista::texto_altura(j.altura), suave),
        Coluna::Posicao => (nome_posicao(j.posicao).to_string(), normal),
        Coluna::Time => (if j.clube.is_empty() { "Sem clube".to_string() } else { j.clube.clone() }, suave),
        Coluna::Pais => (j.nacao.clone(), suave),
        Coluna::Overall => (formatar_faixa(j.overall), normal),
        Coluna::Potencial => (formatar_faixa(j.potencial), theme::FIELD_GREEN),
        Coluna::Valor => match state.valor_exato(j.player_id) {
            Some(v) => (formatar_dinheiro(i64::from(v)), normal),
            None => (format!("≈ {}", formatar_dinheiro(state.valor_estimado(j))), suave),
        },
        Coluna::Salario => {
            if j.salario_conhecido() {
                (format!("≈ {}/sem", formatar_dinheiro(j.salario_estimado())), suave)
            } else {
                ("em observação".to_string(), theme::TEXT_DISABLED)
            }
        }
        Coluna::Contrato => {
            let cor = if contrato_a_vencer(j.contrato_ate, hoje) { theme::WARNING } else { suave };
            (formatar_contrato(j.contrato_ate, hoje), cor)
        }
        Coluna::Origem => (item.origem.clone(), suave),
    }
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

/// Triângulo no canto direito do cabeçalho: para baixo = maior primeiro.
fn seta_de_ordem(ui: &Ui, decrescente: bool) {
    let [_, y0] = ui.item_rect_min();
    let [x1, y1] = ui.item_rect_max();
    let centro = [x1 - theme::ESPACO_3, (y0 + y1) * 0.5];
    let lado = 5.0;
    let (a, b, c) = if decrescente {
        ([centro[0] - lado, centro[1] - lado * 0.6], [centro[0] + lado, centro[1] - lado * 0.6], [centro[0], centro[1] + lado * 0.8])
    } else {
        ([centro[0] - lado, centro[1] + lado * 0.6], [centro[0] + lado, centro[1] + lado * 0.6], [centro[0], centro[1] - lado * 0.8])
    };
    ui.get_window_draw_list().add_triangle(a, b, c, theme::ACCENT_PRIMARY).filled(true).build();
}

// ---------------------------------------------------------------------
// Painel de filtros (Y)
// ---------------------------------------------------------------------

/// O que o jogador fez no painel neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoPainel {
    Nenhuma,
    Fechar,
}

/// Move o limite de uma faixa em `delta`, sem sair de `limites` e sem deixar
/// o mínimo passar do máximo (o outro limite vai junto).
pub fn ajustar_faixa(faixa: FaixaAtributo, minimo: bool, delta: i32, limites: (u8, u8)) -> FaixaAtributo {
    let (menor, maior) = limites;
    let limitar = |v: i32| u8::try_from(v.clamp(i32::from(menor), i32::from(maior))).unwrap_or(menor);
    let FaixaAtributo { min, max } = faixa;
    if minimo {
        let novo = limitar(i32::from(min) + delta);
        FaixaAtributo { min: novo, max: max.max(novo) }
    } else {
        let novo = limitar(i32::from(max) + delta);
        FaixaAtributo { min: min.min(novo), max: novo }
    }
}

/// A janela de filtros da lista `id`, por cima da tela.
pub fn painel_de_filtros(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, id: ListaId) -> AcaoPainel {
    let mut acao = AcaoPainel::Nenhuma;
    let [largura_tela, altura_tela] = ui.io().display_size;
    let filtros = state.filtros_da_lista(id);
    let mut novos = filtros.clone();

    let _fundo = ui.push_style_color(StyleColor::WindowBg, theme::BG_PANEL_RAISED);
    let _borda = ui.push_style_color(StyleColor::Border, theme::BORDER_HAIRLINE);
    let _raio = ui.push_style_var(StyleVar::WindowRounding(theme::RAIO_LG));
    let _padding = ui.push_style_var(StyleVar::WindowPadding([theme::ESPACO_5, theme::ESPACO_4]));
    ui.window("Filtros##painel_de_filtros")
        .position([largura_tela * 0.5, altura_tela * 0.5], Condition::Always)
        .position_pivot([0.5, 0.5])
        .focused(true)
        .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_SAVED_SETTINGS | WindowFlags::ALWAYS_AUTO_RESIZE)
        .build(|| {
            ui.dummy([LARGURA_PAINEL, 0.0]);
            com_fonte(ui, fonts.map(|f| f.heading), || ui.text(format!("Filtros · {}", nome_da_lista(id))));
            com_fonte(ui, fonts.map(|f| f.meta), || {
                ui.text_colored(theme::TEXT_SECONDARY, "Valem para os dois modos (Cards e Tabular). B fecha.");
            });
            ui.dummy([0.0, theme::ESPACO_2]);

            linha_de_faixa(ui, fonts, "Overall", &mut novos.overall, lista::OVERALL);
            linha_de_faixa(ui, fonts, "Potencial", &mut novos.potencial, lista::POTENCIAL);
            linha_de_faixa(ui, fonts, "Idade", &mut novos.idade, lista::IDADE);
            linha_pe(ui, fonts, &mut novos);
            linha_ritmo(ui, fonts, "Ritmo no ataque", true, &mut novos);
            linha_ritmo(ui, fonts, "Ritmo na defesa", false, &mut novos);
            linha_posicoes(ui, fonts, &mut novos);

            ui.dummy([0.0, theme::ESPACO_3]);
            if componentes::botao(ui, fonts, "Fechar", EstiloBotao::Primario, true) {
                acao = AcaoPainel::Fechar;
            }
            ui.set_item_default_focus();
            ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            if componentes::botao(ui, fonts, "Limpar filtros", EstiloBotao::Secundario, filtros.ativos() > 0) {
                novos.limpar_painel();
            }
            ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            let ativos = novos.ativos();
            com_fonte(ui, fonts.map(|f| f.meta), || {
                let y = ui.cursor_pos()[1];
                ui.set_cursor_pos([ui.cursor_pos()[0], y + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
                let texto = match ativos {
                    0 => "Nenhum filtro ligado.".to_string(),
                    1 => "1 filtro ligado.".to_string(),
                    n => format!("{n} filtros ligados."),
                };
                ui.text_colored(theme::TEXT_SECONDARY, texto);
            });
        });
    if novos != filtros {
        state.mutar_filtros_da_lista(id, |f| *f = novos);
    }
    acao
}

fn rotulo(ui: &Ui, fonts: Option<&Fonts>, texto: &str) {
    com_fonte(ui, fonts.map(|f| f.body), || {
        let y = ui.cursor_pos()[1];
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text_colored(theme::TEXT_SECONDARY, texto);
    });
}

/// `Overall  [-] 70 [+]  até  [-] 99 [+]`
fn linha_de_faixa(ui: &Ui, fonts: Option<&Fonts>, nome: &str, faixa: &mut FaixaAtributo, limites: (u8, u8)) {
    let _id = ui.push_id(nome);
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, nome);
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    for (indice, minimo) in [true, false].into_iter().enumerate() {
        if indice == 1 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_4);
            rotulo(ui, fonts, "até");
            ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        }
        let valor = if minimo { faixa.min } else { faixa.max };
        if let Some(delta) = stepper(ui, fonts, if minimo { "min" } else { "max" }, valor, limites) {
            *faixa = ajustar_faixa(*faixa, minimo, delta, limites);
        }
    }
}

/// `[-] valor [+]`; segurar repete. Devolve o passo pedido neste frame.
fn stepper(ui: &Ui, fonts: Option<&Fonts>, id: &str, valor: u8, (menor, maior): (u8, u8)) -> Option<i32> {
    let _id = ui.push_id(id);
    let _repetir = ui.push_button_repeat(true);
    let mut delta = None;
    let lado = Some(theme::ALVO_MINIMO);
    if componentes::botao_com_largura(ui, fonts, "-##menos", EstiloBotao::Secundario, valor > menor, lado) {
        delta = Some(-1);
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    let texto = valor.to_string();
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let inicio = ui.cursor_pos();
    com_fonte(ui, mono, || {
        let [w, h] = ui.calc_text_size(&texto);
        ui.set_cursor_pos([inicio[0] + (LARGURA_VALOR - w) * 0.5, inicio[1] + (theme::ALVO_MINIMO - h) * 0.5]);
        ui.text(&texto);
    });
    ui.same_line_with_spacing(inicio[0] + LARGURA_VALOR + theme::ESPACO_2, 0.0);
    if componentes::botao_com_largura(ui, fonts, "+##mais", EstiloBotao::Secundario, valor < maior, lado) {
        delta = Some(1);
    }
    delta
}

/// Pé: Qualquer / Direito / Esquerdo (escolha única: foco = escolha).
fn linha_pe(ui: &Ui, fonts: Option<&Fonts>, filtros: &mut FiltrosLista) {
    let _id = ui.push_id("pe");
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, "Pé preferido");
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    let opcoes: [(Option<Pe>, &str); 3] = [(None, "Qualquer"), (Some(Pe::Direito), "Direito"), (Some(Pe::Esquerdo), "Esquerdo")];
    for (indice, (opcao, nome)) in opcoes.into_iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        }
        let atual = filtros.pe == opcao;
        let estilo = if atual { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        let clicou = componentes::botao_com_largura(ui, fonts, nome, estilo, true, Some(LARGURA_OPCAO));
        if (clicou || componentes::focado_pelo_controle(ui)) && !atual {
            filtros.pe = opcao;
        }
    }
}

/// Ritmo de trabalho: Baixo / Médio / Alto, cada um entra ou sai ao ser
/// ativado (nenhum marcado = qualquer).
fn linha_ritmo(ui: &Ui, fonts: Option<&Fonts>, nome: &str, ataque: bool, filtros: &mut FiltrosLista) {
    let _id = ui.push_id(nome);
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, nome);
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    for (indice, ritmo) in RitmoTrabalho::TODOS.into_iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        }
        let marcados = if ataque { &filtros.ritmo_ataque } else { &filtros.ritmo_defesa };
        let estilo = if marcados.contains(&ritmo) { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        if componentes::botao_com_largura(ui, fonts, ritmo.nome(), estilo, true, Some(LARGURA_OPCAO)) {
            filtros.alternar_ritmo(ataque, ritmo);
        }
    }
}

/// Posições: um botão por grupo de posição nativa (multisseleção).
fn linha_posicoes(ui: &Ui, fonts: Option<&Fonts>, filtros: &mut FiltrosLista) {
    let _id = ui.push_id("posicoes");
    rotulo(ui, fonts, "Posição");
    for (indice, perfil) in Perfil::TODOS.into_iter().enumerate() {
        if indice % 5 != 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        }
        let estilo = if filtros.perfis.contains(&perfil) { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        if componentes::botao_com_largura(ui, fonts, perfil.nome(), estilo, true, Some(LARGURA_PERFIL)) {
            filtros.alternar_perfil(perfil);
        }
        if ui.is_item_hovered() {
            ui.tooltip_text(perfil.nome());
        }
    }
    // o texto-resumo sob os botões
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, nova_missao::texto_posicoes(&filtros.perfis)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_steppers_never_cross_and_stay_inside_the_limits() {
        let f = FaixaAtributo { min: 70, max: 75 };
        assert_eq!(ajustar_faixa(f, true, 1, lista::OVERALL), FaixaAtributo { min: 71, max: 75 });
        assert_eq!(ajustar_faixa(f, true, 10, lista::OVERALL), FaixaAtributo { min: 80, max: 80 }, "o máximo vai junto");
        assert_eq!(ajustar_faixa(f, false, -10, lista::OVERALL), FaixaAtributo { min: 65, max: 65 }, "o mínimo vai junto");
        assert_eq!(ajustar_faixa(f, true, -200, lista::OVERALL), FaixaAtributo { min: 1, max: 75 });
        assert_eq!(ajustar_faixa(f, false, 200, lista::OVERALL), FaixaAtributo { min: 70, max: 99 });
        let idade = FaixaAtributo { min: 15, max: 45 };
        assert_eq!(ajustar_faixa(idade, true, -1, lista::IDADE), idade, "15 é o piso");
        assert_eq!(ajustar_faixa(idade, false, 1, lista::IDADE), idade, "45 é o teto");
    }

    #[test]
    fn every_list_has_its_columns_and_the_origin_column_is_named_by_the_list() {
        let escolhidos = colunas(ListaId::Escolhidos);
        assert!(escolhidos.contains(&Coluna::Time) && escolhidos.contains(&Coluna::Valor) && escolhidos.contains(&Coluna::Altura));
        assert!(!colunas(ListaId::RelatorioAberto).contains(&Coluna::Origem), "no Relatório aberto a origem é a do cabeçalho");
        assert_eq!(titulo_da_coluna(ListaId::Escolhidos, Coluna::Origem), "Situação");
        assert_eq!(titulo_da_coluna(ListaId::Base, Coluna::Origem), "Visto por");
        assert_eq!(proxima_coluna(ListaId::Base, Coluna::Origem), Coluna::Nome, "o ciclo dá a volta");
        assert_eq!(proxima_coluna(ListaId::Base, Coluna::Nome), Coluna::Idade);
        for id in ListaId::TODAS {
            assert!(!nome_da_lista(id).is_empty());
        }
        assert!(!MSG_NENHUM_NO_FILTRO.contains('!'));
    }
}
