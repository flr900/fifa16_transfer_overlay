//! Aba Missões: o botão "Nova Missão" (Story 2.2) e a lista das Missões
//! encomendadas nesta carreira, cada uma com Olheiro, Modo de Busca,
//! Qualidade estimada e o progresso (Story 2.3): barra (trilho escuro,
//! preenchimento roxo) SEMPRE acompanhada do texto de estimativa — nunca a
//! barra sozinha (UX-DR7).
//!
//! O progresso usa a data guardada quando o painel abriu
//! (`ScoutState::missoes`), não um relógio ao vivo (FR-8). Sem data (erro
//! de leitura) a lista continua aparecendo, sem barra.

use imgui::Ui;
use uuid::Uuid;

use super::componentes::{self, badge_novo, badge_qualidade, badge_tier, card, desenhar_badge, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::scout::state::{MissaoNaLista, ModoBusca, ProgressoMissao, ScoutState, StatusMissao};

const ALTURA_CARD: f32 = 104.0;
const ALTURA_BARRA: f32 = 6.0;

pub const MSG_SEM_MISSOES: &str = "Nenhuma Missão encomendada ainda.";
pub const MSG_SEM_DATA: &str = "Progresso indisponível: não foi possível ler a data da carreira.";

/// Rótulo do Modo de Busca.
pub fn nome_modo(modo: ModoBusca) -> &'static str {
    match modo {
        ModoBusca::Rapida => "Rápida",
        ModoBusca::Completa => "Completa",
    }
}

/// Rótulo curto do status, na linha de detalhes.
pub fn nome_status(status: StatusMissao, progresso: Option<ProgressoMissao>) -> &'static str {
    match status {
        StatusMissao::Pendente if progresso.is_some_and(|p| p.prazo_atingido) => "Pronta",
        StatusMissao::Pendente => "Em andamento",
        StatusMissao::EmExecucao => "Gerando relatório",
        StatusMissao::Concluida => "Concluída",
    }
}

/// O texto que acompanha a barra (UX-DR7: valores exatos, sem exclamação).
pub fn texto_estimativa(linha: &MissaoNaLista) -> String {
    if let (StatusMissao::Pendente, Some(falha)) = (linha.missao.status, &linha.falha) {
        return format!("A busca falhou: {falha} Ela roda de novo quando o painel abrir.");
    }
    match (linha.missao.status, linha.progresso) {
        (StatusMissao::Concluida, _) if linha.relatorio_id.is_some() => "Concluída: ative para abrir o Relatório.".to_string(),
        (StatusMissao::Concluida, _) => "Concluída: Relatório disponível.".to_string(),
        (StatusMissao::EmExecucao, _) => "Gerando o Relatório…".to_string(),
        (StatusMissao::Pendente, None) => MSG_SEM_DATA.to_string(),
        (StatusMissao::Pendente, Some(p)) if p.prazo_atingido => {
            format!("Pronta: prazo cumprido em {}.", formatar_data(linha.missao.prazo_estimado))
        }
        (StatusMissao::Pendente, Some(p)) => {
            let dias = if p.dias_restantes == 1 { "~1 dia".to_string() } else { format!("~{} dias", p.dias_restantes) };
            format!("Relatório pronto em {dias} de carreira ({}).", formatar_data(linha.missao.prazo_estimado))
        }
    }
}

/// Fração exibida na barra (Concluída e "gerando" já contam como cheia).
pub fn fracao_da_barra(linha: &MissaoNaLista) -> Option<f32> {
    match linha.missao.status {
        StatusMissao::Pendente => linha.progresso.map(|p| p.fracao.clamp(0.0, 1.0)),
        StatusMissao::EmExecucao | StatusMissao::Concluida => Some(1.0),
    }
}

/// O que a aba pediu neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    NovaMissao,
    /// Card de uma Missão com Relatório ativado (Story 2.5).
    AbrirRelatorio(Uuid),
}

/// Desenha a aba. `pode_encomendar` é `false` no estado de erro de leitura
/// (a lista aparece, sem o botão).
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState, pode_encomendar: bool) -> Acao {
    let mut acao = Acao::Nenhuma;
    if pode_encomendar && componentes::botao(ui, fonts, "Nova Missão", EstiloBotao::Primario, true) {
        acao = Acao::NovaMissao;
    }
    ui.dummy([0.0, theme::ESPACO_3]);

    let missoes = state.missoes();
    if missoes.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_MISSOES));
        return acao;
    }
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text_colored(theme::TEXT_SECONDARY, "Missões"));
    for linha in &missoes {
        if card_missao(ui, fonts, linha) {
            if let Some(id) = linha.relatorio_id {
                acao = Acao::AbrirRelatorio(id);
            }
        }
    }
    acao
}

