//! Aba Escolhidos (Épico 6, pedido do Felipe em 2026-10-04): os jogadores
//! que o técnico separou a partir das Fichas, com o que se sabe de cada um
//! e o quão atual isso está.
//!
//! - A observação vale como foi feita por 6 meses; até 1 ano e meio as
//!   faixas alargam (DESATUALIZADO a partir de 1 ano); depois vence e os
//!   atributos somem até uma análise nova (`quality::frescor`).
//! - Generalistas designados ("Olheiros do acompanhamento") mantêm X
//!   jogadores atualizados cada; acompanhado, o jogador não envelhece e as
//!   faixas fecham até o valor exato.
//!
//! Cada card é UM item navegável: ativar abre a Ficha (onde ficam
//! Priorizar e Remover). Os dados vêm de `scout::state` (AD-1).

use imgui::Ui;

use super::componentes::{self, badge_frescor, card, desenhar_badge, desenhar_badge_texto, texto_em, EstiloBadge, EstiloBotao};
use super::lista_jogadores;
use super::relatorio::{formatar_faixa, silhueta, truncar};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::save_repo::{nome_posicao, Date};
use crate::scout::lista::{ItemLista, ListaId};
use crate::scout::minifaces::Rosto;
use crate::scout::quality::Frescor;
use crate::scout::state::{Densidade, EscolhidoNaLista, ResumoAcompanhamento, ScoutState, StatusNativo};

const ALTURA_CARD: f32 = 92.0;
const LADO_ROSTO: f32 = 68.0;

pub const MSG_VAZIA: &str =
    "Nenhum jogador escolhido. Abra a Ficha de um jogador num Relatório e use “Adicionar aos Escolhidos”.";
pub const MSG_REGRAS: &str = "A observação vale como foi feita por 6 meses; depois as faixas alargam e, com 1 ano e meio, é preciso uma análise nova. Generalistas designados mantêm os jogadores atualizados e fecham as faixas até o valor exato.";

/// O que o jogador fez na aba neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    /// "Olheiros do acompanhamento": abre a designação dos Generalistas.
    GerenciarAcompanhamento,
    AbrirFicha(u32),
}

/// "2 Generalistas acompanham 7 de 12 jogadores (14 vagas)".
pub fn texto_resumo(r: &ResumoAcompanhamento) -> String {
    match (r.generalistas, r.jogadores) {
        (0, 0) => "Nenhum Generalista acompanhando.".to_string(),
        (0, n) => format!("Nenhum Generalista acompanhando: os {n} jogadores envelhecem desde a última observação."),
        (g, n) => format!(
            "{g} {} {} de {n} {} ({} {}).",
            if g == 1 { "Generalista acompanha" } else { "Generalistas acompanham" },
            r.acompanhados,
            if n == 1 { "jogador" } else { "jogadores" },
            r.vagas,
            if r.vagas == 1 { "vaga" } else { "vagas" }
        ),
    }
}

/// A linha de situação da sincronização com o FIFA (Épico 7).
pub fn texto_sincronizacao(s: &StatusNativo) -> String {
    let base = match (s.ligada, s.localizando, s.escolhidos && s.conhecimento) {
        (false, _, _) => "Desligada: a Central não escreve no jogo. O que já foi escrito continua lá.".to_string(),
        (true, true, _) => "Localizando o scout do jogo…".to_string(),
        (true, false, true) => {
            "Ligada: os Escolhidos entram na lista do jogo e o que os Olheiros descobrem aparece nas telas do FIFA.".to_string()
        }
        (true, false, false) => "Ligada, mas o scout do jogo não foi localizado (carreira recém-carregada?).".to_string(),
    };
    match (&s.erro, s.ligada) {
        (Some(erro), true) => format!("{base} Último problema: {erro}"),
        _ => base,
    }
}

/// Meses de calendário completos entre duas datas (para "há 7 meses").
fn meses_entre(de: Date, ate: Date) -> i32 {
    let meses = (ate.year() * 12 + ate.month()) - (de.year() * 12 + de.month()) - i32::from(ate.day() < de.day());
    meses.max(0)
}

