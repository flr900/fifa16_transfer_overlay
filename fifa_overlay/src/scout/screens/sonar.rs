//! Aba Sonar (Épico 4, FR-11, UX-DR17): o cartograma em modo visualização,
//! um quadro por país com liga na carreira (onde o filtro "onde o jogador
//! joga" procura), pintado pela cobertura das Missões (nunca escaneado /
//! Missão ativa / Missão concluída), com legenda em texto (NFR5). Missões
//! de continente inteiro e sem filtro geográfico ficam em linhas acima do
//! mapa (`scout::cobertura`).
//!
//! Clicar num país (ou focá-lo com o controle e apertar A) mostra o resumo
//! dele nesta mesma aba, acima do mapa (Story 4.2). No controle, foco =
//! escolha: o resumo acompanha o quadro focado.

use imgui::Ui;

use super::cartograma::{self, EstadoQuadro};
use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, selecao_geografica};
use crate::scout::cobertura::{Contagem, EstadoPais};
use crate::scout::search::NACAO_OUTROS;
use crate::scout::state::{Carga, Confederacao, Nacao, ScoutState};

pub const MSG_SEM_MISSOES: &str = "Nenhuma Missão criada ainda: todos os países aparecem como nunca escaneados.";
pub const MSG_ESCOLHA_PAIS: &str = "Clique num país para ver as Missões dele. Países pelas ligas da carreira.";
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

/// `1 ativa · 0 concluídas` (linhas de continente e globais).
fn texto_curto(c: Contagem) -> String {
    let ativas = if c.ativas == 1 { "ativa" } else { "ativas" };
    let concluidas = if c.concluidas == 1 { "concluída" } else { "concluídas" };
    format!("{} {ativas} · {} {concluidas}", c.ativas, c.concluidas)
}

/// Linha das Missões sem filtro geográfico, que não pintam o mapa.
pub fn texto_globais(c: Contagem) -> String {
    format!("Missões no mundo todo: {}", texto_curto(c))
}

/// Linha das Missões de continente inteiro: `Continentes inteiros —
/// Europa: 1 ativa · 0 concluídas; Ásia: …`.
pub fn texto_continentes(lista: &[(Confederacao, Contagem)]) -> String {
    let partes: Vec<String> = lista.iter().map(|(c, n)| format!("{}: {}", c.nome(), texto_curto(*n))).collect();
    format!("Continentes inteiros — {}", partes.join("; "))
}

fn quadro_de(estado: EstadoPais) -> EstadoQuadro {
    match estado {
        EstadoPais::NuncaEscaneado => EstadoQuadro::Livre,
        EstadoPais::MissaoAtiva => EstadoQuadro::MissaoAtiva,
        EstadoPais::MissaoConcluida => EstadoQuadro::MissaoConcluida,
    }
}

fn nome_quadro(id: u16, quadros: &[Nacao]) -> String {
    if id == NACAO_OUTROS {
        return "Outros".to_string();
    }
    quadros.iter().find(|q| q.id == id).map_or_else(|| format!("País {id}"), |q| q.nome.clone())
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) {
    let meta = |cor, texto: &str| com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(cor, texto));
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Sonar de Cobertura"));

    let ligas = match state.listar_ligas() {
        Carga::Pronto(ligas) => ligas,
        Carga::Carregando => {
            com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, selecao_geografica::MSG_LENDO));
            return;
        }
        Carga::Erro => {
            com_fonte(ui, fonts.map(|f| f.body), || ui.text(selecao_geografica::MSG_ERRO));
            if componentes::botao(ui, fonts, "Tentar novamente", EstiloBotao::Primario, true) {
                state.reler_ligas();
            }
            return;
        }
    };
    let cobertura = state.cobertura(&ligas).unwrap_or_default();
    // nações só para nomear países de Missões antigas fora das ligas
    let nacoes = state.nacoes().unwrap_or_default();
    let quadros = cobertura.quadros(&ligas, &nacoes);

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
    let continentes = cobertura.continentes();
    if !continentes.is_empty() {
        com_fonte(ui, fonts.map(|f| f.meta), || {
            let _cor = ui.push_style_color(imgui::StyleColor::Text, theme::TEXT_SECONDARY);
            ui.text_wrapped(texto_continentes(&continentes));
        });
    }

    // Resumo do país escolhido (fica acima do mapa, sem rolar com ele).
    let pais = state.pais_sonar();
    if let Some(pais) = pais {
        ui.dummy([0.0, theme::ESPACO_2]);
        com_fonte(ui, fonts.map(|f| f.body), || ui.text(nome_quadro(pais, &quadros)));
        let contagem = cobertura.contagem(pais);
        meta(theme::TEXT_SECONDARY, &if contagem.vazia() { MSG_NUNCA_ESCANEADO.to_string() } else { texto_contagem(contagem) });
        let continente = quadros.iter().find(|q| q.id == pais).map(|q| q.confederacao);
        if let Some(c) = continente.filter(|c| !cobertura.do_continente(*c).vazia()) {
            meta(theme::TEXT_SECONDARY, &format!("Mais, pelo continente inteiro ({}): {}", c.nome(), texto_curto(cobertura.do_continente(c))));
        }
    }
    ui.dummy([0.0, theme::ESPACO_1]);

    let mut escolhido = None;
    let estado = |id: u16| quadro_de(cobertura.estado(id));
    ui.child_window("##sonar_mapa").size([0.0, 0.0]).border(false).flags(super::flags_conteudo()).build(|| {
        let resposta = cartograma::render(ui, fonts, &quadros, cobertura.tem_outros(), pais, &estado);
        // foco = escolha (controle): andar pelo mapa já mostra o país
        escolhido = resposta.ativado.or(resposta.focado);
    });
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
        assert_eq!(texto_globais(Contagem { ativas: 1, concluidas: 0 }), "Missões no mundo todo: 1 ativa · 0 concluídas");
        assert_eq!(
            texto_continentes(&[(Confederacao::Europa, Contagem { ativas: 2, concluidas: 1 })]),
            "Continentes inteiros — Europa: 2 ativas · 1 concluída"
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
