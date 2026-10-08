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

use super::componentes::{self, badge_novo, badge_qualidade, badge_tier, desenhar_badge, texto_em, EstiloBotao};
use super::olheiros;
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::scout::state::{Missao, MissaoNaLista, ModoBusca, ProgressoMissao, ScoutState, StatusMissao};

const LARGURA_CARD: f32 = 440.0;
const ALTURA_CARD: f32 = 330.0;
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
    if linha.missao.continua && linha.missao.status == StatusMissao::Pendente {
        return texto_continua(linha);
    }
    let parcial = match linha.revelados {
        0 => String::new(),
        n => format!("Relatório parcial: {} de {}. ", n, linha.previstos),
    };
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
            format!("{parcial}Relatório pronto em {dias} de carreira ({}).", formatar_data(linha.missao.prazo_estimado))
        }
    }
}

/// Texto de uma Missão contínua em andamento (Story 2.10; contrato de 12
/// meses desde 2026-10-07).
pub fn texto_continua(linha: &MissaoNaLista) -> String {
    let m = &linha.missao;
    let achados = super::aviso::texto_jogadores(linha.revelados);
    let vencido = linha.progresso.is_some_and(|p| p.prazo_atingido);
    match (m.tem_contrato(), vencido) {
        (true, true) => format!(
            "Contrato de 12 meses encerrado em {} ({achados} até agora). Renove para continuar ou encerre a Missão.",
            formatar_data(m.fim_do_contrato())
        ),
        (true, false) => format!(
            "Contrato de 12 meses até {} · {achados} até agora · {}",
            formatar_data(m.fim_do_contrato()),
            if m.renovar_sozinho { "renova sozinho." } else { "não renova sozinho." }
        ),
        // contínua de antes, paga bloco a bloco
        (false, true) => format!(
            "Bloco {} encerrado em {} ({achados} até agora). Renove para continuar ou encerre a Missão.",
            m.blocos,
            formatar_data(m.prazo_estimado)
        ),
        (false, false) => format!("Contínua · bloco {} até {} · {achados} até agora.", m.blocos, formatar_data(m.prazo_estimado)),
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
    /// Y (ou o botão direito) num card: as Opções do Olheiro da Missão
    /// (renovar, cancelar, ajustar o perfil...).
    Opcoes(Uuid),
}

/// Desenha a aba: os cards das Missões em grade. `pode_encomendar` é `false`
/// no estado de erro de leitura (a lista aparece, sem o botão nem as
/// opções).
pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState, pode_encomendar: bool) -> Acao {
    let mut acao = Acao::Nenhuma;
    if pode_encomendar && componentes::botao(ui, fonts, "Nova Missão", EstiloBotao::Primario, true) {
        acao = Acao::NovaMissao;
    }
    if pode_encomendar {
        ui.same_line_with_spacing(0.0, theme::ESPACO_4);
        com_fonte(ui, fonts.map(|f| f.meta), || {
            let y = ui.cursor_pos()[1];
            ui.set_cursor_pos([ui.cursor_pos()[0], y + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
            ui.text_colored(theme::TEXT_SECONDARY, "A abre o Relatório · Y (ou botão direito) abre as opções da Missão: renovar, cancelar, ajustar o perfil.");
        });
    }
    ui.dummy([0.0, theme::ESPACO_3]);

    let missoes = state.missoes();
    if missoes.is_empty() {
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, MSG_SEM_MISSOES));
        return acao;
    }
    let pediu_opcoes = pode_encomendar && state.opcoes_pedidas();
    let avisos: Vec<Option<String>> = missoes.iter().map(|l| aviso_do_contrato(state, &l.missao)).collect();
    let regioes: Vec<String> = missoes.iter().map(|l| state.regiao_da_missao(&l.missao.filtros)).collect();
    let nacoes_e_bandeiras = |ui: &Ui, fonts: Option<&Fonts>, linha: &MissaoNaLista, regiao: &str, aviso: Option<&str>| {
        olheiros::com_nacoes(state, |nacoes| card_missao(ui, fonts, linha, regiao, aviso, nacoes))
    };
    let por_linha = (((ui.content_region_avail()[0] + theme::ESPACO_3) / (LARGURA_CARD + theme::ESPACO_3)).floor() as usize).max(1);
    for (indice, linha) in missoes.iter().enumerate() {
        if indice % por_linha != 0 {
            ui.same_line_with_spacing(0.0, theme::ESPACO_3);
        }
        let r = nacoes_e_bandeiras(ui, fonts, linha, &regioes[indice], avisos[indice].as_deref());
        if r.ativou {
            if let Some(id) = linha.relatorio_id {
                acao = Acao::AbrirRelatorio(id);
            }
        }
        if pode_encomendar && (r.opcoes || (r.focado && pediu_opcoes)) && linha.olheiro.is_some() {
            acao = Acao::Opcoes(linha.missao.olheiro_id);
        }
        if indice == 0 && r.focado && ui.scroll_y() > 0.0 {
            ui.set_scroll_y(0.0);
        }
        if indice % por_linha == por_linha - 1 {
            ui.dummy([0.0, theme::ESPACO_1]);
        }
    }
    acao
}