/// A situação de um Escolhido, numa linha: acompanhado ("exato em ~40
/// dias"), observado ("observado em 03/07/2035, há 7 meses: faixas +2"),
/// vencido.
pub fn texto_situacao(e: &EscolhidoNaLista, hoje: Option<Date>) -> String {
    let observado = e.escolhido.observado_em;
    let ha = hoje.map(|h| meses_entre(observado, h)).unwrap_or(0);
    let quando = match ha {
        0 => format!("observado em {}", formatar_data(observado)),
        1 => format!("observado em {}, há 1 mês", formatar_data(observado)),
        n => format!("observado em {}, há {n} meses", formatar_data(observado)),
    };
    match (e.acompanhado, e.dias_para_exato, e.frescor) {
        (true, Some(0), _) => "Acompanhado: valores exatos.".to_string(),
        (true, Some(d), _) => format!("Acompanhado: ±{} agora, exato em ~{d} dias de carreira.", e.precisao),
        (true, None, _) => "Acompanhado: a primeira observação sai na próxima abertura do painel.".to_string(),
        (false, _, Frescor::Atualizado) => format!("{quando}: precisão de ±{}.", e.precisao),
        (false, _, Frescor::Envelhecendo { extra }) => format!("{quando}: faixas {extra} pontos mais largas (±{}).", e.precisao),
        (false, _, Frescor::Desatualizado { extra }) => {
            format!("{quando}: desatualizado, faixas {extra} pontos mais largas (±{}).", e.precisao)
        }
        (false, _, Frescor::Vencido) => format!("{quando}: análise vencida; os atributos voltam com um Generalista no acompanhamento."),
    }
}

/// Aviso do falso positivo revelado.
pub const MSG_FORA_DO_FILTRO: &str = "Com os valores exatos, ele não passa no filtro da Missão de origem (o Olheiro se enganou).";

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let mut acao = Acao::Nenhuma;
    if componentes::botao(ui, fonts, "Olheiros do acompanhamento  ›", EstiloBotao::Secundario, true) {
        acao = Acao::GerenciarAcompanhamento;
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    let resumo = state.resumo_acompanhamento();
    com_fonte(ui, fonts.map(|f| f.body), || {
        let y = ui.cursor_pos()[1];
        ui.set_cursor_pos([ui.cursor_pos()[0], y + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text(texto_resumo(&resumo));
    });
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_wrapped(MSG_REGRAS));
    ui.dummy([0.0, theme::ESPACO_2]);

    let escolhidos = state.escolhidos();
    if escolhidos.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_VAZIA));
        return acao;
    }
    let hoje = state.data_da_carreira();
    let itens: Vec<ItemLista<'_>> = escolhidos
        .iter()
        .map(|e| {
            let mut item = ItemLista::novo(&e.jogador, situacao_curta(e));
            // observação vencida: os atributos somem
            item.detalhado = item.detalhado && e.frescor != Frescor::Vencido;
            item
        })
        .collect();
    lista_jogadores::barra(ui, fonts, state, ListaId::Escolhidos, &itens, |_, _| false);
    let visiveis = lista_jogadores::preparar(state, ListaId::Escolhidos, itens);
    if visiveis.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, lista_jogadores::MSG_NENHUM_NO_FILTRO));
        return acao;
    }
    if state.modo_da_lista(ListaId::Escolhidos) == Densidade::Tabular {
        if let Some(item) = lista_jogadores::tabela(ui, fonts, state, ListaId::Escolhidos, &visiveis).and_then(|i| visiveis.get(i)) {
            acao = Acao::AbrirFicha(item.jogador.player_id);
        }
        return acao;
    }
    for item in &visiveis {
        let Some(e) = escolhidos.iter().find(|e| e.jogador.player_id == item.jogador.player_id) else {
            continue;
        };
        if card_escolhido(ui, fonts, state, e, hoje) {
            acao = Acao::AbrirFicha(e.escolhido.jogador.player_id);
        }
    }
    acao
}

/// A situação numa palavra ou duas, para a coluna da visão Tabular.
pub fn situacao_curta(e: &EscolhidoNaLista) -> String {
    if e.fora_do_filtro {
        return "Fora do filtro".to_string();
    }
    match (e.acompanhado, e.dias_para_exato, e.frescor) {
        (true, Some(0), _) => "Acompanhado · exato".to_string(),
        (true, Some(d), _) => format!("Acompanhado · exato em ~{d} dias"),
        (true, None, _) => "Acompanhado".to_string(),
        (false, _, Frescor::Atualizado) => format!("Atualizado · ±{}", e.precisao),
        (false, _, Frescor::Envelhecendo { extra }) => format!("Envelhecendo · +{extra}"),
        (false, _, Frescor::Desatualizado { extra }) => format!("Desatualizado · +{extra}"),
        (false, _, Frescor::Vencido) => "Vencido".to_string(),
    }
}


fn badge_prioridade() -> EstiloBadge {
    EstiloBadge { texto: "PRIORIDADE", contorno: theme::TIER_ELITE, fundo: theme::TRANSPARENTE, cor_texto: theme::TIER_ELITE }
}

