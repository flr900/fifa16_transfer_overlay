//! Ficha de Jogador (Stories 3.1/3.2, UX-DR15): tela satélite aberta a
//! partir de uma linha (ou card) do Relatório. Uma tela só, sem sub-abas:
//! à esquerda o cabeçalho biográfico (rosto, nome, idade, posição nativa
//! com o Fit Posicional, pé, nação, clube) e a lista inteira de atributos
//! revelados; à direita o Radar de Atributos. Voltar (ou B) cai no mesmo
//! Relatório, na mesma rolagem (o ImGui guarda a rolagem da tabela).
//!
//! "Comparar com jogador do elenco" fica sempre visível: abre o seletor de
//! elenco (AD-13) e, na volta, o jogador escolhido aparece em tracejado
//! verde no Radar e numa coluna da lista, sem sair da Ficha.

use imgui::{StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, Ui};

use super::componentes::{self, badge_fit, desenhar_badge_texto, texto_em, EstiloBotao};
use super::relatorio::{formatar_faixa, texto_fit, texto_percentual, PerfilPedido, MSG_BAIXA};
use super::theme::{self, Fonts};
use super::{com_fonte, radar};
use crate::save_repo::nome_posicao;
use crate::scout::minifaces::Rosto;
use crate::scout::state::{Atributo, FichaAberta, JogadorEncontrado, Qualidade, ScoutState};

const LADO_ROSTO: f32 = 112.0;
const FRACAO_ESQUERDA: f32 = 0.46;
const LARGURA_VALOR: f32 = 72.0;
const ALTURA_LEGENDA: f32 = 84.0;

pub const ROTULO_COMPARAR: &str = "Comparar com jogador do elenco";
const NAO_OBSERVADO: &str = "não observado";

/// O que o jogador fez na Ficha neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Voltar,
    /// Abrir o seletor de elenco para comparar (Story 3.2).
    Comparar,
    /// Arquivou o Relatório daqui (Story 2.7): volta à lista.
    Arquivou,
}

/// "22 anos · MEI · Pé esquerdo" (o pé some em Relatórios antigos).
pub fn linha_bio(j: &JogadorEncontrado) -> String {
    let mut partes = vec![format!("{} anos", j.idade), nome_posicao(j.posicao).to_string()];
    if let Some(pe) = j.pe {
        partes.push(format!("Pé {}", pe.nome().to_lowercase()));
    }
    partes.join(" · ")
}

/// "Ritmo alto/baixo · dribles 5/5 · pé fraco 4/5" (some em Relatórios de
/// antes de 2026-10-03).
pub fn linha_caracteristicas(j: &JogadorEncontrado) -> Option<String> {
    let mut partes = Vec::new();
    if let (Some(a), Some(d)) = (j.ritmo_ataque, j.ritmo_defesa) {
        partes.push(format!("Ritmo {}/{}", a.nome().to_lowercase(), d.nome().to_lowercase()));
    }
    if let Some(e) = j.estrelas_drible {
        partes.push(format!("dribles {e}/5"));
    }
    if let Some(e) = j.pe_fraco {
        partes.push(format!("pé fraco {e}/5"));
    }
    (!partes.is_empty()).then(|| partes.join(" · "))
}

/// "OVR ≈ 72 (-2)": o Overall (meio da faixa revelada) mais a variação
/// estimada na posição-alvo.
pub fn texto_overall_no_alvo(j: &JogadorEncontrado) -> Option<String> {
    let variacao = j.variacao_overall?;
    let meio = (i16::from(j.overall.min) + i16::from(j.overall.max)) / 2;
    let estimado = (meio + i16::from(variacao)).clamp(1, 99);
    Some(format!("OVR ≈ {estimado} ({})", super::relatorio::texto_variacao(variacao)))
}

