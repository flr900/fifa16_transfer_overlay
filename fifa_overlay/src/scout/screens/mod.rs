//! `scout::screens` — UI do painel (ImGui). Uma tela por arquivo; aqui
//! fica a moldura comum: janela do painel, cabeçalho, barra de abas e os
//! estados vazios (State Patterns do EXPERIENCE.md).
//!
//! AD-1: as telas só falam com `scout::state`. Formatar valores para
//! exibição (milhar, data dd/mm/aaaa) é responsabilidade desta camada.

pub mod aviso;
mod componentes;
mod confirmacao_contratacao;
mod escolher_olheiro;
mod missoes;
mod campo_atributo;
mod campo_fit;
mod cartograma;
mod ficha_jogador;
mod nova_missao;
mod olheiros;
mod radar;
mod relatorio;
mod relatorios;
mod selecao_geografica;
mod seletor_elenco;
mod sonar;
pub mod theme;

use imgui::{Condition, FontId, StyleColor, StyleVar, Ui, WindowFlags};

use super::state::{CarreiraStatus, DestinoOlheiro, Especializacao, ScoutState, Tier};
use super::{Aba, ContextoSeletor, Navigation, Satelite, ScoutScreen};
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
    // Por baixo do modal aparece a tela de onde ele veio (as ofertas).
    if confirmando && state.previa_contratacao().is_none() {
        state.cancelar_contratacao();
        nav.pop();
        confirmando = false;
    }
    // O formulário Nova Missão só existe com a tela dele na pilha (trocar de
    // aba descarta o rascunho; um painel de campo fica por cima dele) e
    // fecha se a carreira deixar de estar pronta.
    let na_nova_missao = nav.contem(Satelite::NovaMissao);
    if na_nova_missao && state.previa_missao().is_none() {
        state.cancelar_nova_missao();
        nav.reset_para_aba();
    } else if !na_nova_missao && state.tem_nova_missao() {
        state.cancelar_nova_missao();
    }
    // A tela do Relatório fecha se ele deixou de existir; a Ficha, se o
    // jogador saiu do Relatório (ou o Relatório fechou).
    if nav.tela_atual() == ScoutScreen::Satelite(Satelite::Relatorio) && state.relatorio_aberto().is_none() {
        nav.pop();
    }
    if nav.tela_atual() == ScoutScreen::Satelite(Satelite::FichaJogador) && state.ficha_aberta().is_none() {
        state.fechar_ficha();
        nav.pop();
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
            // Foco no primeiro item da tela nova (só com conteúdo de verdade
            // na tela; com "Localizando…" o pedido espera).
            let com_conteudo = matches!(state.status(), CarreiraStatus::Pronta(_) | CarreiraStatus::ErroLeitura);
            let focar = !confirmando && com_conteudo && nav.tomar_foco_pendente();
            let tela = if confirmando { nav.tela_abaixo() } else { nav.tela_atual() };
            pedido = conteudo(ui, fonts, nav.aba_ativa(), tela, state, focar);
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

    match pedido {
        Some(Pedido::Contratar(especializacao, tier)) if !confirmando => {
            state.preparar_contratacao(especializacao, tier);
            nav.push(Satelite::ConfirmacaoContratacao);
        }
        Some(Pedido::EscolherOlheiro) => {
            nav.push(Satelite::EscolherOlheiro);
        }
        Some(Pedido::AbrirNovaMissao(olheiro)) => {
            state.abrir_nova_missao(olheiro);
            if state.tem_nova_missao() {
                nav.push(Satelite::NovaMissao);
            }
        }
        Some(Pedido::FecharNovaMissao) => {
            state.cancelar_nova_missao();
            nav.reset_para_aba();
        }
        Some(Pedido::MissaoEncomendada) => {
            // a Missão nova aparece na aba Missões
            state.cancelar_nova_missao();
            nav.trocar_aba(Aba::Missoes);
            state.definir_aba_ativa(Aba::Missoes);
        }
        Some(Pedido::AbrirContratacao) => {
            nav.push(Satelite::ContratarOlheiro);
        }
        Some(Pedido::AtivarOlheiro(id)) => match state.destino_do_olheiro(id) {
            Some(DestinoOlheiro::NovaMissao(olheiro)) => {
                state.abrir_nova_missao(olheiro);
                if state.tem_nova_missao() {
                    nav.push(Satelite::NovaMissao);
                }
            }
            Some(DestinoOlheiro::Relatorio(relatorio)) => {
                state.abrir_relatorio(relatorio);
                if state.relatorio_aberto().is_some() {
                    nav.push(Satelite::Relatorio);
                }
            }
            Some(DestinoOlheiro::Missoes) => {
                nav.trocar_aba(Aba::Missoes);
                state.definir_aba_ativa(Aba::Missoes);
            }
            None => {}
        },
        Some(Pedido::AbrirCampo(satelite)) => {
            if satelite == Satelite::SelecaoGeografica {
                state.focar_geografia(super::state::FocoGeografico::Continentes);
            }
            nav.push(satelite);
        }
        Some(Pedido::FecharCampo) => nav.pop(),
        Some(Pedido::AbrirRelatorio(id)) => {
            state.abrir_relatorio(id);
            if state.relatorio_aberto().is_some() {
                nav.push(Satelite::Relatorio);
            }
        }
        Some(Pedido::FecharRelatorio) => {
            state.fechar_relatorio();
            nav.pop();
        }
        Some(Pedido::AbrirFicha(player_id)) => {
            state.abrir_ficha(player_id);
            if state.ficha_aberta().is_some() {
                nav.push(Satelite::FichaJogador);
            }
        }
        Some(Pedido::FecharFicha) => {
            state.fechar_ficha();
            nav.pop();
        }
        Some(Pedido::ArquivouDaFicha) => {
            state.fechar_relatorio();
            nav.reset_para_aba();
        }
        _ => {}
    }
    if confirmando {
        match confirmacao_contratacao::render(ui, fonts, state) {
            confirmacao_contratacao::Acao::Nenhuma => {}
            // contratado: volta à lista de Olheiros, com o novo nela
            confirmacao_contratacao::Acao::Contratou => nav.reset_para_aba(),
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
///
/// A barra fica numa janela filha SEM navegação (`NO_NAV`): o controle
/// troca de aba só com LB/RB e o foco nunca "sobe" para a barra — o mouse
/// continua clicando nela normalmente.
fn barra_de_abas(ui: &Ui, fonts: Option<&Fonts>, nav: &mut Navigation, state: &mut ScoutState) {
    let altura = theme::ALVO_MINIMO + theme::ESPACO_1 + 6.0;
    ui.child_window("##barra_de_abas")
        .size([0.0, altura])
        .border(false)
        .flags(WindowFlags::NO_NAV | WindowFlags::NO_SCROLLBAR | WindowFlags::NO_SCROLL_WITH_MOUSE)
        .build(|| botoes_das_abas(ui, fonts, nav, state));
}

fn botoes_das_abas(ui: &Ui, fonts: Option<&Fonts>, nav: &mut Navigation, state: &mut ScoutState) {
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
/// O que o conteúdo pediu para a navegação neste frame.
enum Pedido {
    Contratar(Especializacao, Tier),
    /// "Contratar Olheiro" na aba Olheiros: abre as ofertas.
    AbrirContratacao,
    /// Clique num Olheiro contratado (ver `DestinoOlheiro`).
    AtivarOlheiro(uuid::Uuid),
    /// "Nova Missão" na aba Missões: passo 1, escolher o Olheiro.
    EscolherOlheiro,
    /// Abre o formulário com este Olheiro.
    AbrirNovaMissao(uuid::Uuid),
    /// Cancelou o formulário: volta para a aba.
    FecharNovaMissao,
    /// Confirmou: vai para a aba Missões.
    MissaoEncomendada,
    AbrirRelatorio(uuid::Uuid),
    FecharRelatorio,
    /// Ficha de um jogador do Relatório aberto (Story 3.1).
    AbrirFicha(u32),
    FecharFicha,
    /// Arquivou o Relatório pela Ficha: volta para a aba.
    ArquivouDaFicha,
    /// Abre um painel de campo do formulário (Story 2.8/2.9) ou o seletor
    /// de elenco (Épico 3).
    AbrirCampo(Satelite),
    /// Escolheu (ou voltou) no painel de campo ou no seletor: volta à tela
    /// de baixo.
    FecharCampo,
}

fn conteudo(ui: &Ui, fonts: Option<&Fonts>, aba: Aba, tela: ScoutScreen, state: &mut ScoutState, focar: bool) -> Option<Pedido> {
    let status = state.status().clone();
    let id = format!("##conteudo_{:?}", aba);
    let mut pedido = None;
    ui.child_window(id).size([0.0, 0.0]).border(false).flags(flags_conteudo()).build(|| {
        if focar {
            // O próximo item navegável desta tela recebe o foco (botões não
            // são "clicados": só focados).
            unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
        }
        conteudo_da_tela(ui, fonts, aba, tela, state, &status, &mut pedido, focar);
    });
    pedido
}

#[allow(clippy::too_many_arguments)]
fn conteudo_da_tela(
    ui: &Ui,
    fonts: Option<&Fonts>,
    aba: Aba,
    tela: ScoutScreen,
    state: &mut ScoutState,
    status: &CarreiraStatus,
    pedido: &mut Option<Pedido>,
    focar: bool,
) {
    match status.clone() {
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
            if componentes::botao(ui, fonts, "Tentar novamente", componentes::EstiloBotao::Primario, true) {
                state.tentar_novamente();
            }
            // A lista de Missões não some com o erro (Story 2.3): aparece
            // sem progresso e sem o botão "Nova Missão".
            if aba == Aba::Missoes {
                ui.dummy([0.0, theme::ESPACO_4]);
                let _ = missoes::render(ui, fonts, state, false);
            }
        }
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::NovaMissao) => {
            match nova_missao::render(ui, fonts, state) {
                nova_missao::Acao::Nenhuma => {}
                nova_missao::Acao::AbrirCampo(satelite) => *pedido = Some(Pedido::AbrirCampo(satelite)),
                nova_missao::Acao::Confirmou => *pedido = Some(Pedido::MissaoEncomendada),
                nova_missao::Acao::Cancelou => *pedido = Some(Pedido::FecharNovaMissao),
            }
        }
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::EscolherOlheiro) => {
            match escolher_olheiro::render(ui, fonts, state) {
                escolher_olheiro::Acao::Voltar => *pedido = Some(Pedido::FecharCampo),
                escolher_olheiro::Acao::Escolheu(id) => *pedido = Some(Pedido::AbrirNovaMissao(id)),
                escolher_olheiro::Acao::Nenhuma => {}
            }
        }
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::ContratarOlheiro) => {
            match olheiros::render_contratacao(ui, fonts, state) {
                olheiros::AcaoContratacao::Voltar => *pedido = Some(Pedido::FecharCampo),
                olheiros::AcaoContratacao::Contratar(e, t) => *pedido = Some(Pedido::Contratar(e, t)),
                olheiros::AcaoContratacao::Nenhuma => {}
            }
        }
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::SelecaoGeografica) => {
            if selecao_geografica::render(ui, fonts, state) {
                *pedido = Some(Pedido::FecharCampo);
            }
        }
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::CampoAtributo) => {
            if campo_atributo::render(ui, fonts, state, focar) {
                *pedido = Some(Pedido::FecharCampo);
            }
        }
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::CampoFit) => {
            if campo_fit::render(ui, fonts, state, focar) {
                *pedido = Some(Pedido::FecharCampo);
            }
        }
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::Relatorio) => match relatorio::render(ui, fonts, state) {
            relatorio::Acao::Voltar => *pedido = Some(Pedido::FecharRelatorio),
            relatorio::Acao::AbrirFicha(player_id) => *pedido = Some(Pedido::AbrirFicha(player_id)),
            relatorio::Acao::Nenhuma => {}
        },
        CarreiraStatus::Pronta(_) if tela == ScoutScreen::Satelite(Satelite::FichaJogador) => match ficha_jogador::render(ui, fonts, state) {
            ficha_jogador::Acao::Voltar => *pedido = Some(Pedido::FecharFicha),
            ficha_jogador::Acao::Comparar => {
                *pedido = Some(Pedido::AbrirCampo(Satelite::SeletorElenco(ContextoSeletor::ComparacaoFicha)));
            }
            ficha_jogador::Acao::Arquivou => *pedido = Some(Pedido::ArquivouDaFicha),
            ficha_jogador::Acao::Nenhuma => {}
        },
        CarreiraStatus::Pronta(_) if matches!(tela, ScoutScreen::Satelite(Satelite::SeletorElenco(_))) => {
            let contexto = match tela {
                ScoutScreen::Satelite(Satelite::SeletorElenco(c)) => c,
                _ => ContextoSeletor::ComparacaoFicha,
            };
            if seletor_elenco::render(ui, fonts, state, contexto, focar) {
                *pedido = Some(Pedido::FecharCampo);
            }
        }
        CarreiraStatus::Pronta(_) => match aba {
            Aba::Olheiros => {
                *pedido = match olheiros::render(ui, fonts, state) {
                    olheiros::Acao::AbrirContratacao => Some(Pedido::AbrirContratacao),
                    olheiros::Acao::Ativar(id) => Some(Pedido::AtivarOlheiro(id)),
                    olheiros::Acao::Nenhuma => None,
                };
            }
            Aba::Missoes => match missoes::render(ui, fonts, state, true) {
                missoes::Acao::NovaMissao => *pedido = Some(Pedido::EscolherOlheiro),
                missoes::Acao::AbrirRelatorio(id) => *pedido = Some(Pedido::AbrirRelatorio(id)),
                missoes::Acao::Nenhuma => {}
            },
            Aba::Relatorios => {
                if let Some(id) = relatorios::render(ui, fonts, state) {
                    *pedido = Some(Pedido::AbrirRelatorio(id));
                }
            }
            Aba::Sonar => sonar::render(ui, fonts),
        },
    }
}

fn mensagem(ui: &Ui, fonts: Option<&Fonts>, texto: &str) {
    com_fonte(ui, fonts.map(|f| f.body), || ui.text_wrapped(texto));
}

/// Placeholder neutro das abas que ainda não têm conteúdo.
fn aba_vazia(ui: &Ui, fonts: Option<&Fonts>) {
    com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_ABA_VAZIA));
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
