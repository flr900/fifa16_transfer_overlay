//! Formulário Nova Missão (Story 2.2, UX-DR10): tela satélite em lista
//! vertical estilo menu de console — uma linha por campo — e um rodapé
//! FIXO com custo, prazo e Qualidade, recalculados a cada mudança pela
//! tabela de `scout::quality` (síncrono, sem `save_repo` — AD-4).
//!
//! Desde 2026-10-03 o Olheiro vem escolhido de antes (passo 1,
//! `escolher_olheiro`, ou o clique nele na aba Olheiros) e o formulário
//! abre com os filtros ideais da Especialização dele; "Restaurar sugestão"
//! volta a eles. B volta ao passo anterior.
//!
//! A barra de abas e o cabeçalho continuam visíveis (é uma tela, não um
//! modal). Confirmar debita o orçamento com as garantias da contratação
//! (Story 1.5) e grava a Missão como `Pendente`; nenhuma busca roda agora
//! (AD-8). B / "Cancelar" volta para a aba sem gravar nada.
//!
//! Filtro geográfico e atributo dominante viraram linhas desta lista nas
//! Stories 2.8/2.9; Fit Posicional (3.4) e Jogador de Referência (3.3), no
//! Épico 3 — cada um abre um painel em tela cheia (o de referência é o
//! seletor de elenco, AD-13).

use imgui::{StyleColor, Ui};

use super::componentes::{self, badge_qualidade, EstiloBotao};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data, formatar_milhar, olheiros};
use crate::scout::quality::TipoMissao;
use crate::scout::{ContextoSeletor, Satelite};
use crate::scout::quality;
use crate::scout::state::{Atributo, BloqueioMissao, Carga, CampoFaixa, ErroCompra, FaixaAtributo, ModoBusca, PreviaMissao, ScoutState};

const ALTURA_RODAPE: f32 = 176.0;
const LARGURA_ROTULO: f32 = 170.0;
const LARGURA_VALOR: f32 = 44.0;
const LARGURA_MODO: f32 = 150.0;
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
        BloqueioMissao::FaixaInvalida { campo: CampoFaixa::IdadeMin | CampoFaixa::IdadeMax } => {
            "A idade mínima não pode ser maior que a máxima.".to_string()
        }
        BloqueioMissao::FaixaInvalida { campo: CampoFaixa::ContratoMin | CampoFaixa::ContratoMax } => {
            "O contrato mínimo não pode ser maior que o máximo.".to_string()
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
            cabecalho_olheiro(ui, fonts, state, &previa);
            divisor(ui);
            let f = &previa.rascunho.filtros;
            campo_faixa(ui, fonts, state, "Overall", f.overall, CampoFaixa::OverallMin, CampoFaixa::OverallMax, "");
            divisor(ui);
            campo_faixa(ui, fonts, state, "Potencial", f.potencial, CampoFaixa::PotencialMin, CampoFaixa::PotencialMax, "");
            divisor(ui);
            campo_faixa(ui, fonts, state, "Idade", f.idade, CampoFaixa::IdadeMin, CampoFaixa::IdadeMax, "anos");
            divisor(ui);
            campo_faixa(ui, fonts, state, "Contrato", f.contrato, CampoFaixa::ContratoMin, CampoFaixa::ContratoMax, "anos restantes");
            com_fonte(ui, fonts.map(|f| f.meta), || {
                ui.set_cursor_pos([ui.cursor_pos()[0] + LARGURA_ROTULO, ui.cursor_pos()[1]]);
                ui.text_colored(theme::TEXT_SECONDARY, texto_contrato(f.contrato));
            });
            divisor(ui);
            let geografia = match state.listar_ligas() {
                Carga::Pronto(ligas) => super::selecao_geografica::resumo_geografia(f, &ligas),
                _ if f.tem_geografia() => "Lendo as ligas…".to_string(),
                _ => "O mundo todo".to_string(),
            };
            if campo_painel(ui, fonts, "Filtro geográfico", &geografia) {
                campo = Some(Satelite::SelecaoGeografica);
            }
            divisor(ui);
            if campo_painel(ui, fonts, "Atributos dominantes", &texto_atributos(&f.atributos_dominantes)) {
                campo = Some(Satelite::CampoAtributo);
            }
            divisor(ui);
            if campo_painel(ui, fonts, "Fit Posicional", &super::campo_fit::texto_fit(previa.rascunho.filtros.fit_posicional)) {
                campo = Some(Satelite::CampoFit);
            }
            divisor(ui);
            let referencia = previa.rascunho.filtros.referencia.as_ref().map_or("Nenhum", |r| r.nome.as_str());
            if campo_painel(ui, fonts, "Jogador de Referência", referencia) {
                campo = Some(Satelite::SeletorElenco(ContextoSeletor::FiltroMissao));
            }
            divisor(ui);
            campo_modo(ui, fonts, state, previa.rascunho.modo);
            divisor(ui);
            campo_duracao(ui, fonts, state, previa.rascunho.continua);
        });

    match (rodape(ui, fonts, state, &previa), campo) {
        (Acao::Nenhuma, Some(satelite)) => Acao::AbrirCampo(satelite),
        (acao, _) => acao,
    }
}

