//! Formulário Nova Missão (Story 2.2, UX-DR10): tela satélite sobre a aba
//! Missões, em lista vertical estilo menu de console — uma linha por campo
//! (Olheiro, Overall, Potencial, Modo de Busca) — e um rodapé FIXO com
//! custo, prazo e Qualidade, recalculados a cada mudança pela tabela de
//! `scout::quality` (síncrono, sem `save_repo` — AD-4).
//!
//! A barra de abas e o cabeçalho continuam visíveis (é uma tela, não um
//! modal). Confirmar debita o orçamento com as garantias da contratação
//! (Story 1.5) e grava a Missão como `Pendente`; nenhuma busca roda agora
//! (AD-8). B / "Cancelar" volta para a aba sem gravar nada.
//!
//! Filtro geográfico, atributo dominante, Fit Posicional e Jogador de
//! Referência viram linhas desta lista nas Stories 2.8/2.9 e no Épico 3.

use imgui::{StyleColor, Ui};

use super::componentes::{self, badge_qualidade, badge_tier, card, desenhar_badge, texto_em, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data, formatar_milhar, olheiros};
use crate::scout::quality::TipoMissao;
use crate::scout::Satelite;
use crate::scout::state::{
    BloqueioMissao, CampoFaixa, ErroCompra, FaixaAtributo, ModoBusca, OlheiroContratado, PreviaMissao, ScoutState,
};

const ALTURA_RODAPE: f32 = 176.0;
const LARGURA_ROTULO: f32 = 170.0;
const ALTURA_OLHEIRO: f32 = 52.0;
const LARGURA_VALOR: f32 = 44.0;
const LARGURA_MODO: f32 = 150.0;
const RAIO_RADIO: f32 = 7.0;
const LARGURA_VALOR_CAMPO: f32 = 320.0;

pub const MSG_SEM_OLHEIRO: &str = "Nenhum Olheiro disponível.";

/// O que o jogador fez no formulário neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Confirmou,
    Cancelou,
    /// Abrir um painel de campo (Atributo dominante, Filtro geográfico).
    AbrirCampo(Satelite),
}

/// Rótulo do tipo de Missão derivado das faixas.
pub fn nome_tipo(tipo: TipoMissao) -> &'static str {
    match tipo {
        TipoMissao::Jovens => "Jovens",
        TipoMissao::Medalhoes => "Medalhões",
        TipoMissao::Tatica => "Tática",
        TipoMissao::Geral => "Geral",
    }
}

/// Explica o tipo e se o Olheiro combina (o bônus de Qualidade da tabela).
pub fn texto_tipo(previa: &PreviaMissao) -> String {
    let tipo = nome_tipo(previa.tipo);
    let especialista = match previa.tipo {
        TipoMissao::Jovens => Some("Caçador de Jovens"),
        TipoMissao::Medalhoes => Some("Caçador de Medalhões"),
        TipoMissao::Tatica => Some("Tático"),
        TipoMissao::Geral => None,
    };
    match (previa.combina, especialista) {
        (true, Some(nome)) => format!("Tipo de Missão: {tipo}. O {nome} combina com ela: Qualidade maior."),
        (false, Some(nome)) => format!("Tipo de Missão: {tipo}. Um {nome} teria Qualidade maior."),
        (_, None) => format!("Tipo de Missão: {tipo}."),
    }
}

pub fn texto_bloqueio(bloqueio: BloqueioMissao) -> String {
    match bloqueio {
        BloqueioMissao::SemOlheiroDisponivel => MSG_SEM_OLHEIRO.to_string(),
        BloqueioMissao::FaixaInvalida { campo: CampoFaixa::OverallMin | CampoFaixa::OverallMax } => {
            "O Overall mínimo não pode ser maior que o máximo.".to_string()
        }
        BloqueioMissao::FaixaInvalida { campo: CampoFaixa::PotencialMin | CampoFaixa::PotencialMax } => {
            "O Potencial mínimo não pode ser maior que o máximo.".to_string()
        }
        BloqueioMissao::OrcamentoInsuficiente { faltam } => olheiros::texto_faltam(faltam),
    }
}

