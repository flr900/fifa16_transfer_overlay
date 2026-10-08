//! Listas de jogadores (2026-10-08): o que as telas com jogadores têm em
//! comum — Escolhidos, Relatórios (por jogador), Base do Scout e o Relatório
//! aberto. Aqui mora só a lógica, sem tela: os grupos de posição (Todos,
//! Detalhados, Gol, Zag, Mei, Ata), os filtros do painel do Y, a ordenação
//! da visão Tabular e as preferências de cada lista. O desenho fica em
//! `screens::lista_jogadores`.
//!
//! Tudo olha para o que o Olheiro REVELOU (`JogadorEncontrado`): faixas
//! entram pelo meio, e o que ele não viu (pé, ritmos, salário ainda em
//! observação) não passa por um filtro que o pede e vai para o fim numa
//! ordenação.

use std::cmp::Ordering;

use super::quality::{perfil_da_posicao, Perfil};
use super::state::{meio_da_faixa, Densidade, FaixaAtributo, JogadorEncontrado};
use crate::save_repo::{funcao_da_posicao, Funcao, Pe, RitmoTrabalho};

/// Quantos atributos um jogador "detalhado" já tem mapeados (o máximo que
/// um Relatório revela, `quality::atributos_da_pontuacao`).
pub const ATRIBUTOS_DETALHADO: usize = 28;

/// As listas de jogadores do Scout; cada uma guarda a visão, os filtros e a
/// ordenação próprios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListaId {
    Escolhidos,
    /// A aba Relatórios, na visão por jogador.
    Relatorios,
    Base,
    /// Os jogadores de um Relatório aberto.
    RelatorioAberto,
}

impl ListaId {
    #[allow(dead_code)] // usado nos testes
    pub const TODAS: [ListaId; 4] = [ListaId::Escolhidos, ListaId::Relatorios, ListaId::Base, ListaId::RelatorioAberto];
}

/// Um jogador numa lista: o que o Olheiro revelou e o que a tela sabe dele
/// (de onde veio, se já está detalhado).
#[derive(Debug, Clone)]
pub struct ItemLista<'a> {
    pub jogador: &'a JogadorEncontrado,
    /// Todos os atributos mapeados (e não vencidos).
    pub detalhado: bool,
    /// "Olheiro · Missão", para a coluna e a ordenação por origem.
    pub origem: String,
    /// Identidade do item na tela (ids do ImGui): o `player_id`, ou, na lista
    /// de Relatórios por jogador (o mesmo jogador em vários Relatórios), o
    /// `player_id` junto com o Relatório.
    pub chave: u64,
}

impl<'a> ItemLista<'a> {
    /// Detalhado = os 28 atributos do Relatório já revelados.
    pub fn novo(jogador: &'a JogadorEncontrado, origem: String) -> Self {
        let detalhado = jogador.atributos.len() >= ATRIBUTOS_DETALHADO;
        ItemLista { jogador, detalhado, origem, chave: u64::from(jogador.player_id) }
    }

    /// Com a identidade própria (o mesmo jogador em mais de um Relatório).
    pub fn com_chave(mut self, chave: u64) -> Self {
        self.chave = chave;
        self
    }
}

// ---------------------------------------------------------------------
// Grupos de posição
// ---------------------------------------------------------------------

/// A linha de botões acima da lista (escolha única): todos, os detalhados
/// ou um grupo de posição.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum GrupoPosicao {
    #[default]
    Todos,
    /// Os que já têm todos os atributos mapeados.
    Detalhados,
    Goleiros,
    /// Zagueiros e laterais.
    Defensores,
    Meias,
    Atacantes,
}

impl GrupoPosicao {
    pub const TODOS: [GrupoPosicao; 6] = [
        GrupoPosicao::Todos,
        GrupoPosicao::Detalhados,
        GrupoPosicao::Goleiros,
        GrupoPosicao::Defensores,
        GrupoPosicao::Meias,
        GrupoPosicao::Atacantes,
    ];

