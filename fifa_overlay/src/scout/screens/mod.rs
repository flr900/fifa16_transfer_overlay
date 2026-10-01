//! `scout::screens` — UI do painel (ImGui). Uma tela por arquivo; aqui
//! fica a moldura comum: janela do painel, cabeçalho, barra de abas e os
//! estados vazios (State Patterns do EXPERIENCE.md).
//!
//! AD-1: as telas só falam com `scout::state`. Formatar valores para
//! exibição (milhar, data dd/mm/aaaa) é responsabilidade desta camada.

pub mod aviso;
mod confirmacao_contratacao;
mod missoes;
mod olheiros;
mod relatorios;
mod sonar;
pub mod theme;

use imgui::{Condition, FontId, StyleColor, StyleVar, Ui, WindowFlags};

use super::state::{CarreiraStatus, Especializacao, ScoutState, Tier};
use super::{Aba, Navigation, Satelite, ScoutScreen};
use crate::save_repo::Date;
use theme::Fonts;

/// Fração da resolução do jogo ocupada pelo painel (DESIGN.md: ~70% ×
/// ~75%, centralizado, com margem do jogo visível em volta).
const FRACAO_LARGURA: f32 = 0.70;
const FRACAO_ALTURA: f32 = 0.75;

const LARGURA_ABA: f32 = 150.0;

pub const MSG_SEM_CARREIRA: &str =
    "Nenhuma carreira carregada. Abra uma carreira no FIFA 16 para usar a Central de Scout.";
pub const MSG_ERRO_LEITURA: &str = "Não foi possível ler o save ativo.";
pub const MSG_LOCALIZANDO: &str = "Localizando carreira…";
const MSG_LOCALIZANDO_DETALHE: &str = "Isso leva cerca de 15 segundos na primeira abertura.";
const MSG_ABA_VAZIA: &str = "Esta aba ainda não tem conteúdo.";

/// Desenha o painel inteiro (só é chamada com o painel aberto).
pub fn render_painel(ui: &Ui, fonts: Option<&Fonts>, nav: &mut Navigation, state: &mut ScoutState) {
    let [largura_tela, altura_tela] = ui.io().display_size;
    let tamanho = [largura_tela * FRACAO_LARGURA, altura_tela * FRACAO_ALTURA];
    let posicao = [(largura_tela - tamanho[0]) * 0.5, (altura_tela - tamanho[1]) * 0.5];

    // A Confirmação de Contratação some se a contratação deixou de valer
    // (ex.: carreira saiu de "pronta" com o modal aberto).
    let mut confirmando = nav.tela_atual() == ScoutScreen::Satelite(Satelite::ConfirmacaoContratacao);
    if confirmando && state.previa_contratacao().is_none() {
        state.cancelar_contratacao();
        nav.pop();
        confirmando = false;
    }
    let mut pedido = None;

    ui.window("Central de Scout##painel")
        .position(posicao, Condition::Always)
        .size(tamanho, Condition::Always)
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_SAVED_SETTINGS
                | WindowFlags::NO_SCROLLBAR
                | WindowFlags::NO_SCROLL_WITH_MOUSE,
        )
        .build(|| {
            // Com o modal aberto o painel fica inerte e escurecido por baixo.
            let desabilitado = ui.begin_disabled(confirmando);
            cabecalho(ui, fonts, state.status());
            ui.dummy([0.0, theme::ESPACO_2]);
            barra_de_abas(ui, fonts, nav, state);
            ui.dummy([0.0, theme::ESPACO_4]);
            pedido = conteudo(ui, fonts, nav.aba_ativa(), state);
            drop(desabilitado);
            if confirmando {
                let canto = ui.window_pos();
                let [w, h] = ui.window_size();
                ui.get_window_draw_list()
                    .add_rect(canto, [canto[0] + w, canto[1] + h], theme::FUNDO_MODAL)
                    .filled(true)
                    .rounding(theme::RAIO_LG)
                    .build();
            }
        });

    if let Some((especializacao, tier)) = pedido {
        if !confirmando {
            state.preparar_contratacao(especializacao, tier);
            nav.push(Satelite::ConfirmacaoContratacao);
        }
    }
    if confirmando {
        match confirmacao_contratacao::render(ui, fonts, state) {
            confirmacao_contratacao::Acao::Nenhuma => {}
            confirmacao_contratacao::Acao::Contratou => nav.pop(),
            confirmacao_contratacao::Acao::Cancelou => {
                state.cancelar_contratacao();
                nav.pop();
            }
        }
    }
}