/// Falhas ao confirmar (sempre dizendo se o dinheiro saiu ou não).
pub fn texto_erro(erro: &ErroCompra) -> String {
    match erro {
        ErroCompra::OrcamentoInsuficiente { faltam } => olheiros::texto_faltam(*faltam),
        ErroCompra::OrcamentoMudou { atual } => format!(
            "O orçamento mudou para {} antes da confirmação. Nada foi debitado; confira os valores e confirme de novo.",
            formatar_milhar(*atual)
        ),
        ErroCompra::SemCarreira => "Nenhuma carreira carregada. Nada foi debitado.".to_string(),
        ErroCompra::EstadoNaoSalvavel => {
            "Não é possível salvar o estado do Scout desta carreira. Nada foi debitado.".to_string()
        }
        ErroCompra::EscritaFalhou => "Não foi possível debitar o orçamento. A Missão não foi encomendada.".to_string(),
        ErroCompra::NaoSalvo => {
            "Não foi possível salvar a Missão. O débito foi desfeito e nada foi encomendado.".to_string()
        }
        ErroCompra::DebitadoSemSalvar { debitado } => format!(
            "Não foi possível salvar a Missão, e o débito de {} não pôde ser desfeito. Confira o orçamento no jogo.",
            formatar_milhar(*debitado)
        ),
    }
}

pub fn descricao_modo(modo: ModoBusca) -> &'static str {
    match modo {
        ModoBusca::Rapida => "Mais nomes, Qualidade menor, termina antes.",
        ModoBusca::Completa => "Menos nomes, Qualidade maior, leva mais tempo.",
    }
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some(previa) = state.previa_missao() else {
        return Acao::Cancelou;
    };

    let mut campo = None;
    let altura_campos = (ui.content_region_avail()[1] - ALTURA_RODAPE).max(120.0);
    ui.child_window("##campos_nova_missao")
        .size([0.0, altura_campos])
        .border(false)
        .flags(super::flags_conteudo())
        .build(|| {
            com_fonte(ui, fonts.map(|f| f.heading), || ui.text("Nova Missão"));
            ui.dummy([0.0, theme::ESPACO_2]);
            campo_olheiro(ui, fonts, state, &previa);
            divisor(ui);
            campo_faixa(ui, fonts, state, "Overall", previa.rascunho.filtros.overall, CampoFaixa::OverallMin, CampoFaixa::OverallMax);
            divisor(ui);
            campo_faixa(
                ui,
                fonts,
                state,
                "Potencial",
                previa.rascunho.filtros.potencial,
                CampoFaixa::PotencialMin,
                CampoFaixa::PotencialMax,
            );
            divisor(ui);
            if campo_painel(ui, fonts, "Atributo dominante", &texto_atributo(previa.rascunho.filtros.atributo_dominante)) {
                campo = Some(Satelite::CampoAtributo);
            }
            divisor(ui);
            campo_modo(ui, fonts, state, previa.rascunho.modo);
        });

    match (rodape(ui, fonts, state, &previa), campo) {
        (Acao::Nenhuma, Some(satelite)) => Acao::AbrirCampo(satelite),
        (acao, _) => acao,
    }
}

/// Valor da linha "Atributo dominante".
pub fn texto_atributo(atributo: Option<crate::scout::state::Atributo>) -> String {
    match atributo {
        Some(a) => a.nome().to_string(),
        None => "Qualquer um".to_string(),
    }
}

/// Linha que abre um painel de campo em tela cheia (UX-DR11): rótulo à
/// esquerda e um botão com o valor atual. `true` = ativada.
fn campo_painel(ui: &Ui, fonts: Option<&Fonts>, nome: &str, valor: &str) -> bool {
    let _id = ui.push_id(nome);
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, nome);
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    let texto = format!("{valor}  ›");
    componentes::botao_com_largura(ui, fonts, &texto, EstiloBotao::Secundario, true, Some(LARGURA_VALOR_CAMPO))
}