/// "Observados: 15 de 28 atributos".
pub fn texto_observados(ficha: &FichaAberta) -> String {
    let eixos = radar::eixos(&ficha.jogador, None);
    format!("Observados: {} de {} atributos", ficha.jogador.atributos.len(), eixos.len())
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some(ficha) = state.ficha_aberta() else {
        return Acao::Voltar;
    };
    let mut acao = Acao::Nenhuma;

    // Ações sempre visíveis no topo.
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = Acao::Voltar;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    if componentes::botao(ui, fonts, ROTULO_COMPARAR, EstiloBotao::Secundario, true) {
        acao = Acao::Comparar;
    }
    if ficha.comparacao.is_some() {
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        if componentes::botao(ui, fonts, "Tirar comparação", EstiloBotao::Secundario, true) {
            state.comparar_com(None);
        }
    }
    if ScoutState::pode_arquivar(&ficha.item) {
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        if componentes::botao(ui, fonts, "Arquivar", EstiloBotao::Secundario, true) && state.arquivar_relatorio(ficha.item.relatorio.id) {
            acao = Acao::Arquivou;
        }
    }
    ui.dummy([0.0, theme::ESPACO_2]);

    let [largura, altura] = ui.content_region_avail();
    let esquerda = (largura * FRACAO_ESQUERDA).max(320.0);
    ui.child_window("##ficha_bio").size([esquerda, altura]).border(false).flags(super::flags_conteudo()).build(|| {
        cabecalho(ui, fonts, state, &ficha);
        ui.dummy([0.0, theme::ESPACO_3]);
        lista_de_atributos(ui, fonts, &ficha);
    });
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    ui.child_window("##ficha_radar").size([0.0, altura]).border(false).build(|| {
        let [w, h] = ui.content_region_avail();
        let lado = w.min(h - ALTURA_LEGENDA).max(160.0);
        let inicio = ui.cursor_screen_pos();
        let centro = [inicio[0] + w * 0.5, inicio[1] + lado * 0.5];
        // espaço para as siglas em volta do radar
        let raio = lado * 0.5 - theme::ESPACO_5 - theme::ESPACO_2;
        let eixos = radar::eixos(&ficha.jogador, ficha.comparacao.as_ref());
        radar::desenhar(ui, fonts, &ui.get_window_draw_list(), centro, raio, &eixos);
        ui.dummy([w, lado]);
        legenda(ui, fonts, &ficha);
    });
    acao
}

fn cabecalho(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState, ficha: &FichaAberta) {
    let j = &ficha.jogador;
    let perfil = PerfilPedido::de(&ficha.item);
    let dl = ui.get_window_draw_list();
    let min = ui.cursor_screen_pos();
    let max = [min[0] + LADO_ROSTO, min[1] + LADO_ROSTO];
    dl.add_rect(min, max, theme::BG_BASE).filled(true).rounding(theme::RAIO_MD).build();
    match state.rosto(j.player_id) {
        Rosto::Pronto(textura) => dl.add_image(textura, min, max).build(),
        Rosto::Ausente => super::relatorio::silhueta(&dl, min, LADO_ROSTO),
        Rosto::Carregando => {}
    }

    let x = max[0] + theme::ESPACO_4;
    let mut y = min[1];
    y += texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &j.nome)[1] + theme::ESPACO_1;
    // posição nativa + Fit Posicional (Story 3.5)
    let [w_bio, h_bio] = texto_em(ui, fonts.map(|f| f.body), &dl, [x, y], theme::TEXT_SECONDARY, &linha_bio(j));
    if let Some(alvo) = perfil.alvo {
        let texto = format!("FIT {}", texto_fit(alvo, j.fit, j.variacao_overall, perfil.aproximado));
        desenhar_badge_texto(ui, fonts, &dl, &badge_fit(), &texto, [x + w_bio + theme::ESPACO_2, y], h_bio);
    }
    y += h_bio;
    if let Some(caracteristicas) = linha_caracteristicas(j) {
        y += texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::TEXT_SECONDARY, &caracteristicas)[1];
    }
    if let (Some(alvo), Some(texto)) = (perfil.alvo, texto_overall_no_alvo(j)) {
        let linha = format!("Como {}: {texto} (estimativa)", alvo.nome());
        y += texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::ACCENT_PRIMARY, &linha)[1];
    }
    let clube = if j.clube.is_empty() { "Sem clube" } else { j.clube.as_str() };
    y += texto_em(ui, fonts.map(|f| f.body), &dl, [x, y], theme::TEXT_SECONDARY, &format!("{} · {clube}", j.nacao))[1]
        + theme::ESPACO_1;
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let [w_ovr, h_ovr] = texto_em(ui, mono, &dl, [x, y], theme::TEXT_PRIMARY, &format!("OVR {}", formatar_faixa(j.overall)));
    texto_em(ui, mono, &dl, [x + w_ovr + theme::ESPACO_3, y], theme::FIELD_GREEN, &format!("POT {}", formatar_faixa(j.potencial)));
    y += h_ovr;
    if let Some(nome) = &perfil.referencia {
        let texto = format!("Similaridade com {nome}: {}", texto_percentual(j.similaridade, perfil.aproximado));
        y += texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::ACCENT_PRIMARY, &texto)[1];
    }
    drop(dl);
    let altura = (y - min[1]).max(LADO_ROSTO);
    ui.dummy([0.0, altura]);
    if ficha.item.relatorio.qualidade == Qualidade::Baixa {
        let _cor = ui.push_style_color(StyleColor::Text, theme::WARNING);
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_wrapped(MSG_BAIXA));
    }
}

