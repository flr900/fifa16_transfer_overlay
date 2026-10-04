//! Painel de campo "Atributos dominantes" (Story 2.8, UX-DR11): tela cheia
//! sobre o formulário Nova Missão, com a barra de abas e o cabeçalho ainda
//! visíveis e o resumo do rodapé (custo, prazo, Qualidade) recalculado ao
//! vivo.
//!
//! Desde 2026-10-03 (pedido do Felipe) é multisseleção: até
//! `quality::MAX_DOMINANTES` atributos, cada um entra/sai ao ser ativado
//! (aqui foco não é escolha: mover o D-pad não marca nada). Com o máximo
//! escolhido, os outros ficam desabilitados. "Concluir" ou B volta ao
//! formulário com a linha mostrando a escolha.

use imgui::{StyleColor, Ui};

use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, nova_missao};
use crate::scout::quality;
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

/// Explicação no topo do painel.
pub fn explicacao() -> String {
    format!(
        "Escolha até {}. O Olheiro só traz jogadores que têm todos eles entre os maiores atributos (um: top {}; dois: top {}; três: top {}) e os observa primeiro. A Missão vira Tática.",
        quality::MAX_DOMINANTES,
        quality::top_para(1),
        quality::top_para(2),
        quality::top_para(3)
    )
}

/// O que ativar uma opção faz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escolha {
    Nenhum,
    Alternar(Atributo),
}

/// Desenha o painel; `true` = voltar ao formulário ("Concluir" ou B).
/// `focar`: a tela acabou de abrir — o foco começa no primeiro atributo já
/// escolhido (ou em "Qualquer um").
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, focar: bool) -> bool {
    let Some(previa) = state.previa_missao() else {
        return true;
    };
    let atuais = previa.rascunho.filtros.atributos_dominantes.clone();
    let cheio = atuais.len() >= quality::MAX_DOMINANTES;
    let mut voltar = false;
    let mut escolha: Option<Escolha> = None;

    let altura = (ui.content_region_avail()[1] - nova_missao::ALTURA_RESUMO).max(160.0);
    let rolagem = state.rolagem();
    ui.child_window("##campo_atributo").size([0.0, altura]).border(false).flags(super::flags_conteudo()).build(|| {
        super::rolar_com_analogico(ui, rolagem);
        if componentes::botao(ui, fonts, "Concluir", EstiloBotao::Primario, true) {
            voltar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Atributos dominantes"));
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, explicacao()));
        ui.dummy([0.0, theme::ESPACO_2]);
        if focar && atuais.is_empty() {
            unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
        }
        if opcao(ui, fonts, "Qualquer um", atuais.is_empty(), true) {
            escolha = Some(Escolha::Nenhum);
        }
        for (grupo, atributos) in GRUPOS {
            ui.dummy([0.0, theme::ESPACO_1]);
            com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, grupo));
            for (indice, &a) in atributos.iter().enumerate() {
                if indice > 0 {
                    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
                }
                let escolhido = atuais.contains(&a);
                if focar && atuais.first() == Some(&a) {
                    unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
                }
                if opcao(ui, fonts, a.nome(), escolhido, escolhido || !cheio) {
                    escolha = Some(Escolha::Alternar(a));
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

    match escolha {
        Some(Escolha::Nenhum) => state.limpar_atributos_da_missao(),
        Some(Escolha::Alternar(a)) => state.alternar_atributo_da_missao(a),
        None => {}
    }
    voltar
}

fn opcao(ui: &Ui, fonts: Option<&Fonts>, rotulo: &str, escolhida: bool, habilitada: bool) -> bool {
    let estilo = if escolhida { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
    componentes::botao_com_largura(ui, fonts, rotulo, estilo, habilitada, Some(LARGURA_OPCAO))
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
