//! Seletor de elenco (AD-13): UMA tela para os dois papéis, empilhada como
//! qualquer satélite (B volta igual em todo lugar).
//! - `FiltroMissao` (Story 3.3): escolhe o Jogador de Referência do
//!   formulário Nova Missão; tem "Nenhum" e o resumo ao vivo do rodapé
//!   (UX-DR11).
//! - `ComparacaoFicha` (Story 3.2): escolhe o jogador a sobrepor no Radar.
//!
//! O elenco vem só de `scout::state::listar_elenco_atual()` (AD-1/AD-13),
//! que o lê em background: a tela mostra "Lendo o elenco…" e, se a leitura
//! falhar, o erro com "Tentar novamente".

use imgui::{StyleColor, Ui};

use super::componentes::{self, card, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, nova_missao};
use crate::save_repo::nome_posicao;
use crate::scout::state::{EstadoElenco, JogadorElenco, ScoutState};
use crate::scout::ContextoSeletor;

const ALTURA_LINHA: f32 = 44.0;

pub const MSG_LENDO: &str = "Lendo o elenco do save…";
pub const MSG_ERRO: &str = "Não foi possível ler o elenco do save ativo.";
pub const MSG_VAZIO: &str = "Nenhum jogador no elenco.";

/// Título e explicação: o rótulo diz para que o seletor serve (AD-13).
pub fn rotulos(contexto: ContextoSeletor) -> (&'static str, &'static str) {
    match contexto {
        ContextoSeletor::FiltroMissao => (
            "Jogador de Referência",
            "Escolha um jogador do seu elenco: a Missão procura jogadores de perfil parecido com o dele. A Missão vira Tática.",
        ),
        ContextoSeletor::ComparacaoFicha => (
            "Comparar com jogador do elenco",
            "Escolha um jogador do seu elenco para sobrepor no Radar da Ficha. Nada muda no Relatório.",
        ),
    }
}

/// "ATA · 27 anos · OVR 84 · POT 86".
pub fn detalhe(j: &JogadorElenco) -> String {
    format!("{} · {} anos · OVR {} · POT {}", nome_posicao(j.posicao), j.idade, j.overall, j.potencial)
}

/// Desenha o seletor; `true` = voltar à tela de baixo (escolheu ou voltou).
/// `focar`: a tela acabou de abrir — o foco começa no jogador já escolhido.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, contexto: ContextoSeletor, focar: bool) -> bool {
    let previa = match contexto {
        ContextoSeletor::FiltroMissao => match state.previa_missao() {
            Some(p) => Some(p),
            None => return true,
        },
        ContextoSeletor::ComparacaoFicha => None,
    };
    let atual = match contexto {
        ContextoSeletor::FiltroMissao => previa.as_ref().and_then(|p| p.rascunho.filtros.referencia.as_ref()).map(|r| r.player_id),
        ContextoSeletor::ComparacaoFicha => state.ficha_aberta().and_then(|f| f.comparacao).map(|c| c.player_id),
    };
    let (titulo, explicacao) = rotulos(contexto);
    let mut voltar = false;
    let mut escolha: Option<Option<u32>> = None;

    let altura = match previa {
        Some(_) => (ui.content_region_avail()[1] - nova_missao::ALTURA_RESUMO).max(160.0),
        None => 0.0,
    };
    ui.child_window("##seletor_elenco").size([0.0, altura]).border(false).flags(super::flags_conteudo()).build(|| {
        if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
            voltar = true;
        }
        ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        com_fonte(ui, fonts.map(|f| f.heading), || ui.text(titulo));
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, explicacao));
        ui.dummy([0.0, theme::ESPACO_2]);

        match state.listar_elenco_atual() {
            EstadoElenco::Carregando => {
                com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_LENDO));
            }
            EstadoElenco::Erro => {
                com_fonte(ui, fonts.map(|f| f.body), || ui.text(MSG_ERRO));
                ui.dummy([0.0, theme::ESPACO_2]);
                if componentes::botao(ui, fonts, "Tentar novamente", EstiloBotao::Primario, true) {
                    state.reler_elenco();
                }
            }
            EstadoElenco::Pronto(elenco) => {
                if contexto == ContextoSeletor::FiltroMissao {
                    if focar && atual.is_none() {
                        unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
                    }
                    let estilo = if atual.is_none() { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
                    if componentes::botao_com_largura(ui, fonts, "Nenhum", estilo, true, Some(200.0)) {
                        escolha = Some(None);
                    }
                    ui.dummy([0.0, theme::ESPACO_1]);
                }
                if elenco.is_empty() {
                    com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_VAZIO));
                }
                for j in elenco.iter() {
                    let escolhido = atual == Some(j.player_id);
                    if focar && escolhido {
                        unsafe { imgui::sys::igSetKeyboardFocusHere(0) };
                    }
                    if linha(ui, fonts, j, escolhido) {
                        escolha = Some(Some(j.player_id));
                    }
                }
            }
        }
    });

    if let Some(previa) = &previa {
        {
            let _c = ui.push_style_color(StyleColor::Separator, theme::BORDER_HAIRLINE);
            ui.separator();
        }
        ui.dummy([0.0, theme::ESPACO_1]);
        nova_missao::resumo(ui, fonts, previa);
    }

    if let Some(player_id) = escolha {
        match contexto {
            ContextoSeletor::FiltroMissao => state.definir_referencia_da_missao(player_id),
            ContextoSeletor::ComparacaoFicha => state.comparar_com(player_id),
        }
        voltar = true;
    }
    voltar
}