/// Valor da linha "Atributos dominantes".
pub fn texto_atributos(atributos: &[Atributo]) -> String {
    if atributos.is_empty() {
        return "Qualquer um".to_string();
    }
    atributos.iter().map(|a| a.nome()).collect::<Vec<_>>().join(" + ")
}

/// Explicação da faixa de contrato: "Termina nesta temporada", "De 1 a 3
/// anos", "Qualquer contrato".
pub fn texto_contrato(faixa: FaixaAtributo) -> String {
    let anos = |n: u8| match n {
        n if n >= quality::CONTRATO_MAIOR => format!("{n} anos ou mais"),
        1 => "1 ano".to_string(),
        n => format!("{n} anos"),
    };
    match (faixa.min, faixa.max) {
        (0, m) if m >= quality::CONTRATO_MAIOR => "Qualquer contrato.".to_string(),
        (0, 0) => "Contrato termina nesta temporada (sai mais barato ou de graça).".to_string(),
        (0, m) => format!("Termina nesta temporada ou em até {}.", anos(m)),
        (a, b) if a == b => format!("Faltam {}.", anos(a)),
        (a, b) => format!("Faltam de {} a {}.", a, anos(b)),
    }
}

/// Valor mostrado num stepper: o máximo do contrato é "5+".
fn texto_valor(campo: CampoFaixa, valor: u8) -> String {
    let (_, maior) = campo.limites();
    if matches!(campo, CampoFaixa::ContratoMin | CampoFaixa::ContratoMax) && valor >= maior {
        format!("{valor}+")
    } else {
        valor.to_string()
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

/// O Olheiro escolhido no passo 1 (não muda aqui; B volta para trocar) e
/// "Restaurar sugestão", que devolve os filtros ideais dele.
fn cabecalho_olheiro(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, previa: &PreviaMissao) {
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, "Olheiro");
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    let escolhido = previa
        .rascunho
        .olheiro_id
        .and_then(|id| previa.olheiros.iter().find(|c| c.olheiro.id == id))
        .map(|c| c.olheiro.clone());
    match escolhido {
        Some(o) => {
            olheiros::nome_com_badge(ui, fonts, o.especializacao, o.tier);
            ui.same_line_with_spacing(0.0, theme::ESPACO_4);
            if componentes::botao(ui, fonts, "Restaurar sugestão", EstiloBotao::Secundario, true) {
                state.restaurar_filtros_ideais();
            }
            ui.set_cursor_pos([inicio[0] + LARGURA_ROTULO, ui.cursor_pos()[1]]);
            com_fonte(ui, fonts.map(|f| f.meta), || {
                ui.text_colored(
                    theme::TEXT_SECONDARY,
                    format!("{} Os filtros abaixo já vêm com o que combina com ele.", olheiros::descricao(o.especializacao)),
                )
            });
        }
        None => com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_OLHEIRO)),
    }
}

/// Linha de faixa: `[-] min [+]  até  [-] max [+]  unidade`. Segurar o
/// botão (mouse ou A) repete o passo.
#[allow(clippy::too_many_arguments)]
fn campo_faixa(
    ui: &Ui,
    fonts: Option<&Fonts>,
    state: &mut ScoutState,
    nome: &str,
    faixa: FaixaAtributo,
    campo_min: CampoFaixa,
    campo_max: CampoFaixa,
    unidade: &str,
) {
    let _id = ui.push_id(nome);
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, nome);
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    if let Some(delta) = stepper(ui, fonts, "min", campo_min, faixa.min) {
        state.ajustar_faixa_da_missao(campo_min, delta);
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    rotulo(ui, fonts, "até");
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    if let Some(delta) = stepper(ui, fonts, "max", campo_max, faixa.max) {
        state.ajustar_faixa_da_missao(campo_max, delta);
    }
    if !unidade.is_empty() {
        ui.same_line_with_spacing(0.0, theme::ESPACO_3);
        rotulo(ui, fonts, unidade);
    }
    if !faixa.valida() {
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::DANGER, "Mínimo maior que o máximo."));
    }
}

