//! Aba Base do Scout (2026-10-08, pedido do Felipe): todos os jogadores que
//! algum Olheiro do clube já encontrou, um por jogador, com o melhor registro
//! que o clube tem dele (o que mais atributos revelou) e quem o viu. Sai dos
//! Relatórios — arquivados também —, então arquivar um Relatório não tira
//! ninguém da Base. Não é a Lista de Escolhidos: Escolhidos são os que o
//! técnico separou; a Base é tudo o que o scout já mapeou.
//!
//! As Missões novas consultam esta Base antes de sair procurando
//! (`ScoutState::base_do_scout`). Mesma barra, filtros, Cards e Tabular das
//! outras listas (`lista_jogadores`); ativar um jogador abre a Ficha dele no
//! Relatório do melhor registro.

use imgui::Ui;
use uuid::Uuid;

use super::relatorios;
use super::theme::{self, Fonts};
use super::com_fonte;
use crate::scout::lista::ListaId;
use crate::scout::state::{Ocorrencia, ScoutState};

pub const MSG_VAZIA: &str =
    "A Base do Scout está vazia. Cada jogador que um Olheiro encontrar numa Missão entra aqui, mesmo depois de arquivar o Relatório.";

/// "123 jogadores mapeados por 4 Olheiros."
pub fn texto_resumo(jogadores: usize, olheiros: usize) -> String {
    let j = if jogadores == 1 { "jogador mapeado" } else { "jogadores mapeados" };
    let o = if olheiros == 1 { "Olheiro" } else { "Olheiros" };
    format!("{jogadores} {j} por {olheiros} {o}.")
}

/// Desenha a aba; devolve `(Relatório, jogador)` do jogador ativado.
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Option<(Uuid, u32)> {
    let base = state.base_do_scout();
    if base.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_VAZIA));
        return None;
    }
    let mut olheiros: Vec<&str> = base.iter().flat_map(|b| b.vistos.iter()).filter_map(|o| o.olheiro.as_deref()).collect();
    olheiros.sort_unstable();
    olheiros.dedup();
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.text_colored(
            theme::TEXT_SECONDARY,
            format!("{} Novas Missões consultam esta Base antes de procurar.", texto_resumo(base.len(), olheiros.len())),
        );
    });
    ui.dummy([0.0, theme::ESPACO_2]);
    let registros: Vec<(&Ocorrencia, String)> = base.iter().map(|b| (&b.melhor, b.origem())).collect();
    relatorios::ocorrencias_na_tela(ui, fonts, state, ListaId::Base, &registros)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_counts_players_and_scouts() {
        assert_eq!(texto_resumo(123, 4), "123 jogadores mapeados por 4 Olheiros.");
        assert_eq!(texto_resumo(1, 1), "1 jogador mapeado por 1 Olheiro.");
        assert!(!MSG_VAZIA.contains('!'));
    }
}