/// Um jogador do elenco (card navegável inteiro); `true` = ativado.
fn linha(ui: &Ui, fonts: Option<&Fonts>, j: &JogadorElenco, escolhido: bool) -> bool {
    let borda = if escolhido { theme::ACCENT_PRIMARY } else { theme::BORDER_HAIRLINE_SUBTLE };
    let c = card(ui, &j.player_id.to_string(), ALTURA_LINHA, borda);
    let dl = ui.get_window_draw_list();
    let altura = com_fonte(ui, fonts.map(|f| f.heading), || ui.calc_text_size(&j.nome)[1]);
    let y = (c.min[1] + c.max[1] - altura) * 0.5;
    let x = c.min[0] + theme::ESPACO_4;
    let [w, _] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &j.nome);
    let detalhe = detalhe(j);
    let h_detalhe = com_fonte(ui, fonts.map(|f| f.meta), || ui.calc_text_size(&detalhe)[1]);
    texto_em(
        ui,
        fonts.map(|f| f.meta),
        &dl,
        [x + w + theme::ESPACO_4, (c.min[1] + c.max[1] - h_detalhe) * 0.5],
        theme::TEXT_SECONDARY,
        &detalhe,
    );
    if escolhido {
        let rotulo = "Escolhido";
        let largura = com_fonte(ui, fonts.map(|f| f.body), || ui.calc_text_size(rotulo)[0]);
        texto_em(ui, fonts.map(|f| f.body), &dl, [c.max[0] - theme::ESPACO_4 - largura, y], theme::ACCENT_PRIMARY, rotulo);
    }
    c.ativou
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_context_says_what_the_choice_is_for() {
        let (filtro, explica_filtro) = rotulos(ContextoSeletor::FiltroMissao);
        let (comparar, explica_comparar) = rotulos(ContextoSeletor::ComparacaoFicha);
        assert_eq!(filtro, "Jogador de Referência");
        assert_eq!(comparar, "Comparar com jogador do elenco");
        assert!(explica_filtro.contains("Missão procura"));
        assert!(explica_comparar.contains("Radar"));
        for t in [explica_filtro, explica_comparar, MSG_LENDO, MSG_ERRO, MSG_VAZIO] {
            assert!(!t.contains('!'));
        }
        let j = JogadorElenco { player_id: 1, nome: "A".to_string(), idade: 27, posicao: 25, overall: 84, potencial: 86, atributos: vec![50; 33] };
        assert_eq!(detalhe(&j), "ATA · 27 anos · OVR 84 · POT 86");
    }
}
