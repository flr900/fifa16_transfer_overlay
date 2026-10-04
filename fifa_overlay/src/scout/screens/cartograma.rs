//! Cartograma de países (Story 2.9): um quadro por nação, agrupados por
//! continente, desenhados com `ImDrawList`. Componente reaproveitável: o
//! Painel de Seleção Geográfica usa no modo seleção e o Sonar (Épico 4)
//! vai usar no modo visualização, com os estados de Missão.
//!
//! Cada quadro é UM item navegável (mouse e controle: o D-pad anda entre
//! vizinhos, A ativa); o nome inteiro aparece no tooltip. Cor nunca é o
//! único indicador: selecionado também ganha borda de 2 px e um "•".

use imgui::Ui;

use super::componentes::{focado_pelo_controle, texto_em};
use super::relatorio::truncar;
use super::theme::{self, Fonts};
use super::{com_fonte, ESPESSURA_FOCO};
use crate::scout::search::NACAO_OUTROS;
use crate::scout::state::{Confederacao, Nacao};

const LARGURA_QUADRO: f32 = 132.0;
const ALTURA_QUADRO: f32 = 36.0;

/// Como um quadro aparece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoQuadro {
    Livre,
    /// Escolhido no filtro (roxo, contorno + preenchimento tênue).
    Selecionado,
    /// Sonar: há Missão ativa no país (roxo).
    MissaoAtiva,
    /// Sonar: Missão concluída, Relatório disponível (verde).
    MissaoConcluida,
}

/// Fundo, borda, espessura da borda e cor do texto de um estado (também
/// usados pela legenda do Sonar, para amostra e quadro nunca divergirem).
pub fn cores(estado: EstadoQuadro) -> ([f32; 4], [f32; 4], f32, [f32; 4]) {
    match estado {
        EstadoQuadro::Livre => (theme::BG_PANEL_RAISED, theme::BORDER_HAIRLINE_SUBTLE, 1.0, theme::TEXT_SECONDARY),
        EstadoQuadro::Selecionado | EstadoQuadro::MissaoAtiva => {
            (theme::ACCENT_PRIMARY_DIM, theme::ACCENT_PRIMARY, ESPESSURA_FOCO, theme::TEXT_PRIMARY)
        }
        EstadoQuadro::MissaoConcluida => (theme::FIELD_GREEN_DIM, theme::FIELD_GREEN, ESPESSURA_FOCO, theme::TEXT_PRIMARY),
    }
}

/// Amostra de legenda: um quadrinho no estilo do estado, do tamanho da
/// linha de texto, alinhado com o texto que vem depois (`same_line`).
pub fn amostra(ui: &Ui, estado: EstadoQuadro, lado: f32) {
    let min = ui.cursor_screen_pos();
    let max = [min[0] + lado, min[1] + lado];
    let (fundo, borda, espessura, _) = cores(estado);
    let dl = ui.get_window_draw_list();
    dl.add_rect(min, max, fundo).filled(true).rounding(theme::RAIO_SM).build();
    dl.add_rect(min, max, borda).rounding(theme::RAIO_SM).thickness(espessura).build();
    ui.dummy([lado, lado]);
}

/// Nações de um continente, por nome.
pub fn do_continente(nacoes: &[Nacao], continente: Confederacao) -> Vec<&Nacao> {
    let mut lista: Vec<&Nacao> = nacoes.iter().filter(|n| n.confederacao == continente).collect();
    lista.sort_by(|a, b| a.nome.cmp(&b.nome));
    lista
}

/// O que aconteceu no cartograma neste frame (ids de quadro;
/// `NACAO_OUTROS` para "Outros").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resposta {
    /// Clicado, ou A no controle.
    pub ativado: Option<u16>,
    /// Com o foco do controle/teclado (o Sonar trata foco = escolha; a
    /// seleção do filtro, não, senão andar pelo mapa marcaria países).
    pub focado: Option<u16>,
}