/// Borda roxa sólida no item anterior quando ele tem hover do mouse OU o
/// foco do controle/teclado: um só desenho para os dois (o `NavHighlight`
/// nativo do ImGui fica transparente no tema), então trocar de entrada
/// nunca muda o destaque nem perde a referência (UX-DR19, Story 1.6).
pub(super) fn contorno_hover(ui: &Ui, raio: f32) {
    let focado = ui.is_item_focused() && ui.io().nav_visible;
    if !(ui.is_item_hovered() || focado) {
        return;
    }
    let distancia = 3.0 + ESPESSURA_FOCO * 0.5;
    let [x0, y0] = ui.item_rect_min();
    let [x1, y1] = ui.item_rect_max();
    ui.get_window_draw_list()
        .add_rect([x0 - distancia, y0 - distancia], [x1 + distancia, y1 + distancia], theme::ACCENT_PRIMARY)
        .rounding(raio)
        .thickness(ESPESSURA_FOCO)
        .build();
}

/// Espessura do contorno de foco/hover (a mesma do `NavHighlight`).
pub(super) const ESPESSURA_FOCO: f32 = 2.0;

/// Filhos com `NavFlattened`: o D-pad passa da barra de abas para o
/// conteúdo (e volta) sem precisar "entrar" na janela filha com A. A flag
/// existe no ImGui 1.89 do hudhook, mas o `imgui-rs` 0.12 não a expõe.
fn flags_conteudo() -> WindowFlags {
    // SAFETY: bit válido do ImGui 1.89 (`ImGuiWindowFlags_NavFlattened`).
    unsafe { WindowFlags::from_bits_unchecked(imgui::sys::ImGuiWindowFlags_NavFlattened) }
}

/// Empilha uma fonte do tema (se as fontes já foram carregadas).
fn com_fonte<R>(ui: &Ui, fonte: Option<FontId>, f: impl FnOnce() -> R) -> R {
    let _token = fonte.map(|id| ui.push_font(id));
    f()
}

/// Título à esquerda; orçamento, data e técnico alinhados à direita.
fn cabecalho(ui: &Ui, fonts: Option<&Fonts>, status: &CarreiraStatus) {
    let inicio = ui.cursor_pos();
    com_fonte(ui, fonts.map(|f| f.display), || ui.text("Central de Scout"));
    let fim_titulo = ui.cursor_pos();

    if let CarreiraStatus::Pronta(carreira) = status {
        let rotulo = "Orçamento de Scouting";
        let valor = formatar_milhar(carreira.orcamento_transferencias);
        let linha2 = format!("{} · {}", formatar_data(carreira.data_atual), carreira.tecnico);

        let largura_rotulo = com_fonte(ui, fonts.map(|f| f.meta), || ui.calc_text_size(rotulo)[0]);
        let largura_valor = com_fonte(ui, fonts.map(|f| f.heading), || ui.calc_text_size(&valor)[0]);
        let largura_linha2 = com_fonte(ui, fonts.map(|f| f.meta), || ui.calc_text_size(&linha2)[0]);
        let largura_linha1 = largura_rotulo + theme::ESPACO_2 + largura_valor;
        let direita = inicio[0] + ui.content_region_avail()[0];

        ui.set_cursor_pos([direita - largura_linha1, inicio[1]]);
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, rotulo));
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        com_fonte(ui, fonts.map(|f| f.heading), || ui.text(&valor));

        ui.set_cursor_pos([direita - largura_linha2, ui.cursor_pos()[1]]);
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, &linha2));

        let y = fim_titulo[1].max(ui.cursor_pos()[1]);
        ui.set_cursor_pos([inicio[0], y]);
    }
    ui.separator();
}

/// 4 abas fixas: ativa em roxo sólido com texto escuro, inativas só com
/// texto secundário (DESIGN.md → tab-bar). Botões próprios em vez do
/// TabBar do ImGui porque ele não troca a cor do texto da aba ativa.
fn barra_de_abas(ui: &Ui, fonts: Option<&Fonts>, nav: &mut Navigation, state: &mut ScoutState) {
    com_fonte(ui, fonts.map(|f| f.heading), || {
        let _raio = ui.push_style_var(StyleVar::FrameRounding(theme::RAIO_MD));
        for (indice, aba) in Aba::TODAS.into_iter().enumerate() {
            if indice > 0 {
                ui.same_line_with_spacing(0.0, theme::ESPACO_2);
            }
            let ativa = nav.aba_ativa() == aba;
            let (fundo, destaque, texto) = if ativa {
                (theme::ACCENT_PRIMARY, theme::ACCENT_PRIMARY, theme::BG_BASE)
            } else {
                (theme::TRANSPARENTE, theme::ACCENT_PRIMARY_DIM, theme::TEXT_SECONDARY)
            };
            let _c1 = ui.push_style_color(StyleColor::Button, fundo);
            let _c2 = ui.push_style_color(StyleColor::ButtonHovered, destaque);
            let _c3 = ui.push_style_color(StyleColor::ButtonActive, destaque);
            let _c4 = ui.push_style_color(StyleColor::Text, texto);
            let rotulo = format!("{}##aba", aba.rotulo());
            let clicou = ui.button_with_size(rotulo, [LARGURA_ABA, theme::ALVO_MINIMO + theme::ESPACO_1]);
            contorno_hover(ui, theme::RAIO_MD);
            if clicou && !ativa {
                nav.trocar_aba(aba);
                state.definir_aba_ativa(aba);
            }
        }
    });
}

