//! Tela do Relatório (Story 2.5, UX-DR14): tela satélite aberta a partir
//! da aba Missões ou Relatórios. Cabeçalho com a Missão, o Olheiro e o
//! badge de Qualidade; abaixo, um card por jogador (Story 2.6). A visão
//! Tabular saiu em 2026-10-03 (pedido do Felipe): o card mostra o que
//! importa para decidir — rosto, idade, posição, Fit, Overall/Potencial,
//! valor e salário estimados, tempo de contrato e a comparação com o
//! titular do elenco na posição.
//!
//! Nada é inventado: valores aparecem como o Olheiro revelou — faixa
//! ("65–78") ou exato ("72"). No Relatório parcial o Olheiro observa por
//! etapas (branch `claude/relatorio-ficha`): primeiro mercado e contrato,
//! depois o salário, por fim os atributos. Texto longo é cortado com "…" e
//! o texto inteiro aparece no tooltip ao passar o mouse ou focar o card
//! com o controle (NFR5).
//!
//! Épico 3: ativar um card abre a Ficha do jogador (Story 3.1). Fit
//! Posicional (3.5) e similaridade com o Jogador de Referência (3.3) são
//! calculados pelo que o Olheiro revelou e levam "≈" abaixo da Qualidade
//! Alta.

use imgui::Ui;

use super::componentes::{
    self, badge_escolhido, badge_fit, badge_qualidade, badge_tier, card_com_largura, desenhar_badge_texto, texto_em, EstiloBotao,
};
use super::theme::{self, Fonts};
use super::{com_fonte, formatar_data};
use crate::save_repo::{nome_posicao, Date};
use crate::scout::minifaces::Rosto;
use crate::scout::state::{meio_da_faixa, FaixaAtributo, JogadorEncontrado, PosicaoAlvo, Qualidade, RelatorioNaLista, ScoutState};

const LARGURA_CARD: f32 = 460.0;
const ALTURA_CARD: f32 = 196.0;
const LADO_ROSTO: f32 = 112.0;
/// Atributos mostrados em cada card (os primeiros que o Olheiro observou).
const ATRIBUTOS_NO_CARD: usize = 3;
/// Contrato com até tantos meses aparece em destaque ("a vencer").
const MESES_A_VENCER: i32 = 6;

pub const MSG_BAIXA: &str =
    "Relatório de Qualidade baixa: os valores aparecem em faixas largas e só alguns atributos foram observados.";
pub const MSG_SEM_JOGADORES: &str = "O Olheiro não encontrou nenhum jogador com esses filtros.";
pub const MSG_PARCIAL: &str = "Relatório parcial: mais jogadores aparecem conforme os dias de carreira passam.";
pub const MSG_PARCIAL_VAZIO: &str = "O Olheiro ainda não enviou nenhum nome. Volte em alguns dias de carreira.";
pub const MSG_EM_OBSERVACAO: &str = "Atributos em observação…";
const NAO_OBSERVADO: &str = "—";

/// O que o jogador fez na tela neste frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Voltar,
    /// Abrir a Ficha deste jogador (Story 3.1).
    AbrirFicha(u32),
}

/// Similaridade e fit são "≈" abaixo da Qualidade Alta: vêm de faixas
/// largas e de poucos atributos observados (Story 3.3).
pub fn aproximado(qualidade: Qualidade) -> bool {
    qualidade < Qualidade::Alta
}

/// "87%" ou "≈87%"; sem valor, "—".
pub fn texto_percentual(valor: Option<u8>, aproximado: bool) -> String {
    match valor {
        Some(v) if aproximado => format!("≈{v}%"),
        Some(v) => format!("{v}%"),
        None => NAO_OBSERVADO.to_string(),
    }
}

/// "VOL ≈96% (-2)": posição-alvo, força do fit e, se houver, a variação
/// estimada do Overall jogando lá.
pub fn texto_fit(alvo: PosicaoAlvo, forca: Option<u8>, variacao: Option<i8>, aproximado: bool) -> String {
    let base = format!("{} {}", alvo.sigla(), texto_percentual(forca, aproximado));
    match variacao {
        Some(v) => format!("{base} ({})", texto_variacao(v)),
        None => base,
    }
}

