//! Aba Relatórios (Story 2.5; refeita em 2026-10-08): dois jeitos de ver o
//! que os Olheiros trouxeram.
//!
//! - **Por jogador** (padrão): um registro por jogador de cada Relatório, com
//!   o nome do Olheiro que o encontrou, a Missão e onde ela procurou — para
//!   não perder de vista que jogador veio de qual Relatório. Cards ou
//!   Tabular, com a barra de filtros e posição de `lista_jogadores`.
//!   Ativar um jogador abre a Ficha dele no Relatório de origem.
//! - **Por Relatório**: um card por Relatório, do mais novo para o mais
//!   antigo, com a Missão, o Olheiro, a Qualidade e o indicador "novo" até
//!   ser aberto pela primeira vez. Ativar o card abre o Relatório.
//!
//! "Ativos / Arquivados" vale para os dois.

use std::collections::HashMap;

use imgui::Ui;
use uuid::Uuid;

use super::componentes::{self, badge_novo, badge_qualidade, badge_tier, card_com_largura, desenhar_badge, texto_em, EstiloBotao};
use super::lista_jogadores;
use super::relatorio::{self, PerfilPedido};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::scout::lista::{ItemLista, ListaId};
use crate::scout::state::{Densidade, Ocorrencia, RelatorioNaLista, ScoutState, VisaoRelatorios};

const ALTURA_CARD: f32 = 76.0;
/// Coluna do botão Arquivar/Restaurar à direita de cada card (Story 2.7).
const LARGURA_ACAO: f32 = 130.0;
const LARGURA_VISAO: f32 = 150.0;

pub const MSG_SEM_ARQUIVADOS: &str = "Nenhum Relatório arquivado.";

pub const MSG_SEM_RELATORIOS: &str =
    "Nenhum Relatório ainda. Quando o prazo de uma Missão passar, o Relatório aparece aqui.";

/// O que o jogador fez na aba neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    /// Abrir o Relatório (ativou um card, na visão por Relatório).
    AbrirRelatorio(Uuid),
    /// Abrir a Ficha de um jogador no Relatório de onde ele veio.
    AbrirJogador { relatorio: Uuid, player_id: u32 },
}

/// Linha de detalhes do card: "17 jogadores · gerado em 03/07/2026 · Rápida"
/// (ou "parcial: 9 de 17 jogadores" com a Missão ainda rodando).
pub fn detalhe_card(item: &RelatorioNaLista) -> String {
    let n = item.relatorio.jogadores.len();
    let mut partes = vec![if item.parcial {
        format!("parcial: {n} de {} jogadores", item.previstos)
    } else {
        super::aviso::texto_jogadores(n)
    }];
    if let Some(data) = item.relatorio.gerado_em {
        partes.push(format!("gerado em {}", formatar_data(data)));
    }
    if let Some(m) = &item.missao {
        partes.push(super::missoes::nome_modo(m.modo_busca).to_string());
    }
    partes.join(" · ")
}

/// Desenha a aba.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    // visão (por jogador / por Relatório) e filtro Ativos / Arquivados (Story 2.7)
    let visao = state.visao_dos_relatorios();
    let atual = usize::from(visao == VisaoRelatorios::PorRelatorio);
    match componentes::alternador(ui, fonts, &["Por jogador", "Por Relatório"], atual, LARGURA_VISAO) {
        Some(0) => state.definir_visao_dos_relatorios(VisaoRelatorios::PorJogador),
        Some(_) => state.definir_visao_dos_relatorios(VisaoRelatorios::PorRelatorio),
        None => {}
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_5);
    let arquivados = state.vendo_arquivados();
    if let Some(indice) = componentes::alternador(ui, fonts, &["Ativos", "Arquivados"], usize::from(arquivados), 130.0) {
        state.ver_arquivados(indice == 1);
    }
    ui.dummy([0.0, theme::ESPACO_2]);
    let arquivados = state.vendo_arquivados();
    match state.visao_dos_relatorios() {
        VisaoRelatorios::PorJogador => por_jogador(ui, fonts, state, arquivados),
        VisaoRelatorios::PorRelatorio => por_relatorio(ui, fonts, state, arquivados),
    }
}

fn mensagem_vazia(arquivados: bool) -> &'static str {
    if arquivados {
        MSG_SEM_ARQUIVADOS
    } else {
        MSG_SEM_RELATORIOS
    }
}

fn por_jogador(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, arquivados: bool) -> Acao {
    let ocorrencias = state.ocorrencias(arquivados);
    if ocorrencias.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, mensagem_vazia(arquivados)));
        return Acao::Nenhuma;
    }
    let registros: Vec<(&Ocorrencia, String)> = ocorrencias.iter().map(|o| (o, o.origem_completa())).collect();
    match ocorrencias_na_tela(ui, fonts, state, ListaId::Relatorios, &registros) {
        Some((relatorio, player_id)) => Acao::AbrirJogador { relatorio, player_id },
        None => Acao::Nenhuma,
    }
}