/// Área de conteúdo: um child window por aba, para cada aba guardar o
/// próprio scroll (o ImGui mantém o estado por ID mesmo sem desenhar).
fn conteudo(ui: &Ui, fonts: Option<&Fonts>, aba: Aba, state: &mut ScoutState) -> Option<(Especializacao, Tier)> {
    let status = state.status().clone();
    let id = format!("##conteudo_{:?}", aba);
    let mut pedido = None;
    ui.child_window(id).size([0.0, 0.0]).border(false).flags(flags_conteudo()).build(|| match status {
        CarreiraStatus::Localizando => {
            mensagem(ui, fonts, MSG_LOCALIZANDO);
            com_fonte(ui, fonts.map(|f| f.meta), || {
                ui.text_colored(theme::TEXT_SECONDARY, MSG_LOCALIZANDO_DETALHE)
            });
        }
        CarreiraStatus::SemCarreira => mensagem(ui, fonts, MSG_SEM_CARREIRA),
        CarreiraStatus::ErroLeitura => {
            mensagem(ui, fonts, MSG_ERRO_LEITURA);
            ui.dummy([0.0, theme::ESPACO_2]);
            if botao_primario(ui, "Tentar novamente") {
                state.tentar_novamente();
            }
        }
        CarreiraStatus::Pronta(_) => match aba {
            Aba::Olheiros => pedido = olheiros::render(ui, fonts, state),
            Aba::Missoes => missoes::render(ui, fonts),
            Aba::Relatorios => relatorios::render(ui, fonts),
            Aba::Sonar => sonar::render(ui, fonts),
        },
    });
    pedido
}

fn mensagem(ui: &Ui, fonts: Option<&Fonts>, texto: &str) {
    com_fonte(ui, fonts.map(|f| f.body), || ui.text_wrapped(texto));
}

/// Placeholder neutro das abas que ainda não têm conteúdo.
fn aba_vazia(ui: &Ui, fonts: Option<&Fonts>) {
    com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_ABA_VAZIA));
}

/// Botão primário (DESIGN.md → button-primary): verde-campo, texto escuro.
fn botao_primario(ui: &Ui, rotulo: &str) -> bool {
    let _c1 = ui.push_style_color(StyleColor::Button, theme::FIELD_GREEN);
    let _c2 = ui.push_style_color(StyleColor::ButtonHovered, theme::FIELD_GREEN);
    let _c3 = ui.push_style_color(StyleColor::ButtonActive, theme::FIELD_GREEN);
    let _c4 = ui.push_style_color(StyleColor::Text, theme::BG_BASE);
    let largura = ui.calc_text_size(rotulo)[0] + theme::ESPACO_5 * 2.0;
    let clicou = ui.button_with_size(rotulo, [largura, theme::ALVO_MINIMO]);
    contorno_hover(ui, theme::RAIO_PADRAO);
    clicou
}

/// `63999988` → `63.999.988` (separador de milhar brasileiro).
pub fn formatar_milhar(valor: i32) -> String {
    let digitos = valor.unsigned_abs().to_string();
    let mut grupos = Vec::new();
    let mut fim = digitos.len();
    while fim > 3 {
        grupos.push(digitos.get(fim - 3..fim).unwrap_or_default());
        fim -= 3;
    }
    grupos.push(digitos.get(..fim).unwrap_or_default());
    grupos.reverse();
    let sinal = if valor < 0 { "-" } else { "" };
    format!("{sinal}{}", grupos.join("."))
}

/// `Date(20260703)` → `03/07/2026`.
pub fn formatar_data(data: Date) -> String {
    format!("{:02}/{:02}/{:04}", data.day(), data.month(), data.year())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_use_dots() {
        assert_eq!(formatar_milhar(63_999_988), "63.999.988");
        assert_eq!(formatar_milhar(500_000), "500.000");
        assert_eq!(formatar_milhar(999), "999");
        assert_eq!(formatar_milhar(1_000), "1.000");
        assert_eq!(formatar_milhar(0), "0");
        assert_eq!(formatar_milhar(-2_100_000), "-2.100.000");
        assert_eq!(formatar_milhar(i32::MIN), "-2.147.483.648");
    }

    #[test]
    fn dates_are_day_month_year() {
        assert_eq!(formatar_data(Date(20260703)), "03/07/2026");
        assert_eq!(formatar_data(Date(20281231)), "31/12/2028");
    }

    #[test]
    fn empty_state_texts_match_the_acceptance_criteria() {
        assert_eq!(
            MSG_SEM_CARREIRA,
            "Nenhuma carreira carregada. Abra uma carreira no FIFA 16 para usar a Central de Scout."
        );
        assert_eq!(MSG_ERRO_LEITURA, "Não foi possível ler o save ativo.");
        assert_eq!(MSG_LOCALIZANDO, "Localizando carreira…");
    }
}