/// O aviso do contrato de uma Missão contínua que pede atenção (a renovação
/// falhou, ou não há verba): `None` se está tudo certo.
pub fn aviso_do_contrato(state: &ScoutState, m: &Missao) -> Option<String> {
    if !m.continua {
        return None;
    }
    let vencido = state.contrato_vencido(m);
    let falta = state
        .orcamento()
        .map(|saldo| ScoutState::custo_do_contrato(m).saturating_sub(saldo))
        .filter(|f| *f > 0);
    match (state.erro_da_missao(m.id), falta) {
        (Some(erro), _) if vencido && m.tem_contrato() && m.renovar_sozinho => {
            Some(format!("O contrato não renovou sozinho. {}", super::nova_missao::texto_erro(erro)))
        }
        (Some(erro), _) => Some(super::nova_missao::texto_erro(erro)),
        (None, Some(f)) if vencido => Some(super::olheiros::texto_faltam(f)),
        _ => None,
    }
}

/// A linha de apoio sobre o contrato (a renovação e a multa), para as Opções.
pub fn texto_dica_do_contrato(m: &Missao, custo: i32) -> String {
    if !m.tem_contrato() {
        return "Renovar fica disponível quando o bloco terminar.".to_string();
    }
    let carencia = if m.contratos.len() == 1 { " Até lá, tirar o Olheiro para outra localidade cobra a multa dele." } else { "" };
    let renovacao = if m.renovar_sozinho {
        format!("No fim, renova sozinho por {} se houver verba.", super::formatar_milhar(custo))
    } else {
        "No fim, o contrato acaba (a renovação sozinha está desligada).".to_string()
    };
    format!("{renovacao} Encerrar antes do fim não devolve o que foi pago.{carencia}")
}

/// O perfil que a Missão pede, numa frase: posições, nível, idade, Overall,
/// Potencial, atributos, fit, referência, tetos de valor e salário.
pub fn texto_perfil(m: &Missao) -> String {
    let f = &m.filtros;
    let mut partes: Vec<String> = Vec::new();
    if !f.posicoes.is_empty() {
        partes.push(f.posicoes.iter().map(|p| p.sigla()).collect::<Vec<_>>().join("/"));
    }
    if let Some(nivel) = f.nivel_elenco {
        partes.push(nivel.nome().to_lowercase());
    }
    let detalhes = super::nova_missao::resumo_detalhes(f);
    if detalhes != "Nenhum filtro extra." {
        partes.push(detalhes);
    }
    if let Some(teto) = f.teto_valor {
        partes.push(format!("até {}", super::relatorio::formatar_dinheiro(teto)));
    }
    if let Some(teto) = f.teto_salario {
        partes.push(format!("salário até {}/sem", super::relatorio::formatar_dinheiro(teto)));
    }
    if partes.is_empty() {
        "Sem filtros: qualquer jogador".to_string()
    } else {
        partes.join(" · ")
    }
}