/// Todos os atributos revelados, na ordem fixa; com comparação, uma coluna
/// a mais com o valor do jogador do elenco (inclusive onde o Olheiro não
/// observou).
fn lista_de_atributos(ui: &Ui, fonts: Option<&Fonts>, ficha: &FichaAberta) {
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, texto_observados(ficha)));
    let comparando = ficha.comparacao.as_ref();
    let colunas = if comparando.is_some() { 3 } else { 2 };
    let flags = TableFlags::ROW_BG | TableFlags::BORDERS_INNER_H | TableFlags::SIZING_FIXED_FIT | TableFlags::NO_SAVED_SETTINGS;
    let _c1 = ui.push_style_color(StyleColor::TableRowBg, theme::TRANSPARENTE);
    let _c2 = ui.push_style_color(StyleColor::TableRowBgAlt, theme::LINHA_ALTERNADA);
    let _c3 = ui.push_style_color(StyleColor::TableBorderLight, theme::BORDER_HAIRLINE_SUBTLE);
    let Some(_tabela) = ui.begin_table_with_flags("##atributos_ficha", colunas, flags) else {
        return;
    };
    let coluna = |nome: &str, largura: f32| {
        let mut setup = TableColumnSetup::new(nome.to_string());
        setup.flags = if largura > 0.0 { TableColumnFlags::WIDTH_FIXED } else { TableColumnFlags::WIDTH_STRETCH };
        setup.init_width_or_weight = if largura > 0.0 { largura } else { 1.0 };
        setup
    };
    ui.table_setup_column_with(coluna("Atributo", 0.0));
    ui.table_setup_column_with(coluna("Valor", LARGURA_VALOR));
    if let Some(c) = comparando {
        let nome = c.nome.split_whitespace().last().unwrap_or("Elenco").to_string();
        ui.table_setup_column_with(coluna(&nome, LARGURA_VALOR));
    }
    com_fonte(ui, fonts.map(|f| f.meta), || {
        let _cor = ui.push_style_color(StyleColor::Text, theme::TEXT_SECONDARY);
        ui.table_headers_row();
    });
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let linhas: Vec<Atributo> = if comparando.is_some() {
        radar::eixos(&ficha.jogador, None).into_iter().map(|e| e.atributo).collect()
    } else {
        Atributo::TODOS.into_iter().filter(|a| ficha.jogador.atributo(*a).is_some()).collect()
    };
    for a in linhas {
        ui.table_next_row();
        ui.table_set_column_index(0);
        let valor = ficha.jogador.atributo(a);
        let cor_nome = if valor.is_some() { theme::TEXT_PRIMARY } else { theme::TEXT_DISABLED };
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(cor_nome, a.nome()));
        ui.table_set_column_index(1);
        match valor {
            Some(f) => com_fonte(ui, mono, || ui.text_colored(theme::ACCENT_PRIMARY, formatar_faixa(f))),
            None => com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_DISABLED, "—")),
        }
        if let Some(c) = comparando {
            ui.table_set_column_index(2);
            let texto = c.atributo(a).map_or("—".to_string(), |v| v.to_string());
            com_fonte(ui, mono, || ui.text_colored(theme::FIELD_GREEN, texto));
        }
    }
}