/// "+3", "-2", "±0".
pub fn texto_variacao(variacao: i8) -> String {
    if variacao == 0 {
        "±0".to_string()
    } else {
        format!("{variacao:+}")
    }
}

/// "18,6 M", "350 mil", "900".
pub fn formatar_dinheiro(valor: i64) -> String {
    if valor >= 1_000_000 {
        let decimos = (valor + 50_000) / 100_000;
        format!("{},{} M", decimos / 10, decimos % 10)
    } else if valor >= 1_000 {
        format!("{} mil", valor / 1_000)
    } else {
        valor.to_string()
    }
}

/// Meses de contrato que faltam em `hoje`: o contrato do FIFA termina no
/// fim da temporada, 30 de junho do ano de término.
pub fn meses_de_contrato(ano: u16, hoje: Date) -> i32 {
    (i32::from(ano) * 12 + 6) - (hoje.year() * 12 + hoje.month())
}

/// Quanto falta de contrato: "1 ano 4 meses". Sem a data da carreira, o
/// ano ("até 2031"); Relatório antigo ou sem clube, "—".
pub fn formatar_contrato(ano: Option<u16>, hoje: Option<Date>) -> String {
    let Some(ano) = ano else {
        return NAO_OBSERVADO.to_string();
    };
    let Some(hoje) = hoje else {
        return format!("até {ano}");
    };
    let meses = meses_de_contrato(ano, hoje);
    if meses <= 0 {
        return "termina nesta temporada".to_string();
    }
    let (anos, resto) = (meses / 12, meses % 12);
    let texto_anos = match anos {
        0 => None,
        1 => Some("1 ano".to_string()),
        a => Some(format!("{a} anos")),
    };
    let texto_meses = match resto {
        0 => None,
        1 => Some("1 mês".to_string()),
        m => Some(format!("{m} meses")),
    };
    [texto_anos, texto_meses].into_iter().flatten().collect::<Vec<_>>().join(" ")
}

/// Contrato acabando (até `MESES_A_VENCER` meses): o card destaca.
pub fn contrato_a_vencer(ano: Option<u16>, hoje: Option<Date>) -> bool {
    matches!((ano, hoje), (Some(a), Some(h)) if meses_de_contrato(a, h) <= MESES_A_VENCER)
}

/// "Valor ≈ 18,6 M · Salário ≈ 120 mil/sem" (o salário só depois que o
/// Olheiro o descobre, no Relatório parcial).
pub fn texto_mercado(j: &JogadorEncontrado) -> String {
    let valor = format!("Valor ≈ {}", formatar_dinheiro(j.valor_estimado()));
    if j.salario_conhecido() {
        format!("{valor} · Salário ≈ {}/sem", formatar_dinheiro(j.salario_estimado()))
    } else {
        format!("{valor} · salário em observação")
    }
}

/// "Seu titular: 71 (+3)" — o Overall (meio da faixa) comparado ao titular
/// do elenco na posição com que ele foi comparado na busca.
pub fn texto_titular(j: &JogadorEncontrado) -> Option<String> {
    let titular = j.titular_elenco?;
    let diferenca = i16::from(meio_da_faixa(j.overall)) - i16::from(titular);
    let sinal = i8::try_from(diferenca.clamp(-99, 99)).unwrap_or(0);
    Some(format!("seu titular: {titular} ({})", texto_variacao(sinal)))
}

/// O que a Missão pediu de perfil (decide os extras do card).
#[derive(Debug, Clone, PartialEq)]
pub struct PerfilPedido {
    pub alvo: Option<PosicaoAlvo>,
    /// Nome do Jogador de Referência.
    pub referencia: Option<String>,
    pub aproximado: bool,
}

impl PerfilPedido {
    pub fn de(item: &RelatorioNaLista) -> PerfilPedido {
        let filtros = item.missao.as_ref().map(|m| &m.filtros);
        PerfilPedido {
            alvo: filtros.and_then(|f| f.fit_posicional),
            referencia: filtros.and_then(|f| f.referencia.as_ref()).map(|r| r.nome.clone()),
            aproximado: aproximado(item.relatorio.qualidade),
        }
    }
}