fn rotulo(ui: &Ui, fonts: Option<&Fonts>, texto: &str) {
    com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, texto));
}

fn divisor(ui: &Ui) {
    ui.dummy([0.0, theme::ESPACO_2]);
    let _c = ui.push_style_color(StyleColor::Separator, theme::BORDER_HAIRLINE_SUBTLE);
    ui.separator();
    ui.dummy([0.0, theme::ESPACO_2]);
}

/// Linha "Olheiro": um card por contratado; os "Em Missão" aparecem
/// apagados e não são escolhíveis.
fn campo_olheiro(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, previa: &PreviaMissao) {
    rotulo(ui, fonts, "Olheiro");
    if !previa.olheiros.iter().any(|c| !c.em_missao) {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_OLHEIRO));
    }
    for contratado in &previa.olheiros {
        let escolhido = previa.rascunho.olheiro_id == Some(contratado.olheiro.id);
        if linha_olheiro(ui, fonts, contratado, escolhido) && !contratado.em_missao {
            state.escolher_olheiro_da_missao(contratado.olheiro.id);
        }
    }
}

fn linha_olheiro(ui: &Ui, fonts: Option<&Fonts>, contratado: &OlheiroContratado, escolhido: bool) -> bool {
    let borda = if escolhido { theme::BORDER_HAIRLINE } else { theme::BORDER_HAIRLINE_SUBTLE };
    let c = card(ui, &contratado.olheiro.id.to_string(), ALTURA_OLHEIRO, borda);
    let dl = ui.get_window_draw_list();
    let ocupado = contratado.em_missao;
    let cor_texto = if ocupado { theme::TEXT_DISABLED } else { theme::TEXT_PRIMARY };

    // "Rádio": contorno sempre, miolo roxo quando escolhido.
    let centro = [c.min[0] + theme::ESPACO_4 + RAIO_RADIO, (c.min[1] + c.max[1]) * 0.5];
    dl.add_circle(centro, RAIO_RADIO, if ocupado { theme::TEXT_DISABLED } else { theme::ACCENT_PRIMARY })
        .thickness(1.5)
        .build();
    if escolhido {
        dl.add_circle(centro, RAIO_RADIO - 3.0, theme::ACCENT_PRIMARY).filled(true).build();
    }

    let x = centro[0] + RAIO_RADIO + theme::ESPACO_3;
    let nome = contratado.olheiro.especializacao.nome();
    let altura = com_fonte(ui, fonts.map(|f| f.heading), || ui.calc_text_size(nome)[1]);
    let y = (c.min[1] + c.max[1] - altura) * 0.5;
    let [largura_nome, _] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], cor_texto, nome);
    desenhar_badge(ui, fonts, &dl, &badge_tier(contratado.olheiro.tier), [x + largura_nome + theme::ESPACO_2, y], altura);

    let (status, cor) = match (ocupado, escolhido) {
        (true, _) => ("Em Missão", theme::WARNING),
        (false, true) => ("Escolhido", theme::ACCENT_PRIMARY),
        (false, false) => ("Disponível", theme::FIELD_GREEN),
    };
    let largura_status = com_fonte(ui, fonts.map(|f| f.body), || ui.calc_text_size(status)[0]);
    texto_em(ui, fonts.map(|f| f.body), &dl, [c.max[0] - theme::ESPACO_4 - largura_status, y], cor, status);
    c.ativou
}

/// Linha de faixa: `[-] min [+]  até  [-] max [+]`. Segurar o botão (mouse
/// ou A) repete o passo.
fn campo_faixa(
    ui: &Ui,
    fonts: Option<&Fonts>,
    state: &mut ScoutState,
    nome: &str,
    faixa: FaixaAtributo,
    campo_min: CampoFaixa,
    campo_max: CampoFaixa,
) {
    let _id = ui.push_id(nome);
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, nome);
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    if let Some(delta) = stepper(ui, fonts, "min", faixa.min) {
        state.ajustar_faixa_da_missao(campo_min, delta);
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    rotulo(ui, fonts, "até");
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    if let Some(delta) = stepper(ui, fonts, "max", faixa.max) {
        state.ajustar_faixa_da_missao(campo_max, delta);
    }
    if !faixa.valida() {
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::DANGER, "Mínimo maior que o máximo."));
    }
}

