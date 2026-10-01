//! Painel de campo "Atributo dominante" (Story 2.8, UX-DR11): tela cheia
//! sobre o formulário Nova Missão, com a barra de abas e o cabeçalho ainda
//! visíveis e o resumo do rodapé (custo, prazo, Qualidade) recalculado ao
//! vivo. Escolher uma opção volta ao formulário com a linha mostrando a
//! escolha; B / "Voltar" volta sem mudar nada.

use imgui::{StyleColor, Ui};

use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, nova_missao};
use crate::scout::state::{Atributo, ScoutState};

const LARGURA_OPCAO: f32 = 200.0;

/// Grupos do painel, na ordem da tela (os mesmos blocos do jogo).
pub const GRUPOS: [(&str, &[Atributo]); 7] = [
    ("Ritmo", &[Atributo::Aceleracao, Atributo::Velocidade]),
    (
        "Drible",
        &[Atributo::Drible, Atributo::ControleDeBola, Atributo::Agilidade, Atributo::Equilibrio, Atributo::Reacao],
    ),
    (
        "Finalização",
        &[
            Atributo::Finalizacao,
            Atributo::PosicionamentoOfensivo,
            Atributo::ForcaDoChute,
            Atributo::ChuteDeLonge,
            Atributo::Voleio,
            Atributo::Penaltis,
        ],
    ),
    (
        "Passe",
        &[Atributo::Visao, Atributo::PasseCurto, Atributo::PasseLongo, Atributo::Cruzamento, Atributo::Falta, Atributo::Curva],
    ),
    (
        "Defesa",
        &[Atributo::Marcacao, Atributo::DesarmeEmPe, Atributo::Carrinho, Atributo::Interceptacao, Atributo::Cabeceio],
    ),
    ("Físico", &[Atributo::Forca, Atributo::Folego, Atributo::Impulsao, Atributo::Agressividade]),
    (
        "Goleiro",
        &[Atributo::GkReflexos, Atributo::GkMergulho, Atributo::GkColocacao, Atributo::GkManejo, Atributo::GkReposicao],
    ),
];

/// Desenha o painel; `true` = voltar ao formulário (escolheu ou voltou).
/// `focar`: a tela acabou de abrir — o foco começa na opção já escolhida
/// (não no "Voltar"), para foco e escolha não ficarem em lugares diferentes.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, focar: bool) -> bool {
    let Some(previa) = state.previa_missao() else {
        return true;
    };
    let atual = previa.rascunho.filtros.atributo_dominante;
    let mut voltar = false;
    let mut escolha: Option<Option<Atributo>> = None;

    let altura = (ui.content_region_avail()[1] - nova_missao::ALTURA_RESUMO).max(160.0);
    ui.child_window("##campo_atributo").size([0.0, altura]).border(false).flags(super::flags_conteudo()).build(|| {
        if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
            voltar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Atributo dominante"));
        com_fonte(ui, fonts.map(|f| f.meta), || {
            ui.text_colored(
                theme::TEXT_SECONDARY,
                "O Olheiro só traz jogadores que têm este atributo entre os 3 maiores, e o observa primeiro. A Missão vira Tática.",
            );
        });
        ui.dummy([0.0, theme::ESPACO_2]);
        if focar && atual.is_none() {
            unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
        }
        if opcao(ui, fonts, "Qualquer um", atual.is_none()) {
            escolha = Some(None);
        }
        for (grupo, atributos) in GRUPOS {
            ui.dummy([0.0, theme::ESPACO_1]);
            com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, grupo));
            for (indice, &a) in atributos.iter().enumerate() {
                if indice > 0 {
                    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
                }
                if focar && atual == Some(a) {
                    unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
                }
                if opcao(ui, fonts, a.nome(), atual == Some(a)) {
                    escolha = Some(Some(a));
                }
            }
        }
    });

    {
        let _c = ui.push_style_color(StyleColor::Separator, theme::BORDER_HAIRLINE);
        ui.separator();
    }
    ui.dummy([0.0, theme::ESPACO_1]);
    nova_missao::resumo(ui, fonts, &previa);

    if let Some(atributo) = escolha {
        state.definir_atributo_da_missao(atributo);
        voltar = true;
    }
    voltar
}

fn opcao(ui: &Ui, fonts: Option<&Fonts>, rotulo: &str, escolhida: bool) -> bool {
    let estilo = if escolhida { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
    componentes::botao_com_largura(ui, fonts, rotulo, estilo, true, Some(LARGURA_OPCAO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_attribute_appears_exactly_once_in_the_groups() {
        let mut todos: Vec<Atributo> = GRUPOS.iter().flat_map(|(_, a)| a.iter().copied()).collect();
        assert_eq!(todos.len(), Atributo::TODOS.len());
        todos.sort_unstable();
        todos.dedup();
        assert_eq!(todos.len(), Atributo::TODOS.len());
    }
}