/// Quebra `texto` em até `max_linhas` linhas de `largura` px (por palavras),
/// terminando em "…" se não coube.
pub fn quebrar(texto: &str, largura: f32, max_linhas: usize, medir: impl Fn(&str) -> f32) -> Vec<String> {
    let mut linhas: Vec<String> = Vec::new();
    let mut atual = String::new();
    for palavra in texto.split_whitespace() {
        let candidata = if atual.is_empty() { palavra.to_string() } else { format!("{atual} {palavra}") };
        if medir(&candidata) <= largura || atual.is_empty() {
            atual = candidata;
        } else {
            linhas.push(std::mem::take(&mut atual));
            atual = palavra.to_string();
        }
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    if linhas.len() > max_linhas {
        linhas.truncate(max_linhas);
        if let Some(ultima) = linhas.last_mut() {
            let (cortada, _) = super::relatorio::truncar(&format!("{ultima}…"), largura, &medir);
            *ultima = cortada;
        }
    }
    linhas
}

/// O que o jogador fez no card neste frame.
struct ResultadoCard {
    ativou: bool,
    focado: bool,
    opcoes: bool,
}

/// Card vertical de uma Missão (2026-10-08): o tipo e a Qualidade, o Olheiro,
/// onde procura, o perfil pedido, a barra com o texto de andamento e quantos
/// jogadores já vieram. `true` em `ativou` só faz algo com Relatório pronto.
fn card_missao(
    ui: &Ui,
    fonts: Option<&Fonts>,
    linha: &MissaoNaLista,
    regiao: &str,
    aviso: Option<&str>,
    nacoes: &olheiros::Nacoes<'_>,
) -> ResultadoCard {
    let missao = &linha.missao;
    let _id = ui.push_id(missao.id.to_string());
    let min = ui.cursor_screen_pos();
    let max = [min[0] + LARGURA_CARD, min[1] + ALTURA_CARD];
    let ativou = ui.invisible_button("##card", [LARGURA_CARD, ALTURA_CARD]);
    let hover = ui.is_item_hovered();
    let focado = ui.is_item_focused() && ui.io().nav_visible;
    let opcoes = ui.is_item_clicked_with_button(imgui::MouseButton::Right);
    let dl = ui.get_window_draw_list();
    let (borda, espessura) = if hover || focado { (theme::ACCENT_PRIMARY, super::ESPESSURA_FOCO) } else { (theme::BORDER_HAIRLINE_SUBTLE, 1.0) };
    dl.add_rect(min, max, theme::BG_PANEL_RAISED).filled(true).rounding(theme::RAIO_MD).build();
    dl.add_rect(min, max, borda).rounding(theme::RAIO_MD).thickness(espessura).build();

    let meta = fonts.map(|f| f.meta);
    let medir_meta = |t: &str| com_fonte(ui, meta, || ui.calc_text_size(t)[0]);
    let x = min[0] + theme::ESPACO_3;
    let direita = max[0] - theme::ESPACO_3;
    let largura = direita - x;
    let mut y = min[1] + theme::ESPACO_3;

    // ---- o tipo (e a Qualidade à direita), com a natureza e a verba embaixo
    let titulo = format!("Missão {}", super::nova_missao::nome_tipo(missao.tipo));
    let [_, altura_titulo] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &titulo);
    let qualidade = badge_qualidade(missao.estimativa.qualidade);
    let largura_badge = com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(qualidade.texto)[0]) + theme::ESPACO_2 * 2.0;
    desenhar_badge(ui, fonts, &dl, &qualidade, [direita - largura_badge, y], altura_titulo);
    y += altura_titulo + theme::ESPACO_1;
    let natureza = if missao.continua { "Contínua" } else { "Prazo fixo" };
    let detalhe = format!(
        "{natureza} · {} · {} · verba {}",
        nome_modo(missao.modo_busca),
        nome_status(missao.status, linha.progresso).to_lowercase(),
        missao.investimento.nome().to_lowercase()
    );
    y += texto_em(ui, meta, &dl, [x, y], theme::TEXT_SECONDARY, &detalhe)[1] + theme::ESPACO_2;

    // ---- o Olheiro
    let altura_nome = com_fonte(ui, fonts.map(|f| f.body), || ui.text_line_height());
    let mut xo = x;
    if let Some(n) = linha.olheiro.as_ref().and_then(|o| o.nacao.as_ref()) {
        let altura_bandeira = (altura_nome * 0.8).round();
        xo += componentes::bandeira(&dl, (nacoes.bandeira)(n.id), [x, y + (altura_nome - altura_bandeira) * 0.5], altura_bandeira) + theme::ESPACO_2;
    }
    let nome = linha.olheiro.as_ref().map_or_else(|| "Olheiro removido".to_string(), |o| o.nome_exibicao());
    let [largura_nome, _] = texto_em(ui, fonts.map(|f| f.body), &dl, [xo, y], theme::TEXT_PRIMARY, &nome);
    let mut xb = xo + largura_nome + theme::ESPACO_2;
    if let Some(o) = &linha.olheiro {
        xb += desenhar_badge(ui, fonts, &dl, &badge_tier(o.tier), [xb, y], altura_nome)[0] + theme::ESPACO_2;
    }
    if linha.relatorio_novo {
        desenhar_badge(ui, fonts, &dl, &badge_novo(), [xb, y], altura_nome);
    }
    y += altura_nome + theme::ESPACO_2;
    y = divisor(&dl, x, direita, y);

    // ---- onde procura e o perfil pedido
    y = rotulo_e_texto(ui, meta, &dl, [x, y], largura, "Onde", regiao, 2, &medir_meta);
    y = rotulo_e_texto(ui, meta, &dl, [x, y], largura, "Perfil", &texto_perfil(missao), 3, &medir_meta);
    y = divisor(&dl, x, direita, y + theme::ESPACO_1);

    // ---- andamento: barra e o texto (nunca a barra sozinha: UX-DR7)
    if let Some(fracao) = fracao_da_barra(linha) {
        dl.add_rect([x, y], [direita, y + ALTURA_BARRA], theme::BG_BASE).filled(true).rounding(theme::RAIO_SM).build();
        if fracao > 0.0 {
            let cor = if missao.status == StatusMissao::Concluida { theme::FIELD_GREEN } else { theme::ACCENT_PRIMARY };
            dl.add_rect([x, y], [x + largura * fracao, y + ALTURA_BARRA], cor).filled(true).rounding(theme::RAIO_SM).build();
        }
    }
    y += ALTURA_BARRA + theme::ESPACO_2;
    let pronta = linha.progresso.is_some_and(|p| p.prazo_atingido) || missao.status == StatusMissao::Concluida;
    let (texto, cor) = match (aviso, &linha.falha, pronta) {
        (Some(aviso), _, _) => (aviso.to_string(), theme::DANGER),
        (None, Some(_), _) if missao.status == StatusMissao::Pendente => (texto_estimativa(linha), theme::DANGER),
        (None, _, true) => (texto_estimativa(linha), theme::FIELD_GREEN),
        _ => (texto_estimativa(linha), theme::TEXT_SECONDARY),
    };
    for parte in quebrar(&texto, largura, 3, &medir_meta) {
        y += texto_em(ui, meta, &dl, [x, y], cor, &parte)[1];
    }

    // ---- rodapé: jogadores e a dica das Opções
    let altura_meta = com_fonte(ui, meta, || ui.text_line_height());
    let y_rodape = max[1] - theme::ESPACO_3 - altura_meta;
    let jogadores = format!("{} de {} jogadores", linha.revelados, linha.previstos);
    texto_em(ui, meta, &dl, [x, y_rodape], theme::TEXT_PRIMARY, &jogadores);
    let dica = "Y  Opções";
    texto_em(ui, meta, &dl, [direita - medir_meta(dica), y_rodape], theme::TEXT_SECONDARY, dica);
    ResultadoCard { ativou, focado, opcoes }
}