/// Uma lista de jogadores com a origem de cada um (Relatórios por jogador e
/// Base do Scout): barra, filtros, e os cards em grade ou a tabela. Devolve
/// `(Relatório, jogador)` do registro ativado.
pub(super) fn ocorrencias_na_tela(
    ui: &Ui,
    fonts: Option<&Fonts>,
    state: &mut ScoutState,
    id: ListaId,
    registros: &[(&Ocorrencia, String)],
) -> Option<(Uuid, u32)> {
    let por_chave: HashMap<u64, (&Ocorrencia, &str)> = registros.iter().map(|(o, origem)| (o.chave(), (*o, origem.as_str()))).collect();
    let itens: Vec<ItemLista<'_>> =
        registros.iter().map(|(o, origem)| ItemLista::novo(&o.jogador, origem.clone()).com_chave(o.chave())).collect();
    lista_jogadores::barra(ui, fonts, state, id, &itens);
    let visiveis = lista_jogadores::preparar(state, id, itens);
    if visiveis.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, lista_jogadores::MSG_NENHUM_NO_FILTRO));
        return None;
    }
    let ativado = if state.modo_da_lista(id) == Densidade::Tabular {
        lista_jogadores::tabela(ui, fonts, state, id, &visiveis)
    } else {
        let hoje = state.data_da_carreira();
        let estado: &ScoutState = state;
        lista_jogadores::grade(ui, relatorio::LARGURA_CARD, &visiveis, |item| {
            let Some((o, origem)) = por_chave.get(&item.chave) else {
                return false;
            };
            let perfil = PerfilPedido { alvo: o.fit_alvo, referencia: o.referencia.clone(), aproximado: relatorio::aproximado(o.qualidade) };
            relatorio::card_jogador(ui, fonts, estado, &format!("{id:?}_{}", item.chave), &o.jogador, &perfil, hoje, Some(origem))
        })
    };
    let item = ativado.and_then(|i| visiveis.get(i))?;
    let (o, _) = por_chave.get(&item.chave)?;
    Some((o.relatorio_id, o.jogador.player_id))
}

fn por_relatorio(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, arquivados: bool) -> Acao {
    let lista = state.relatorios(arquivados);
    if lista.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, mensagem_vazia(arquivados)));
        return Acao::Nenhuma;
    }
    let mut acao = Acao::Nenhuma;
    for item in &lista {
        if card_relatorio(ui, fonts, item) {
            acao = Acao::AbrirRelatorio(item.relatorio.id);
        }
        // Arquivar só para Relatório já aberto; Restaurar no filtro.
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        let y = ui.cursor_pos()[1];
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (ALTURA_CARD - theme::ALVO_MINIMO) * 0.5]);
        let _id = ui.push_id(item.relatorio.id.to_string());
        if arquivados {
            if componentes::botao_com_largura(ui, fonts, "Restaurar", EstiloBotao::Secundario, true, Some(LARGURA_ACAO)) {
                state.restaurar_relatorio(item.relatorio.id);
            }
        } else if ScoutState::pode_arquivar(item) {
            if componentes::botao_com_largura(ui, fonts, "Arquivar", EstiloBotao::Secundario, true, Some(LARGURA_ACAO)) {
                state.arquivar_relatorio(item.relatorio.id);
            }
        } else {
            ui.dummy([LARGURA_ACAO, theme::ALVO_MINIMO]);
        }
        ui.set_cursor_pos([ui.cursor_pos()[0], y + ALTURA_CARD + theme::ESPACO_2]);
    }
    acao
}

/// Card de um Relatório; `true` = ativado (clique ou A).
pub fn card_relatorio(ui: &Ui, fonts: Option<&Fonts>, item: &RelatorioNaLista) -> bool {
    let largura = (ui.content_region_avail()[0] - LARGURA_ACAO - theme::ESPACO_2).max(200.0);
    let c = card_com_largura(ui, &item.relatorio.id.to_string(), largura, ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let dl = ui.get_window_draw_list();
    let x = c.min[0] + theme::ESPACO_4;
    let mut y = c.min[1] + theme::ESPACO_3;

    let titulo = super::relatorio::titulo(item);
    let [largura, altura] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &titulo);
    let mut xb = x + largura + theme::ESPACO_2;
    if let Some(o) = &item.olheiro {
        xb += desenhar_badge(ui, fonts, &dl, &badge_tier(o.tier), [xb, y], altura)[0] + theme::ESPACO_2;
    }
    if item.novo {
        desenhar_badge(ui, fonts, &dl, &badge_novo(), [xb, y], altura);
    }
    let qualidade = badge_qualidade(item.relatorio.qualidade);
    let largura_badge =
        com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(qualidade.texto)[0]) + theme::ESPACO_2 * 2.0;
    desenhar_badge(ui, fonts, &dl, &qualidade, [c.max[0] - theme::ESPACO_4 - largura_badge, y], altura);
    y += altura + theme::ESPACO_1;

    let olheiro = item.olheiro.as_ref().map_or_else(|| "Olheiro removido".to_string(), |o| o.nome_exibicao());
    let linha2 = format!("{olheiro} · {}", detalhe_card(item));
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::TEXT_SECONDARY, &linha2);
    c.ativou
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_repo::Date;
    use crate::scout::state::{Missao, Relatorio, StatusMissao};

    #[test]
    fn card_details_and_empty_state() {
        let missao = Missao::de_teste(Uuid::new_v4(), StatusMissao::Concluida);
        let mut relatorio = Relatorio::de_teste(missao.id);
        relatorio.gerado_em = Some(Date(20260703));
        let item = RelatorioNaLista { relatorio, missao: Some(missao), olheiro: None, previstos: 17, parcial: false, novo: false };
        assert_eq!(detalhe_card(&item), "nenhum jogador · gerado em 03/07/2026 · Rápida");
        let parcial = RelatorioNaLista { parcial: true, ..item };
        assert_eq!(detalhe_card(&parcial), "parcial: 0 de 17 jogadores · gerado em 03/07/2026 · Rápida");
        assert!(!MSG_SEM_RELATORIOS.contains('!'));
        assert_eq!(MSG_SEM_ARQUIVADOS, "Nenhum Relatório arquivado.");
        assert_eq!(mensagem_vazia(true), MSG_SEM_ARQUIVADOS);
    }
}