/// Desenha o cartograma.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, nacoes: &[Nacao], estado_de: &dyn Fn(u16) -> EstadoQuadro) -> Resposta {
    let mut resposta = Resposta::default();
    for continente in Confederacao::TODAS {
        let lista = do_continente(nacoes, continente);
        let com_outros = continente == Confederacao::Outras;
        if lista.is_empty() && !com_outros {
            continue;
        }
        ui.dummy([0.0, theme::ESPACO_1]);
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, continente.nome()));
        let disponivel = ui.content_region_avail()[0];
        let por_linha = (((disponivel + theme::ESPACO_1) / (LARGURA_QUADRO + theme::ESPACO_1)).floor() as usize).max(1);
        let quadros = lista
            .iter()
            .map(|n| (n.id, n.nome.as_str()))
            .chain(com_outros.then_some((NACAO_OUTROS, "Outros")));
        for (indice, (id, nome)) in quadros.enumerate() {
            if indice % por_linha != 0 {
                ui.same_line_with_spacing(0.0, theme::ESPACO_1);
            }
            if quadro(ui, fonts, id, nome, estado_de(id)) {
                resposta.ativado = Some(id);
            }
            if focado_pelo_controle(ui) {
                resposta.focado = Some(id);
            }
        }
    }
    resposta
}

fn quadro(ui: &Ui, fonts: Option<&Fonts>, id: u16, nome: &str, estado: EstadoQuadro) -> bool {
    let _id = ui.push_id_usize(usize::from(id));
    let min = ui.cursor_screen_pos();
    let max = [min[0] + LARGURA_QUADRO, min[1] + ALTURA_QUADRO];
    let ativou = ui.invisible_button("##pais", [LARGURA_QUADRO, ALTURA_QUADRO]);
    let foco = ui.is_item_hovered() || (ui.is_item_focused() && ui.io().nav_visible);
    let (fundo, borda, espessura, texto) = cores(estado);
    let dl = ui.get_window_draw_list();
    dl.add_rect(min, max, fundo).filled(true).rounding(theme::RAIO_SM).build();
    dl.add_rect(min, max, borda).rounding(theme::RAIO_SM).thickness(espessura).build();
    if foco {
        let d = 3.0;
        dl.add_rect([min[0] - d, min[1] - d], [max[0] + d, max[1] + d], theme::ACCENT_PRIMARY)
            .rounding(theme::RAIO_SM)
            .thickness(ESPESSURA_FOCO)
            .build();
    }
    let marca = if estado == EstadoQuadro::Selecionado { "• " } else { "" };
    let largura_texto = LARGURA_QUADRO - theme::ESPACO_2 * 2.0;
    let medir = |t: &str| com_fonte(ui, fonts.map(|f| f.meta), || ui.calc_text_size(t)[0]);
    let (visivel, cortou) = truncar(&format!("{marca}{nome}"), largura_texto, medir);
    let altura = com_fonte(ui, fonts.map(|f| f.meta), || ui.text_line_height());
    texto_em(ui, fonts.map(|f| f.meta), &dl, [min[0] + theme::ESPACO_2, min[1] + (ALTURA_QUADRO - altura) * 0.5], texto, &visivel);
    if foco && cortou {
        ui.tooltip_text(nome);
    }
    ativou
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countries_are_grouped_by_continent_and_sorted_by_name() {
        let n = |id, nome: &str, c| Nacao { id, nome: nome.to_string(), iso: String::new(), confederacao: c };
        let nacoes = vec![
            n(54, "Brazil", Confederacao::AmericaDoSul),
            n(52, "Argentina", Confederacao::AmericaDoSul),
            n(14, "England", Confederacao::Europa),
        ];
        let sul: Vec<&str> = do_continente(&nacoes, Confederacao::AmericaDoSul).iter().map(|x| x.nome.as_str()).collect();
        assert_eq!(sul, ["Argentina", "Brazil"]);
        assert!(do_continente(&nacoes, Confederacao::Asia).is_empty());
    }
}
