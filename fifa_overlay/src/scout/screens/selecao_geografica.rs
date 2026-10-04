//! Painel de Seleção Geográfica (Story 2.9, UX-DR12): tela cheia sobre o
//! formulário Nova Missão, com o cartograma em modo seleção. Cada clique
//! (ou A no controle) põe/tira o país da seleção, sem tecla modificadora;
//! "Confirmar" ou B volta ao formulário com a seleção resumida na linha.
//! O resumo do rodapé (Qualidade, precisão, custo, prazo) muda ao vivo:
//! quanto mais amplo, menos preciso (FR4).

use imgui::{StyleColor, Ui};

use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{cartograma, com_fonte, nova_missao};
use crate::scout::quality::AmplitudeGeografica;
use crate::scout::search::NACAO_OUTROS;
use crate::scout::state::{Nacao, ScoutState};

pub const MSG_TODOS: &str = "Nenhum país escolhido: o Olheiro procura em todos os países.";

/// Nome da amplitude para o jogador.
pub fn nome_amplitude(amplitude: AmplitudeGeografica) -> &'static str {
    match amplitude {
        AmplitudeGeografica::Pais => "um país",
        AmplitudeGeografica::VariosPaises => "vários países",
        AmplitudeGeografica::Continente => "um continente inteiro",
        AmplitudeGeografica::Mundo => "mundo",
    }
}

/// Nome de um quadro do mapa (também usado pelo resumo do Sonar).
pub fn nome_pais(id: u16, nacoes: &[Nacao]) -> String {
    if id == NACAO_OUTROS {
        return "Outros".to_string();
    }
    nacoes.iter().find(|n| n.id == id).map_or_else(|| format!("País {id}"), |n| n.nome.clone())
}

/// Resumo da seleção para a linha do formulário: "Todos os países",
/// "Brazil", "Brazil, Argentina", "Brazil, Argentina e mais 3".
pub fn resumo_paises(paises: &[u16], nacoes: &[Nacao]) -> String {
    let nome = |id: &u16| nome_pais(*id, nacoes);
    match paises {
        [] => "Todos os países".to_string(),
        [um] => nome(um),
        [a, b] => format!("{}, {}", nome(a), nome(b)),
        [a, b, resto @ ..] => format!("{}, {} e mais {}", nome(a), nome(b), resto.len()),
    }
}

/// Desenha o painel; `true` = voltar ao formulário.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> bool {
    let Some(previa) = state.previa_missao() else {
        return true;
    };
    let mut voltar = false;
    let mut alternar = None;
    let mut limpar = false;
    let selecao = previa.rascunho.filtros.paises.clone();
    let nacoes = state.nacoes();

    let altura = (ui.content_region_avail()[1] - nova_missao::ALTURA_RESUMO).max(160.0);
    ui.child_window("##selecao_geografica").size([0.0, altura]).border(false).flags(super::flags_conteudo()).build(|| {
        if componentes::botao(ui, fonts, "Confirmar", EstiloBotao::Primario, true) {
            voltar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        if componentes::botao(ui, fonts, "Limpar", EstiloBotao::Secundario, !selecao.is_empty()) {
            limpar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Filtro geográfico"));
        let linha = if selecao.is_empty() {
            MSG_TODOS.to_string()
        } else {
            format!(
                "{} · amplitude: {} · clique de novo num país para tirá-lo.",
                super::aviso::texto_paises(selecao.len()),
                nome_amplitude(previa.amplitude)
            )
        };
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, &linha));
        ui.dummy([0.0, theme::ESPACO_1]);

        match &nacoes {
            None => com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, "Carregando os países…")),
            Some(nacoes) => {
                let estado = |id: u16| {
                    if selecao.contains(&id) {
                        cartograma::EstadoQuadro::Selecionado
                    } else {
                        cartograma::EstadoQuadro::Livre
                    }
                };
                ui.child_window("##mapa").size([0.0, 0.0]).border(false).flags(super::flags_conteudo()).build(|| {
                    alternar = cartograma::render(ui, fonts, nacoes, &estado).ativado;
                });
            }
        }
    });

    {
        let _c = ui.push_style_color(StyleColor::Separator, theme::BORDER_HAIRLINE);
        ui.separator();
    }
    ui.dummy([0.0, theme::ESPACO_1]);
    nova_missao::resumo(ui, fonts, &previa);

    if let Some(id) = alternar {
        state.alternar_pais_da_missao(id);
    }
    if limpar {
        state.limpar_paises_da_missao();
    }
    voltar
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::Confederacao;

    #[test]
    fn the_form_row_summarises_the_selection_and_says_all_countries_when_empty() {
        let n = |id, nome: &str| Nacao { id, nome: nome.to_string(), iso: String::new(), confederacao: Confederacao::AmericaDoSul };
        let nacoes = vec![n(54, "Brazil"), n(52, "Argentina"), n(60, "Uruguay")];
        assert_eq!(resumo_paises(&[], &nacoes), "Todos os países");
        assert_eq!(resumo_paises(&[54], &nacoes), "Brazil");
        assert_eq!(resumo_paises(&[54, 52], &nacoes), "Brazil, Argentina");
        assert_eq!(resumo_paises(&[54, 52, 60, NACAO_OUTROS], &nacoes), "Brazil, Argentina e mais 2");
        assert_eq!(resumo_paises(&[NACAO_OUTROS], &nacoes), "Outros");
        assert!(MSG_TODOS.contains("todos os países"));
    }
}