    /// Rótulo do botão.
    pub fn sigla(self) -> &'static str {
        match self {
            GrupoPosicao::Todos => "Todos",
            GrupoPosicao::Detalhados => "Detalhados",
            GrupoPosicao::Goleiros => "Gol",
            GrupoPosicao::Defensores => "Zag",
            GrupoPosicao::Meias => "Mei",
            GrupoPosicao::Atacantes => "Ata",
        }
    }

    /// Texto do tooltip.
    pub fn nome(self) -> &'static str {
        match self {
            GrupoPosicao::Todos => "Todos os jogadores",
            GrupoPosicao::Detalhados => "Só os que já têm todos os atributos mapeados",
            GrupoPosicao::Goleiros => "Goleiros",
            GrupoPosicao::Defensores => "Zagueiros e laterais",
            GrupoPosicao::Meias => "Meio-campistas",
            GrupoPosicao::Atacantes => "Atacantes",
        }
    }

    pub fn passa(self, item: &ItemLista<'_>) -> bool {
        let funcao = funcao_da_posicao(item.jogador.posicao);
        match self {
            GrupoPosicao::Todos => true,
            GrupoPosicao::Detalhados => item.detalhado,
            GrupoPosicao::Goleiros => funcao == Funcao::Goleiro,
            GrupoPosicao::Defensores => funcao == Funcao::Defensor,
            GrupoPosicao::Meias => funcao == Funcao::MeioCampo,
            GrupoPosicao::Atacantes => funcao == Funcao::Atacante,
        }
    }
}

// ---------------------------------------------------------------------
// Filtros do painel (Y)
// ---------------------------------------------------------------------

/// Limites das faixas do painel (o neutro é o intervalo todo).
pub const OVERALL: (u8, u8) = (1, FaixaAtributo::MAIOR);
pub const POTENCIAL: (u8, u8) = (1, FaixaAtributo::MAIOR);
pub const IDADE: (u8, u8) = (15, 45);

fn faixa((min, max): (u8, u8)) -> FaixaAtributo {
    FaixaAtributo { min, max }
}

/// Os filtros de uma lista: o grupo de posição (a linha de botões) e o
/// painel do Y.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiltrosLista {
    pub grupo: GrupoPosicao,
    pub overall: FaixaAtributo,
    pub potencial: FaixaAtributo,
    pub idade: FaixaAtributo,
    /// `None` = qualquer pé.
    pub pe: Option<Pe>,
    /// Ritmos aceitos (vazio = qualquer).
    pub ritmo_ataque: Vec<RitmoTrabalho>,
    pub ritmo_defesa: Vec<RitmoTrabalho>,
    /// Posições (grupos de posição nativa) aceitas; vazio = todas.
    pub perfis: Vec<Perfil>,
}

impl Default for FiltrosLista {
    fn default() -> Self {
        FiltrosLista {
            grupo: GrupoPosicao::Todos,
            overall: faixa(OVERALL),
            potencial: faixa(POTENCIAL),
            idade: faixa(IDADE),
            pe: None,
            ritmo_ataque: Vec::new(),
            ritmo_defesa: Vec::new(),
            perfis: Vec::new(),
        }
    }
}

impl FiltrosLista {
    /// Quantos filtros do painel estão ligados (o grupo não conta: ele tem a
    /// linha de botões própria).
    pub fn ativos(&self) -> usize {
        let padrao = FiltrosLista::default();
        [
            self.overall != padrao.overall,
            self.potencial != padrao.potencial,
            self.idade != padrao.idade,
            self.pe.is_some(),
            !self.ritmo_ataque.is_empty(),
            !self.ritmo_defesa.is_empty(),
            !self.perfis.is_empty(),
        ]
        .into_iter()
        .filter(|ligado| *ligado)
        .count()
    }

    /// Desliga os filtros do painel; o grupo de posição fica.
    pub fn limpar_painel(&mut self) {
        *self = FiltrosLista { grupo: self.grupo, ..FiltrosLista::default() };
    }

    pub fn passa(&self, item: &ItemLista<'_>) -> bool {
        let j = item.jogador;
        let na_faixa = |f: FaixaAtributo, v: u8| (f.min..=f.max).contains(&v);
        self.grupo.passa(item)
            && na_faixa(self.overall, meio_da_faixa(j.overall))
            && na_faixa(self.potencial, meio_da_faixa(j.potencial))
            && na_faixa(self.idade, j.idade)
            && self.pe.is_none_or(|pe| j.pe == Some(pe))
            && (self.ritmo_ataque.is_empty() || j.ritmo_ataque.is_some_and(|r| self.ritmo_ataque.contains(&r)))
            && (self.ritmo_defesa.is_empty() || j.ritmo_defesa.is_some_and(|r| self.ritmo_defesa.contains(&r)))
            && (self.perfis.is_empty() || self.perfis.contains(&perfil_da_posicao(j.posicao)))
    }