/// Linha fina entre os blocos do card; devolve o `y` de depois dela.
fn divisor(dl: &imgui::DrawListMut<'_>, x0: f32, x1: f32, y: f32) -> f32 {
    dl.add_line([x0, y], [x1, y], theme::BORDER_HAIRLINE_SUBTLE).build();
    y + theme::ESPACO_2
}

/// "Onde" em cinza e, ao lado, o texto em até `max_linhas` linhas.
#[allow(clippy::too_many_arguments)]
fn rotulo_e_texto(
    ui: &Ui,
    meta: Option<imgui::FontId>,
    dl: &imgui::DrawListMut<'_>,
    pos: [f32; 2],
    largura: f32,
    rotulo: &str,
    texto: &str,
    max_linhas: usize,
    medir: &dyn Fn(&str) -> f32,
) -> f32 {
    const LARGURA_ROTULO: f32 = 52.0;
    texto_em(ui, meta, dl, pos, theme::TEXT_SECONDARY, rotulo);
    let mut y = pos[1];
    for linha in quebrar(texto, largura - LARGURA_ROTULO, max_linhas, medir) {
        y += texto_em(ui, meta, dl, [pos[0] + LARGURA_ROTULO, y], theme::TEXT_PRIMARY, &linha)[1];
    }
    y + theme::ESPACO_1
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
            revelados: 0,
            previstos: 17,
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
        let parcial = MissaoNaLista { revelados: 9, ..linha(StatusMissao::Pendente, Some(20260706)) };
        assert_eq!(
            texto_estimativa(&parcial),
            "Relatório parcial: 9 de 17. Relatório pronto em ~5 dias de carreira (11/07/2026)."
        );
        let mut continua = linha(StatusMissao::Pendente, Some(20260711));
        continua.missao.continua = true;
        continua.revelados = 17;
        assert_eq!(
            texto_estimativa(&continua),
            "Bloco 1 encerrado em 11/07/2026 (17 jogadores até agora). Renove para continuar ou encerre a Missão."
        );
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

    #[test]
    fn the_profile_sentence_lists_what_the_missao_asks_for() {
        use crate::scout::quality::{NivelEquipe, Perfil};
        let mut m = Missao::de_teste(Uuid::new_v4(), StatusMissao::Pendente);
        assert_eq!(texto_perfil(&m), "Sem filtros: qualquer jogador");
        m.filtros.posicoes = vec![Perfil::MeioCampista, Perfil::Centroavante];
        m.filtros.nivel_elenco = Some(NivelEquipe::MudaPatamar);
        m.filtros.idade = crate::scout::state::FaixaAtributo { min: 18, max: 23 };
        m.filtros.teto_valor = Some(15_000_000);
        let texto = texto_perfil(&m);
        assert!(texto.starts_with(&format!("{}/{}", Perfil::MeioCampista.sigla(), Perfil::Centroavante.sigla())), "{texto}");
        assert!(texto.contains("muda patamar") && texto.contains("Idade 18–23") && texto.ends_with("até 15,0 M"), "{texto}");
    }

    #[test]
    fn long_texts_wrap_by_words_and_end_with_an_ellipsis_when_they_do_not_fit() {
        let medir = |t: &str| t.chars().count() as f32;
        assert_eq!(quebrar("um dois tres", 7.0, 3, medir), vec!["um dois", "tres"]);
        assert_eq!(quebrar("curto", 20.0, 2, medir), vec!["curto"]);
        let cortado = quebrar("a b c d e f g h i j k l", 5.0, 2, medir);
        assert_eq!(cortado.len(), 2);
        assert!(cortado[1].ends_with('…'), "{cortado:?}");
        assert_eq!(quebrar("", 10.0, 2, medir), Vec::<String>::new());
        assert_eq!(quebrar("palavra-gigante", 5.0, 2, medir), vec!["palavra-gigante"], "palavra maior que a linha fica inteira");
    }
}
