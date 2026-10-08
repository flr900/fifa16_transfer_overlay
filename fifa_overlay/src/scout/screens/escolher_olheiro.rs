//! Nova Missão, passo 1 (2026-10-03, pedido do Felipe): primeiro o Olheiro,
//! depois os filtros. Uma lista dos contratados, os livres primeiro (do mais
//! experiente ao menos, 2026-10-08), depois os que estão num contrato de
//! Missão contínua (dá para escolher, com a multa) e, por último, os "Em
//! Missão", cinzas e inertes. Ativar um livre (clique ou A) abre o formulário
//! com os filtros ideais da Especialização dele; B / "Voltar" volta à aba.
//!
//! Aqui foco não é escolha: escolher leva para outra tela, então mover o
//! foco com o D-pad só destaca o card.

use imgui::Ui;
use uuid::Uuid;

use super::componentes::{self, badge_tier, card, desenhar_badge, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, olheiros};
use crate::scout::state::{OlheiroContratado, ScoutState};

const ALTURA_CARD: f32 = 64.0;

pub const MSG_SEM_OLHEIROS: &str = "Nenhum Olheiro contratado. Contrate um na aba Olheiros para encomendar uma Missão.";
pub const MSG_TODOS_OCUPADOS: &str =
    "Todos os Olheiros estão em Missão ou acompanhando os Escolhidos. Contrate outro na aba Olheiros ou espere um terminar.";

/// O que o jogador fez neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Voltar,
    Escolheu(Uuid),
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState) -> Acao {
    let mut acao = Acao::Nenhuma;
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = Acao::Voltar;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Nova Missão · escolha o Olheiro"));
    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.text_colored(theme::TEXT_SECONDARY, "Os filtros já abrem com o que combina com a Especialização dele.")
    });
    ui.dummy([0.0, theme::ESPACO_2]);

    let contratados = state.olheiros_contratados();
    let aviso = if contratados.is_empty() {
        Some(MSG_SEM_OLHEIROS)
    } else if contratados.iter().all(|c| !c.aceita_missao_nova()) {
        Some(MSG_TODOS_OCUPADOS)
    } else {
        None
    };
    if let Some(texto) = aviso {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, texto));
        ui.dummy([0.0, theme::ESPACO_2]);
    }
    let mut ordenados: Vec<&OlheiroContratado> = contratados.iter().collect();
    ordenados.sort_by(|a, b| ordem_de_escolha(a, b));
    olheiros::com_nacoes(state, |nacoes| {
        let mut secao_atual = None;
        for c in ordenados {
            let secao = secao_do_olheiro(c);
            if secao_atual != Some(secao) {
                secao_atual = Some(secao);
                com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, secao.titulo()));
                ui.dummy([0.0, theme::ESPACO_1]);
            }
            if linha(ui, fonts, c, nacoes, secao == Secao::Ocupados) && c.aceita_missao_nova() {
                acao = Acao::Escolheu(c.olheiro.id);
            }
        }
    });
    acao
}

/// Em que bloco da lista o Olheiro aparece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Secao {
    /// Livre: pode receber a Missão já.
    Livres,
    /// Num contrato de Missão contínua que dá para rescindir.
    EmContrato,
    /// Em Missão ou acompanhando os Escolhidos: não dá para escolher.
    Ocupados,
}

impl Secao {
    fn titulo(self) -> &'static str {
        match self {
            Secao::Livres => "Disponíveis",
            Secao::EmContrato => "Em contrato de Missão contínua",
            Secao::Ocupados => "Ocupados",
        }
    }
}

fn secao_do_olheiro(c: &OlheiroContratado) -> Secao {
    if !c.aceita_missao_nova() {
        Secao::Ocupados
    } else if c.em_missao {
        Secao::EmContrato
    } else {
        Secao::Livres
    }
}

/// Livres antes, depois os de contrato, depois os ocupados; em cada bloco, do
/// mais experiente (Tier e estrelas do foco) ao menos, e pelo nome.
pub fn ordem_de_escolha(a: &OlheiroContratado, b: &OlheiroContratado) -> std::cmp::Ordering {
    let experiencia = |c: &OlheiroContratado| (c.olheiro.tier, c.olheiro.perfil().principal());
    secao_do_olheiro(a)
        .cmp(&secao_do_olheiro(b))
        .then_with(|| experiencia(b).cmp(&experiencia(a)))
        .then_with(|| a.olheiro.nome_exibicao().cmp(&b.olheiro.nome_exibicao()))
}

