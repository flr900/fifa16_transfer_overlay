//! Opções do Olheiro (2026-10-08, pedido do Felipe): o Y num Olheiro (ou o
//! botão direito do mouse) abre esta janela, por cima da aba, com o que dá
//! para fazer com ele agora:
//!
//! - **Nova Missão**, se ele está livre;
//! - **Cancelar a pesquisa em andamento** (pede confirmação: o que já
//!   apareceu fica no Relatório, o que foi pago não volta);
//! - **Ajustar o perfil do jogador** (Missão contínua): muda o perfil
//!   procurado na MESMA região, sem custo, a partir do próximo bloco;
//! - **Mudar de região** (Missão contínua): é uma Missão nova — nos 12
//!   primeiros meses do contrato, com a multa dele, e a Missão toda paga de
//!   novo;
//! - **Demitir** (só livre; pede confirmação na janela seguinte).
//!
//! B fecha (no passo de confirmação, volta ao menu). Desenhada como janela
//! própria por cima do painel, como os outros avisos.

use imgui::{Condition, StyleColor, StyleVar, Ui, WindowFlags};
use uuid::Uuid;

use super::componentes::{botao, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data, formatar_milhar, olheiros};
use crate::scout::state::{OlheiroContratado, PassoOpcoes, ScoutState, StatusMissao};

const LARGURA: f32 = 560.0;

/// O que o jogador escolheu neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Fechar,
    NovaMissao(Uuid),
    AjustarPerfil(Uuid),
    MudarRegiao(Uuid),
    Demitir(Uuid),
}

/// O que dá para fazer com o Olheiro agora (decide os botões).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Possibilidades {
    pub nova_missao: bool,
    pub cancelar: bool,
    pub ajustar_perfil: bool,
    pub mudar_regiao: bool,
    pub demitir: bool,
    /// A busca da Missão está rodando: nada de mexer até terminar.
    pub buscando: bool,
}

pub fn possibilidades(c: &OlheiroContratado) -> Possibilidades {
    let missao = c.missao.as_ref();
    let buscando = missao.is_some_and(|m| m.status == StatusMissao::EmExecucao);
    let parada = missao.is_some_and(|m| m.status == StatusMissao::Pendente);
    let continua_parada = parada && missao.is_some_and(|m| m.continua);
    Possibilidades {
        nova_missao: !c.ocupado(),
        cancelar: parada && !c.acompanhando,
        ajustar_perfil: continua_parada && c.rescisao.is_some(),
        mudar_regiao: continua_parada && c.rescisao.is_some(),
        demitir: !c.em_missao,
        buscando,
    }
}

/// O que o "Mudar de região" custa: a multa (na carência) e a Missão toda.
pub fn texto_mudar_regiao(c: &OlheiroContratado) -> String {
    match c.rescisao {
        Some(r) if r.multa > 0 => format!(
            "Abre uma Missão nova para ele. Dentro da carência (até {}), custa a multa de {} e a Missão é paga inteira de novo.",
            formatar_data(r.ate),
            formatar_milhar(r.multa)
        ),
        _ => "Abre uma Missão nova para ele, sem multa (fora da carência); a Missão é paga inteira de novo.".to_string(),
    }
}