/// Card de uma Missão; `true` = ativado (só faz algo com Relatório pronto).
fn card_missao(ui: &Ui, fonts: Option<&Fonts>, linha: &MissaoNaLista) -> bool {
    let missao = &linha.missao;
    let c = card(ui, &missao.id.to_string(), ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let dl = ui.get_window_draw_list();
    let x = c.min[0] + theme::ESPACO_4;
    let largura = c.max[0] - c.min[0] - theme::ESPACO_4 * 2.0;
    let mut y = c.min[1] + theme::ESPACO_3;

    // Linha 1: Olheiro + Tier; Qualidade estimada à direita.
    let nome = linha.olheiro.as_ref().map_or("Olheiro removido", |o| o.especializacao.nome());
    let [largura_nome, altura_nome] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, nome);
    let mut xb = x + largura_nome + theme::ESPACO_2;
    if let Some(o) = &linha.olheiro {
        xb += desenhar_badge(ui, fonts, &dl, &badge_tier(o.tier), [xb, y], altura_nome)[0] + theme::ESPACO_2;
    }
    if linha.relatorio_novo {
        desenhar_badge(ui, fonts, &dl, &badge_novo(), [xb, y], altura_nome);
    }
    let qualidade = badge_qualidade(missao.estimativa.qualidade);
    let largura_badge =
        com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(qualidade.texto)[0]) + theme::ESPACO_2 * 2.0;
    desenhar_badge(ui, fonts, &dl, &qualidade, [c.max[0] - theme::ESPACO_4 - largura_badge, y], altura_nome);
    y += altura_nome + theme::ESPACO_1;

    // Linha 2: Modo · status · prazo.
    let detalhe = format!(
        "{} · {} · prazo {}",
        nome_modo(missao.modo_busca),
        nome_status(missao.status, linha.progresso),
        formatar_data(missao.prazo_estimado)
    );
    let [_, altura_detalhe] = texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], theme::TEXT_SECONDARY, &detalhe);
    y += altura_detalhe + theme::ESPACO_2;

    // Linha 3: barra (só com data conhecida) e, sempre, o texto.
    if let Some(fracao) = fracao_da_barra(linha) {
        let fim = [x + largura, y + ALTURA_BARRA];
        dl.add_rect([x, y], fim, theme::BG_BASE).filled(true).rounding(theme::RAIO_SM).build();
        if fracao > 0.0 {
            let cor = if missao.status == StatusMissao::Concluida { theme::FIELD_GREEN } else { theme::ACCENT_PRIMARY };
            dl.add_rect([x, y], [x + largura * fracao, y + ALTURA_BARRA], cor)
                .filled(true)
                .rounding(theme::RAIO_SM)
                .build();
        }
    }
    y += ALTURA_BARRA + theme::ESPACO_1;
    let pronta = linha.progresso.is_some_and(|p| p.prazo_atingido) || missao.status == StatusMissao::Concluida;
    let cor = match (&linha.falha, pronta) {
        (Some(_), _) if missao.status == StatusMissao::Pendente => theme::DANGER,
        (_, true) => theme::FIELD_GREEN,
        _ => theme::TEXT_SECONDARY,
    };
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y], cor, &texto_estimativa(linha));
    c.ativou
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_repo::Date;
    use crate::scout::state::{progresso_missao, Missao};
    use uuid::Uuid;

    fn linha(status: StatusMissao, hoje: Option<i32>) -> MissaoNaLista {
        let mut missao = Missao::de_teste(Uuid::new_v4(), status);
        missao.criada_em = Date(20260701);
        missao.prazo_estimado = Date(20260711);
        MissaoNaLista {
            progresso: hoje.map(|d| progresso_missao(missao.criada_em, missao.prazo_estimado, Date(d))),
            missao,
            olheiro: None,
            falha: None,
            relatorio_id: None,
            relatorio_novo: false,
        }
    }

    #[test]
    fn the_estimate_text_always_comes_with_the_bar_and_has_exact_values() {
        assert_eq!(
            texto_estimativa(&linha(StatusMissao::Pendente, Some(20260704))),
            "Relatório pronto em ~7 dias de carreira (11/07/2026)."
        );
        assert_eq!(
            texto_estimativa(&linha(StatusMissao::Pendente, Some(20260710))),
            "Relatório pronto em ~1 dia de carreira (11/07/2026)."
        );
        assert_eq!(texto_estimativa(&linha(StatusMissao::Pendente, Some(20260801))), "Pronta: prazo cumprido em 11/07/2026.");
        assert_eq!(texto_estimativa(&linha(StatusMissao::Pendente, None)), MSG_SEM_DATA);
        let falhou = MissaoNaLista { falha: Some("Não foi possível ler o save ativo.".to_string()), ..linha(StatusMissao::Pendente, Some(20260801)) };
        assert_eq!(
            texto_estimativa(&falhou),
            "A busca falhou: Não foi possível ler o save ativo. Ela roda de novo quando o painel abrir."
        );
        for status in [StatusMissao::Pendente, StatusMissao::EmExecucao, StatusMissao::Concluida] {
            assert!(!texto_estimativa(&linha(status, Some(20260704))).contains('!'));
        }
    }

    #[test]
    fn the_bar_never_goes_out_of_range_and_ready_missions_say_so() {
        assert_eq!(fracao_da_barra(&linha(StatusMissao::Pendente, Some(20260706))), Some(0.5));
        assert_eq!(fracao_da_barra(&linha(StatusMissao::Pendente, Some(20270101))), Some(1.0));
        assert_eq!(fracao_da_barra(&linha(StatusMissao::Pendente, Some(20250101))), Some(0.0));
        assert_eq!(fracao_da_barra(&linha(StatusMissao::Pendente, None)), None);
        assert_eq!(fracao_da_barra(&linha(StatusMissao::Concluida, None)), Some(1.0));
        let pronta = linha(StatusMissao::Pendente, Some(20260711));
        assert_eq!(nome_status(pronta.missao.status, pronta.progresso), "Pronta");
        let andando = linha(StatusMissao::Pendente, Some(20260705));
        assert_eq!(nome_status(andando.missao.status, andando.progresso), "Em andamento");
    }

    #[test]
    fn labels_are_portuguese_and_without_exclamation() {
        assert_eq!(nome_modo(ModoBusca::Rapida), "Rápida");
        assert_eq!(nome_modo(ModoBusca::Completa), "Completa");
        assert_eq!(MSG_SEM_MISSOES, "Nenhuma Missão encomendada ainda.");
    }
}