/// Legenda do Radar: sempre em texto, nunca só cor (NFR5).
fn legenda(ui: &Ui, fonts: Option<&Fonts>, ficha: &FichaAberta) {
    item_da_legenda(
        ui,
        fonts,
        |dl, a, b| dl.add_line(a, b, theme::ACCENT_PRIMARY).thickness(2.0).build(),
        &format!("{} (Relatório; faixa = traço no eixo)", ficha.jogador.nome),
        theme::TEXT_PRIMARY,
    );
    if let Some(c) = &ficha.comparacao {
        let texto = format!("{} (seu elenco)", c.nome);
        item_da_legenda(ui, fonts, |dl, a, b| radar::tracejado(dl, a, b, theme::FIELD_GREEN, 2.0), &texto, theme::TEXT_PRIMARY);
    }
    item_da_legenda(ui, fonts, |dl, a, b| radar::pontilhado(dl, a, b, theme::TEXT_DISABLED), NAO_OBSERVADO, theme::TEXT_SECONDARY);
}

/// Amostra do traço (28 px) e o texto ao lado.
fn item_da_legenda(
    ui: &Ui,
    fonts: Option<&Fonts>,
    desenho: impl Fn(&imgui::DrawListMut<'_>, [f32; 2], [f32; 2]),
    texto: &str,
    cor: [f32; 4],
) {
    let pos = ui.cursor_screen_pos();
    let h = ui.text_line_height();
    let (a, b) = ([pos[0], pos[1] + h * 0.5], [pos[0] + 28.0, pos[1] + h * 0.5]);
    desenho(&ui.get_window_draw_list(), a, b);
    ui.dummy([28.0, h]);
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(cor, texto));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{FaixaAtributo, Pe};

    #[test]
    fn the_bio_line_has_age_position_and_foot() {
        let mut j = JogadorEncontrado {
            player_id: 1,
            nome: "A".to_string(),
            idade: 22,
            posicao: 18,
            nacao_id: 54,
            nacao: "Brazil".to_string(),
            clube: String::new(),
            overall: FaixaAtributo { min: 70, max: 74 },
            potencial: FaixaAtributo { min: 80, max: 84 },
            atributos: Vec::new(),
            pe: Some(Pe::Esquerdo),
            similaridade: None,
            fit: None,
            variacao_overall: None,
            ritmo_ataque: None,
            ritmo_defesa: None,
            estrelas_drible: None,
            pe_fraco: None,
        };
        assert_eq!(linha_bio(&j), "22 anos · MEI · Pé esquerdo");
        j.pe = None;
        assert_eq!(linha_bio(&j), "22 anos · MEI", "Relatório de antes da Story 3.1");
        assert_eq!(linha_caracteristicas(&j), None);
        j.ritmo_ataque = Some(crate::scout::state::RitmoTrabalho::Alto);
        j.ritmo_defesa = Some(crate::scout::state::RitmoTrabalho::Baixo);
        j.estrelas_drible = Some(5);
        j.pe_fraco = Some(4);
        assert_eq!(linha_caracteristicas(&j).as_deref(), Some("Ritmo alto/baixo · dribles 5/5 · pé fraco 4/5"));
        j.variacao_overall = Some(-2);
        assert_eq!(texto_overall_no_alvo(&j).as_deref(), Some("OVR ≈ 70 (-2)"));
        assert!(!ROTULO_COMPARAR.contains('!'));
    }
}