    /// Entra ou sai da lista de posições aceitas.
    pub fn alternar_perfil(&mut self, perfil: Perfil) {
        match self.perfis.iter().position(|p| *p == perfil) {
            Some(i) => {
                self.perfis.remove(i);
            }
            None => self.perfis.push(perfil),
        }
    }

    pub fn alternar_ritmo(&mut self, ataque: bool, ritmo: RitmoTrabalho) {
        let lista = if ataque { &mut self.ritmo_ataque } else { &mut self.ritmo_defesa };
        match lista.iter().position(|r| *r == ritmo) {
            Some(i) => {
                lista.remove(i);
            }
            None => lista.push(ritmo),
        }
    }
}

/// Quantos itens passam por CADA grupo de posição (os outros filtros do
/// painel valendo), para o número nos botões.
pub fn contagem_por_grupo(itens: &[ItemLista<'_>], filtros: &FiltrosLista) -> Vec<(GrupoPosicao, usize)> {
    GrupoPosicao::TODOS
        .iter()
        .map(|&grupo| {
            let f = FiltrosLista { grupo, ..filtros.clone() };
            (grupo, itens.iter().filter(|i| f.passa(i)).count())
        })
        .collect()
}

// ---------------------------------------------------------------------
// Ordenação da visão Tabular
// ---------------------------------------------------------------------

/// As colunas da visão Tabular (as mesmas da ordenação).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coluna {
    Nome,
    Idade,
    Altura,
    Posicao,
    Time,
    Pais,
    Overall,
    Potencial,
    Valor,
    Salario,
    Contrato,
    /// De onde veio (Olheiro · Missão).
    Origem,
}