/// "72" quando exato, "65–78" quando faixa.
pub fn formatar_faixa(faixa: FaixaAtributo) -> String {
    if faixa.min == faixa.max {
        faixa.min.to_string()
    } else {
        format!("{}–{}", faixa.min, faixa.max)
    }
}

/// Jogadores do melhor para o pior pelo meio da faixa de Overall (empate:
/// Potencial, depois nome).
pub fn ordenar(jogadores: &[JogadorEncontrado]) -> Vec<&JogadorEncontrado> {
    let meio = |f: FaixaAtributo| u16::from(f.min) + u16::from(f.max);
    let mut lista: Vec<&JogadorEncontrado> = jogadores.iter().collect();
    lista.sort_by(|a, b| {
        meio(b.overall)
            .cmp(&meio(a.overall))
            .then(meio(b.potencial).cmp(&meio(a.potencial)))
            .then(a.nome.cmp(&b.nome))
    });
    lista
}

/// Corta `texto` para caber em `largura` (medida por `medir`), terminando
/// em "…". Devolve o texto e se cortou.
pub fn truncar(texto: &str, largura: f32, medir: impl Fn(&str) -> f32) -> (String, bool) {
    if medir(texto) <= largura {
        return (texto.to_string(), false);
    }
    let mut corte: String = texto.to_string();
    while !corte.is_empty() {
        corte.pop();
        let candidato = format!("{}…", corte.trim_end());
        if medir(&candidato) <= largura {
            return (candidato, true);
        }
    }
    ("…".to_string(), true)
}

/// Título da tela: "Relatório · Missão Jovens".
pub fn titulo(item: &RelatorioNaLista) -> String {
    match &item.missao {
        Some(m) => format!("Relatório · Missão {}", super::nova_missao::nome_tipo(m.tipo)),
        None => "Relatório".to_string(),
    }
}

/// Linha de detalhes sob o título.
pub fn detalhe(item: &RelatorioNaLista) -> String {
    let r = &item.relatorio;
    let mut partes = Vec::new();
    if let Some(o) = &item.olheiro {
        partes.push(format!("{} ({})", o.nome_exibicao(), o.tier.nome()));
    }
    if let Some(m) = &item.missao {
        partes.push(super::missoes::nome_modo(m.modo_busca).to_string());
        if let Some(nivel) = m.filtros.nivel_elenco {
            partes.push(nivel.nome().to_lowercase());
        }
        if !m.filtros.atributos_dominantes.is_empty() {
            partes.push(format!("foco em {}", super::nova_missao::texto_atributos(&m.filtros.atributos_dominantes)));
        }
        if let Some(alvo) = m.filtros.fit_posicional {
            partes.push(format!("fit em {}", alvo.nome()));
        }
        if let Some(r) = &m.filtros.referencia {
            partes.push(format!("parecidos com {}", r.nome));
        }
        if let Some(teto) = m.filtros.teto_valor {
            partes.push(format!("até {}", formatar_dinheiro(teto)));
        }
    }
    if let Some(data) = r.gerado_em {
        partes.push(format!("gerado em {}", formatar_data(data)));
    }
    partes.push(if item.parcial {
        format!("parcial: {} de {} jogadores até agora", r.jogadores.len(), item.previstos)
    } else {
        super::aviso::texto_jogadores(r.jogadores.len())
    });
    partes.push(if r.precisao_mais_menos == 0 {
        "valores exatos".to_string()
    } else {
        format!("precisão de ±{}", r.precisao_mais_menos)
    });
    partes.join(" · ")
}