/// A frase de confirmação do cancelamento.
pub fn texto_cancelamento(c: &OlheiroContratado) -> String {
    let nome = c.olheiro.nome_exibicao();
    let contrato = match &c.missao {
        Some(m) if m.tem_contrato() => format!(" O contrato (até {}) acaba junto, sem devolução.", formatar_data(m.fim_do_contrato())),
        _ => String::new(),
    };
    format!(
        "Cancelar a pesquisa de {nome}? Os jogadores que já apareceram ficam no Relatório; os que ainda não apareceram são descartados, e o que foi pago não volta.{contrato}"
    )
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some((c, passo)) = state.opcoes_do_olheiro() else {
        return Acao::Fechar;
    };
    let mut acao = Acao::Nenhuma;
    let id = c.olheiro.id;
    let pode = possibilidades(&c);
    let [largura_tela, altura_tela] = ui.io().display_size;

    let _fundo = ui.push_style_color(StyleColor::WindowBg, theme::BG_PANEL_RAISED);
    let _borda = ui.push_style_color(StyleColor::Border, theme::BORDER_HAIRLINE);
    let _raio = ui.push_style_var(StyleVar::WindowRounding(theme::RAIO_LG));
    let _padding = ui.push_style_var(StyleVar::WindowPadding([theme::ESPACO_5, theme::ESPACO_5]));
    ui.window("Opções do Olheiro##opcoes_olheiro")
        .position([largura_tela * 0.5, altura_tela * 0.5], Condition::Always)
        .position_pivot([0.5, 0.5])
        .focused(true)
        .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_SAVED_SETTINGS | WindowFlags::ALWAYS_AUTO_RESIZE)
        .build(|| {
            ui.dummy([LARGURA, 0.0]);
            olheiros::nome_com_badge(ui, fonts, &c.olheiro.nome_exibicao(), c.olheiro.tier);
            olheiros::com_nacoes(state, |nacoes| olheiros::origem_no_fluxo(ui, fonts, &c.olheiro, nacoes, true, 2));
            let regiao = c.missao.as_ref().map(|m| state.regiao_da_missao(&m.filtros)).unwrap_or_default();
            com_fonte(ui, fonts.map(|f| f.meta), || {
                for linha in olheiros::texto_situacao(&c, &regiao) {
                    ui.text_colored(theme::TEXT_SECONDARY, linha);
                }
            });
            ui.dummy([0.0, theme::ESPACO_3]);
            ui.separator();
            ui.dummy([0.0, theme::ESPACO_2]);

            match passo {
                PassoOpcoes::ConfirmarCancelamento => {
                    com_fonte(ui, fonts.map(|f| f.body), || {
                        let _quebra = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + LARGURA);
                        ui.text(texto_cancelamento(&c));
                    });
                    ui.dummy([0.0, theme::ESPACO_3]);
                    // o foco inicial no lado seguro: voltar
                    if botao(ui, fonts, "Voltar", EstiloBotao::Primario, true) {
                        state.definir_passo_das_opcoes(PassoOpcoes::Menu);
                    }
                    ui.set_item_default_focus();
                    ui.same_line_with_spacing(0.0, theme::ESPACO_3);
                    if botao(ui, fonts, "Confirmar cancelamento", EstiloBotao::Secundario, true) && state.cancelar_pesquisa(id) {
                        state.definir_passo_das_opcoes(PassoOpcoes::Menu);
                    }
                }
                PassoOpcoes::Menu => {
                    if pode.buscando {
                        com_fonte(ui, fonts.map(|f| f.meta), || {
                            ui.text_colored(theme::WARNING, "A busca desta Missão está rodando: espere o Relatório terminar para mexer nela.");
                        });
                        ui.dummy([0.0, theme::ESPACO_2]);
                    }
                    let mut primeiro = true;
                    let mut opcao = |ui: &Ui, rotulo: &str, dica: &str, habilitada: bool, estilo: EstiloBotao| -> bool {
                        let clicou = botao(ui, fonts, rotulo, estilo, habilitada);
                        if primeiro && habilitada {
                            ui.set_item_default_focus();
                            primeiro = false;
                        }
                        com_fonte(ui, fonts.map(|f| f.meta), || {
                            let _quebra = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + LARGURA);
                            ui.text_colored(theme::TEXT_SECONDARY, dica);
                        });
                        ui.dummy([0.0, theme::ESPACO_2]);
                        clicou && habilitada
                    };
                    if pode.nova_missao && opcao(ui, "Nova Missão", "Encomenda uma pesquisa com ele, já com os filtros ideais do foco dele.", true, EstiloBotao::Primario) {
                        acao = Acao::NovaMissao(id);
                    }
                    // o contrato da Missão contínua: a renovação e o aviso
                    if let Some(m) = c.missao.as_ref().filter(|m| m.continua) {
                        let custo = ScoutState::custo_do_contrato(m);
                        let vencido = state.contrato_vencido(m);
                        let falta = state.orcamento().map(|s| custo.saturating_sub(s)).filter(|f| *f > 0);
                        if let Some(aviso) = super::missoes::aviso_do_contrato(state, m) {
                            com_fonte(ui, fonts.map(|f| f.meta), || {
                                let _quebra = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + LARGURA);
                                ui.text_colored(theme::DANGER, aviso);
                            });
                            ui.dummy([0.0, theme::ESPACO_2]);
                        }
                        let rotulo = format!("Renovar o contrato por {}", formatar_milhar(custo));
                        let dica = super::missoes::texto_dica_do_contrato(m, custo);
                        if opcao(ui, &rotulo, &dica, vencido && falta.is_none() && pode.cancelar, EstiloBotao::Secundario) {
                            state.renovar_missao(m.id);
                        }
                        if m.tem_contrato() {
                            let estilo = if m.renovar_sozinho { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
                            let texto = if m.renovar_sozinho { "Renova sozinho: ligado" } else { "Renova sozinho: desligado" };
                            if opcao(ui, texto, "Liga ou desliga a renovação sozinha do contrato, no fim dos 12 meses, havendo verba.", true, estilo) {
                                state.definir_renovar_sozinho(m.id, !m.renovar_sozinho);
                            }
                        }
                    }
                    if c.em_missao {
                        if opcao(
                            ui,
                            "Cancelar a pesquisa em andamento",
                            "Libera o Olheiro agora. O que já apareceu fica no Relatório; o que foi pago não volta.",
                            pode.cancelar,
                            EstiloBotao::Secundario,
                        ) {
                            state.definir_passo_das_opcoes(PassoOpcoes::ConfirmarCancelamento);
                        }
                        if c.missao.as_ref().is_some_and(|m| m.continua) {
                            if opcao(
                                ui,
                                "Ajustar o perfil do jogador",
                                "Muda o perfil procurado na mesma região, sem custo. Vale a partir do próximo bloco de busca.",
                                pode.ajustar_perfil,
                                EstiloBotao::Secundario,
                            ) {
                                acao = Acao::AjustarPerfil(id);
                            }
                            if opcao(ui, "Mudar de região", &texto_mudar_regiao(&c), pode.mudar_regiao, EstiloBotao::Secundario) {
                                acao = Acao::MudarRegiao(id);
                            }
                        }
                    }
                    if c.acompanhando {
                        com_fonte(ui, fonts.map(|f| f.meta), || {
                            ui.text_colored(theme::TEXT_SECONDARY, "Acompanha os Escolhidos: libere-o na aba Escolhidos para dar outra tarefa.");
                        });
                        ui.dummy([0.0, theme::ESPACO_2]);
                    }
                    let dica_demitir = if pode.demitir {
                        "Tira o Olheiro da equipe. Pede confirmação; o que foi pago na contratação não volta."
                    } else {
                        "Cancele a pesquisa antes: um Olheiro em Missão não pode ser demitido."
                    };
                    if opcao(ui, "Demitir Olheiro", dica_demitir, pode.demitir, EstiloBotao::Secundario) {
                        acao = Acao::Demitir(id);
                    }
                    if botao(ui, fonts, "Fechar", EstiloBotao::Secundario, true) {
                        acao = Acao::Fechar;
                    }
                }
            }
        });
    acao
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{Especializacao, Missao, Olheiro, Rescisao, Tier};
    use crate::save_repo::Date;

    fn contratado(missao: Option<Missao>, rescisao: Option<Rescisao>, acompanhando: bool) -> OlheiroContratado {
        let olheiro = Olheiro { nome: "Zé".to_string(), especializacao: Especializacao::Generalista, tier: Tier::Experiente, ..Default::default() };
        OlheiroContratado { olheiro, em_missao: missao.is_some(), acompanhando, missao, relatorio_atual: None, relatorios: 0, rescisao }
    }

    #[test]
    fn what_can_be_done_depends_on_what_the_olheiro_is_doing() {
        let livre = possibilidades(&contratado(None, None, false));
        assert!(livre.nova_missao && livre.demitir && !livre.cancelar && !livre.ajustar_perfil && !livre.buscando);

        let fixa = Missao::de_teste(Uuid::nil(), StatusMissao::Pendente);
        let em_fixa = possibilidades(&contratado(Some(fixa), None, false));
        assert!(em_fixa.cancelar && !em_fixa.nova_missao && !em_fixa.demitir, "em Missão: cancelar, sem demitir");
        assert!(!em_fixa.ajustar_perfil && !em_fixa.mudar_regiao, "só a contínua ajusta e muda de região");

        let mut continua = Missao::de_teste(Uuid::nil(), StatusMissao::Pendente);
        continua.continua = true;
        continua.contratos = vec![Date(20260701)];
        let rescisao = Rescisao { missao: continua.id, ate: Date(20270701), multa: 90_000, vencido: false };
        let em_continua = possibilidades(&contratado(Some(continua.clone()), Some(rescisao), false));
        assert!(em_continua.cancelar && em_continua.ajustar_perfil && em_continua.mudar_regiao && !em_continua.demitir);

        let buscando = Missao { status: StatusMissao::EmExecucao, ..continua };
        let p = possibilidades(&contratado(Some(buscando), None, false));
        assert!(p.buscando && !p.cancelar && !p.ajustar_perfil, "com a busca rodando, nada");
    }

    #[test]
    fn the_texts_say_what_it_costs() {
        let rescisao = Rescisao { missao: Uuid::nil(), ate: Date(20270701), multa: 90_000, vencido: false };
        let mut m = Missao::de_teste(Uuid::nil(), StatusMissao::Pendente);
        m.continua = true;
        m.contratos = vec![Date(20260701)];
        let c = contratado(Some(m), Some(rescisao), false);
        let texto = texto_mudar_regiao(&c);
        assert!(texto.contains("carência (até 01/07/2027)") && texto.contains("90.000") && texto.contains("paga inteira de novo"), "{texto}");
        let fora = contratado(None, Some(Rescisao { multa: 0, ..rescisao }), false);
        assert!(texto_mudar_regiao(&fora).contains("sem multa"));
        let cancelamento = texto_cancelamento(&c);
        assert!(cancelamento.contains("Zé") && cancelamento.contains("o que foi pago não volta") && cancelamento.contains("contrato (até 01/07/2027)"), "{cancelamento}");
        assert!(!cancelamento.contains('!'));
    }
}
