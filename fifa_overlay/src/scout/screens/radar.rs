//! Radar de Atributos (Story 3.1/3.2, UX-DR16), desenhado com o
//! `ImDrawList`: um eixo por atributo, contorno roxo sólido para o jogador
//! do Relatório e tracejado verde para o jogador do elenco sobreposto.
//!
//! Nada é inventado (FR10): eixo não observado fica pontilhado e vazio —
//! nunca zero —, e a linha do jogador só liga eixos vizinhos que têm valor.
//! Faixa revelada ("65–78") vira um traço do mínimo ao máximo no eixo, com
//! a linha passando pelo meio. O jogador do elenco (valores reais) aparece
//! em todos os eixos, inclusive nos que o Olheiro não observou: a
//! comparação não esconde o lado conhecido.

use imgui::{DrawListMut, Ui};

use super::componentes::texto_em;
use super::theme::{self, Fonts};
use crate::scout::quality;
use crate::scout::state::{Atributo, FaixaAtributo, JogadorElenco, JogadorEncontrado};

/// Maior valor de atributo (borda do radar).
const ESCALA: f32 = 99.0;
/// Anéis de referência (valores).
const ANEIS: [f32; 4] = [25.0, 50.0, 75.0, 99.0];
const ESPESSURA_JOGADOR: f32 = 2.0;
const ESPESSURA_FAIXA: f32 = 4.0;
const ESPESSURA_COMPARACAO: f32 = 2.0;
const TRACO: f32 = 6.0;
const VAZIO: f32 = 4.0;
const PASSO_PONTILHADO: f32 = 6.0;

/// Um eixo do radar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eixo {
    pub atributo: Atributo,
    /// Faixa revelada do jogador do Relatório (`None` = não observado).
    pub valor: Option<FaixaAtributo>,
    /// Valor real do jogador do elenco sobreposto.
    pub comparacao: Option<u8>,
}

/// Eixos do radar: os atributos de linha (ou os de goleiro + reflexo e
/// físico, para um goleiro) e todo outro atributo revelado, na ordem fixa
/// de `Atributo::TODOS` (ritmo → drible → chute → passe → defesa → físico
/// → goleiro).
pub fn eixos(jogador: &JogadorEncontrado, comparacao: Option<&JogadorElenco>) -> Vec<Eixo> {
    let base = quality::atributos_comparados(jogador.posicao == 0);
    Atributo::TODOS
        .into_iter()
        .filter(|a| base.contains(a) || jogador.atributo(*a).is_some())
        .map(|atributo| Eixo { atributo, valor: jogador.atributo(atributo), comparacao: comparacao.and_then(|c| c.atributo(atributo)) })
        .collect()
}

/// Ponto do eixo `indice` (de `total`, o primeiro para cima, sentido
/// horário) no valor `valor`.
pub fn ponto(centro: [f32; 2], raio: f32, indice: usize, total: usize, valor: f32) -> [f32; 2] {
    let angulo = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * indice as f32 / total.max(1) as f32;
    let r = raio * (valor / ESCALA).clamp(0.0, 1.0);
    [centro[0] + r * angulo.cos(), centro[1] + r * angulo.sin()]
}

/// Trechos de eixos vizinhos que têm valor, para a linha do jogador não
/// "pular" um eixo não observado. Devolve os trechos (índices em ordem) e
/// se o contorno fecha (todos os eixos têm valor).
pub fn trechos(tem_valor: &[bool]) -> (Vec<Vec<usize>>, bool) {
    let n = tem_valor.len();
    if n > 0 && tem_valor.iter().all(|&t| t) {
        return (vec![(0..n).collect()], true);
    }
    // começa logo depois de um eixo vazio, para um trecho que "dá a volta"
    // no fim da lista não ficar partido em dois
    let inicio = tem_valor.iter().position(|&t| !t).map_or(0, |i| i + 1);
    let mut lista = Vec::new();
    let mut atual: Vec<usize> = Vec::new();
    for passo in 0..n {
        let i = (inicio + passo) % n;
        if tem_valor.get(i).copied().unwrap_or(false) {
            atual.push(i);
        } else if !atual.is_empty() {
            lista.push(std::mem::take(&mut atual));
        }
    }
    if !atual.is_empty() {
        lista.push(atual);
    }
    (lista, false)
}