pub fn render(ui: &Ui, fonts: Option<&Fonts>, state: &mut ScoutState) -> Acao {
    let Some(item) = state.relatorio_aberto() else {
        return Acao::Voltar;
    };
    let mut acao = Acao::Nenhuma;
    if componentes::botao(ui, fonts, "Voltar", EstiloBotao::Secundario, true) {
        acao = Acao::Voltar;
    }
    // Arquivar daqui também (o Relatório já está aberto): volta à lista,
    // onde ele passa para "Arquivados" (Story 2.7).
    if ScoutState::pode_arquivar(&item) {
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        if componentes::botao(ui, fonts, "Arquivar", EstiloBotao::Secundario, true) && state.arquivar_relatorio(item.relatorio.id) {
            acao = Acao::Voltar;
        }
    }
    ui.same_line_with_spacing(0.0, theme::ESPACO_4);
    cabecalho(ui, fonts, &item);
    ui.dummy([0.0, theme::ESPACO_2]);

    let r = &item.relatorio;
    if r.qualidade == Qualidade::Baixa {
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::WARNING, MSG_BAIXA));
    }
    if item.parcial {
        com_fonte(ui, fonts.map(|f| f.meta), || ui.text_colored(theme::TEXT_SECONDARY, MSG_PARCIAL));
    }
    if r.jogadores.is_empty() {
        let msg = if item.parcial { MSG_PARCIAL_VAZIO } else { MSG_SEM_JOGADORES };
        com_fonte(ui, fonts.map(|f| f.body), || ui.text_colored(theme::TEXT_SECONDARY, msg));
        return acao;
    }
    let perfil = PerfilPedido::de(&item);
    let hoje = state.data_da_carreira();
    match cards(ui, fonts, state, &r.jogadores, &perfil, hoje) {
        Some(player_id) if acao == Acao::Nenhuma => Acao::AbrirFicha(player_id),
        _ => acao,
    }
}

/// Grade de cards. Devolve o jogador cujo card foi ativado (abre a Ficha).
fn cards(
    ui: &Ui,
    fonts: Option<&Fonts>,
    state: &ScoutState,
    jogadores: &[JogadorEncontrado],
    perfil: &PerfilPedido,
    hoje: Option<Date>,
) -> Option<u32> {
    let mut ativado = None;
    ui.child_window("##cards_relatorio").size([0.0, 0.0]).border(false).flags(super::flags_conteudo()).build(|| {
        super::rolar_com_analogico(ui, state.rolagem());
        let disponivel = ui.content_region_avail()[0];
        let por_linha = (((disponivel + theme::ESPACO_3) / (LARGURA_CARD + theme::ESPACO_3)).floor() as usize).max(1);
        for (indice, j) in ordenar(jogadores).into_iter().enumerate() {
            if indice % por_linha != 0 {
                ui.same_line_with_spacing(0.0, theme::ESPACO_3);
            }
            if card_jogador(ui, fonts, state, j, perfil, hoje) {
                ativado = Some(j.player_id);
            }
            if indice % por_linha == por_linha - 1 {
                ui.dummy([0.0, theme::ESPACO_1]);
            }
        }
    });
    ativado
}