/// `[-] valor [+]`; devolve o passo pedido neste frame.
fn stepper(ui: &Ui, fonts: Option<&Fonts>, id: &str, valor: u8) -> Option<i32> {
    let _id = ui.push_id(id);
    let _repetir = ui.push_button_repeat(true);
    let mut delta = None;
    let lado = Some(theme::ALVO_MINIMO);
    if componentes::botao_com_largura(ui, fonts, "-##menos", EstiloBotao::Secundario, valor > FaixaAtributo::MENOR, lado) {
        delta = Some(-1);
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    let texto = valor.to_string();
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let inicio = ui.cursor_pos();
    com_fonte(ui, mono, || {
        let [w, h] = ui.calc_text_size(&texto);
        ui.set_cursor_pos([inicio[0] + (LARGURA_VALOR - w) * 0.5, inicio[1] + (theme::ALVO_MINIMO - h) * 0.5]);
        ui.text(&texto);
    });
    ui.same_line_with_spacing(inicio[0] + LARGURA_VALOR + theme::ESPACO_2, 0.0);
    if componentes::botao_com_largura(ui, fonts, "+##mais", EstiloBotao::Secundario, valor < FaixaAtributo::MAIOR, lado) {
        delta = Some(1);
    }
    delta
}

fn campo_modo(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, modo: ModoBusca) {
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, "Modo de Busca");
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    for (indice, opcao) in ModoBusca::TODOS.into_iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        }
        let estilo = if opcao == modo { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        if componentes::botao_com_largura(ui, fonts, super::missoes::nome_modo(opcao), estilo, true, Some(LARGURA_MODO)) {
            state.definir_modo_da_missao(opcao);
        }
    }
    ui.set_cursor_pos([inicio[0] + LARGURA_ROTULO, ui.cursor_pos()[1]]);
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, descricao_modo(modo)));
}

/// Rodapé fixo: resumo ao vivo, motivo do bloqueio e os botões.
fn rodape(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, previa: &PreviaMissao) -> Acao {
    resumo(ui, fonts, previa);

    let mut acao = Acao::Nenhuma;
    let habilitado = previa.bloqueio.is_none();
    let rotulo_confirmar = if previa.rascunho.erro.is_some() { "Tentar novamente" } else { "Confirmar Missão" };
    if componentes::botao(ui, fonts, rotulo_confirmar, EstiloBotao::Primario, habilitado) && state.confirmar_nova_missao() {
        acao = Acao::Confirmou;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_3);
    if componentes::botao(ui, fonts, "Cancelar", EstiloBotao::Secundario, true) {
        acao = Acao::Cancelou;
    }
    acao
}

