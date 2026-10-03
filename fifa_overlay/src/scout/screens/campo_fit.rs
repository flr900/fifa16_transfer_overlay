//! Painel de campo "Fit Posicional" (Story 3.4, UX-DR11): tela cheia sobre
//! o formulário Nova Missão, com a barra de abas, o cabeçalho e o resumo
//! do rodapé (custo, prazo, Qualidade) ao vivo. Escolher uma posição volta
//! ao formulário com a linha mostrando a escolha; B / "Voltar" volta sem
//! mudar nada.

use imgui::{StyleColor, Ui};

use super::componentes::{self, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, nova_missao};
use crate::scout::state::{PosicaoAlvo, ScoutState};

const LARGURA_OPCAO: f32 = 200.0;

pub const EXPLICACAO: &str = "O Olheiro procura jogadores de outra posição cujo perfil de atributos serve nesta, e observa primeiro os atributos que ela pede. A Missão vira Tática.";

/// Grupos do painel, da defesa para o ataque.
pub const GRUPOS: [(&str, &[PosicaoAlvo]); 3] = [
    (
        "Defesa",
        &[PosicaoAlvo::Zagueiro, PosicaoAlvo::LateralDireito, PosicaoAlvo::LateralEsquerdo, PosicaoAlvo::AlaDireito, PosicaoAlvo::AlaEsquerdo],
    ),
    (
        "Meio-campo",
        &[PosicaoAlvo::Volante, PosicaoAlvo::MeioCampista, PosicaoAlvo::MeiaAtacante, PosicaoAlvo::MeiaDireita, PosicaoAlvo::MeiaEsquerda],
    ),
    ("Ataque", &[PosicaoAlvo::PontaDireita, PosicaoAlvo::PontaEsquerda, PosicaoAlvo::SegundoAtacante, PosicaoAlvo::Centroavante]),
];

/// Valor da linha "Fit Posicional" no formulário.
pub fn texto_fit(alvo: Option<PosicaoAlvo>) -> String {
    match alvo {
        Some(a) => format!("{} ({})", a.nome(), a.sigla()),
        None => "Nenhum".to_string(),
    }
}

/// Desenha o painel; `true` = voltar ao formulário (escolheu ou voltou).
/// `focar`: a tela acabou de abrir — o foco começa na opção já escolhida.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, focar: bool) -> bool {
    let Some(previa) = state.previa_missao() else {
        return true;
    };
    let atual = previa.rascunho.filtros.fit_posicional;
    let mut voltar = false;
    let mut escolha: Option<Option<PosicaoAlvo>> = None;

    let altura = (ui.content_region_avail()[1] - nova_missao::ALTURA_RESUMO).max(160.0);
    ui.child_window("##campo_fit").size([0.0, altura]).border(false).flags(super::flags_conteudo()).build(|| {
        if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
            voltar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Fit Posicional"));
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, EXPLICACAO));
        ui.dummy([0.0, theme::ESPACO_2]);
        if focar && atual.is_none() {
            unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
        }
        if opcao(ui, fonts, "Nenhum", atual.is_none()) {
            escolha = Some(None);
        }
        for (grupo, posicoes) in GRUPOS {
            ui.dummy([0.0, theme::ESPACO_1]);
            com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, grupo));
            for (indice, &p) in posicoes.iter().enumerate() {
                if indice > 0 {
                    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
                }
                if focar && atual == Some(p) {
                    unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
                }
                if opcao(ui, fonts, p.nome(), atual == Some(p)) {
                    escolha = Some(Some(p));
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

    if let Some(alvo) = escolha {
        state.definir_fit_da_missao(alvo);
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
    fn every_target_position_appears_exactly_once() {
        let mut todas: Vec<PosicaoAlvo> = GRUPOS.iter().flat_map(|(_, p)| p.iter().copied()).collect();
        assert_eq!(todas.len(), PosicaoAlvo::TODAS.len());
        todas.sort_unstable();
        todas.dedup();
        assert_eq!(todas.len(), PosicaoAlvo::TODAS.len());
        assert_eq!(texto_fit(Some(PosicaoAlvo::Volante)), "Volante (VOL)");
        assert_eq!(texto_fit(None), "Nenhum");
        assert!(!EXPLICACAO.contains('!'));
    }
}