fn card_escolhido(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState, e: &EscolhidoNaLista, hoje: Option<Date>) -> bool {
    let j = &e.jogador;
    let c = card(ui, &format!("escolhido_{}", j.player_id), ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let visivel = ui.is_rect_visible(c.min, c.max);
    let dl = ui.get_window_draw_list();
    let r_min = [c.min[0] + theme::ESPACO_3, c.min[1] + (ALTURA_CARD - LADO_ROSTO) * 0.5];
    let r_max = [r_min[0] + LADO_ROSTO, r_min[1] + LADO_ROSTO];
    dl.add_rect(r_min, r_max, theme::BG_BASE).filled(true).rounding(theme::RAIO_MD).build();
    match if visivel { state.rosto(j.player_id) } else { Rosto::Carregando } {
        Rosto::Pronto(textura) => dl.add_image(textura, r_min, r_max).build(),
        Rosto::Ausente => silhueta(&dl, r_min, LADO_ROSTO),
        Rosto::Carregando => {}
    }
    let x = r_max[0] + theme::ESPACO_3;
    let largura = c.max[0] - theme::ESPACO_3 - x;
    let mut y = c.min[1] + theme::ESPACO_2;
    let medir = |fonte: Option<imgui::FontId>| move |t: &str| com_fonte(ui, fonte, || ui.calc_text_size(t)[0]);

    let [w_nome, h_nome] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &j.nome);
    let mut bx = x + w_nome + theme::ESPACO_2;
    bx += desenhar_badge(ui, fonts, &dl, &badge_frescor(e.frescor, e.acompanhado), [bx, y], h_nome)[0] + theme::ESPACO_2;
    if e.escolhido.prioridade {
        bx += desenhar_badge(ui, fonts, &dl, &badge_prioridade(), [bx, y], h_nome)[0] + theme::ESPACO_2;
    }
    if e.fora_do_filtro {
        let alerta = EstiloBadge { texto: "FORA DO FILTRO", contorno: theme::DANGER, fundo: theme::TRANSPARENTE, cor_texto: theme::DANGER };
        desenhar_badge_texto(ui, fonts, &dl, &alerta, alerta.texto, [bx, y], h_nome);
    }
    y += h_nome;

    let meta = fonts.map(|f| f.meta);
    let clube = if j.clube.is_empty() { "Sem clube" } else { j.clube.as_str() };
    let bio = format!("{} anos · {} · {} · {clube}", j.idade, nome_posicao(j.posicao), j.nacao);
    let (bio, _) = truncar(&bio, largura, medir(meta));
    y += texto_em(ui, meta, &dl, [x, y], theme::TEXT_SECONDARY, &bio)[1];

    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));
    let ovr = format!("OVR {}", formatar_faixa(j.overall));
    let [w_ovr, h_ovr] = texto_em(ui, mono, &dl, [x, y], theme::TEXT_PRIMARY, &ovr);
    let pot = format!("POT {}", formatar_faixa(j.potencial));
    let [w_pot, _] = texto_em(ui, mono, &dl, [x + w_ovr + theme::ESPACO_3, y], theme::FIELD_GREEN, &pot);
    let atributos = if e.frescor == Frescor::Vencido {
        "atributos ocultos".to_string()
    } else {
        format!("{} atributos observados", j.atributos.len())
    };
    texto_em(ui, meta, &dl, [x + w_ovr + w_pot + theme::ESPACO_3 * 2.0, y + 2.0], theme::TEXT_SECONDARY, &atributos);
    y += h_ovr;

    let situacao = if e.fora_do_filtro { MSG_FORA_DO_FILTRO.to_string() } else { texto_situacao(e, hoje) };
    let cor = match (e.fora_do_filtro, e.acompanhado, e.frescor) {
        (true, _, _) => theme::DANGER,
        (false, true, _) => theme::ACCENT_PRIMARY,
        (false, false, Frescor::Atualizado) => theme::TEXT_SECONDARY,
        (false, false, Frescor::Vencido) => theme::DANGER,
        _ => theme::WARNING,
    };
    let (situacao, cortou) = truncar(&situacao, largura, medir(meta));
    texto_em(ui, meta, &dl, [x, y], cor, &situacao);
    let ativo = ui.is_item_hovered() || (ui.is_item_focused() && ui.io().nav_visible);
    if ativo && cortou {
        ui.tooltip_text(texto_situacao(e, hoje));
    }
    c.ativou
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{escolhido_em, Acompanhamento, Escolhido, FaixaAtributo, JogadorEncontrado};

    fn status(ligada: bool, localizando: bool, escolhidos: bool, conhecimento: bool, erro: Option<&str>) -> StatusNativo {
        StatusNativo { ligada, localizando, escolhidos, conhecimento, erro: erro.map(str::to_string) }
    }

    #[test]
    fn the_sync_line_says_what_is_going_on() {
        assert!(texto_sincronizacao(&status(false, false, true, true, None)).starts_with("Desligada"));
        assert!(texto_sincronizacao(&status(true, false, true, true, None)).starts_with("Ligada: os Escolhidos"));
        assert!(texto_sincronizacao(&status(true, false, true, false, None)).contains("não foi localizado"));
        assert_eq!(texto_sincronizacao(&status(true, true, false, false, None)), "Localizando o scout do jogo…");
        let com_erro = texto_sincronizacao(&status(true, false, true, true, Some("O scout do jogo mudou; nada foi escrito.")));
        assert!(com_erro.ends_with("Último problema: O scout do jogo mudou; nada foi escrito."));
        // desligada não repete um erro velho
        assert!(!texto_sincronizacao(&status(false, false, true, true, Some("x"))).contains("problema"));
    }

    fn jogador() -> JogadorEncontrado {
        JogadorEncontrado {
            player_id: 7,
            nome: "A".to_string(),
            idade: 20,
            posicao: 25,
            nacao_id: 54,
            nacao: "Brazil".to_string(),
            clube: "Clube".to_string(),
            clube_id: None,
            contrato_ate: None,
            observacao: Default::default(),
            overall: FaixaAtributo { min: 70, max: 76 },
            potencial: FaixaAtributo { min: 80, max: 86 },
            atributos: Vec::new(),
            pe: None,
            similaridade: None,
            fit: None,
            fit_alvo: None,
            variacao_overall: None,
            ritmo_ataque: None,
            ritmo_defesa: None,
            estrelas_drible: None,
            pe_fraco: None,
            titular_elenco: None,
            altura: None,
            da_base: false,
            dias_de_curadoria: 0,
            visto_em: None,
            falso_positivo: false,
        }
    }

    fn escolhido(observado: i32) -> Escolhido {
        Escolhido {
            jogador: jogador(),
            adicionado_em: Date(observado),
            observado_em: Date(observado),
            precisao: 3,
            prioridade: false,
            acompanhamento: None,
            alvo: None,
            referencia: None,
            relatorio_id: None,
            no_jogo: false,
            importado: false,
        }
    }

    #[test]
    fn the_summary_and_situation_read_naturally() {
        let r = ResumoAcompanhamento { generalistas: 2, vagas: 14, jogadores: 12, acompanhados: 12 };
        assert_eq!(texto_resumo(&r), "2 Generalistas acompanham 12 de 12 jogadores (14 vagas).");
        let sem = ResumoAcompanhamento { generalistas: 0, vagas: 0, jogadores: 3, acompanhados: 0 };
        assert!(texto_resumo(&sem).contains("envelhecem"));
        let hoje = Date(20360301);
        let recente = escolhido_em(&escolhido(20360201), hoje, false);
        assert_eq!(texto_situacao(&recente, Some(hoje)), "observado em 01/02/2036, há 1 mês: precisão de ±3.");
        let velho = escolhido_em(&escolhido(20350101), hoje, false);
        assert!(texto_situacao(&velho, Some(hoje)).contains("desatualizado"), "{}", texto_situacao(&velho, Some(hoje)));
        let vencido = escolhido_em(&escolhido(20340101), hoje, false);
        assert!(texto_situacao(&vencido, Some(hoje)).contains("análise vencida"));
        let mut acompanhado = escolhido(20360301);
        acompanhado.acompanhamento =
            Some(Acompanhamento { inicio: Date(20360301), precisao_inicial: 3, atributos_iniciais: 28, dias_para_exato: 30, dias: 0 });
        let e = escolhido_em(&acompanhado, hoje, true);
        assert_eq!(texto_situacao(&e, Some(hoje)), "Acompanhado: ±3 agora, exato em ~30 dias de carreira.");
        assert!(!MSG_VAZIA.contains('!') && !MSG_REGRAS.contains('!'));
        assert_eq!(situacao_curta(&recente), "Atualizado · ±3");
        assert!(situacao_curta(&velho).starts_with("Desatualizado"));
        assert_eq!(situacao_curta(&vencido), "Vencido");
        assert_eq!(situacao_curta(&e), "Acompanhado · exato em ~30 dias");
    }
}
