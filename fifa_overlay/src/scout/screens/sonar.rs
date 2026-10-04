//! Aba Sonar (Épico 4, FR-11, UX-DR17): o cartograma da Story 2.9 em modo
//! visualização, cada país pintado pela cobertura das Missões (nunca
//! escaneado / Missão ativa / Missão concluída), com legenda em texto
//! (NFR5). Clicar num país (ou focá-lo com o controle e apertar A) mostra
//! o resumo dele nesta mesma aba, acima do mapa (Story 4.2). No controle,
//! foco = escolha: o resumo acompanha o quadro focado.

use imgui::Ui;

use super::cartograma::{self, EstadoQuadro};
use super::theme::{self, Fonts};
use super::{com_fonte, selecao_geografica};
use crate::scout::cobertura::{Contagem, EstadoPais};
use crate::scout::state::ScoutState;

pub const MSG_SEM_MISSOES: &str = "Nenhuma Missão criada ainda: todos os países aparecem como nunca escaneados.";
pub const MSG_ESCOLHA_PAIS: &str = "Clique num país para ver as Missões dele.";
pub const MSG_NUNCA_ESCANEADO: &str = "Nunca escaneado: nenhuma Missão neste país.";

const LEGENDA: [(EstadoQuadro, &str); 3] = [
    (EstadoQuadro::Livre, "Nunca escaneado"),
    (EstadoQuadro::MissaoAtiva, "Missão ativa"),
    (EstadoQuadro::MissaoConcluida, "Missão concluída"),
];

/// `1 Missão ativa · 2 Missões concluídas`.
pub fn texto_contagem(c: Contagem) -> String {
    let ativas = if c.ativas == 1 { "Missão ativa" } else { "Missões ativas" };
    let concluidas = if c.concluidas == 1 { "Missão concluída" } else { "Missões concluídas" };
    format!("{} {ativas} · {} {concluidas}", c.ativas, c.concluidas)
}

/// Linha das Missões sem filtro geográfico, que não pintam o mapa.
pub fn texto_globais(c: Contagem) -> String {
    let ativas = if c.ativas == 1 { "ativa" } else { "ativas" };
    let concluidas = if c.concluidas == 1 { "concluída" } else { "concluídas" };
    format!("Missões globais (todos os países): {} {ativas} · {} {concluidas}", c.ativas, c.concluidas)
}

fn quadro_de(estado: EstadoPais) -> EstadoQuadro {
    match estado {
        EstadoPais::NuncaEscaneado => EstadoQuadro::Livre,
        EstadoPais::MissaoAtiva => EstadoQuadro::MissaoAtiva,
        EstadoPais::MissaoConcluida => EstadoQuadro::MissaoConcluida,
    }
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) {
    let cobertura = state.cobertura().unwrap_or_default();
    let nacoes = state.nacoes();
    let meta = |cor, texto: &str| com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(cor, texto));

    com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Sonar de Cobertura"));
    meta(theme::TEXT_SECONDARY, if cobertura.total == 0 { MSG_SEM_MISSOES } else { MSG_ESCOLHA_PAIS });
    ui.dummy([0.0, theme::ESPACO_1]);

    // Legenda: amostra + nome de cada estado (cor nunca sozinha).
    let lado = com_fonte(ui, fonts.map(|f| f.meta), || ui.text_line_height());
    for (indice, (estado, nome)) in LEGENDA.into_iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        }
        cartograma::amostra(ui, estado, lado);
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        meta(theme::TEXT_PRIMARY, nome);
    }
    if !cobertura.globais.vazia() {
        meta(theme::TEXT_SECONDARY, &texto_globais(cobertura.globais));
    }

    // Resumo do país escolhido (fica acima do mapa, sem rolar com ele).
    if let (Some(pais), Some(nacoes)) = (state.pais_sonar(), &nacoes) {
        ui.dummy([0.0, theme::ESPACO_2]);
        com_fonte(ui, fonts.map(|f| f.body), || ui.text(selecao_geografica::nome_pais(pais, nacoes)));
        let contagem = cobertura.contagem(pais);
        if contagem.vazia() {
            meta(theme::TEXT_SECONDARY, MSG_NUNCA_ESCANEADO);
        } else {
            meta(theme::TEXT_SECONDARY, &texto_contagem(contagem));
        }
    }
    ui.dummy([0.0, theme::ESPACO_1]);

    let mut escolhido = None;
    match &nacoes {
        None => com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, "Carregando os países…")),
        Some(nacoes) => {
            let estado = |id: u16| quadro_de(cobertura.estado(id));
            ui.child_window("##sonar_mapa").size([0.0, 0.0]).border(false).flags(super::flags_conteudo()).build(|| {
                let resposta = cartograma::render(ui, fonts, nacoes, &estado);
                // foco = escolha (controle): andar pelo mapa já mostra o país
                escolhido = resposta.ativado.or(resposta.focado);
            });
        }
    }
    if let Some(pais) = escolhido {
        state.escolher_pais_sonar(pais);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_exact_and_pluralised() {
        assert_eq!(texto_contagem(Contagem { ativas: 1, concluidas: 2 }), "1 Missão ativa · 2 Missões concluídas");
        assert_eq!(texto_contagem(Contagem { ativas: 0, concluidas: 1 }), "0 Missões ativas · 1 Missão concluída");
        assert_eq!(
            texto_globais(Contagem { ativas: 1, concluidas: 0 }),
            "Missões globais (todos os países): 1 ativa · 0 concluídas"
        );
    }

    #[test]
    fn the_legend_names_the_three_states_in_text() {
        let nomes: Vec<&str> = LEGENDA.iter().map(|(_, n)| *n).collect();
        assert_eq!(nomes, ["Nunca escaneado", "Missão ativa", "Missão concluída"]);
        assert_eq!(quadro_de(EstadoPais::NuncaEscaneado), EstadoQuadro::Livre);
        assert_eq!(quadro_de(EstadoPais::MissaoAtiva), EstadoQuadro::MissaoAtiva);
        assert_eq!(quadro_de(EstadoPais::MissaoConcluida), EstadoQuadro::MissaoConcluida);
    }
}