/// O que custa escolher um Olheiro que está num contrato de Missão contínua:
/// a multa de rescisão (para outra localidade) ou nada, se o contrato venceu.
pub fn texto_rescisao(c: &OlheiroContratado) -> Option<String> {
    let r = c.rescisao?;
    Some(if r.multa > 0 {
        format!(
            "Carência até {}. Tirá-lo para outra localidade antes disso custa uma multa de {}.",
            super::formatar_data(r.ate),
            super::formatar_milhar(r.multa)
        )
    } else if r.vencido {
        "Contrato encerrado: ele pode receber uma Missão nova sem multa.".to_string()
    } else {
        format!(
            "Fora da carência de 12 meses: pode mudar de localidade sem multa (o contrato em curso, até {}, não é devolvido).",
            super::formatar_data(r.ate)
        )
    })
}

fn linha(ui: &Ui, fonts: Option<&Fonts>, c: &OlheiroContratado, nacoes: &olheiros::Nacoes<'_>, apagado: bool) -> bool {
    let borda = if apagado { theme::BORDER_HAIRLINE_SUBTLE } else { theme::BORDER_HAIRLINE };
    let c_card = card(ui, &c.olheiro.id.to_string(), ALTURA_CARD, borda);
    let dl = ui.get_window_draw_list();
    // quem está num contrato de Missão contínua que dá para rescindir pode ser
    // escolhido: o texto diz o que custa
    let ocupado = !c.aceita_missao_nova();
    let cor = if ocupado { theme::TEXT_DISABLED } else { theme::TEXT_PRIMARY };
    let x = c_card.min[0] + theme::ESPACO_4;
    let y = c_card.min[1] + theme::ESPACO_2;
    let altura_nome = com_fonte(ui, fonts.map(|f| f.heading), || ui.calc_text_size(&c.olheiro.nome_exibicao())[1]);
    // a bandeira da nação antes do nome
    let x_nome = match &c.olheiro.nacao {
        Some(n) => {
            let altura_bandeira = (altura_nome * 0.72).round();
            let w = componentes::bandeira(&dl, (nacoes.bandeira)(n.id), [x, y + (altura_nome - altura_bandeira) * 0.5], altura_bandeira);
            x + w + theme::ESPACO_2
        }
        None => x,
    };
    let nome = c.olheiro.nome_exibicao();
    let [w, h] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x_nome, y], cor, &nome);
    let badge = desenhar_badge(ui, fonts, &dl, &badge_tier(c.olheiro.tier), [x_nome + w + theme::ESPACO_2, y], h);
    let perfil = c.olheiro.perfil();
    let foco = format!("{} · {} estrelas", perfil.foco().nome(), perfil.principal().texto());
    texto_em(ui, fonts.map(|f| f.meta), &dl, [x_nome + w + badge[0] + theme::ESPACO_4, y + 2.0], theme::TEXT_SECONDARY, &foco);
    let y_detalhe = y + h + theme::ESPACO_1;
    if let Some(texto) = texto_rescisao(c) {
        texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y_detalhe], theme::WARNING, &texto);
    } else if ocupado {
        texto_em(ui, fonts.map(|f| f.meta), &dl, [x, y_detalhe], theme::TEXT_SECONDARY, &olheiros::texto_missao(c));
    } else {
        let largura = c_card.max[0] - theme::ESPACO_4 - 120.0 - x;
        olheiros::desenhar_chips(ui, fonts, &dl, [x, y_detalhe], largura, 1, &olheiros::chips_do_olheiro(&c.olheiro, nacoes, false));
    }
    let (status, cor_status) = if let Some(r) = c.rescisao {
        if r.multa > 0 {
            ("Em carência", theme::WARNING)
        } else {
            ("Escolher", theme::FIELD_GREEN)
        }
    } else if c.em_missao {
        ("Em Missão", theme::WARNING)
    } else if c.acompanhando {
        ("Acompanhando", theme::ACCENT_PRIMARY)
    } else {
        ("Escolher", theme::FIELD_GREEN)
    };
    let largura = com_fonte(ui, fonts.map(|f| f.body), || ui.calc_text_size(status)[0]);
    let y_status = (c_card.min[1] + c_card.max[1]) * 0.5 - h * 0.5;
    texto_em(ui, fonts.map(|f| f.body), &dl, [c_card.max[0] - theme::ESPACO_4 - largura, y_status], cor_status, status);
    if apagado {
        // cinza por cima: o ocupado não compete com os disponíveis
        dl.add_rect(c_card.min, c_card.max, [0.04, 0.05, 0.05, 0.55]).filled(true).rounding(theme::RAIO_MD).build();
    }
    c_card.ativou
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_contract_in_force_shows_the_fine_and_an_expired_one_is_free() {
        use crate::save_repo::Date;
        use crate::scout::state::{Olheiro, Rescisao};
        let base = OlheiroContratado {
            olheiro: Olheiro::default(),
            em_missao: true,
            acompanhando: false,
            missao: None,
            relatorio_atual: None,
            relatorios: 0,
            rescisao: None,
        };
        assert_eq!(texto_rescisao(&base), None, "numa Missão comum, sem texto");
        assert!(!base.aceita_missao_nova());
        let em_curso = OlheiroContratado { rescisao: Some(Rescisao { missao: Uuid::nil(), ate: Date(20270903), multa: 90_000, vencido: false }), ..base.clone() };
        let texto = texto_rescisao(&em_curso).unwrap_or_default();
        assert!(texto.contains("03/09/2027") && texto.contains("multa de 90.000"), "{texto}");
        assert!(em_curso.aceita_missao_nova());
        assert!(texto.contains("Carência até 03/09/2027"), "{texto}");
        let renovado = OlheiroContratado {
            rescisao: Some(Rescisao { missao: Uuid::nil(), ate: Date(20280903), multa: 0, vencido: false }),
            ..base.clone()
        };
        let texto = texto_rescisao(&renovado).unwrap_or_default();
        assert!(texto.contains("Fora da carência") && texto.contains("sem multa"), "{texto}");
        assert!(renovado.aceita_missao_nova());
        let vencido = OlheiroContratado {
            rescisao: Some(Rescisao { missao: Uuid::nil(), ate: Date(20270903), multa: 0, vencido: true }),
            ..base
        };
        assert!(texto_rescisao(&vencido).unwrap_or_default().contains("encerrado"));
    }

    #[test]
    fn free_olheiros_come_first_by_experience_and_the_busy_ones_go_last() {
        use crate::scout::state::{Especializacao, Olheiro, Tier};
        let novo = |nome: &str, tier: Tier, em_missao: bool, acompanhando: bool| OlheiroContratado {
            olheiro: Olheiro { nome: nome.to_string(), especializacao: Especializacao::Tatico, tier, ..Olheiro::default() },
            em_missao,
            acompanhando,
            missao: None,
            relatorio_atual: None,
            relatorios: 0,
            rescisao: None,
        };
        let mut lista = vec![
            novo("Ocupado Elite", Tier::Elite, true, false),
            novo("Livre Junior", Tier::Junior, false, false),
            novo("Acompanha", Tier::Experiente, false, true),
            novo("Livre Elite", Tier::Elite, false, false),
            novo("Livre Exp", Tier::Experiente, false, false),
        ];
        lista.sort_by(ordem_de_escolha);
        let nomes: Vec<&str> = lista.iter().map(|c| c.olheiro.nome.as_str()).collect();
        assert_eq!(nomes, ["Livre Elite", "Livre Exp", "Livre Junior", "Ocupado Elite", "Acompanha"]);
    }

    #[test]
    fn empty_states_point_to_the_olheiros_tab() {
        assert!(MSG_SEM_OLHEIROS.contains("aba Olheiros"));
        assert!(MSG_TODOS_OCUPADOS.contains("aba Olheiros"));
        assert!(!MSG_SEM_OLHEIROS.contains('!') && !MSG_TODOS_OCUPADOS.contains('!'));
    }
}