/// Resumo ao vivo (custo, prazo, Qualidade, tipo, aviso). Também aparece
/// nos painéis de campo, que não têm os botões (UX-DR11).
pub fn resumo(ui: &Ui, fonts: Option<&Fonts>, previa: &PreviaMissao) {
    {
        let _c = ui.push_style_color(StyleColor::Separator, theme::BORDER_HAIRLINE);
        ui.separator();
    }
    ui.dummy([0.0, theme::ESPACO_1]);

    let estimativa = previa.estimativa;
    let custo = estimativa.map_or("—".to_string(), |e| formatar_milhar(e.custo));
    let prazo = match (estimativa, previa.prazo()) {
        (Some(e), Some(data)) => format!("~{} dias de carreira (pronta em {})", e.duracao_dias, formatar_data(data)),
        _ => "—".to_string(),
    };

    // Linha de resumo: Custo | Prazo | Qualidade (badge)
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    rotulo(ui, fonts, "Custo");
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    com_fonte(ui, mono, || ui.text(&custo));
    ui.same_line_with_spacing(0.0, theme::ESPACO_5);
    rotulo(ui, fonts, "Prazo");
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    com_fonte(ui, fonts.map(|f| f.body), || ui.text(&prazo));
    ui.same_line_with_spacing(0.0, theme::ESPACO_5);
    rotulo(ui, fonts, "Qualidade");
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    match estimativa {
        Some(e) => {
            let altura = ui.text_line_height();
            componentes::badge_no_fluxo(ui, fonts, &badge_qualidade(e.qualidade), altura);
        }
        None => com_fonte(ui, fonts.map(|f| f.body), || ui.text("—")),
    }

    com_fonte(ui, fonts.map(|f| f.meta), || {
        ui.text_colored(theme::TEXT_SECONDARY, texto_tipo(previa));
        if let Some(e) = estimativa {
            ui.text_colored(
                theme::TEXT_SECONDARY,
                format!(
                    "Relatório: até {} jogadores, {} atributos por jogador, precisão de ±{}.",
                    e.alvo_jogadores, e.atributos_revelados, e.precisao_mais_menos
                ),
            );
        }
    });

    let aviso = match (&previa.bloqueio, &previa.rascunho.erro) {
        (Some(bloqueio), _) => Some(texto_bloqueio(*bloqueio)),
        (None, Some(erro)) => Some(texto_erro(erro)),
        (None, None) => None,
    };
    match aviso {
        Some(texto) => com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::DANGER, texto)),
        None => ui.dummy([0.0, ui.text_line_height()]),
    }
}

/// Altura reservada para o resumo no rodapé de um painel de campo.
pub const ALTURA_RESUMO: f32 = 120.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_repo::Date;
    use crate::scout::state::{FiltrosMissao, RascunhoMissao};

    fn previa(tipo: TipoMissao, combina: bool) -> PreviaMissao {
        PreviaMissao {
            rascunho: RascunhoMissao { olheiro_id: None, filtros: FiltrosMissao::default(), modo: ModoBusca::Rapida, erro: None },
            olheiros: Vec::new(),
            tipo,
            combina,
            estimativa: None,
            orcamento_atual: 0,
            data_atual: Date(20280924),
            bloqueio: None,
        }
    }

    #[test]
    fn the_type_line_explains_the_quality_bonus() {
        assert_eq!(texto_tipo(&previa(TipoMissao::Geral, false)), "Tipo de Missão: Geral.");
        assert_eq!(
            texto_tipo(&previa(TipoMissao::Jovens, true)),
            "Tipo de Missão: Jovens. O Caçador de Jovens combina com ela: Qualidade maior."
        );
        assert_eq!(
            texto_tipo(&previa(TipoMissao::Medalhoes, false)),
            "Tipo de Missão: Medalhões. Um Caçador de Medalhões teria Qualidade maior."
        );
    }

    #[test]
    fn blocking_and_error_texts_are_exact_and_without_exclamation() {
        assert_eq!(texto_bloqueio(BloqueioMissao::SemOlheiroDisponivel), "Nenhum Olheiro disponível.");
        assert_eq!(
            texto_bloqueio(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::OverallMin }),
            "O Overall mínimo não pode ser maior que o máximo."
        );
        assert_eq!(
            texto_bloqueio(BloqueioMissao::OrcamentoInsuficiente { faltam: 2_100_000 }),
            "Orçamento insuficiente: faltam 2.100.000."
        );
        for erro in [
            ErroCompra::EscritaFalhou,
            ErroCompra::NaoSalvo,
            ErroCompra::DebitadoSemSalvar { debitado: 880_000 },
            ErroCompra::OrcamentoMudou { atual: 1 },
        ] {
            assert!(!texto_erro(&erro).contains('!'));
        }
        assert!(texto_erro(&ErroCompra::DebitadoSemSalvar { debitado: 880_000 }).contains("880.000"));
    }
}