/// Desenha o radar com centro `centro` e raio `raio`.
pub fn desenhar(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, centro: [f32; 2], raio: f32, eixos: &[Eixo]) {
    let n = eixos.len();
    if n < 3 {
        return;
    }
    // Anéis e eixos.
    for anel in ANEIS {
        let pontos: Vec<[f32; 2]> = (0..n).map(|i| ponto(centro, raio, i, n, anel)).collect();
        dl.add_polyline(pontos, theme::BORDER_HAIRLINE_SUBTLE).thickness(1.0).build();
        let fim = ponto(centro, raio, 0, n, anel);
        let inicio = ponto(centro, raio, n - 1, n, anel);
        dl.add_line(inicio, fim, theme::BORDER_HAIRLINE_SUBTLE).build();
    }
    for (i, eixo) in eixos.iter().enumerate() {
        let ponta = ponto(centro, raio, i, n, ESCALA);
        if eixo.valor.is_some() {
            dl.add_line(centro, ponta, theme::BORDER_HAIRLINE_SUBTLE).build();
        } else {
            pontilhado(dl, centro, ponta, theme::TEXT_DISABLED);
        }
        rotulo(ui, fonts, dl, centro, raio, i, n, eixo);
    }

    // Jogador do elenco: tracejado verde em todos os eixos que ele tem.
    let comparacao: Vec<[f32; 2]> =
        eixos.iter().enumerate().filter_map(|(i, e)| e.comparacao.map(|v| ponto(centro, raio, i, n, f32::from(v)))).collect();
    if comparacao.len() == n {
        for i in 0..n {
            if let (Some(&a), Some(&b)) = (comparacao.get(i), comparacao.get((i + 1) % n)) {
                tracejado(dl, a, b, theme::FIELD_GREEN, ESPESSURA_COMPARACAO);
            }
        }
    }

    // Jogador do Relatório: faixa no eixo + linha sólida pelo meio.
    let meio = |i: usize| eixos.get(i).and_then(|e| e.valor).map(|f| (f32::from(f.min) + f32::from(f.max)) * 0.5);
    for (i, eixo) in eixos.iter().enumerate() {
        if let Some(f) = eixo.valor {
            if f.min != f.max {
                let a = ponto(centro, raio, i, n, f32::from(f.min));
                let b = ponto(centro, raio, i, n, f32::from(f.max));
                dl.add_line(a, b, theme::ACCENT_PRIMARY_DIM_FORTE).thickness(ESPESSURA_FAIXA).build();
            }
        }
    }
    let tem_valor: Vec<bool> = eixos.iter().map(|e| e.valor.is_some()).collect();
    let (lista, fechado) = trechos(&tem_valor);
    for trecho in lista {
        let pontos: Vec<[f32; 2]> = trecho.iter().filter_map(|&i| meio(i).map(|v| ponto(centro, raio, i, n, v))).collect();
        if pontos.len() == 1 {
            if let Some(&p) = pontos.first() {
                dl.add_circle(p, 3.0, theme::ACCENT_PRIMARY).filled(true).build();
            }
            continue;
        }
        if fechado {
            if let (Some(&primeiro), Some(&ultimo)) = (pontos.first(), pontos.last()) {
                dl.add_line(ultimo, primeiro, theme::ACCENT_PRIMARY).thickness(ESPESSURA_JOGADOR).build();
            }
        }
        for p in &pontos {
            dl.add_circle(*p, 2.5, theme::ACCENT_PRIMARY).filled(true).build();
        }
        dl.add_polyline(pontos, theme::ACCENT_PRIMARY).thickness(ESPESSURA_JOGADOR).build();
    }
}

/// Sigla do atributo na ponta do eixo (apagada se não observado).
#[allow(clippy::too_many_arguments)]
fn rotulo(ui: &Ui, fonts: Option<&Fonts>, dl: &DrawListMut<'_>, centro: [f32; 2], raio: f32, i: usize, n: usize, eixo: &Eixo) {
    let texto = eixo.atributo.sigla();
    let cor = if eixo.valor.is_some() { theme::TEXT_SECONDARY } else { theme::TEXT_DISABLED };
    let fonte = fonts.map(|f| f.meta);
    let [w, h] = super::com_fonte(ui, fonte, || ui.calc_text_size(texto));
    let [x, y] = ponto(centro, raio + theme::ESPACO_4, i, n, ESCALA);
    texto_em(ui, fonte, dl, [x - w * 0.5, y - h * 0.5], cor, texto);
}

/// Linha tracejada de `a` a `b`.
pub fn tracejado(dl: &DrawListMut<'_>, a: [f32; 2], b: [f32; 2], cor: [f32; 4], espessura: f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let comprimento = (dx * dx + dy * dy).sqrt();
    if comprimento <= f32::EPSILON {
        return;
    }
    let (ux, uy) = (dx / comprimento, dy / comprimento);
    let mut t = 0.0;
    while t < comprimento {
        let fim = (t + TRACO).min(comprimento);
        dl.add_line([a[0] + ux * t, a[1] + uy * t], [a[0] + ux * fim, a[1] + uy * fim], cor).thickness(espessura).build();
        t += TRACO + VAZIO;
    }
}