/// Card de um jogador; `true` = ativado.
fn card_jogador(ui: &Ui, fonts: Option<&Fonts>, state: &ScoutState, j: &JogadorEncontrado, perfil: &PerfilPedido, hoje: Option<Date>) -> bool {
    let c = card_com_largura(ui, &j.player_id.to_string(), LARGURA_CARD, ALTURA_CARD, theme::BORDER_HAIRLINE_SUBTLE);
    let ativo = ui.is_item_hovered() || (ui.is_item_focused() && ui.io().nav_visible);
    // Rosto só para cards visíveis: a lista pode ser longa (carga preguiçosa).
    let visivel = ui.is_rect_visible(c.min, c.max);
    let dl = ui.get_window_draw_list();

    let r_min = [c.min[0] + theme::ESPACO_3, c.min[1] + theme::ESPACO_3];
    let r_max = [r_min[0] + LADO_ROSTO, r_min[1] + LADO_ROSTO];
    dl.add_rect(r_min, r_max, theme::BG_BASE).filled(true).rounding(theme::RAIO_MD).build();
    match if visivel { state.rosto(j.player_id) } else { Rosto::Carregando } {
        Rosto::Pronto(textura) => dl.add_image(textura, r_min, r_max).build(),
        Rosto::Ausente => silhueta(&dl, r_min, LADO_ROSTO),
        Rosto::Carregando => {}
    }

    let x = r_max[0] + theme::ESPACO_3;
    let largura_texto = c.max[0] - theme::ESPACO_3 - x;
    let mut y = c.min[1] + theme::ESPACO_3;
    let medir = |fonte: Option<imgui::FontId>| move |t: &str| com_fonte(ui, fonte, || ui.calc_text_size(t)[0]);
    let meta = fonts.map(|f| f.meta);
    let mono = fonts.and_then(|f| f.mono).or(fonts.map(|f| f.body));

    // Já na Lista de Escolhidos (Épico 6): badge ao lado do nome.
    let escolhido = state.esta_nos_escolhidos(j.player_id);
    let largura_badge = if escolhido {
        com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(badge_escolhido().texto)[0]) + theme::ESPACO_2 * 3.0
    } else {
        0.0
    };
    let (nome, nome_cortado) = truncar(&j.nome, largura_texto - largura_badge, medir(fonts.map(|f| f.heading)));
    let [w_nome, h_nome] = texto_em(ui, fonts.map(|f| f.heading), &dl, [x, y], theme::TEXT_PRIMARY, &nome);
    if escolhido {
        componentes::desenhar_badge(ui, fonts, &dl, &badge_escolhido(), [x + w_nome + theme::ESPACO_2, y], h_nome);
    }
    y += h_nome;

    let clube = if j.clube.is_empty() { "Sem clube" } else { j.clube.as_str() };
    // Fit Posicional: badge extra na linha da posição nativa (Story 3.5).
    let fit = perfil
        .alvo
        .filter(|_| j.atributos_observados())
        .map(|alvo| format!("FIT {}", texto_fit(alvo, j.fit, j.variacao_overall, perfil.aproximado)));
    let largura_fit = fit.as_ref().map_or(0.0, |t| {
        com_fonte(ui, fonts.map(|f| f.badge), || ui.calc_text_size(t)[0]) + theme::ESPACO_2 * 3.0
    });
    let linha2 = format!("{} anos · {} · {}", j.idade, nome_posicao(j.posicao), j.nacao);
    let (linha2, cortou2) = truncar(&linha2, largura_texto - largura_fit, medir(meta));
    let [w2, h2] = texto_em(ui, meta, &dl, [x, y], theme::TEXT_SECONDARY, &linha2);
    if let Some(texto) = &fit {
        desenhar_badge_texto(ui, fonts, &dl, &badge_fit(), texto, [x + w2 + theme::ESPACO_2, y], h2);
    }
    y += h2;
    let (clube_visivel, cortou3) = truncar(clube, largura_texto, medir(meta));
    y += texto_em(ui, meta, &dl, [x, y], theme::TEXT_SECONDARY, &clube_visivel)[1] + theme::ESPACO_1;

    let ovr = format!("OVR {}", formatar_faixa(j.overall));
    let [w_ovr, h_ovr] = texto_em(ui, mono, &dl, [x, y], theme::TEXT_PRIMARY, &ovr);
    let x_pot = x + w_ovr + theme::ESPACO_3;
    let [w_pot, _] = texto_em(ui, mono, &dl, [x_pot, y], theme::FIELD_GREEN, &format!("POT {}", formatar_faixa(j.potencial)));
    if perfil.referencia.is_some() && j.atributos_observados() {
        let sim = format!("SIM {}", texto_percentual(j.similaridade, perfil.aproximado));
        texto_em(ui, mono, &dl, [x_pot + w_pot + theme::ESPACO_3, y], theme::ACCENT_PRIMARY, &sim);
    }
    y += h_ovr + theme::ESPACO_1;

    // Abaixo do rosto: a largura toda do card.
    let x_largo = c.min[0] + theme::ESPACO_3;
    let largura_larga = c.max[0] - theme::ESPACO_3 - x_largo;
    y = y.max(r_max[1] + theme::ESPACO_2);
    let (mercado, _) = truncar(&texto_mercado(j), largura_larga, medir(meta));
    y += texto_em(ui, meta, &dl, [x_largo, y], theme::TEXT_PRIMARY, &mercado)[1];
    let contrato = format!("Contrato: {}", formatar_contrato(j.contrato_ate, hoje));
    let cor_contrato = if contrato_a_vencer(j.contrato_ate, hoje) { theme::WARNING } else { theme::TEXT_SECONDARY };
    let [w_contrato, h_contrato] = texto_em(ui, meta, &dl, [x_largo, y], cor_contrato, &contrato);
    if let Some(titular) = texto_titular(j) {
        texto_em(ui, meta, &dl, [x_largo + w_contrato + theme::ESPACO_3, y], theme::TEXT_SECONDARY, &format!("· {titular}"));
    }
    y += h_contrato;
    if j.atributos_observados() {
        let chave: Vec<String> =
            j.atributos.iter().take(ATRIBUTOS_NO_CARD).map(|a| format!("{} {}", a.atributo.sigla(), formatar_faixa(a.valor))).collect();
        if !chave.is_empty() {
            let (linha, _) = truncar(&chave.join(" · "), largura_larga, medir(mono));
            texto_em(ui, mono, &dl, [x_largo, y], theme::TEXT_SECONDARY, &linha);
        }
    } else {
        texto_em(ui, meta, &dl, [x_largo, y], theme::TEXT_DISABLED, MSG_EM_OBSERVACAO);
    }

    if ativo && (nome_cortado || cortou2 || cortou3) {
        ui.tooltip(|| {
            com_fonte(ui, fonts.map(|f| f.body), || ui.text(&j.nome));
            com_fonte(ui, meta, || ui.text_colored(theme::TEXT_SECONDARY, format!("{} · {clube}", j.nacao)));
        });
    }
    if let (true, Some(nome)) = (ativo && j.atributos_observados(), &perfil.referencia) {
        ui.tooltip_text(format!("Similaridade com {nome}: {}", texto_percentual(j.similaridade, perfil.aproximado)));
    }
    c.ativou
}