/// `[-] valor [+]`, dentro dos limites do campo; devolve o passo pedido
/// neste frame.
fn stepper(ui: &Ui, fonts: Option<&Fonts>, id: &str, campo: CampoFaixa, valor: u8) -> Option<i32> {
    let _id = ui.push_id(id);
    let _repetir = ui.push_button_repeat(true);
    let (menor, maior) = campo.limites();
    let mut delta = None;
    let lado = Some(theme::ALVO_MINIMO);
    if componentes::botao_com_largura(ui, fonts, "-##menos", EstiloBotao::Secundario, valor > menor, lado) {
        delta = Some(-1);
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    let texto = texto_valor(campo, valor);
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let inicio = ui.cursor_pos();
    com_fonte(ui, mono, || {
        let [w, h] = ui.calc_text_size(&texto);
        ui.set_cursor_pos([inicio[0] + (LARGURA_VALOR - w) * 0.5, inicio[1] + (theme::ALVO_MINIMO - h) * 0.5]);
        ui.text(&texto);
    });
    ui.same_line_with_spacing(inicio[0] + LARGURA_VALOR + theme::ESPACO_2, 0.0);
    if componentes::botao_com_largura(ui, fonts, "+##mais", EstiloBotao::Secundario, valor < maior, lado) {
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
        let clicou = componentes::botao_com_largura(ui, fonts, super::missoes::nome_modo(opcao), estilo, true, Some(LARGURA_MODO));
        if (clicou || componentes::focado_pelo_controle(ui)) && opcao != modo {
            state.definir_modo_da_missao(opcao);
        }
    }
    ui.set_cursor_pos([inicio[0] + LARGURA_ROTULO, ui.cursor_pos()[1]]);
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, descricao_modo(modo)));
}

/// Prazo fixo ou contínua (Story 2.10).
fn campo_duracao(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, continua: bool) {
    let inicio = ui.cursor_pos();
    rotulo(ui, fonts, "Duração");
    ui.same_line_with_spacing(inicio[0] + LARGURA_ROTULO, 0.0);
    for (indice, (nome, valor)) in [("Prazo fixo", false), ("Contínua", true)].into_iter().enumerate() {
        if indice > 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        }
        let estilo = if valor == continua { EstiloBotao::Selecionado } else { EstiloBotao::Secundario };
        let clicou = componentes::botao_com_largura(ui, fonts, nome, estilo, true, Some(LARGURA_MODO));
        if (clicou || componentes::focado_pelo_controle(ui)) && valor != continua {
            state.definir_continua_da_missao(valor);
        }
    }
    ui.set_cursor_pos([inicio[0] + LARGURA_ROTULO, ui.cursor_pos()[1]]);
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, descricao_duracao(continua)));
}

pub fn descricao_duracao(continua: bool) -> String {
    if continua {
        format!(
            "O Olheiro fica na Missão até você encerrar. Cada bloco de {} dias de carreira é pago na confirmação; ao fim do bloco, você decide se renova.",
            crate::scout::quality::DIAS_BLOCO_CONTINUO
        )
    } else {
        "O Relatório chega aos poucos e fica completo no prazo.".to_string()
    }
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
    let continua = previa.rascunho.continua;
    let custo = estimativa.map_or("—".to_string(), |e| {
        if continua {
            format!("{} por bloco", formatar_milhar(e.custo))
        } else {
            formatar_milhar(e.custo)
        }
    });
    let prazo = match (previa.duracao_dias(), previa.prazo()) {
        (Some(dias), Some(data)) if continua => format!("blocos de {dias} dias (o 1º termina em {})", formatar_data(data)),
        (Some(dias), Some(data)) => format!("~{dias} dias de carreira (pronta em {})", formatar_data(data)),
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
                    "Relatório: até {} jogadores{}, {} atributos por jogador, precisão de ±{}.",
                    e.alvo_jogadores,
                    if continua { " por bloco" } else { "" },
                    e.atributos_revelados,
                    e.precisao_mais_menos
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
            rascunho: RascunhoMissao {
                olheiro_id: None,
                filtros: FiltrosMissao::default(),
                modo: ModoBusca::Rapida,
                continua: false,
                erro: None,
            },
            olheiros: Vec::new(),
            tipo,
            amplitude: crate::scout::quality::AmplitudeGeografica::Mundo,
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
    fn contract_and_attribute_rows_read_naturally() {
        let faixa = |min, max| FaixaAtributo { min, max };
        assert_eq!(texto_contrato(faixa(0, 5)), "Qualquer contrato.");
        assert!(texto_contrato(faixa(0, 0)).starts_with("Contrato termina nesta temporada"));
        assert_eq!(texto_contrato(faixa(0, 1)), "Termina nesta temporada ou em até 1 ano.");
        assert_eq!(texto_contrato(faixa(2, 2)), "Faltam 2 anos.");
        assert_eq!(texto_contrato(faixa(1, 5)), "Faltam de 1 a 5 anos ou mais.");
        assert_eq!(texto_valor(CampoFaixa::ContratoMax, 5), "5+");
        assert_eq!(texto_valor(CampoFaixa::IdadeMax, 45), "45");
        assert_eq!(texto_atributos(&[]), "Qualquer um");
        assert_eq!(texto_atributos(&[Atributo::Velocidade, Atributo::Drible]), "Velocidade + Drible");
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