/// Linha pontilhada de `a` a `b` (eixo não observado).
pub fn pontilhado(dl: &DrawListMut<'_>, a: [f32; 2], b: [f32; 2], cor: [f32; 4]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let comprimento = (dx * dx + dy * dy).sqrt();
    let passos = (comprimento / PASSO_PONTILHADO).floor() as usize;
    for k in 1..=passos {
        let t = k as f32 * PASSO_PONTILHADO / comprimento.max(f32::EPSILON);
        dl.add_circle([a[0] + dx * t, a[1] + dy * t], 1.0, cor).filled(true).build();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::AtributoRevelado;

    fn encontrado(posicao: u8, revelados: &[(Atributo, u8, u8)]) -> JogadorEncontrado {
        JogadorEncontrado {
            player_id: 1,
            nome: "A".to_string(),
            idade: 20,
            posicao,
            nacao_id: 54,
            nacao: "Brazil".to_string(),
            clube: "Clube".to_string(),
            overall: FaixaAtributo { min: 70, max: 74 },
            potencial: FaixaAtributo { min: 80, max: 84 },
            atributos: revelados.iter().map(|&(atributo, min, max)| AtributoRevelado { atributo, valor: FaixaAtributo { min, max } }).collect(),
            pe: None,
            similaridade: None,
            fit: None,
            variacao_overall: None,
            ritmo_ataque: None,
            ritmo_defesa: None,
            estrelas_drible: None,
            pe_fraco: None,
        }
    }

    #[test]
    fn axes_cover_the_outfield_profile_and_unrevealed_ones_stay_empty() {
        let j = encontrado(24, &[(Atributo::Finalizacao, 80, 84), (Atributo::Velocidade, 70, 70)]);
        let e = eixos(&j, None);
        assert_eq!(e.len(), 28, "um eixo por atributo de linha");
        assert_eq!(e.iter().filter(|x| x.valor.is_some()).count(), 2);
        let vazio = e.iter().find(|x| x.atributo == Atributo::Marcacao).expect("eixo");
        assert_eq!(vazio.valor, None, "não observado é vazio, não zero");
        // goleiro: atributos de goleiro + reflexo/físico, e o que mais foi revelado
        let g = encontrado(0, &[(Atributo::GkReflexos, 80, 82), (Atributo::PasseCurto, 50, 55)]);
        let eg = eixos(&g, None);
        assert_eq!(eg.len(), 10);
        assert!(eg.iter().any(|x| x.atributo == Atributo::PasseCurto));
    }

    #[test]
    fn the_squad_player_shows_on_axes_the_olheiro_did_not_observe() {
        let j = encontrado(24, &[(Atributo::Finalizacao, 80, 84)]);
        let meu = JogadorElenco {
            player_id: 9,
            nome: "Meu".to_string(),
            idade: 25,
            posicao: 24,
            overall: 78,
            potencial: 80,
            atributos: (1..=33).collect(),
        };
        let e = eixos(&j, Some(&meu));
        let marcacao = e.iter().find(|x| x.atributo == Atributo::Marcacao).expect("eixo");
        assert_eq!(marcacao.valor, None);
        assert_eq!(marcacao.comparacao, Some(meu.atributos[Atributo::Marcacao.indice()]));
        assert!(e.iter().all(|x| x.comparacao.is_some()));
    }

    #[test]
    fn the_line_only_joins_neighbouring_revealed_axes() {
        assert_eq!(trechos(&[true, true, false, true, false]), (vec![vec![3], vec![0, 1]], false));
        // um trecho que dá a volta no fim da lista fica inteiro
        assert_eq!(trechos(&[true, false, true, true]), (vec![vec![2, 3, 0]], false));
        assert_eq!(trechos(&[true, true, true]), (vec![vec![0, 1, 2]], true));
        assert_eq!(trechos(&[false, false]), (vec![], false));
    }

    #[test]
    fn points_start_at_the_top_and_scale_with_the_value() {
        let p = ponto([100.0, 100.0], 50.0, 0, 4, 99.0);
        assert!((p[0] - 100.0).abs() < 0.01 && (p[1] - 50.0).abs() < 0.01, "{p:?}");
        let meio = ponto([100.0, 100.0], 50.0, 1, 4, 49.5);
        assert!((meio[0] - 125.0).abs() < 0.01 && (meio[1] - 100.0).abs() < 0.01, "{meio:?}");
        assert_eq!(ponto([0.0, 0.0], 50.0, 2, 4, 0.0), [0.0, 0.0]);
    }
}