/// Silhueta neutra (cabeça + ombros) para jogador sem rosto no jogo.
pub(super) fn silhueta(dl: &imgui::DrawListMut<'_>, min: [f32; 2], lado: f32) {
    let cor = theme::TEXT_DISABLED;
    let centro = [min[0] + lado * 0.5, min[1] + lado * 0.38];
    dl.add_circle(centro, lado * 0.18, cor).filled(true).build();
    dl.add_rect([min[0] + lado * 0.2, min[1] + lado * 0.62], [min[0] + lado * 0.8, min[1] + lado * 0.92], cor)
        .filled(true)
        .rounding(lado * 0.2)
        .build();
}

fn cabecalho(ui: &Ui, fonts: Option<&Fonts>, item: &RelatorioNaLista) {
    let inicio = ui.cursor_pos();
    let altura_titulo = com_fonte(ui, fonts.map(|f| f.heading), || {
        let titulo = titulo(item);
        ui.set_cursor_pos([inicio[0], inicio[1] + (theme::ALVO_MINIMO - ui.text_line_height()) * 0.5]);
        ui.text(&titulo);
        ui.text_line_height()
    });
    ui.same_line_with_spacing(0.0, theme::ESPACO_2);
    componentes::badge_no_fluxo(ui, fonts, &badge_qualidade(item.relatorio.qualidade), altura_titulo);
    if let Some(o) = &item.olheiro {
        ui.same_line_with_spacing(0.0, theme::ESPACO_2);
        componentes::badge_no_fluxo(ui, fonts, &badge_tier(o.tier), altura_titulo);
    }
    com_fonte(ui, fonts.map(|f| f.meta), || ui.text_wrapped(detalhe(item)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::quality::Observacao;
    use crate::scout::state::{Atributo, AtributoRevelado};

    fn faixa(min: u8, max: u8) -> FaixaAtributo {
        FaixaAtributo { min, max }
    }

    fn jogador(nome: &str, overall: (u8, u8), atributos: &[Atributo]) -> JogadorEncontrado {
        JogadorEncontrado {
            player_id: 1,
            nome: nome.to_string(),
            idade: 20,
            posicao: 24,
            nacao_id: 54,
            nacao: "Brazil".to_string(),
            clube: "Clube".to_string(),
            clube_id: None,
            contrato_ate: None,
            observacao: Default::default(),
            overall: faixa(overall.0, overall.1),
            potencial: faixa(80, 84),
            atributos: atributos.iter().map(|&a| AtributoRevelado { atributo: a, valor: faixa(70, 72) }).collect(),
            pe: None,
            similaridade: None,
            fit: None,
            variacao_overall: None,
            ritmo_ataque: None,
            ritmo_defesa: None,
            estrelas_drible: None,
            pe_fraco: None,
            titular_elenco: None,
            falso_positivo: false,
        }
    }

    #[test]
    fn values_show_ranges_or_exact_never_invented() {
        assert_eq!(formatar_faixa(faixa(72, 72)), "72");
        assert_eq!(formatar_faixa(faixa(65, 78)), "65–78");
        let j = jogador("A", (70, 74), &[Atributo::Finalizacao]);
        assert_eq!(j.atributo(Atributo::Drible), None, "não observado não vira número");
    }

    #[test]
    fn cards_go_from_best_to_worst_overall() {
        let lista = [jogador("Baixo", (60, 64), &[]), jogador("Alto", (80, 84), &[]), jogador("Meio", (70, 74), &[])];
        let nomes: Vec<&str> = ordenar(&lista).iter().map(|j| j.nome.as_str()).collect();
        assert_eq!(nomes, ["Alto", "Meio", "Baixo"]);
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        let medir = |t: &str| t.chars().count() as f32 * 10.0;
        assert_eq!(truncar("Curto", 100.0, medir), ("Curto".to_string(), false));
        let (cortado, cortou) = truncar("Gonçalo Filipe de Oliveira", 100.0, medir);
        assert!(cortou);
        assert!(cortado.ends_with('…'));
        assert!(medir(&cortado) <= 100.0);
        assert_eq!(truncar("abc", 5.0, medir), ("…".to_string(), true));
    }

    #[test]
    fn percentages_are_marked_approximate_below_high_quality() {
        assert!(aproximado(Qualidade::Baixa) && aproximado(Qualidade::Media) && !aproximado(Qualidade::Alta));
        assert_eq!(texto_percentual(Some(87), true), "≈87%");
        assert_eq!(texto_percentual(Some(87), false), "87%");
        assert_eq!(texto_percentual(None, true), "—");
        assert_eq!(texto_fit(PosicaoAlvo::Volante, Some(96), None, false), "VOL 96%");
        assert_eq!(texto_fit(PosicaoAlvo::Volante, Some(96), Some(-2), true), "VOL ≈96% (-2)");
        assert_eq!(texto_variacao(3), "+3");
        assert_eq!(texto_variacao(0), "±0");
    }

    #[test]
    fn money_and_contract_read_naturally() {
        assert_eq!(formatar_dinheiro(18_640_000), "18,6 M");
        assert_eq!(formatar_dinheiro(350_000), "350 mil");
        assert_eq!(formatar_dinheiro(900), "900");
        // 01/02/2030 → 30/06/2031: 1 ano e 4 meses
        assert_eq!(formatar_contrato(Some(2031), Some(Date(20300201))), "1 ano 4 meses");
        assert_eq!(formatar_contrato(Some(2031), Some(Date(20310115))), "5 meses");
        assert_eq!(formatar_contrato(Some(2030), Some(Date(20300801))), "termina nesta temporada");
        assert_eq!(formatar_contrato(Some(2031), None), "até 2031");
        assert_eq!(formatar_contrato(None, Some(Date(20300201))), "—");
        assert!(contrato_a_vencer(Some(2031), Some(Date(20310115))));
        assert!(!contrato_a_vencer(Some(2033), Some(Date(20310115))));
    }

    #[test]
    fn the_card_shows_wage_only_once_observed_and_compares_with_the_starter() {
        let mut j = jogador("A", (72, 76), &[]);
        j.observacao = Observacao::SoMercado;
        assert!(texto_mercado(&j).ends_with("salário em observação"));
        j.observacao = Observacao::Completa;
        assert!(texto_mercado(&j).contains("Salário ≈"));
        assert_eq!(texto_titular(&j), None);
        j.titular_elenco = Some(71);
        assert_eq!(texto_titular(&j).as_deref(), Some("seu titular: 71 (+3)"));
    }

    #[test]
    fn low_quality_message_is_factual() {
        assert!(!MSG_BAIXA.contains('!'));
        assert!(!MSG_SEM_JOGADORES.contains('!'));
    }
}
