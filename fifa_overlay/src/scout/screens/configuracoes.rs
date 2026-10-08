//! Configurações do Scout (2026-10-08, pedido do Felipe): uma janela por cima
//! da tela, aberta com o Select (ou pelo botão da barra de abas), que junta
//! o que não precisa ficar no meio das abas:
//!
//! - **Sincronizar com o FIFA**: liga/desliga a escrita no scout do jogo (era
//!   um botão da aba Escolhidos);
//! - **Visão de cada aba**: Cards ou Tabular para Olheiros, Escolhidos, Base
//!   do Scout e o Relatório aberto. É a mesma visão que o botão da própria
//!   aba troca: a que ficou é a que abre da próxima vez.
//!
//! B ou Select fecham. Aqui nada muda só por receber o foco: tudo é A ou
//! clique.

use imgui::{Condition, StyleColor, StyleVar, Ui, WindowFlags};

use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, escolhidos};
use crate::scout::lista::ListaId;
use crate::scout::state::{Densidade, ScoutState};

const LARGURA_JANELA: f32 = 700.0;
const LARGURA_ROTULO: f32 = 190.0;
const LARGURA_OPCAO: f32 = 120.0;

/// O que o jogador fez na janela neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Fechar,
}

/// Abas que têm uma visão Cards/Tabular para escolher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visao {
    Olheiros,
    Lista(ListaId),
}

const VISOES: [(Visao, &str); 4] = [
    (Visao::Olheiros, "Olheiros"),
    (Visao::Lista(ListaId::Escolhidos), "Escolhidos"),
    (Visao::Lista(ListaId::Base), "Base do Scout"),
    (Visao::Lista(ListaId::RelatorioAberto), "Relatório aberto"),
];

/// A janela de Configurações, por cima da tela.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let mut acao = Acao::Nenhuma;
    let [largura_tela, altura_tela] = ui.io().display_size;

    let _fundo = ui.push_style_color(StyleColor::WindowBg, theme::BG_PANEL_RAISED);
    let _borda = ui.push_style_color(StyleColor::Border, theme::BORDER_HAIRLINE);
    let _raio = ui.push_style_var(StyleVar::WindowRounding(theme::RAIO_LG));
    let _padding = ui.push_style_var(StyleVar::WindowPadding([theme::ESPACO_5, theme::ESPACO_4]));
    ui.window("Configurações##configuracoes")
        .position([largura_tela * 0.5, altura_tela * 0.5], Condition::Always)
        .position_pivot([0.5, 0.5])
        .focused(true)
        .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_SAVED_SETTINGS | WindowFlags::ALWAYS_AUTO_RESIZE)
        .build(|| {
            ui.dummy([LARGURA_JANELA, 0.0]);
            com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Configurações"));
            com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, "B ou Select fecham."));
            ui.dummy([0.0, theme::ESPACO_3]);

            secao(ui, fonts, "Scout do jogo");
            sincronizacao(ui, fonts, state);
            ui.dummy([0.0, theme::ESPACO_3]);

            secao(ui, fonts, "Visão de cada aba");
            for (visao, nome) in VISOES {
                linha_de_visao(ui, fonts, state, visao, nome);
            }
            ui.dummy([0.0, theme::ESPACO_3]);

            if componentes::botao(ui, fonts, "Fechar", EstiloBotao::Primario, true) {
                acao = Acao::Fechar;
            }
            ui.set_item_default_focus();
        });
    acao
}

fn secao(ui: &Ui, fonts: Option<&Fonts>, titulo: &str) {
    com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::ACCENT_PRIMARY, titulo));
    ui.dummy([0.0, theme::ESPACO_1]);
}

/// Interruptor "Sincronizar com o FIFA", "Tentar de novo" e a situação.
fn sincronizacao(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) {
    let status = state.status_nativo();
    let (rotulo, estilo) = if status.ligada {
        ("Sincronizar com o FIFA: ligado", EstiloBotao::Selecionado)
    } else {
        ("Sincronizar com o FIFA: desligado", EstiloBotao::Secundario)
    };
    if componentes::botao(ui, fonts, rotulo, estilo, true) {
        state.alternar_sincronizacao_nativa();
    }
    if status.ligada && !status.localizando && !(status.escolhidos && status.conhecimento) {
        ui.same_line_with_spacing(0.0, theme::ESPACO_3);
        if componentes::botao(ui, fonts, "Tentar de novo", EstiloBotao::Secundario, true) {
            state.localizar_nativo_de_novo();
        }
    }
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.text_wrapped(escolhidos::texto_sincronizacao(&status));
    });
}

fn linha_de_visao(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, visao: Visao, nome: &str) {
    let _id = ui.push_id(nome);
    let atual = match visao {
        Visao::Olheiros => state.densidade_olheiros(),
        Visao::Lista(id) => state.modo_da_lista(id),
    };
    let inicio = ui.cursor_pos();
    com_fonte(ui, fonts.map(|f| f.body), || {
        let y = ui.cursor_pos()[1];
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text_colored(theme::TEXT_SECONDARY, nome);
    });
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    let escolha = componentes::alternador_por_clique(ui, fonts, &["Cards", "Tabular"], usize::from(atual == Densidade::Tabular), LARGURA_OPCAO);
    if let Some(indice) = escolha {
        let modo = if indice == 0 { Densidade::Cards } else { Densidade::Tabular };
        match visao {
            Visao::Olheiros => state.definir_densidade_olheiros(modo),
            Visao::Lista(id) => state.definir_modo_da_lista(id, modo),
        }
    }
}