impl Coluna {
    pub fn titulo(self) -> &'static str {
        match self {
            Coluna::Nome => "Jogador",
            Coluna::Idade => "Idade",
            Coluna::Altura => "Altura",
            Coluna::Posicao => "Pos.",
            Coluna::Time => "Time",
            Coluna::Pais => "País",
            Coluna::Overall => "OVR",
            Coluna::Potencial => "POT",
            Coluna::Valor => "Valor",
            Coluna::Salario => "Salário",
            Coluna::Contrato => "Contrato",
            Coluna::Origem => "Visto por",
        }
    }

    /// Ao escolher a coluna, os números começam do maior (Overall, Valor…) e
    /// os textos do começo do alfabeto.
    pub fn comeca_decrescente(self) -> bool {
        !matches!(self, Coluna::Nome | Coluna::Posicao | Coluna::Time | Coluna::Pais | Coluna::Origem)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ordenacao {
    pub coluna: Coluna,
    pub decrescente: bool,
}

impl Default for Ordenacao {
    /// Do melhor para o pior Overall (como o Relatório sempre mostrou).
    fn default() -> Self {
        Ordenacao { coluna: Coluna::Overall, decrescente: true }
    }
}

impl Ordenacao {
    /// Apertar o cabeçalho de uma coluna: a coluna nova começa na ordem
    /// natural dela; a mesma coluna inverte.
    pub fn alternar(self, coluna: Coluna) -> Ordenacao {
        if self.coluna == coluna {
            Ordenacao { coluna, decrescente: !self.decrescente }
        } else {
            Ordenacao { coluna, decrescente: coluna.comeca_decrescente() }
        }
    }
}

/// O valor de transferência de um jogador, como a tela o mostra (a Central
/// ajusta a estimativa com as leituras exatas do jogo).
pub type ValorDoJogador<'a> = &'a dyn Fn(&JogadorEncontrado) -> i64;

/// Chave numérica de uma coluna; `None` = o Olheiro não viu (vai para o fim).
fn chave_numerica(item: &ItemLista<'_>, coluna: Coluna, valor: ValorDoJogador<'_>) -> Option<i64> {
    let j = item.jogador;
    match coluna {
        Coluna::Idade => Some(i64::from(j.idade)),
        Coluna::Altura => j.altura.map(i64::from),
        Coluna::Overall => Some(i64::from(u16::from(j.overall.min) + u16::from(j.overall.max))),
        Coluna::Potencial => Some(i64::from(u16::from(j.potencial.min) + u16::from(j.potencial.max))),
        Coluna::Valor => Some(valor(j)),
        Coluna::Salario => j.salario_conhecido().then(|| j.salario_estimado()),
        Coluna::Contrato => j.contrato_ate.map(i64::from),
        Coluna::Posicao => Some(i64::from(j.posicao)),
        Coluna::Nome | Coluna::Time | Coluna::Pais | Coluna::Origem => None,
    }
}

fn chave_texto(item: &ItemLista<'_>, coluna: Coluna) -> String {
    let j = item.jogador;
    match coluna {
        Coluna::Nome => j.nome.to_lowercase(),
        Coluna::Time => j.clube.to_lowercase(),
        Coluna::Pais => j.nacao.to_lowercase(),
        Coluna::Origem => item.origem.to_lowercase(),
        _ => String::new(),
    }
}

/// Ordena `itens` pela ordenação escolhida. Empate: Overall (do melhor), depois
/// o nome — assim a ordem nunca pula de um frame para o outro.
pub fn ordenar(itens: &mut [ItemLista<'_>], ordenacao: Ordenacao, valor: ValorDoJogador<'_>) {
    let numerica = !matches!(ordenacao.coluna, Coluna::Nome | Coluna::Time | Coluna::Pais | Coluna::Origem);
    itens.sort_by(|a, b| {
        let principal = if numerica {
            // sem valor (não visto) fica sempre no fim, qualquer que seja o sentido
            match (chave_numerica(a, ordenacao.coluna, valor), chave_numerica(b, ordenacao.coluna, valor)) {
                (Some(x), Some(y)) => {
                    let o = x.cmp(&y);
                    if ordenacao.decrescente { o.reverse() } else { o }
                }
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        } else {
            let o = chave_texto(a, ordenacao.coluna).cmp(&chave_texto(b, ordenacao.coluna));
            if ordenacao.decrescente { o.reverse() } else { o }
        };
        let meio = |i: &ItemLista<'_>| u16::from(i.jogador.overall.min) + u16::from(i.jogador.overall.max);
        principal
            .then_with(|| meio(b).cmp(&meio(a)))
            .then_with(|| a.jogador.nome.cmp(&b.jogador.nome))
            .then_with(|| a.jogador.player_id.cmp(&b.jogador.player_id))
    });
}

// ---------------------------------------------------------------------
// Preferências de cada lista
// ---------------------------------------------------------------------

/// O que cada lista lembra enquanto o painel está aberto (a visão também
/// vai para o arquivo da carreira; filtros e ordenação, não).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefsLista {
    pub modo: Densidade,
    pub filtros: FiltrosLista,
    pub ordenacao: Ordenacao,
}

impl PrefsLista {
    pub fn nova(modo: Densidade) -> Self {
        PrefsLista { modo, filtros: FiltrosLista::default(), ordenacao: Ordenacao::default() }
    }
}

/// "178 cm" ou "—".
pub fn texto_altura(altura: Option<u8>) -> String {
    altura.map_or_else(|| "—".to_string(), |a| format!("{a} cm"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::state::{AtributoRevelado, Atributo};

    fn jogador(id: u32, nome: &str, posicao: u8, overall: (u8, u8), idade: u8) -> JogadorEncontrado {
        JogadorEncontrado {
            player_id: id,
            nome: nome.to_string(),
            idade,
            posicao,
            nacao_id: 54,
            nacao: "Brasil".to_string(),
            clube: "Clube".to_string(),
            clube_id: Some(1),
            contrato_ate: Some(2030),
            observacao: crate::scout::quality::Observacao::Completa,
            overall: FaixaAtributo { min: overall.0, max: overall.1 },
            potencial: FaixaAtributo { min: overall.0 + 5, max: overall.1 + 5 },
            atributos: Vec::new(),
            pe: Some(Pe::Direito),
            similaridade: None,
            fit: None,
            variacao_overall: None,
            ritmo_ataque: Some(RitmoTrabalho::Medio),
            ritmo_defesa: Some(RitmoTrabalho::Medio),
            estrelas_drible: Some(3),
            pe_fraco: Some(3),
            altura: Some(180),
            titular_elenco: None,
            falso_positivo: false,
        }
    }

    fn item(j: &JogadorEncontrado) -> ItemLista<'_> {
        ItemLista::novo(j, "Olheiro · Missão".to_string())
    }

    #[test]
    fn position_groups_follow_the_native_position() {
        let gol = jogador(1, "G", 0, (70, 72), 25);
        let zag = jogador(2, "Z", 5, (70, 72), 25);
        let lat = jogador(3, "L", 7, (70, 72), 25);
        let mei = jogador(4, "M", 14, (70, 72), 25);
        let ata = jogador(5, "A", 25, (70, 72), 25);
        let passa = |g: GrupoPosicao, j: &JogadorEncontrado| g.passa(&item(j));
        assert!(passa(GrupoPosicao::Goleiros, &gol) && !passa(GrupoPosicao::Goleiros, &zag));
        assert!(passa(GrupoPosicao::Defensores, &zag) && passa(GrupoPosicao::Defensores, &lat), "zagueiros e laterais");
        assert!(passa(GrupoPosicao::Meias, &mei) && !passa(GrupoPosicao::Meias, &ata));
        assert!(passa(GrupoPosicao::Atacantes, &ata));
        assert!(GrupoPosicao::TODOS.iter().all(|g| passa(*g, &mei) == (matches!(g, GrupoPosicao::Todos | GrupoPosicao::Meias))),
            "o meia só não passa em Detalhados porque não tem os 28 atributos");
        assert_eq!(GrupoPosicao::TODOS.map(GrupoPosicao::sigla), ["Todos", "Detalhados", "Gol", "Zag", "Mei", "Ata"]);
    }

    #[test]
    fn detailed_means_all_28_attributes_mapped() {
        let mut j = jogador(1, "A", 25, (70, 72), 25);
        assert!(!item(&j).detalhado);
        j.atributos = Atributo::TODOS.iter().take(ATRIBUTOS_DETALHADO).map(|&a| AtributoRevelado { atributo: a, valor: FaixaAtributo { min: 70, max: 72 } }).collect();
        assert!(item(&j).detalhado);
        assert!(GrupoPosicao::Detalhados.passa(&item(&j)));
        j.atributos.truncate(27);
        assert!(!GrupoPosicao::Detalhados.passa(&item(&j)));
    }

    #[test]
    fn panel_filters_use_the_middle_of_the_revealed_ranges() {
        let j = jogador(1, "A", 25, (70, 76), 20); // meio 73, potencial 75–81 → 78
        let mut f = FiltrosLista::default();
        assert!(f.passa(&item(&j)) && f.ativos() == 0);
        f.overall = FaixaAtributo { min: 74, max: 99 };
        assert!(!f.passa(&item(&j)), "meio 73 < 74");
        assert_eq!(f.ativos(), 1);
        f.overall = FaixaAtributo { min: 70, max: 99 };
        f.idade = FaixaAtributo { min: 15, max: 19 };
        assert!(!f.passa(&item(&j)));
        f.idade = FaixaAtributo { min: 15, max: 21 };
        f.pe = Some(Pe::Esquerdo);
        assert!(!f.passa(&item(&j)), "destro não passa no filtro de canhoto");
        f.pe = Some(Pe::Direito);
        assert!(f.passa(&item(&j)));
        f.alternar_ritmo(true, RitmoTrabalho::Alto);
        assert!(!f.passa(&item(&j)), "ritmo médio não é alto");
        f.alternar_ritmo(true, RitmoTrabalho::Alto);
        f.alternar_perfil(Perfil::Centroavante);
        assert!(f.passa(&item(&j)), "posição 25 é centroavante");
        f.alternar_perfil(Perfil::Centroavante);
        f.alternar_perfil(Perfil::Zagueiro);
        assert!(!f.passa(&item(&j)));
        f.limpar_painel();
        assert_eq!(f.ativos(), 0);
    }

    #[test]
    fn what_the_olheiro_did_not_see_fails_a_filter_that_asks_for_it() {
        let mut j = jogador(1, "A", 25, (70, 72), 25);
        j.pe = None;
        j.ritmo_ataque = None;
        let mut f = FiltrosLista::default();
        f.pe = Some(Pe::Direito);
        assert!(!f.passa(&item(&j)));
        f.pe = None;
        f.alternar_ritmo(true, RitmoTrabalho::Medio);
        assert!(!f.passa(&item(&j)));
        f.limpar_painel();
        assert!(f.passa(&item(&j)), "sem filtro, passa");
    }

    #[test]
    fn the_group_counts_follow_the_other_filters() {
        let a = jogador(1, "A", 25, (70, 72), 20);
        let b = jogador(2, "B", 5, (80, 82), 30);
        let c = jogador(3, "C", 14, (75, 77), 24);
        let itens = [item(&a), item(&b), item(&c)];
        let todos = contagem_por_grupo(&itens, &FiltrosLista::default());
        let n = |lista: &[(GrupoPosicao, usize)], g| lista.iter().find(|(x, _)| *x == g).map(|(_, n)| *n);
        assert_eq!((n(&todos, GrupoPosicao::Todos), n(&todos, GrupoPosicao::Atacantes), n(&todos, GrupoPosicao::Defensores)), (Some(3), Some(1), Some(1)));
        let mut jovens = FiltrosLista::default();
        jovens.idade = FaixaAtributo { min: 15, max: 25 };
        let so_jovens = contagem_por_grupo(&itens, &jovens);
        assert_eq!((n(&so_jovens, GrupoPosicao::Todos), n(&so_jovens, GrupoPosicao::Defensores)), (Some(2), Some(0)));
    }

    #[test]
    fn sorting_flips_with_the_same_column_and_unseen_values_stay_last() {
        let a = jogador(1, "Ana", 25, (70, 72), 30);
        let b = jogador(2, "Bia", 5, (80, 82), 20);
        let mut c = jogador(3, "Cris", 14, (75, 77), 25);
        c.altura = None;
        c.contrato_ate = None;
        let valor = |j: &JogadorEncontrado| i64::from(j.idade) * 1000;
        let ordem = |ord: Ordenacao| {
            let mut itens = vec![item(&a), item(&b), item(&c)];
            ordenar(&mut itens, ord, &valor);
            itens.iter().map(|i| i.jogador.nome.clone()).collect::<Vec<_>>()
        };
        assert_eq!(ordem(Ordenacao::default()), ["Bia", "Cris", "Ana"], "Overall do melhor ao pior");
        assert_eq!(ordem(Ordenacao { coluna: Coluna::Overall, decrescente: false }), ["Ana", "Cris", "Bia"]);
        assert_eq!(ordem(Ordenacao { coluna: Coluna::Idade, decrescente: false }), ["Bia", "Cris", "Ana"]);
        assert_eq!(ordem(Ordenacao { coluna: Coluna::Nome, decrescente: true }), ["Cris", "Bia", "Ana"]);
        assert_eq!(ordem(Ordenacao { coluna: Coluna::Valor, decrescente: true }), ["Ana", "Cris", "Bia"]);
        // sem altura/contrato (não visto): fim nos dois sentidos
        for decrescente in [true, false] {
            assert_eq!(ordem(Ordenacao { coluna: Coluna::Altura, decrescente }).last().map(String::as_str), Some("Cris"));
            assert_eq!(ordem(Ordenacao { coluna: Coluna::Contrato, decrescente }).last().map(String::as_str), Some("Cris"));
        }
    }

    #[test]
    fn choosing_a_column_starts_in_its_natural_order_and_choosing_it_again_flips() {
        let o = Ordenacao::default();
        let nome = o.alternar(Coluna::Nome);
        assert_eq!((nome.coluna, nome.decrescente), (Coluna::Nome, false), "texto: A→Z");
        let de_novo = nome.alternar(Coluna::Nome);
        assert!(de_novo.decrescente);
        let valor = de_novo.alternar(Coluna::Valor);
        assert_eq!((valor.coluna, valor.decrescente), (Coluna::Valor, true), "número: do maior");
        assert_eq!(texto_altura(Some(178)), "178 cm");
        assert_eq!(texto_altura(None), "—");
    }
}
