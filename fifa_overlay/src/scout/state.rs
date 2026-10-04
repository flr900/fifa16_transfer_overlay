//! `scout::state` — o estado do Scout que as telas enxergam (AD-1: as
//! telas só falam com esta camada).
//!
//! Nesta story é só o estado da carreira, que decide o que o painel mostra:
//! "Localizando carreira…", "Nenhuma carreira carregada…", erro de leitura
//! com "Tentar novamente", ou a carreira pronta (cabeçalho com orçamento e
//! data, conteúdo das abas).
//!
//! Política de leitura (tudo fora do render pesado — AD-4):
//! - ao ABRIR o painel: uma leitura; se não houver carreira localizada (ou
//!   a localizada não estiver mais carregada — pode ter trocado de save),
//!   dispara UMA localização em background (~15 s);
//! - com o painel aberto: relê a cada `INTERVALO_RELEITURA` (leitura
//!   barata, alguns bytes) para o orçamento/data acompanharem o jogo;
//! - nunca relocaliza sozinho em loop: se a localização falhar, só
//!   reabrindo o painel ou clicando "Tentar novamente".
//!
//! Vigia (Story 1.7): o `tick` roda a cada frame, com o painel aberto OU
//! fechado. Sem carreira pronta, a cada `INTERVALO_SINAL` um `AsyncTask`
//! barato pergunta ao `save_repo` se parece haver uma carreira carregada;
//! quando o sinal acende, dispara UMA localização (o jogador entrou na
//! carreira — o painel abre pronto). O sinal só "rearma" depois de
//! apagar (voltou ao menu) ou de uma localização bem-sucedida, então uma
//! localização que falha não vira loop. Cada mudança relevante gera um
//! `Aviso` para o banner do canto da tela (`scout::screens::aviso`).
//!
//! Estado persistido (Story 1.3): `scout::state` é o ÚNICO chamador de
//! `scout::persistence` (AD-1). Quando a carreira fica pronta, o arquivo
//! dela (`<hash>.json`, AD-11) é carregado uma vez por sessão e guardado
//! num mapa por hash — assim cada arquivo tem um só mutex (AD-7) mesmo
//! que o jogador vá ao menu e volte, ou troque de carreira e retorne.
//! Mutações só valem com a carreira PRONTA: sem carreira não há para
//! qual arquivo escrever (trocar de aba no menu não persiste).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::async_task::{AsyncTask, TaskState};
use crate::save_repo::{Date, SaveRepoError};
pub use crate::save_repo::{Atributo, Confederacao, Funcao, Liga, Nacao, Pe, RitmoTrabalho};
pub use super::quality::{Atalho, NivelEquipe, Perfil, PosicaoAlvo};

use super::minifaces::{Minifaces, Rosto};
pub use super::persistence::Densidade;
use super::persistence::{self, EstadoPersistido};
use super::quality;
use super::search::{self, CareerSnapshot, CareerSource, SaveRepoSource};
use super::Aba;

/// Com a carreira pronta, relê o estado vivo nesse intervalo.
const INTERVALO_RELEITURA: Duration = Duration::from_secs(1);

/// Sem carreira pronta, pergunta "há carreira carregada?" nesse intervalo.
const INTERVALO_SINAL: Duration = Duration::from_secs(2);

/// Quanto tempo um aviso fica no canto da tela (pedido do Felipe,
/// 2026-10-01: "precisa sumir após 3 segundos"). "Carregando carreira…"
/// é a exceção: fica enquanto a localização roda.
pub const DURACAO_AVISO: Duration = Duration::from_secs(3);

/// O que o banner do canto da tela anuncia.
#[derive(Debug, Clone, PartialEq)]
pub enum TipoAviso {
    /// A DLL acabou de ser injetada e o overlay está desenhando.
    Injetado,
    Carregando,
    Pronta(CareerSnapshot),
    Falhou,
    /// O jogo voltou para um save anterior: o Scout desfez o que tinha sido
    /// feito depois dele.
    VoltouNoTempo { data: Date, desfeitos: usize },
    /// Uma Missão terminou a busca e o Relatório está pronto (Story 2.4).
    RelatorioPronto { tipo: quality::TipoMissao, jogadores: usize },
    /// Apareceram jogadores novos num Relatório (Story 2.10).
    RelatorioAtualizado { tipo: quality::TipoMissao, novos: usize },
    /// A busca de uma Missão falhou; ela volta a `Pendente`.
    BuscaFalhou,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Aviso {
    pub tipo: TipoAviso,
    pub desde: Instant,
}

// ---------------------------------------------------------------------
// Entidades persistidas (Structural Seed / ERD; ids UUID v4 — AD-12)
// ---------------------------------------------------------------------
// Só os campos que o ERD e as ADs já fixam. Filtros, modo de busca e
// Qualidade chegam no Épico 2 — cada um com `#[serde(default)]` quando
// entrar, para os arquivos já gravados continuarem válidos.

/// Foco do Olheiro (PRD, Glossário). No JSON: `"cacador_de_jovens"` etc.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Especializacao {
    CacadorDeJovens,
    CacadorDeMedalhoes,
    Tatico,
    #[default]
    Generalista,
}

impl Especializacao {
    /// Ordem do PRD (FR-2), usada na lista de contratação.
    pub const TODAS: [Especializacao; 4] = [
        Especializacao::CacadorDeJovens,
        Especializacao::CacadorDeMedalhoes,
        Especializacao::Tatico,
        Especializacao::Generalista,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Especializacao::CacadorDeJovens => "Caçador de Jovens",
            Especializacao::CacadorDeMedalhoes => "Caçador de Medalhões",
            Especializacao::Tatico => "Tático",
            Especializacao::Generalista => "Generalista",
        }
    }
}

/// Nível do Olheiro (PRD, Glossário). No JSON: `"junior"` etc.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    #[default]
    Junior,
    Experiente,
    Elite,
}

impl Tier {
    #[allow(dead_code)] // usado nos testes
    pub const TODOS: [Tier; 3] = [Tier::Junior, Tier::Experiente, Tier::Elite];

    pub fn nome(self) -> &'static str {
        match self {
            Tier::Junior => "Júnior",
            Tier::Experiente => "Experiente",
            Tier::Elite => "Elite",
        }
    }
}

/// Modo de Busca de uma Missão (PRD, Glossário): Rápida = mais nomes,
/// Qualidade menor, conclui antes; Completa = menos nomes, Qualidade
/// maior, mais devagar. No JSON: `"rapida"` / `"completa"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModoBusca {
    Rapida,
    Completa,
}

impl ModoBusca {
    pub const TODOS: [ModoBusca; 2] = [ModoBusca::Rapida, ModoBusca::Completa];
}

/// Qualidade de um Relatório (PRD, Glossário): define precisão dos
/// atributos, quantos atributos aparecem e quantos jogadores voltam. Em
/// ordem: `Baixa < Media < Alta`. No JSON: `"baixa"` etc.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Qualidade {
    #[default]
    Baixa,
    Media,
    Alta,
}

/// Olheiro contratado. Os campos do Épico 5 (nome, nação, estrelas,
/// mercados) têm `default`: um Olheiro de antes deles continua válido e
/// ganha o perfil equivalente ao v1 (`Olheiro::perfil`), sem nome nem
/// mercado (que vale como "sem penalidade de mercado").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Olheiro {
    pub id: Uuid,
    pub especializacao: Especializacao,
    pub tier: Tier,
    /// Data da carreira na contratação (para "voltar no tempo" se o jogo
    /// for recarregado sem salvar). `None` em arquivos antigos.
    #[serde(default)]
    pub contratado_em: Option<Date>,
    /// Nome dado na contratação (vazio nos Olheiros de antes do Épico 5).
    #[serde(default)]
    pub nome: String,
    #[serde(default)]
    pub nacao: Option<NacaoOlheiro>,
    /// Estrelas (`None` = Olheiro de antes do Épico 5: `PerfilOlheiro::v1`).
    #[serde(default)]
    pub perfil: Option<quality::PerfilOlheiro>,
    #[serde(default)]
    pub mercados: Vec<quality::Mercado>,
    /// A oferta do mercado de onde ele veio (some da lista do mês).
    #[serde(default)]
    pub oferta_id: Option<Uuid>,
    /// Designado para manter a Lista de Escolhidos desde esta data (Épico
    /// 6). Enquanto isso não aceita Missão.
    #[serde(default)]
    pub acompanhando_desde: Option<Date>,
}

/// A nação de um Olheiro como gravada (nome para a tela, continente para
/// os mercados).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NacaoOlheiro {
    pub id: u16,
    pub nome: String,
    pub continente: Confederacao,
}

impl Olheiro {
    /// As estrelas dele (gravadas, ou as equivalentes ao v1).
    pub fn perfil(&self) -> quality::PerfilOlheiro {
        self.perfil.unwrap_or_else(|| quality::PerfilOlheiro::v1(self.especializacao, self.tier))
    }

    /// Nome para a tela: o dado na contratação ou, num Olheiro de antes do
    /// Épico 5, a Especialização (como era).
    pub fn nome_exibicao(&self) -> String {
        if self.nome.trim().is_empty() {
            self.especializacao.nome().to_string()
        } else {
            self.nome.clone()
        }
    }

    pub fn acompanhando(&self) -> bool {
        self.acompanhando_desde.is_some()
    }

    /// Só Generalistas mantêm a Lista de Escolhidos (pedido do Felipe).
    pub fn pode_acompanhar(&self) -> bool {
        self.perfil().foco() == Especializacao::Generalista
    }

    /// Quantos jogadores da lista ele mantém atualizados.
    pub fn capacidade_acompanhamento(&self) -> usize {
        if self.pode_acompanhar() {
            quality::capacidade_acompanhamento(self.perfil().generalista)
        } else {
            0
        }
    }
}

/// Um Olheiro à venda no mercado do mês (Épico 5, item 3). `olheiro` já
/// vem montado (sem id de verdade: o id é dado na contratação).
#[derive(Debug, Clone, PartialEq)]
pub struct OfertaOlheiro {
    /// Identidade da oferta no período (determinística).
    pub id: Uuid,
    pub olheiro: Olheiro,
    pub custo: i32,
    /// Quanto falta no orçamento para contratar (`None` = dá para pagar).
    pub faltam: Option<i32>,
}

/// O mercado de Olheiros como a tela o recebe.
#[derive(Debug, Clone, PartialEq)]
pub struct MercadoOlheiros {
    pub ofertas: Vec<OfertaOlheiro>,
    /// Atratividade do clube (0–100) e de onde ela veio (`None` = o save
    /// não deu o clube: atratividade média).
    pub atratividade: u8,
    pub clube: Option<quality::PerfilClube>,
    /// Primeiro dia do próximo mês da carreira (quando a oferta muda).
    pub renova_em: Date,
}

/// Olheiro contratado como a aba Olheiros o mostra.
#[derive(Debug, Clone, PartialEq)]
pub struct OlheiroContratado {
    pub olheiro: Olheiro,
    /// Tem Missão ainda não concluída (AD-8): "Em Missão" em vez de "Disponível".
    pub em_missao: bool,
    /// Designado para a Lista de Escolhidos (Épico 6).
    pub acompanhando: bool,
    /// A Missão em andamento, se houver.
    pub missao: Option<Missao>,
    /// Relatório da Missão em andamento (existe depois da primeira busca).
    pub relatorio_atual: Option<Uuid>,
    /// Relatórios que este Olheiro já entregou (inclui o atual).
    pub relatorios: usize,
}

impl OlheiroContratado {
    /// Não aceita Missão nova (em Missão ou acompanhando os Escolhidos).
    pub fn ocupado(&self) -> bool {
        self.em_missao || self.acompanhando
    }
}

/// Para onde vai o clique num Olheiro contratado (aba Olheiros).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinoOlheiro {
    /// Livre: abre a Nova Missão com ele.
    NovaMissao(Uuid),
    /// Em Missão: abre o Relatório dela.
    Relatorio(Uuid),
    /// Em Missão sem Relatório ainda: a aba Missões mostra o andamento.
    Missoes,
    /// Acompanhando a Lista de Escolhidos: a aba dela.
    Escolhidos,
}

/// Contratação em andamento: o usuário clicou "Contratar" e o modal de
/// confirmação está aberto (Story 1.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Contratacao {
    pub oferta: OfertaOlheiro,
    /// Nome que o jogador quer dar (começa com o gerado).
    pub nome: String,
    /// Falha da última tentativa de confirmar (o modal mostra e oferece
    /// tentar de novo). `None` = ainda não tentou.
    pub erro: Option<ErroCompra>,
}

impl Contratacao {
    pub fn custo(&self) -> i32 {
        self.oferta.custo
    }
}

/// O que o modal mostra, recalculado do orçamento vivo a cada frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviaContratacao {
    pub contratacao: Contratacao,
    pub orcamento_atual: i32,
    pub orcamento_apos: i32,
    /// Quanto falta (`None` = dá para pagar).
    pub faltam: Option<i32>,
}

/// Por que uma compra (contratar um Olheiro, Story 1.5; encomendar uma
/// Missão, Story 2.2) não aconteceu. Em todos os casos NADA foi salvo; só
/// `DebitadoSemSalvar` deixa o dinheiro gasto.
#[derive(Debug, Clone, PartialEq)]
pub enum ErroCompra {
    OrcamentoInsuficiente { faltam: i32 },
    /// O jogo mexeu no orçamento entre a tela e a confirmação: nada
    /// escrito; a tela já mostra o valor novo.
    OrcamentoMudou { atual: i32 },
    SemCarreira,
    /// O arquivo de estado desta carreira não pode ser gravado: comprar
    /// gastaria o dinheiro sem guardar o que foi comprado.
    EstadoNaoSalvavel,
    /// A escrita do orçamento falhou ou a releitura não conferiu.
    EscritaFalhou,
    /// O orçamento foi debitado, mas gravar a compra falhou; o débito
    /// foi desfeito.
    NaoSalvo,
    /// Gravar a compra falhou E desfazer o débito também: o dinheiro
    /// saiu. Caso raro, dito com todas as letras (nunca fingir sucesso).
    DebitadoSemSalvar { debitado: i32 },
}

/// Faixa de um atributo de 0 a 99 (inclusiva), como as do formulário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaixaAtributo {
    pub min: u8,
    pub max: u8,
}

impl FaixaAtributo {
    /// Limites de um atributo do FIFA (o save guarda 1–99).
    pub const MENOR: u8 = 1;
    pub const MAIOR: u8 = 99;

    pub fn valida(&self) -> bool {
        self.min <= self.max
    }
}

/// Filtros de uma Missão. A Story 2.2 traz Overall e Potencial; a 2.8, o
/// atributo dominante; a 2.9, a geografia; o Épico 3, Fit Posicional e
/// Jogador de Referência; os ajustes de 2026-10-03, idade, contrato,
/// geografia por liga e vários atributos dominantes — cada um com
/// `#[serde(default)]`, para os arquivos já gravados continuarem válidos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiltrosMissao {
    pub overall: FaixaAtributo,
    pub potencial: FaixaAtributo,
    /// Idade (anos completos na data da carreira).
    #[serde(default = "idade_padrao")]
    pub idade: FaixaAtributo,
    /// Anos de contrato que faltam: 0 = termina nesta temporada;
    /// `quality::CONTRATO_MAIOR` = isso ou mais.
    #[serde(default = "contrato_padrao")]
    pub contrato: FaixaAtributo,
    /// "Rápido e driblador": até `quality::MAX_DOMINANTES` atributos, todos
    /// entre os maiores do jogador (ver `quality::top_para`). Arquivos de
    /// antes guardavam UM, em `atributo_dominante` (Story 2.8).
    #[serde(default, alias = "atributo_dominante", deserialize_with = "um_ou_varios")]
    pub atributos_dominantes: Vec<Atributo>,
    /// Legado (Story 2.9): nacionalidades escolhidas no mapa antigo. A tela
    /// não mexe mais nele; Missões antigas continuam filtrando por ele.
    #[serde(default)]
    pub paises: Vec<u16>,
    /// Onde o jogador joga (2026-10-03): continentes inteiros, países (das
    /// ligas) e ligas. Um jogador passa se a liga do clube dele está em
    /// qualquer um. Tudo vazio = o mundo todo.
    #[serde(default)]
    pub continentes: Vec<Confederacao>,
    #[serde(default)]
    pub paises_dos_clubes: Vec<u16>,
    #[serde(default)]
    pub ligas: Vec<u32>,
    /// Ritmos de trabalho aceitos no ataque e na defesa (vazio = qualquer).
    #[serde(default)]
    pub ritmo_ataque: Vec<RitmoTrabalho>,
    #[serde(default)]
    pub ritmo_defesa: Vec<RitmoTrabalho>,
    /// Estrelas de drible (1–5).
    #[serde(default = "estrelas_padrao")]
    pub estrelas_drible: FaixaAtributo,
    /// Pé preferido; `None` = qualquer.
    #[serde(default)]
    pub pe: Option<FiltroPe>,
    /// Nível pedido em relação ao titular do elenco na posição (padrão:
    /// "muda patamar"; ver `quality::NivelEquipe`).
    #[serde(default)]
    pub nivel_elenco: Option<NivelEquipe>,
    /// Posições procuradas (grupos de posição nativa); vazio = todas.
    #[serde(default)]
    pub posicoes: Vec<Perfil>,
    /// Limite do valor de transferência: o orçamento do clube (padrão), um
    /// valor escolhido ou sem limite.
    #[serde(default)]
    pub limite_valor: Limite,
    /// Limite do salário semanal: a folha disponível do clube (padrão), um
    /// valor escolhido ou sem limite.
    #[serde(default)]
    pub limite_salario: Limite,
    /// Teto do valor estimado dos jogadores, fixado na confirmação a partir
    /// de `limite_valor`. `None` = sem teto (ou Missão antiga).
    #[serde(default)]
    pub teto_valor: Option<i64>,
    /// Teto do salário semanal estimado, fixado na confirmação a partir de
    /// `limite_salario`. `None` = sem teto.
    #[serde(default)]
    pub teto_salario: Option<i64>,
    /// Posição-alvo: só entram jogadores de OUTRA posição nativa cujo perfil
    /// serve nela (Story 3.4).
    #[serde(default)]
    pub fit_posicional: Option<PosicaoAlvo>,
    /// "Outro como ele": só entram perfis parecidos com o deste jogador do
    /// elenco (Story 3.3).
    #[serde(default)]
    pub referencia: Option<JogadorReferencia>,
}

/// O Jogador de Referência como estava quando a Missão foi encomendada: a
/// busca compara com esta foto, mesmo se ele sair do elenco depois.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JogadorReferencia {
    pub player_id: u32,
    pub nome: String,
    pub posicao: u8,
    /// Na ordem de `Atributo::TODOS` (valores reais: é jogador do usuário).
    pub atributos: Vec<u8>,
}

impl JogadorReferencia {
    pub fn atributo(&self, atributo: Atributo) -> Option<u8> {
        self.atributos.get(atributo.indice()).copied()
    }

    pub fn goleiro(&self) -> bool {
        self.posicao == 0
    }
}

impl Default for FiltrosMissao {
    /// Faixas amplas: sem restringir quase nada (o formulário abre com os
    /// filtros ideais do Olheiro, `quality::filtros_ideais`).
    fn default() -> Self {
        FiltrosMissao {
            overall: FaixaAtributo { min: 50, max: FaixaAtributo::MAIOR },
            potencial: FaixaAtributo { min: 50, max: FaixaAtributo::MAIOR },
            idade: idade_padrao(),
            contrato: contrato_padrao(),
            atributos_dominantes: Vec::new(),
            paises: Vec::new(),
            continentes: Vec::new(),
            paises_dos_clubes: Vec::new(),
            ligas: Vec::new(),
            ritmo_ataque: Vec::new(),
            ritmo_defesa: Vec::new(),
            estrelas_drible: estrelas_padrao(),
            pe: None,
            nivel_elenco: None,
            posicoes: Vec::new(),
            limite_valor: Limite::DoClube,
            limite_salario: Limite::DoClube,
            teto_valor: None,
            teto_salario: None,
            fit_posicional: None,
            referencia: None,
        }
    }
}

/// Limite de dinheiro de um filtro de orçamento (2026-10-03). No JSON:
/// `"do_clube"`, `{"ate": 15000000}`, `"sem_limite"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Limite {
    /// O que o clube tem: o orçamento depois de pagar a Missão (valor) ou a
    /// folha salarial disponível (salário).
    #[default]
    DoClube,
    Ate(i64),
    SemLimite,
}

impl Limite {
    /// O teto efetivo, dado o que o clube tem (`None` = sem teto).
    pub fn teto(self, do_clube: Option<i64>) -> Option<i64> {
        match self {
            Limite::DoClube => do_clube,
            Limite::Ate(v) => Some(v),
            Limite::SemLimite => None,
        }
    }
}

/// Filtro de pé (2026-10-03). "Ambidestro" = pé fraco com pelo menos
/// `quality::PE_FRACO_AMBIDESTRO` estrelas, qualquer que seja o preferido.
/// No JSON: `"direito"`, `"esquerdo"`, `"ambidestro"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FiltroPe {
    Direito,
    Esquerdo,
    Ambidestro,
}

impl FiltroPe {
    pub const TODOS: [FiltroPe; 3] = [FiltroPe::Direito, FiltroPe::Esquerdo, FiltroPe::Ambidestro];

    pub fn nome(self) -> &'static str {
        match self {
            FiltroPe::Direito => "Direito",
            FiltroPe::Esquerdo => "Esquerdo",
            FiltroPe::Ambidestro => "Ambidestro",
        }
    }
}

fn estrelas_padrao() -> FaixaAtributo {
    FaixaAtributo { min: quality::ESTRELAS_MENOR, max: quality::ESTRELAS_MAIOR }
}

impl FiltrosMissao {
    /// Algum filtro de onde o jogador joga?
    pub fn tem_geografia(&self) -> bool {
        !(self.continentes.is_empty() && self.paises_dos_clubes.is_empty() && self.ligas.is_empty())
    }
}

fn idade_padrao() -> FaixaAtributo {
    FaixaAtributo { min: quality::IDADE_MENOR, max: quality::IDADE_MAIOR }
}

fn contrato_padrao() -> FaixaAtributo {
    FaixaAtributo { min: 0, max: quality::CONTRATO_MAIOR }
}

/// `atributo_dominante` antigo (um ou nenhum) ou a lista nova.
fn um_ou_varios<'de, D>(deserializer: D) -> Result<Vec<Atributo>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum UmOuVarios {
        Varios(Vec<Atributo>),
        Um(Option<Atributo>),
    }
    Ok(match UmOuVarios::deserialize(deserializer)? {
        UmOuVarios::Varios(lista) => lista,
        UmOuVarios::Um(um) => um.into_iter().collect(),
    })
}

/// Qual ponta de qual faixa um botão − / + do formulário mexe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoFaixa {
    OverallMin,
    OverallMax,
    PotencialMin,
    PotencialMax,
    IdadeMin,
    IdadeMax,
    ContratoMin,
    ContratoMax,
    DribleMin,
    DribleMax,
}

impl CampoFaixa {
    /// Menor e maior valor que esta faixa aceita.
    pub fn limites(self) -> (u8, u8) {
        match self {
            CampoFaixa::IdadeMin | CampoFaixa::IdadeMax => (quality::IDADE_MENOR, quality::IDADE_MAIOR),
            CampoFaixa::ContratoMin | CampoFaixa::ContratoMax => (0, quality::CONTRATO_MAIOR),
            CampoFaixa::DribleMin | CampoFaixa::DribleMax => (quality::ESTRELAS_MENOR, quality::ESTRELAS_MAIOR),
            _ => (FaixaAtributo::MENOR, FaixaAtributo::MAIOR),
        }
    }
}

/// Formulário Nova Missão aberto (Story 2.2): o que o jogador escolheu até
/// agora. Nada é persistido antes de confirmar.
#[derive(Debug, Clone, PartialEq)]
pub struct RascunhoMissao {
    pub olheiro_id: Option<Uuid>,
    pub filtros: FiltrosMissao,
    pub modo: ModoBusca,
    /// "Sem prazo": blocos de `quality::DIAS_BLOCO_CONTINUO` dias (2.10).
    pub continua: bool,
    /// Verba da viagem (Épico 5, item 6).
    pub investimento: quality::Investimento,
    pub erro: Option<ErroCompra>,
}

/// Por que o botão confirmar está desabilitado (o texto vai ao lado dele).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BloqueioMissao {
    SemOlheiroDisponivel,
    FaixaInvalida { campo: CampoFaixa },
    OrcamentoInsuficiente { faltam: i32 },
}

/// O formulário inteiro, recalculado a cada frame (síncrono, AD-4).
#[derive(Debug, Clone, PartialEq)]
pub struct PreviaMissao {
    pub rascunho: RascunhoMissao,
    /// Todos os contratados: os "Em Missão" aparecem, mas não são escolhíveis.
    pub olheiros: Vec<OlheiroContratado>,
    pub tipo: quality::TipoMissao,
    /// Amplitude do filtro geográfico (Story 2.9).
    pub amplitude: quality::AmplitudeGeografica,
    /// O Olheiro escolhido combina com o tipo (bônus de Qualidade).
    pub combina: bool,
    /// `None` sem Olheiro escolhido.
    pub estimativa: Option<quality::EstimativaMissao>,
    pub orcamento_atual: i32,
    pub data_atual: Date,
    pub bloqueio: Option<BloqueioMissao>,
    /// Orçamento depois de pagar a Missão (o limite de valor "do clube").
    pub orcamento_apos_missao: Option<i64>,
    /// Folha salarial semanal disponível (o limite de salário "do clube").
    pub folha_disponivel: Option<i64>,
    /// Tetos que a Missão vai usar (`None` = sem limite).
    pub teto: Option<i64>,
    pub teto_salario: Option<i64>,
    /// Penalidade de mercado e de fora do foco para o Olheiro escolhido,
    /// já com a adaptação (Épico 5).
    pub penalidade: quality::Penalidade,
    /// Pior distância entre o filtro geográfico e os mercados dele (0–2).
    pub distancia_mercado: u8,
    /// A Missão está fora do foco dele (e ele ainda não se habituou).
    pub fora_do_foco: bool,
    /// Custo da Missão em cada verba, para a linha "Verba da viagem".
    pub custos_por_verba: Vec<(quality::Investimento, i32)>,
}

impl PreviaMissao {
    /// Dias até o prazo (ou até o fim do primeiro bloco, se contínua).
    pub fn duracao_dias(&self) -> Option<u32> {
        let e = self.estimativa?;
        Some(if self.rascunho.continua { quality::DIAS_BLOCO_CONTINUO } else { e.duracao_dias })
    }

    /// Data em que a Missão fica pronta (ou o primeiro bloco termina), se
    /// confirmada agora.
    pub fn prazo(&self) -> Option<Date> {
        self.duracao_dias().map(|d| self.data_atual.mais_dias(d))
    }
}

/// Andamento de uma Missão numa data (Story 2.3, FR-8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressoMissao {
    /// 0.0 a 1.0 — nunca negativo nem acima de 100%.
    pub fracao: f32,
    /// Dias de carreira até o prazo (0 quando já chegou).
    pub dias_restantes: u32,
    pub prazo_atingido: bool,
}

/// Progresso de `criada` até `prazo` na data `hoje`. Datas fora da janela
/// ficam presas nas pontas: antes da criação (ex.: carregou um save mais
/// antigo) = 0%; no prazo ou depois = 100% e `prazo_atingido`.
pub fn progresso_missao(criada: Date, prazo: Date, hoje: Date) -> ProgressoMissao {
    let total = (prazo.day_number() - criada.day_number()).max(1);
    let passados = (hoje.day_number() - criada.day_number()).clamp(0, total);
    let restantes = (prazo.day_number() - hoje.day_number()).max(0);
    let prazo_atingido = hoje >= prazo;
    ProgressoMissao {
        // Prazo cumprido é sempre barra cheia (inclusive prazo = criação).
        fracao: if prazo_atingido { 1.0 } else { passados as f32 / total as f32 },
        dias_restantes: u32::try_from(restantes).unwrap_or(u32::MAX),
        prazo_atingido,
    }
}

/// Uma linha da aba Missões.
#[derive(Debug, Clone, PartialEq)]
pub struct MissaoNaLista {
    pub missao: Missao,
    pub olheiro: Option<Olheiro>,
    /// `None` quando a data da carreira não pôde ser lida (erro de leitura):
    /// a lista continua visível, só sem progresso (Story 2.3).
    pub progresso: Option<ProgressoMissao>,
    /// Falha da última busca (Story 2.4); a Missão voltou a `Pendente`.
    pub falha: Option<String>,
    /// Relatório desta Missão, se já existe (Story 2.5).
    pub relatorio_id: Option<Uuid>,
    /// Há jogadores que o jogador ainda não viu: indicador "novo"
    /// (UX-DR13; volta quando o Relatório parcial cresce — Story 2.10).
    pub relatorio_novo: bool,
    /// Jogadores já revelados / previstos ao fim dos blocos pagos.
    pub revelados: usize,
    pub previstos: usize,
}

/// Um Relatório como a aba Relatórios e a tela do Relatório o mostram.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatorioNaLista {
    /// `jogadores` traz SÓ os já revelados (Relatório parcial, Story 2.10).
    pub relatorio: Relatorio,
    /// Jogadores que o Relatório terá quando a Missão terminar.
    pub previstos: usize,
    /// A Missão ainda está rodando: mais jogadores vão aparecer.
    pub parcial: bool,
    /// Há jogadores que o jogador ainda não viu.
    pub novo: bool,
    /// `None` se a Missão sumiu do arquivo (não deveria acontecer).
    pub missao: Option<Missao>,
    pub olheiro: Option<Olheiro>,
}

/// O mercado de Olheiros de um mês (Épico 5): os candidatos de
/// `quality::gerar_ofertas`, com nome (`scout::nomes`), nação e custo, sem
/// os já contratados (`contratadas`), em ordem de custo, com o que falta
/// para cada um diante de `orcamento`.
pub fn montar_ofertas(
    candidatos: &[quality::CandidatoOlheiro],
    periodo: u32,
    carreira: u64,
    nacoes: &[Nacao],
    contratadas: &[Uuid],
    orcamento: Option<i32>,
) -> Vec<OfertaOlheiro> {
    let mut ofertas: Vec<OfertaOlheiro> = candidatos
        .iter()
        .map(|c| {
            let id = Uuid::from_u64_pair(carreira, (u64::from(periodo) << 32) | u64::from(c.indice));
            let nacao = c.pais.and_then(|p| nacoes.iter().find(|n| n.id == p));
            let olheiro = Olheiro {
                id,
                especializacao: c.perfil.foco(),
                tier: c.perfil.tier(),
                contratado_em: None,
                nome: crate::scout::nomes::gerar(nacao.map_or("", |n| n.iso.as_str()), c.semente),
                nacao: nacao.map(|n| NacaoOlheiro { id: n.id, nome: n.nome.clone(), continente: n.confederacao }),
                perfil: Some(c.perfil),
                mercados: c.mercados.clone(),
                oferta_id: Some(id),
                acompanhando_desde: None,
            };
            let custo = quality::custo_contratacao(&c.perfil, nacao.map(|n| n.confederacao), c.mercados.len());
            let faltam = orcamento.and_then(|saldo| (saldo < custo).then(|| custo.saturating_sub(saldo)));
            OfertaOlheiro { id, olheiro, custo, faltam }
        })
        .filter(|o| !contratadas.contains(&o.id))
        .collect();
    ofertas.sort_by_key(|o| (o.custo, o.id));
    ofertas
}

/// Semente da carreira (para o mercado de Olheiros) a partir do id do save.
pub fn semente_da_carreira(id_save: &str) -> u64 {
    u64::from_str_radix(id_save.get(..16).unwrap_or("0"), 16).unwrap_or(0)
}

/// Ciclo de vida de uma Missão (AD-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusMissao {
    Pendente,
    EmExecucao,
    Concluida,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Missao {
    pub id: Uuid,
    pub olheiro_id: Uuid,
    pub status: StatusMissao,
    pub criada_em: Date,
    pub prazo_estimado: Date,
    pub filtros: FiltrosMissao,
    pub modo_busca: ModoBusca,
    pub tipo: quality::TipoMissao,
    pub amplitude: quality::AmplitudeGeografica,
    /// O que o jogador viu e pagou ao confirmar (custo, Qualidade, ...).
    /// Numa Missão contínua: o custo e os jogadores de UM bloco.
    pub estimativa: quality::EstimativaMissao,
    /// Missão "sem prazo" (Story 2.10): o Olheiro fica nela, bloco a bloco
    /// de `quality::DIAS_BLOCO_CONTINUO` dias, até ser encerrada.
    #[serde(default)]
    pub continua: bool,
    /// Blocos pagos (sempre 1 numa Missão de prazo fixo).
    #[serde(default = "um_bloco")]
    pub blocos: u16,
    /// Para quantos blocos a busca já rodou: menor que `blocos` = falta
    /// buscar os jogadores do bloco novo.
    #[serde(default)]
    pub blocos_buscados: u16,
    /// Renovações: `(data da renovação, prazo anterior)` — para desfazer
    /// uma renovação se o jogo for recarregado sem salvar.
    #[serde(default)]
    pub renovacoes: Vec<(Date, Date)>,
    /// Verba da viagem escolhida (Épico 5; Padrão nas Missões de antes).
    #[serde(default)]
    pub investimento: quality::Investimento,
}

fn um_bloco() -> u16 {
    1
}

impl Missao {
    /// Jogadores que o Relatório terá ao fim dos blocos pagos.
    pub fn alvo_total(&self) -> usize {
        usize::from(self.estimativa.alvo_jogadores) * usize::from(self.blocos.max(1))
    }

    /// Quantos jogadores de `encontrados` já apareceram em `hoje`
    /// (Story 2.10). Concluída mostra todos; sem data, nenhum novo.
    pub fn revelados(&self, encontrados: usize, hoje: Option<Date>) -> usize {
        match (self.status, hoje) {
            (StatusMissao::Concluida, _) => encontrados,
            (_, None) => 0,
            (_, Some(hoje)) => {
                let fracao = progresso_missao(self.criada_em, self.prazo_estimado, hoje).fracao;
                quality::revelados(fracao, self.alvo_total(), encontrados)
            }
        }
    }
}

#[cfg(test)]
impl Missao {
    /// Missão mínima para testes.
    pub fn de_teste(olheiro_id: Uuid, status: StatusMissao) -> Missao {
        let filtros = FiltrosMissao::default();
        let pedido = quality::PedidoMissao::v1(
            Tier::Junior,
            Especializacao::Tatico,
            ModoBusca::Rapida,
            quality::TipoMissao::Geral,
            quality::AmplitudeGeografica::Mundo,
        );
        Missao {
            id: Uuid::new_v4(),
            olheiro_id,
            status,
            criada_em: Date(20260701),
            prazo_estimado: Date(20260715),
            filtros,
            modo_busca: ModoBusca::Rapida,
            tipo: quality::TipoMissao::Geral,
            amplitude: quality::AmplitudeGeografica::Mundo,
            estimativa: quality::estimar_missao(&pedido),
            continua: false,
            blocos: 1,
            blocos_buscados: 0,
            renovacoes: Vec::new(),
            investimento: quality::Investimento::Padrao,
        }
    }
}

/// Um atributo que o Olheiro observou, como faixa (`min == max` = exato).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtributoRevelado {
    pub atributo: Atributo,
    pub valor: FaixaAtributo,
}

/// Um jogador de um Relatório (AD-12: chave `player_id`). Guarda SÓ o que
/// foi revelado — o valor real nunca vai para o arquivo nem para a tela.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JogadorEncontrado {
    pub player_id: u32,
    pub nome: String,
    pub idade: u8,
    /// `preferredposition1` (ver `save_repo::nome_posicao`).
    pub posicao: u8,
    pub nacao_id: u16,
    pub nacao: String,
    pub clube: String,
    /// Ano de fim do contrato (dado do save, não revelado por faixa).
    /// `None` em Relatórios antigos ou sem clube.
    #[serde(default)]
    pub contrato_ate: Option<u16>,
    /// Quanto o Olheiro já sabe dele agora (Relatório parcial). Não vai
    /// para o arquivo: é calculado pela data ao montar a lista.
    #[serde(skip)]
    pub observacao: quality::Observacao,
    pub overall: FaixaAtributo,
    pub potencial: FaixaAtributo,
    /// Na ordem em que o Olheiro observou (a função do jogador primeiro).
    pub atributos: Vec<AtributoRevelado>,
    /// Pé preferido (Story 3.1; `None` em Relatórios de antes dela).
    #[serde(default)]
    pub pe: Option<Pe>,
    /// Similaridade (%) com o Jogador de Referência da Missão, pelo que o
    /// Olheiro viu (Story 3.3).
    #[serde(default)]
    pub similaridade: Option<u8>,
    /// Força do Fit Posicional (%) na posição-alvo da Missão, pelo que o
    /// Olheiro viu (Story 3.4).
    #[serde(default)]
    pub fit: Option<u8>,
    /// Quanto o Overall mudaria na posição-alvo (estimativa, pelo que o
    /// Olheiro viu; 2026-10-03).
    #[serde(default)]
    pub variacao_overall: Option<i8>,
    /// Ritmos de trabalho (ataque, defesa), estrelas de drible e de pé
    /// fraco (2026-10-03; `None` em Relatórios de antes).
    #[serde(default)]
    pub ritmo_ataque: Option<RitmoTrabalho>,
    #[serde(default)]
    pub ritmo_defesa: Option<RitmoTrabalho>,
    #[serde(default)]
    pub estrelas_drible: Option<u8>,
    #[serde(default)]
    pub pe_fraco: Option<u8>,
    /// Overall do titular do elenco na posição com que ele foi comparado
    /// (2026-10-03).
    #[serde(default)]
    pub titular_elenco: Option<u8>,
    /// Falso positivo (Épico 5, item 7): o Olheiro achou que ele passava no
    /// filtro, mas não passa. Nunca aparece no Relatório; a Lista de
    /// Escolhidos avisa quando o acompanhamento chega ao valor exato.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub falso_positivo: bool,
}

impl JogadorEncontrado {
    pub fn atributo(&self, atributo: Atributo) -> Option<FaixaAtributo> {
        self.atributos.iter().find(|a| a.atributo == atributo).map(|a| a.valor)
    }

    /// O valor "pelo que o Olheiro viu": o meio da faixa revelada.
    pub fn valor_visto(&self, atributo: Atributo) -> Option<f32> {
        self.atributo(atributo).map(|f| (f32::from(f.min) + f32::from(f.max)) * 0.5)
    }

    pub fn goleiro(&self) -> bool {
        crate::save_repo::funcao_da_posicao(self.posicao) == Funcao::Goleiro
    }

    /// Valor de mercado estimado pelo Olheiro (o save não guarda valor):
    /// a partir do meio das faixas reveladas e da idade.
    pub fn valor_estimado(&self) -> i64 {
        quality::valor_estimado(meio_da_faixa(self.overall), meio_da_faixa(self.potencial), self.idade, self.goleiro())
    }

    /// Salário semanal estimado.
    pub fn salario_estimado(&self) -> i64 {
        quality::salario_estimado(meio_da_faixa(self.overall))
    }

    /// O Olheiro já tem a expectativa de salário?
    pub fn salario_conhecido(&self) -> bool {
        self.observacao != quality::Observacao::SoMercado
    }

    /// O Olheiro já observou os atributos?
    pub fn atributos_observados(&self) -> bool {
        self.observacao == quality::Observacao::Completa
    }
}

/// Meio de uma faixa revelada (arredondado para baixo).
pub fn meio_da_faixa(faixa: FaixaAtributo) -> u8 {
    u8::try_from((u16::from(faixa.min) + u16::from(faixa.max)) / 2).unwrap_or(faixa.min)
}

/// Um jogador do elenco do técnico (seletor de elenco, Stories 3.2/3.3).
/// Valores reais: o jogador é dele.
#[derive(Debug, Clone, PartialEq)]
pub struct JogadorElenco {
    pub player_id: u32,
    pub nome: String,
    pub idade: u8,
    pub posicao: u8,
    pub overall: u8,
    pub potencial: u8,
    /// Na ordem de `Atributo::TODOS`.
    pub atributos: Vec<u8>,
}

impl JogadorElenco {
    pub fn atributo(&self, atributo: Atributo) -> Option<u8> {
        self.atributos.get(atributo.indice()).copied()
    }

    pub fn como_referencia(&self) -> JogadorReferencia {
        JogadorReferencia {
            player_id: self.player_id,
            nome: self.nome.clone(),
            posicao: self.posicao,
            atributos: self.atributos.clone(),
        }
    }
}

/// Algo lido do save em background, como as telas o recebem.
#[derive(Debug, Clone, PartialEq)]
pub enum Carga<T> {
    Carregando,
    Pronto(T),
    /// Não deu para ler o save: a tela mostra o erro com "Tentar novamente".
    Erro,
}

/// O elenco como as telas o recebem (`listar_elenco_atual`).
pub type EstadoElenco = Carga<Arc<Vec<JogadorElenco>>>;

/// A Ficha de Jogador aberta (Story 3.1): o jogador do Relatório e, se
/// escolhido, o jogador do elenco sobreposto no Radar (Story 3.2). Aberta
/// da Lista de Escolhidos (Épico 6), `item` é um Relatório de uma linha só
/// (o da Missão de origem, se ainda existe) e `escolhido` traz o estado
/// do acompanhamento.
#[derive(Debug, Clone, PartialEq)]
pub struct FichaAberta {
    pub item: RelatorioNaLista,
    pub jogador: JogadorEncontrado,
    pub comparacao: Option<JogadorElenco>,
    /// O jogador está na Lista de Escolhidos (com o estado de agora).
    pub escolhido: Option<EscolhidoNaLista>,
    /// A Ficha foi aberta da aba Escolhidos (não de um Relatório).
    pub da_lista: bool,
}

// ---------------------------------------------------------------------
// Lista de Escolhidos (Épico 6)
// ---------------------------------------------------------------------

/// Um jogador na Lista de Escolhidos. Guarda a ÚLTIMA observação (só o
/// revelado, como um jogador de Relatório) e quando ela foi feita: dali
/// em diante ela envelhece (`quality::frescor`), a não ser que um
/// Generalista designado o acompanhe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Escolhido {
    pub jogador: JogadorEncontrado,
    pub adicionado_em: Date,
    /// Data da última observação (a do Relatório, ou a da última
    /// atualização do acompanhamento).
    pub observado_em: Date,
    /// Precisão (±) da última observação.
    pub precisao: u8,
    /// Acompanhado antes dos outros quando falta vaga.
    #[serde(default)]
    pub prioridade: bool,
    /// Acompanhamento em andamento (`None` = sem Generalista nele).
    #[serde(default)]
    pub acompanhamento: Option<Acompanhamento>,
    /// Posição-alvo da Missão de origem (Fit Posicional), para recalcular o fit.
    #[serde(default)]
    pub alvo: Option<PosicaoAlvo>,
    /// Jogador de Referência da Missão de origem, para recalcular a similaridade.
    #[serde(default)]
    pub referencia: Option<JogadorReferencia>,
    /// Relatório de onde ele veio.
    #[serde(default)]
    pub relatorio_id: Option<Uuid>,
}

/// O acompanhamento de um Escolhido por um Generalista: de onde partiu e
/// quantos dias de carreira já foram acompanhados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acompanhamento {
    pub inicio: Date,
    pub precisao_inicial: u8,
    pub atributos_iniciais: u8,
    /// Dias até o valor exato (`quality::dias_para_exato` na partida).
    pub dias_para_exato: u32,
    /// Dias de carreira acompanhados até a última atualização.
    pub dias: u32,
}

/// Um Escolhido como a aba e a Ficha o mostram (calculado para a data do
/// painel; nada disto vai para o arquivo).
#[derive(Debug, Clone, PartialEq)]
pub struct EscolhidoNaLista {
    pub escolhido: Escolhido,
    /// A observação como vale agora: faixas alargadas pelo tempo, ou sem
    /// atributos se venceu.
    pub jogador: JogadorEncontrado,
    pub frescor: quality::Frescor,
    /// Tem vaga com um Generalista designado agora.
    pub acompanhado: bool,
    /// Precisão (±) que vale agora.
    pub precisao: u8,
    /// Dias de carreira até ficar exato (só acompanhado).
    pub dias_para_exato: Option<u32>,
    /// Exato e fora do filtro da Missão de origem (falso positivo revelado).
    pub fora_do_filtro: bool,
}

/// O resumo do acompanhamento (cabeçalho da aba Escolhidos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumoAcompanhamento {
    pub generalistas: usize,
    pub vagas: usize,
    pub jogadores: usize,
    pub acompanhados: usize,
}

/// Total de atributos da função do jogador (linha: 28; goleiro: 33).
fn total_de_atributos(goleiro: bool) -> usize {
    Atributo::TODOS.iter().filter(|a| goleiro || !a.goleiro()).count()
}

/// O Escolhido como vale em `hoje`: frescor, faixas alargadas, progresso do
/// acompanhamento (contando os dias desde a última atualização).
pub fn escolhido_em(escolhido: &Escolhido, hoje: Date, acompanhado: bool) -> EscolhidoNaLista {
    let idade = u32::try_from((hoje.day_number() - escolhido.observado_em.day_number()).max(0)).unwrap_or(u32::MAX);
    // acompanhado: a observação é refeita a cada abertura do painel e não envelhece
    let frescor = if acompanhado && escolhido.acompanhamento.is_some() { quality::Frescor::Atualizado } else { quality::frescor(idade) };
    let mut jogador = escolhido.jogador.clone();
    let extra = frescor.extra();
    if frescor == quality::Frescor::Vencido {
        jogador.atributos.clear();
        jogador.similaridade = None;
        jogador.fit = None;
        jogador.variacao_overall = None;
    } else if extra > 0 {
        let alargar = |f: FaixaAtributo| FaixaAtributo {
            min: f.min.saturating_sub(extra).max(FaixaAtributo::MENOR),
            max: f.max.saturating_add(extra).min(FaixaAtributo::MAIOR),
        };
        jogador.overall = alargar(jogador.overall);
        jogador.potencial = alargar(jogador.potencial);
        for a in jogador.atributos.iter_mut() {
            a.valor = alargar(a.valor);
        }
    }
    let precisao = escolhido.precisao.saturating_add(extra);
    let dias_para_exato = escolhido.acompanhamento.filter(|_| acompanhado).map(|a| {
        let feitos = a.dias.saturating_add(idade);
        a.dias_para_exato.saturating_sub(feitos)
    });
    let exato = escolhido.precisao == 0 && frescor == quality::Frescor::Atualizado;
    EscolhidoNaLista {
        fora_do_filtro: exato && escolhido.jogador.falso_positivo,
        escolhido: escolhido.clone(),
        jogador,
        frescor,
        acompanhado,
        precisao,
        dias_para_exato,
    }
}

/// Um passo do acompanhamento em `hoje`, com os valores atuais do save:
/// - começando agora: parte do que a observação vale hoje (envelhecida),
///   ou de uma análise nova se ela venceu;
/// - já em andamento: soma os dias desde a última observação.
///
/// A precisão fecha e os atributos não observados aparecem em linha reta
/// até `dias_para_exato`. Jogador que sumiu do save (aposentado) mantém a
/// última observação, mas conta como visto hoje — senão cada atualização
/// pediria outra leitura do save, sem fim.
pub fn avancar_escolhido(e: &Escolhido, pool: &crate::save_repo::PlayerPool, hoje: Date) -> Escolhido {
    let desde = u32::try_from((hoje.day_number() - e.observado_em.day_number()).max(0)).unwrap_or(0);
    let Some(raw) = pool.jogadores.iter().find(|j| j.player_id == e.jogador.player_id) else {
        let total = total_de_atributos(e.jogador.goleiro());
        let acompanhamento = e.acompanhamento.map_or(
            Acompanhamento {
                inicio: hoje,
                precisao_inicial: e.precisao,
                atributos_iniciais: u8::try_from(e.jogador.atributos.len()).unwrap_or(u8::MAX),
                dias_para_exato: quality::dias_para_exato(e.precisao, e.jogador.atributos.len(), total),
                dias: 0,
            },
            |a| Acompanhamento { dias: a.dias.saturating_add(desde), ..a },
        );
        return Escolhido { observado_em: hoje.max(e.observado_em), acompanhamento: Some(acompanhamento), ..e.clone() };
    };
    let goleiro = crate::save_repo::funcao_da_posicao(raw.posicao) == Funcao::Goleiro;
    let total = total_de_atributos(goleiro);
    let acompanhamento = match e.acompanhamento {
        Some(a) => Acompanhamento { dias: a.dias.saturating_add(desde), ..a },
        None => {
            let atual = escolhido_em(e, hoje, false);
            let (precisao, atributos) = if atual.frescor == quality::Frescor::Vencido {
                (quality::PRECISAO_ANALISE_NOVA, quality::ATRIBUTOS_ANALISE_NOVA)
            } else {
                (atual.precisao, atual.jogador.atributos.len())
            };
            Acompanhamento {
                inicio: hoje,
                precisao_inicial: precisao,
                atributos_iniciais: u8::try_from(atributos).unwrap_or(u8::MAX),
                dias_para_exato: quality::dias_para_exato(precisao, atributos, total),
                dias: 0,
            }
        }
    };
    let precisao = quality::precisao_acompanhada(acompanhamento.precisao_inicial, acompanhamento.dias, acompanhamento.dias_para_exato);
    let atributos = quality::atributos_acompanhados(
        usize::from(acompanhamento.atributos_iniciais),
        total,
        acompanhamento.dias,
        acompanhamento.dias_para_exato,
    );
    Escolhido {
        jogador: super::search::reobservar(&e.jogador, raw, pool, hoje, precisao, atributos, e.alvo, e.referencia.as_ref()),
        observado_em: hoje,
        precisao,
        acompanhamento: Some(acompanhamento),
        ..e.clone()
    }
}

/// Quando o Olheiro observou o `indice`-ésimo jogador de um Relatório: numa
/// Missão contínua, no fim do bloco em que ele apareceu (cada bloco traz o
/// mesmo número de jogadores); numa de prazo fixo, no prazo (ou hoje, se
/// ainda não chegou); sem a Missão, na data do Relatório.
pub fn data_da_observacao(missao: Option<&Missao>, gerado_em: Option<Date>, indice: usize, hoje: Date) -> Date {
    let data = match missao {
        Some(m) if m.continua => {
            let por_bloco = usize::from(m.estimativa.alvo_jogadores).max(1);
            let bloco = u32::try_from(indice / por_bloco).unwrap_or(0);
            m.criada_em.mais_dias(quality::DIAS_BLOCO_CONTINUO * (bloco + 1))
        }
        Some(m) => m.prazo_estimado,
        None => gerado_em.unwrap_or(hoje),
    };
    data.min(hoje)
}

/// Ordem de vaga no acompanhamento: prioritários primeiro, depois os mais
/// antigos na lista.
pub fn ordem_de_acompanhamento(escolhidos: &[Escolhido]) -> Vec<u32> {
    let mut ordem: Vec<&Escolhido> = escolhidos.iter().collect();
    ordem.sort_by_key(|e| (!e.prioridade, e.adicionado_em, e.jogador.player_id));
    ordem.into_iter().map(|e| e.jogador.player_id).collect()
}

/// Relatório de uma Missão (Story 2.4). Campos novos com `default`: um
/// arquivo antigo continua válido (AD-7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relatorio {
    pub id: Uuid,
    pub missao_id: Uuid,
    #[serde(default)]
    pub gerado_em: Option<Date>,
    #[serde(default)]
    pub qualidade: Qualidade,
    #[serde(default)]
    pub precisao_mais_menos: u8,
    #[serde(default)]
    pub jogadores: Vec<JogadorEncontrado>,
    /// Já foi aberto ao menos uma vez (some o "novo"; libera "Arquivar").
    #[serde(default)]
    pub aberto: bool,
    #[serde(default)]
    pub arquivado: bool,
    /// Jogadores revelados da última vez que o Relatório foi aberto: o
    /// "novo" volta quando aparecem mais (Story 2.10).
    #[serde(default)]
    pub vistos: u16,
    /// Jogadores já anunciados no banner "Relatório atualizado".
    #[serde(default)]
    pub notificados: u16,
}

#[cfg(test)]
impl Relatorio {
    /// Relatório vazio para testes.
    pub fn de_teste(missao_id: Uuid) -> Relatorio {
        Relatorio {
            id: Uuid::new_v4(),
            missao_id,
            gerado_em: Some(Date(20260801)),
            qualidade: Qualidade::Media,
            precisao_mais_menos: 3,
            jogadores: Vec::new(),
            aberto: false,
            arquivado: false,
            vistos: 0,
            notificados: 0,
        }
    }
}

/// O que o painel deve mostrar sobre a carreira.
#[derive(Debug, Clone, PartialEq)]
pub enum CarreiraStatus {
    /// Varredura de localização rodando em background.
    Localizando,
    /// Nenhuma carreira carregada no jogo (ou ainda não localizada).
    SemCarreira,
    /// Falha ao ler o save ativo (memória inacessível, build diferente...).
    ErroLeitura,
    Pronta(CareerSnapshot),
}

/// Uma Missão na fila de busca (AD-9), com a carreira dona dela: o
/// resultado é gravado no arquivo certo mesmo se o jogador trocar de
/// carreira enquanto a busca roda.
#[derive(Debug, Clone, PartialEq)]
struct BuscaNaFila {
    id_save: String,
    missao: Missao,
    hoje: Date,
    /// Jogadores já no Relatório (bloco anterior): a busca traz outros.
    excluir: std::collections::HashSet<u32>,
    quantos: usize,
}

pub struct ScoutState {
    fonte: Arc<dyn CareerSource>,
    tarefa_localizar: AsyncTask<()>,
    status: CarreiraStatus,
    /// Uma localização automática por abertura do painel (evita loop).
    localizacao_automatica_disponivel: bool,
    /// Modal de confirmação de contratação aberto (Story 1.5).
    contratacao: Option<Contratacao>,
    /// Formulário Nova Missão aberto (Story 2.2).
    rascunho_missao: Option<RascunhoMissao>,
    ultima_leitura: Option<Instant>,
    /// Pasta dos arquivos de estado (`None` = sem disco, só memória).
    diretorio_estado: Option<PathBuf>,
    /// Estados já carregados nesta sessão, por hash do save.
    estados: HashMap<String, EstadoPersistido>,
    /// Hash da carreira pronta agora (`None` fora de `Pronta`).
    save_ativo: Option<String>,
    /// Última carreira que esteve pronta: com erro de leitura a aba
    /// Missões continua mostrando a lista dela (Story 2.3).
    ultimo_save: Option<String>,
    /// Data usada para o progresso das Missões. Fica FIXA enquanto o painel
    /// está aberto: só muda ao abrir o painel ou quando uma carreira fica
    /// pronta (FR-8: "recalculado ao abrir o painel, sem polling").
    data_progresso: Option<Date>,
    /// Aba salva da carreira que acabou de ficar ativa, para o `Scout`
    /// aplicar na navegação (ver `tomar_aba_restaurada`).
    aba_restaurada: Option<Aba>,
    /// Sinal barato "há carreira carregada?" (ver o vigia no topo).
    tarefa_sinal: AsyncTask<bool>,
    /// Há um resultado do sinal ainda não tratado (`poll` não consome).
    sinal_pendente: bool,
    /// O sinal já disparou uma localização e só rearma ao apagar.
    sinal_consumido: bool,
    proximo_sinal: Option<Instant>,
    aviso: Option<Aviso>,
    /// O aviso "Central de Scout ativa" sai no primeiro `tick` (o primeiro
    /// frame pode vir segundos depois da injeção, com o jogo carregando).
    aviso_inicial_pendente: bool,
    /// Missões `EmExecucao` esperando a vez, em ordem (AD-9: uma de cada
    /// vez, nunca em paralelo).
    fila_busca: VecDeque<BuscaNaFila>,
    tarefa_busca: AsyncTask<Vec<JogadorEncontrado>>,
    /// A busca disparada e ainda não tratada (`poll` não consome).
    busca_atual: Option<BuscaNaFila>,
    /// Última falha de busca por Missão, para a aba Missões dizer.
    falhas_busca: HashMap<Uuid, String>,
    painel_aberto: bool,
    /// Relatório na tela (Story 2.5).
    relatorio_aberto: Option<Uuid>,
    /// Rostos dos jogadores da visão Cards (Story 2.6).
    minifaces: Minifaces,
    /// Nações do mapa (Story 2.9), lidas uma vez em background.
    tarefa_nacoes: AsyncTask<Arc<Vec<Nacao>>>,
    /// Aba Relatórios mostrando o filtro "Arquivados" (Story 2.7; não
    /// persiste: reabrir o painel volta à lista principal).
    vendo_arquivados: bool,
    /// Última data viva conferida para o banner "Relatório atualizado".
    data_avisos: Option<Date>,
    /// Falha da última renovação/encerramento, por Missão (Story 2.10).
    erros_missao: HashMap<Uuid, ErroCompra>,
    /// Elenco do técnico, lido em background e marcado com a carreira dona
    /// (Stories 3.2/3.3).
    tarefa_elenco: AsyncTask<(String, Arc<Vec<JogadorElenco>>)>,
    /// Ficha aberta: `player_id` dentro do Relatório aberto (Story 3.1).
    ficha: Option<u32>,
    /// Jogador do elenco sobreposto no Radar da Ficha (Story 3.2).
    comparacao: Option<u32>,
    /// Ligas com clubes da carreira (filtro geográfico), lidas em
    /// background e marcadas com a carreira dona.
    tarefa_ligas: AsyncTask<(String, Arc<Vec<Liga>>)>,
    /// Nível aberto da árvore do filtro geográfico.
    foco_geografico: FocoGeografico,
    /// LB/RB (ou clique na aba) com a Nova Missão aberta: a aba para onde
    /// ir, esperando o jogador confirmar que descarta o rascunho.
    troca_de_aba_pendente: Option<Aba>,
    /// Pixels a rolar neste frame pelo analógico direito (positivo = para
    /// baixo).
    rolagem: f32,
    /// "Mostrar detalhes" do formulário Nova Missão (fechado ao abrir).
    detalhes_da_missao: bool,
    /// Prestígio, liga e títulos do clube (mercado de Olheiros, Épico 5),
    /// lidos em background e marcados com a carreira dona; `None` = o save
    /// não deu o clube (atratividade média).
    tarefa_clube: AsyncTask<(String, Option<quality::PerfilClube>)>,
    /// Atualização dos Escolhidos acompanhados (Épico 6), com a carreira dona.
    tarefa_escolhidos: AsyncTask<(String, Vec<Escolhido>)>,
    /// Há uma atualização disparada e ainda não tratada.
    atualizacao_escolhidos_pendente: bool,
    /// Pediram outra atualização enquanto uma rodava: roda de novo no fim.
    reatualizar_escolhidos: bool,
    /// A Ficha aberta é de um jogador da Lista de Escolhidos (não do
    /// Relatório aberto).
    ficha_de_escolhido: bool,
}

/// Nível aberto no filtro geográfico: a lista de continentes (o filtro
/// rápido), os países de um continente ou as ligas de um país.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FocoGeografico {
    #[default]
    Continentes,
    Continente(Confederacao),
    Pais(Confederacao, u16),
}

impl ScoutState {
    pub fn new() -> Self {
        Self::com_fonte(Box::new(SaveRepoSource), persistence::diretorio_padrao())
    }

    pub fn com_fonte(fonte: Box<dyn CareerSource>, diretorio_estado: Option<PathBuf>) -> Self {
        ScoutState {
            fonte: Arc::from(fonte),
            tarefa_localizar: AsyncTask::new(),
            status: CarreiraStatus::SemCarreira,
            localizacao_automatica_disponivel: false,
            contratacao: None,
            rascunho_missao: None,
            ultima_leitura: None,
            diretorio_estado,
            estados: HashMap::new(),
            save_ativo: None,
            ultimo_save: None,
            data_progresso: None,
            aba_restaurada: None,
            tarefa_sinal: AsyncTask::new(),
            sinal_pendente: false,
            sinal_consumido: false,
            proximo_sinal: None,
            aviso: None,
            aviso_inicial_pendente: true,
            fila_busca: VecDeque::new(),
            tarefa_busca: AsyncTask::new(),
            busca_atual: None,
            falhas_busca: HashMap::new(),
            painel_aberto: false,
            relatorio_aberto: None,
            minifaces: Minifaces::new(crate::save_repo::ler_miniface),
            tarefa_nacoes: AsyncTask::new(),
            vendo_arquivados: false,
            data_avisos: None,
            erros_missao: HashMap::new(),
            tarefa_elenco: AsyncTask::new(),
            ficha: None,
            comparacao: None,
            tarefa_ligas: AsyncTask::new(),
            foco_geografico: FocoGeografico::Continentes,
            troca_de_aba_pendente: None,
            rolagem: 0.0,
            detalhes_da_missao: false,
            tarefa_clube: AsyncTask::new(),
            tarefa_escolhidos: AsyncTask::new(),
            atualizacao_escolhidos_pendente: false,
            reatualizar_escolhidos: false,
            ficha_de_escolhido: false,
        }
    }

    /// Aviso a mostrar agora no canto da tela, se houver.
    pub fn aviso_visivel(&self, agora: Instant) -> Option<&TipoAviso> {
        let aviso = self.aviso.as_ref()?;
        let visivel = match aviso.tipo {
            TipoAviso::Carregando => self.status == CarreiraStatus::Localizando,
            _ => agora.saturating_duration_since(aviso.desde) < DURACAO_AVISO,
        };
        visivel.then_some(&aviso.tipo)
    }

    fn avisar(&mut self, tipo: TipoAviso) {
        self.aviso = Some(Aviso { tipo, desde: Instant::now() });
    }

    pub fn status(&self) -> &CarreiraStatus {
        &self.status
    }

    /// A localização automática só vale para a leitura de abertura: se o
    /// jogador voltar ao menu com o painel aberto, o painel mostra "sem
    /// carreira" e a releitura periódica recupera quando ele voltar à
    /// mesma carreira (trocar de carreira pede reabrir o painel).
    pub fn ao_abrir_painel(&mut self) {
        self.painel_aberto = true;
        // O elenco é relido na próxima vez que for pedido (pode ter mudado
        // desde a última abertura: transferências, save novo); o clube
        // também (títulos novos mudam o mercado de Olheiros).
        self.tarefa_elenco.reset();
        self.tarefa_clube.reset();
        if matches!(self.tarefa_nacoes.poll(), TaskState::Failed(_)) {
            self.tarefa_nacoes.reset();
        }
        if matches!(self.tarefa_localizar.poll(), TaskState::Running) {
            return;
        }
        self.localizacao_automatica_disponivel = true;
        self.reler();
        self.localizacao_automatica_disponivel = false;
        if let CarreiraStatus::Pronta(carreira) = &self.status {
            let hoje = carreira.data_atual;
            self.data_progresso = Some(hoje);
            self.despachar_missoes(hoje);
            self.atualizar_escolhidos(hoje);
        }
    }

    /// Fechar no meio da confirmação/formulário = cancelar (nada é
    /// debitado nem gravado).
    pub fn ao_fechar_painel(&mut self) {
        self.painel_aberto = false;
        self.cancelar_contratacao();
        self.cancelar_nova_missao();
        self.fechar_relatorio();
        self.vendo_arquivados = false;
        self.troca_de_aba_pendente = None;
    }

    // -----------------------------------------------------------------
    // Elenco do técnico (Stories 3.2/3.3, AD-13)
    // -----------------------------------------------------------------

    /// O elenco da carreira pronta. Única porta das telas para o elenco
    /// (AD-13): a primeira chamada dispara a leitura em background — o
    /// elenco vem do `DATA` do save, e decodificá-lo leva ~0,5 s, o que
    /// travaria o render (emenda do AD-4 na Story 3.2).
    pub fn listar_elenco_atual(&self) -> EstadoElenco {
        let Some(id_save) = self.save_ativo.clone() else {
            return EstadoElenco::Erro;
        };
        match self.tarefa_elenco.poll() {
            TaskState::Done((dono, elenco)) if dono == id_save => EstadoElenco::Pronto(elenco),
            TaskState::Running => EstadoElenco::Carregando,
            TaskState::Failed(_) => EstadoElenco::Erro,
            TaskState::Idle | TaskState::Done(_) => {
                self.tarefa_elenco.reset();
                let fonte = Arc::clone(&self.fonte);
                let (hoje, dono) = (self.data_progresso.unwrap_or(Date(20000101)), id_save);
                self.tarefa_elenco.start(move || {
                    let pool = fonte.read_squad_players()?;
                    Ok((dono, Arc::new(elenco_de(pool, hoje))))
                });
                EstadoElenco::Carregando
            }
        }
    }

    /// "Tentar novamente" do seletor: lê o elenco de novo.
    pub fn reler_elenco(&mut self) {
        self.tarefa_elenco.reset();
    }

    fn jogador_do_elenco(&self, player_id: u32) -> Option<JogadorElenco> {
        match self.listar_elenco_atual() {
            EstadoElenco::Pronto(elenco) => elenco.iter().find(|j| j.player_id == player_id).cloned(),
            _ => None,
        }
    }

    // -----------------------------------------------------------------
    // Ficha de Jogador (Stories 3.1/3.2)
    // -----------------------------------------------------------------

    /// Abre a Ficha de um jogador do Relatório aberto (sem comparação).
    pub fn abrir_ficha(&mut self, player_id: u32) {
        let existe = self.relatorio_aberto().is_some_and(|item| item.relatorio.jogadores.iter().any(|j| j.player_id == player_id));
        if existe {
            self.ficha = Some(player_id);
            self.comparacao = None;
            self.ficha_de_escolhido = false;
        }
    }

    /// Abre a Ficha de um jogador da Lista de Escolhidos (Épico 6).
    pub fn abrir_ficha_de_escolhido(&mut self, player_id: u32) {
        let existe = self.estado_ativo().is_some_and(|e| e.ler(|d| d.escolhidos.iter().any(|x| x.jogador.player_id == player_id)));
        if existe {
            self.ficha = Some(player_id);
            self.comparacao = None;
            self.ficha_de_escolhido = true;
        }
    }

    pub fn fechar_ficha(&mut self) {
        self.ficha = None;
        self.comparacao = None;
        self.ficha_de_escolhido = false;
    }

    /// A Ficha na tela, ou `None` (a tela deve fechar: o jogador saiu do
    /// Relatório ou o Relatório fechou).
    pub fn ficha_aberta(&self) -> Option<FichaAberta> {
        let player_id = self.ficha?;
        let comparacao = self.comparacao.and_then(|id| self.jogador_do_elenco(id));
        let escolhido = self.escolhidos().into_iter().find(|e| e.escolhido.jogador.player_id == player_id);
        if self.ficha_de_escolhido {
            let escolhido = escolhido?;
            let item = self.relatorio_do_escolhido(&escolhido);
            return Some(FichaAberta { item, jogador: escolhido.jogador.clone(), comparacao, escolhido: Some(escolhido), da_lista: true });
        }
        let item = self.relatorio_aberto()?;
        let jogador = item.relatorio.jogadores.iter().find(|j| j.player_id == player_id)?.clone();
        Some(FichaAberta { item, jogador, comparacao, escolhido, da_lista: false })
    }

    /// Um Relatório de uma linha para a Ficha de um Escolhido: a Missão de
    /// origem (se ainda existe) dá o perfil pedido (Fit, referência); a
    /// Qualidade sai da precisão de agora.
    fn relatorio_do_escolhido(&self, e: &EscolhidoNaLista) -> RelatorioNaLista {
        let origem = e.escolhido.relatorio_id.and_then(|id| {
            self.estado_ativo()?.ler(|d| {
                let r = d.relatorios.iter().find(|r| r.id == id)?;
                let missao = d.missoes.iter().find(|m| m.id == r.missao_id).cloned();
                let olheiro = missao.as_ref().and_then(|m| d.olheiros.iter().find(|o| o.id == m.olheiro_id).cloned());
                Some((r.missao_id, missao, olheiro))
            })
        });
        let (missao_id, missao, olheiro) = origem.unwrap_or((Uuid::nil(), None, None));
        let qualidade = match e.precisao {
            0..=2 => Qualidade::Alta,
            3..=6 => Qualidade::Media,
            _ => Qualidade::Baixa,
        };
        RelatorioNaLista {
            relatorio: Relatorio {
                id: e.escolhido.relatorio_id.unwrap_or_else(Uuid::nil),
                missao_id,
                gerado_em: Some(e.escolhido.observado_em),
                qualidade,
                precisao_mais_menos: e.precisao,
                jogadores: vec![e.jogador.clone()],
                aberto: false,
                arquivado: false,
                vistos: 1,
                notificados: 1,
            },
            previstos: 1,
            parcial: false,
            novo: false,
            missao,
            olheiro,
        }
    }

    // -----------------------------------------------------------------
    // Lista de Escolhidos (Épico 6)
    // -----------------------------------------------------------------

    /// Vagas de acompanhamento: a soma das capacidades dos designados.
    fn vagas(dados: &persistence::ScoutStateFile) -> usize {
        dados.olheiros.iter().filter(|o| o.acompanhando()).map(Olheiro::capacidade_acompanhamento).sum()
    }

    /// Os jogadores com vaga agora (prioritários, depois os mais antigos).
    fn acompanhados(dados: &persistence::ScoutStateFile) -> std::collections::HashSet<u32> {
        ordem_de_acompanhamento(&dados.escolhidos).into_iter().take(Self::vagas(dados)).collect()
    }

    /// A Lista de Escolhidos como vale na data do painel, na ordem de vaga.
    pub fn escolhidos(&self) -> Vec<EscolhidoNaLista> {
        let (Some(estado), Some(hoje)) = (self.estado_ativo(), self.data_progresso) else {
            return Vec::new();
        };
        estado.ler(|dados| {
            let cobertos = Self::acompanhados(dados);
            ordem_de_acompanhamento(&dados.escolhidos)
                .into_iter()
                .filter_map(|pid| dados.escolhidos.iter().find(|e| e.jogador.player_id == pid))
                .map(|e| escolhido_em(e, hoje, cobertos.contains(&e.jogador.player_id)))
                .collect()
        })
    }

    pub fn resumo_acompanhamento(&self) -> ResumoAcompanhamento {
        let Some(estado) = self.estado_ativo() else {
            return ResumoAcompanhamento { generalistas: 0, vagas: 0, jogadores: 0, acompanhados: 0 };
        };
        estado.ler(|dados| {
            let vagas = Self::vagas(dados);
            ResumoAcompanhamento {
                generalistas: dados.olheiros.iter().filter(|o| o.acompanhando()).count(),
                vagas,
                jogadores: dados.escolhidos.len(),
                acompanhados: dados.escolhidos.len().min(vagas),
            }
        })
    }

    pub fn esta_nos_escolhidos(&self, player_id: u32) -> bool {
        self.estado_ativo().is_some_and(|e| e.ler(|d| d.escolhidos.iter().any(|x| x.jogador.player_id == player_id)))
    }

    /// "Adicionar aos Escolhidos" na Ficha de um jogador do Relatório: guarda
    /// o que o Olheiro revelou até agora (como a Ficha mostra), com a data e
    /// a precisão do Relatório. `false` = já estava, ou não salvou.
    pub fn adicionar_escolhido_da_ficha(&mut self) -> bool {
        let (Some(ficha), Some(hoje), Some(estado)) = (self.ficha_aberta(), self.data_progresso, self.estado_ativo().cloned()) else {
            return false;
        };
        if ficha.escolhido.is_some() {
            return false;
        }
        let r = &ficha.item.relatorio;
        let missao = ficha.item.missao.as_ref();
        let indice = r.jogadores.iter().position(|j| j.player_id == ficha.jogador.player_id).unwrap_or(0);
        let escolhido = Escolhido {
            observado_em: data_da_observacao(missao, r.gerado_em, indice, hoje),
            precisao: r.precisao_mais_menos,
            jogador: ficha.jogador.clone(),
            adicionado_em: hoje,
            prioridade: false,
            acompanhamento: None,
            alvo: missao.and_then(|m| m.filtros.fit_posicional),
            referencia: missao.and_then(|m| m.filtros.referencia.clone()),
            relatorio_id: Some(r.id),
        };
        match estado.mutar(move |d| d.escolhidos.push(escolhido)) {
            Ok(()) => {
                tracing::info!("[scout::state] {} entrou na Lista de Escolhidos.", ficha.jogador.nome);
                self.atualizar_escolhidos(hoje);
                true
            }
            Err(err) => {
                tracing::warn!("[scout::state] Escolhido não foi salvo: {err:?}");
                false
            }
        }
    }

    /// Tira um jogador da lista (nada mais muda).
    pub fn remover_escolhido(&mut self, player_id: u32) -> bool {
        let Some(estado) = self.estado_ativo().cloned() else {
            return false;
        };
        let resultado = estado.mutar(|d| {
            let antes = d.escolhidos.len();
            d.escolhidos.retain(|e| e.jogador.player_id != player_id);
            antes != d.escolhidos.len()
        });
        match resultado {
            Ok(saiu) => {
                if let (true, Some(hoje)) = (saiu, self.data_progresso) {
                    // a vaga dele pode ir para outro
                    self.atualizar_escolhidos(hoje);
                }
                saiu
            }
            Err(err) => {
                tracing::warn!("[scout::state] Escolhido não foi removido: {err:?}");
                false
            }
        }
    }

    /// Prioridade de vaga no acompanhamento (liga/desliga).
    pub fn alternar_prioridade_escolhido(&mut self, player_id: u32) {
        let Some(estado) = self.estado_ativo().cloned() else {
            return;
        };
        if let Err(err) = estado.mutar(|d| {
            for e in d.escolhidos.iter_mut().filter(|e| e.jogador.player_id == player_id) {
                e.prioridade = !e.prioridade;
            }
        }) {
            tracing::warn!("[scout::state] Prioridade não foi salva: {err:?}");
            return;
        }
        if let Some(hoje) = self.data_progresso {
            self.atualizar_escolhidos(hoje);
        }
    }

    /// Designa (ou libera) um Generalista para o acompanhamento. Só um
    /// Generalista livre (sem Missão) pode ser designado. `false` = não
    /// permitido ou não salvo.
    pub fn designar_acompanhamento(&mut self, olheiro_id: Uuid, designar: bool) -> bool {
        let (Some(estado), Some(hoje)) = (self.estado_ativo().cloned(), self.data_progresso) else {
            return false;
        };
        let Some(c) = self.olheiros_contratados().into_iter().find(|c| c.olheiro.id == olheiro_id) else {
            return false;
        };
        if designar && (c.em_missao || c.acompanhando || !c.olheiro.pode_acompanhar()) {
            return false;
        }
        if !designar && !c.acompanhando {
            return false;
        }
        match estado.mutar(|d| {
            for o in d.olheiros.iter_mut().filter(|o| o.id == olheiro_id) {
                o.acompanhando_desde = designar.then_some(hoje);
            }
        }) {
            Ok(()) => {
                tracing::info!(
                    "[scout::state] {} {} da Lista de Escolhidos.",
                    c.olheiro.nome_exibicao(),
                    if designar { "designado para o acompanhamento" } else { "liberado do acompanhamento" }
                );
                self.atualizar_escolhidos(hoje);
                true
            }
            Err(err) => {
                tracing::warn!("[scout::state] Designação não foi salva: {err:?}");
                false
            }
        }
    }

    /// Atualiza o acompanhamento em `hoje` (abertura do painel, mudança de
    /// designação ou de lista): quem perdeu a vaga deixa de ser acompanhado
    /// (a observação volta a envelhecer); quem tem vaga e não foi observado
    /// hoje entra numa leitura em background do save (`AsyncTask`, AD-4),
    /// que refaz a observação com a precisão do acompanhamento.
    fn atualizar_escolhidos(&mut self, hoje: Date) {
        let (Some(id_save), Some(estado)) = (self.save_ativo.clone(), self.estado_ativo().cloned()) else {
            return;
        };
        // sem arquivo gravável, a observação nova não teria onde ficar e a
        // mesma leitura do save se repetiria sem fim
        if !estado.gravavel() {
            return;
        }
        if self.atualizacao_escolhidos_pendente {
            self.reatualizar_escolhidos = true;
            return;
        }
        let (cobertos, perderam, a_observar) = estado.ler(|dados| {
            let cobertos = Self::acompanhados(dados);
            let perderam = dados
                .escolhidos
                .iter()
                .any(|e| e.acompanhamento.is_some() && !cobertos.contains(&e.jogador.player_id));
            let a_observar: Vec<Escolhido> = dados
                .escolhidos
                .iter()
                .filter(|e| cobertos.contains(&e.jogador.player_id) && (e.acompanhamento.is_none() || e.observado_em < hoje))
                .cloned()
                .collect();
            (cobertos, perderam, a_observar)
        });
        if perderam {
            if let Err(err) = estado.mutar(|d| {
                for e in d.escolhidos.iter_mut().filter(|e| !cobertos.contains(&e.jogador.player_id)) {
                    e.acompanhamento = None;
                }
            }) {
                tracing::warn!("[scout::state] Fim de acompanhamento não foi salvo: {err:?}");
            }
        }
        if a_observar.is_empty() {
            return;
        }
        let fonte = Arc::clone(&self.fonte);
        let iniciou = self.tarefa_escolhidos.start(move || {
            let pool = fonte.read_all_players()?;
            let novos = a_observar.iter().map(|e| avancar_escolhido(e, &pool, hoje)).collect();
            Ok((id_save, novos))
        });
        self.atualizacao_escolhidos_pendente = iniciou;
    }

    /// Trata a atualização dos Escolhidos que terminou (roda a cada frame).
    fn processar_escolhidos(&mut self) {
        if !self.atualizacao_escolhidos_pendente {
            return;
        }
        match self.tarefa_escolhidos.poll() {
            TaskState::Running | TaskState::Idle => {}
            TaskState::Done((dono, novos)) => {
                self.atualizacao_escolhidos_pendente = false;
                self.tarefa_escolhidos.reset();
                self.reatualizar_escolhidos = false;
                let Some(estado) = self.estados.get(&dono).cloned() else {
                    return;
                };
                let n = novos.len();
                let gravou = estado.mutar(|d| {
                    for novo in novos {
                        if let Some(e) = d.escolhidos.iter_mut().find(|e| e.jogador.player_id == novo.jogador.player_id) {
                            // prioridade pode ter mudado enquanto a leitura rodava
                            *e = Escolhido { prioridade: e.prioridade, ..novo };
                        }
                    }
                });
                match gravou {
                    Ok(()) => tracing::info!("[scout::state] Acompanhamento: {n} Escolhido(s) observado(s) de novo."),
                    Err(err) => {
                        // não tenta de novo agora: a mesma leitura viria em
                        // seguida, para sempre; a próxima abertura do painel tenta
                        tracing::warn!("[scout::state] Acompanhamento não foi salvo: {err:?}");
                        return;
                    }
                }
                // a lista ou as vagas podem ter mudado enquanto a leitura rodava
                // (os observados agora não entram de novo: estão com a data de hoje)
                if let Some(hoje) = self.data_progresso {
                    self.atualizar_escolhidos(hoje);
                }
            }
            TaskState::Failed(err) => {
                self.atualizacao_escolhidos_pendente = false;
                self.tarefa_escolhidos.reset();
                tracing::warn!("[scout::state] Acompanhamento dos Escolhidos falhou: {err:?}");
                // só repete se alguém pediu outra atualização enquanto esta rodava
                if let (true, Some(hoje)) = (std::mem::take(&mut self.reatualizar_escolhidos), self.data_progresso) {
                    self.atualizar_escolhidos(hoje);
                }
            }
        }
    }

    /// Jogador do elenco escolhido para sobrepor no Radar (`None` tira).
    pub fn comparar_com(&mut self, player_id: Option<u32>) {
        self.comparacao = player_id;
    }

    /// Botão "Tentar novamente": localiza a carreira de novo.
    pub fn tentar_novamente(&mut self) {
        self.iniciar_localizacao();
    }

    /// Chamado a cada frame, com o painel aberto ou fechado. Barato: dois
    /// `poll` de `AsyncTask` e, no máximo, uma leitura de alguns bytes por
    /// segundo; o sinal e a localização rodam em background (AD-4).
    pub fn tick(&mut self) {
        if self.aviso_inicial_pendente {
            self.aviso_inicial_pendente = false;
            self.avisar(TipoAviso::Injetado);
        }
        self.processar_buscas();
        self.processar_escolhidos();
        self.minifaces.tick();

        match self.tarefa_localizar.poll() {
            TaskState::Running => {
                self.definir_status(CarreiraStatus::Localizando);
                return;
            }
            TaskState::Done(()) if self.status == CarreiraStatus::Localizando => {
                self.reler();
                if !matches!(self.status, CarreiraStatus::Pronta(_) | CarreiraStatus::Localizando) {
                    self.avisar(TipoAviso::Falhou);
                }
                return;
            }
            TaskState::Failed(err) if self.status == CarreiraStatus::Localizando => {
                self.definir_status(status_de_erro(&err));
                self.ultima_leitura = Some(Instant::now());
                self.avisar(TipoAviso::Falhou);
                return;
            }
            _ => {}
        }

        let releitura_devida = self
            .ultima_leitura
            .map_or(true, |t| t.elapsed() >= INTERVALO_RELEITURA);
        let acompanhando = matches!(self.status, CarreiraStatus::Pronta(_) | CarreiraStatus::SemCarreira);
        if acompanhando && releitura_devida {
            self.reler();
            self.avisar_jogadores_novos();
        }

        if !matches!(self.status, CarreiraStatus::Pronta(_) | CarreiraStatus::Localizando) {
            self.vigiar_carreira();
        }
    }

    /// Sem carreira pronta: trata o último sinal e agenda o próximo.
    fn vigiar_carreira(&mut self) {
        match self.tarefa_sinal.poll() {
            TaskState::Running => return,
            TaskState::Done(acesa) if self.sinal_pendente => {
                self.sinal_pendente = false;
                if !acesa {
                    self.sinal_consumido = false;
                } else if !self.sinal_consumido {
                    self.sinal_consumido = true;
                    tracing::info!("[scout::state] Carreira detectada na memória; localizando sem esperar o F10.");
                    self.iniciar_localizacao();
                    return;
                }
            }
            TaskState::Failed(err) if self.sinal_pendente => {
                self.sinal_pendente = false;
                tracing::warn!("[scout::state] Sinal de carreira falhou: {err:?}");
            }
            _ => {}
        }

        let agora = Instant::now();
        if self.proximo_sinal.is_none_or(|quando| agora >= quando) {
            self.proximo_sinal = Some(agora + INTERVALO_SINAL);
            self.sinal_pendente = self.fonte.start_career_probe(&self.tarefa_sinal);
        }
    }

    fn reler(&mut self) {
        self.ultima_leitura = Some(Instant::now());
        match self.fonte.read_snapshot() {
            Ok(snapshot) => self.definir_status(CarreiraStatus::Pronta(snapshot)),
            Err(SaveRepoError::NaoLocalizado | SaveRepoError::CarreiraNaoCarregada)
                if self.localizacao_automatica_disponivel =>
            {
                self.iniciar_localizacao();
            }
            Err(err) => self.definir_status(status_de_erro(&err)),
        }
    }

    fn iniciar_localizacao(&mut self) {
        self.localizacao_automatica_disponivel = false;
        if self.fonte.start_locating(&self.tarefa_localizar) {
            tracing::info!("[scout::state] Localizando a carreira em background.");
            self.definir_status(CarreiraStatus::Localizando);
            self.avisar(TipoAviso::Carregando);
        }
    }

    /// Único ponto que muda `status`: mantém `save_ativo` coerente com ele.
    fn definir_status(&mut self, status: CarreiraStatus) {
        let ativada = match &status {
            CarreiraStatus::Pronta(snapshot) if self.save_ativo.as_deref() != Some(snapshot.id_save.as_str()) => {
                Some(snapshot.clone())
            }
            _ => None,
        };
        if !matches!(status, CarreiraStatus::Pronta(_)) {
            self.save_ativo = None;
        }
        self.status = status;
        if let Some(snapshot) = ativada {
            // Localizou: o sinal rearma (uma troca de carreira sem passar
            // pelo menu precisa poder disparar outra localização).
            self.sinal_consumido = false;
            self.ativar_save(snapshot.id_save.clone());
            self.data_progresso = Some(snapshot.data_atual);
            self.avisar(TipoAviso::Pronta(snapshot.clone()));
            self.voltar_no_tempo(snapshot.data_do_save);
            // Carreira ficou pronta COM o painel aberto: vale como a
            // abertura (AD-8) — senão a Missão vencida esperaria fechar e
            // abrir de novo.
            if self.painel_aberto {
                self.despachar_missoes(snapshot.data_atual);
                self.atualizar_escolhidos(snapshot.data_atual);
            }
        }
    }

    /// A carreira `id_save` ficou pronta: carrega o arquivo dela (só na
    /// primeira vez da sessão) e pede para restaurar a aba salva.
    fn ativar_save(&mut self, id_save: String) {
        let diretorio = self.diretorio_estado.as_deref();
        let novo = !self.estados.contains_key(&id_save);
        let estado = self
            .estados
            .entry(id_save.clone())
            .or_insert_with(|| EstadoPersistido::carregar(diretorio, &id_save));
        if novo {
            destravar_missoes(estado);
        }
        self.aba_restaurada = Some(estado.ler(|dados| dados.ui_prefs.aba_ativa));
        tracing::info!("[scout::state] Carreira ativa: estado {}…", id_save.get(..8).unwrap_or(&id_save));
        self.ultimo_save = Some(id_save.clone());
        self.save_ativo = Some(id_save);
    }

    /// Estado persistido da carreira pronta agora.
    fn estado_ativo(&self) -> Option<&EstadoPersistido> {
        self.estados.get(self.save_ativo.as_deref()?)
    }

    /// `transferbudget` vivo da carreira pronta (o mesmo do cabeçalho).
    pub fn orcamento(&self) -> Option<i32> {
        match &self.status {
            CarreiraStatus::Pronta(carreira) => Some(carreira.orcamento_transferencias),
            _ => None,
        }
    }

    /// Prestígio, liga e títulos do clube (a primeira chamada dispara a
    /// leitura em background). Falha de leitura não trava o mercado: vale
    /// `None` (atratividade média), com aviso no log.
    fn clube(&self) -> Carga<Option<quality::PerfilClube>> {
        let Some(id_save) = self.save_ativo.clone() else {
            return Carga::Erro;
        };
        match self.tarefa_clube.poll() {
            TaskState::Done((dono, clube)) if dono == id_save => Carga::Pronto(clube),
            TaskState::Running => Carga::Carregando,
            TaskState::Idle | TaskState::Done(_) | TaskState::Failed(_) => {
                self.tarefa_clube.reset();
                let fonte = Arc::clone(&self.fonte);
                self.tarefa_clube.start(move || {
                    let clube = match fonte.read_club_profile() {
                        Ok(d) => Some(quality::PerfilClube {
                            prestigio_nacional: d.prestigio_nacional,
                            prestigio_internacional: d.prestigio_internacional,
                            nivel_liga: d.liga.as_ref().map_or(1, |l| l.nivel),
                            titulos: d.titulos,
                            pais: d.liga.as_ref().and_then(|l| l.pais),
                            continente: d.liga.as_ref().map_or(Confederacao::Outras, |l| l.continente),
                        }),
                        Err(err) => {
                            tracing::warn!("[scout::state] Clube do técnico não lido ({err:?}); mercado de Olheiros com atratividade média.");
                            None
                        }
                    };
                    Ok((id_save, clube))
                });
                Carga::Carregando
            }
        }
    }

    /// O mercado de Olheiros do mês (Épico 5, item 3), contra o orçamento
    /// atual. Espera o clube, as ligas e as nações (lidos em background).
    pub fn mercado_de_olheiros(&self) -> Carga<MercadoOlheiros> {
        let (Some(id_save), Some(hoje)) = (self.save_ativo.clone(), self.data_progresso) else {
            return Carga::Erro;
        };
        let clube = match self.clube() {
            Carga::Pronto(c) => c,
            Carga::Carregando => return Carga::Carregando,
            Carga::Erro => None,
        };
        let ligas = match self.listar_ligas() {
            Carga::Pronto(l) => l,
            Carga::Carregando => return Carga::Carregando,
            Carga::Erro => Arc::new(Vec::new()),
        };
        let nacoes = match self.carga_nacoes() {
            Carga::Pronto(n) => n,
            Carga::Carregando => return Carga::Carregando,
            Carga::Erro => Arc::new(Vec::new()),
        };
        // países com clubes no save (sem ligas lidas: todas as nações)
        let mut paises: Vec<quality::PaisCandidato> = ligas
            .iter()
            .filter_map(|l| Some(quality::PaisCandidato { id: l.pais?, continente: l.continente }))
            .collect();
        if paises.is_empty() {
            paises = nacoes.iter().map(|n| quality::PaisCandidato { id: n.id, continente: n.confederacao }).collect();
        }
        paises.sort_by_key(|p| p.id);
        paises.dedup();
        let referencia = clube.unwrap_or(quality::PerfilClube {
            prestigio_nacional: 10,
            prestigio_internacional: 7,
            nivel_liga: 1,
            titulos: 0,
            pais: None,
            continente: Confederacao::Outras,
        });
        let periodo = quality::periodo_do_mercado(hoje.year(), hoje.month());
        // A atratividade fica fixa no mês (um título novo ou uma leitura que
        // falhou não sorteiam outro mercado no meio do mês, e o id de uma
        // oferta contratada continua sendo o mesmo Olheiro).
        let congelada = self.estado_ativo().and_then(|e| e.ler(|d| d.mercado_do_mes)).filter(|(p, _)| *p == periodo).map(|(_, a)| a);
        let atratividade = match (congelada, clube) {
            (Some(a), _) => a,
            (None, Some(_)) => {
                let a = quality::atratividade(&referencia);
                if let Some(estado) = self.estado_ativo() {
                    if let Err(err) = estado.mutar(|d| d.mercado_do_mes = Some((periodo, a))) {
                        tracing::warn!("[scout::state] Mercado do mês não foi fixado: {err:?}");
                    }
                }
                a
            }
            (None, None) => quality::atratividade(&referencia),
        };
        let carreira = semente_da_carreira(&id_save);
        let candidatos = quality::gerar_ofertas(atratividade, periodo, carreira, &referencia, &paises);
        let contratadas = self.estado_ativo().map(|e| e.ler(|d| d.ofertas_contratadas.clone())).unwrap_or_default();
        let ofertas = montar_ofertas(&candidatos, periodo, carreira, &nacoes, &contratadas, self.orcamento());
        let (ano, mes) = if hoje.month() == 12 { (hoje.year() + 1, 1) } else { (hoje.year(), hoje.month() + 1) };
        Carga::Pronto(MercadoOlheiros { ofertas, atratividade, clube, renova_em: Date(ano * 10_000 + mes * 100 + 1) })
    }

    /// Olheiros já contratados nesta carreira, na ordem de contratação.
    pub fn olheiros_contratados(&self) -> Vec<OlheiroContratado> {
        let Some(estado) = self.estado_ativo() else {
            return Vec::new();
        };
        estado.ler(|dados| {
            dados
                .olheiros
                .iter()
                .map(|olheiro| {
                    let missao = dados
                        .missoes
                        .iter()
                        .rev()
                        .find(|m| m.olheiro_id == olheiro.id && m.status != StatusMissao::Concluida)
                        .cloned();
                    let relatorio_atual =
                        missao.as_ref().and_then(|m| dados.relatorios.iter().find(|r| r.missao_id == m.id)).map(|r| r.id);
                    let relatorios = dados
                        .relatorios
                        .iter()
                        .filter(|r| dados.missoes.iter().any(|m| m.id == r.missao_id && m.olheiro_id == olheiro.id))
                        .count();
                    OlheiroContratado {
                        olheiro: olheiro.clone(),
                        em_missao: missao.is_some(),
                        acompanhando: olheiro.acompanhando(),
                        missao,
                        relatorio_atual,
                        relatorios,
                    }
                })
                .collect()
        })
    }

    /// Clique num Olheiro contratado: livre → Nova Missão com ele; em
    /// Missão → o Relatório dela (ou a aba Missões, se ainda não há).
    pub fn destino_do_olheiro(&self, id: Uuid) -> Option<DestinoOlheiro> {
        let contratado = self.olheiros_contratados().into_iter().find(|c| c.olheiro.id == id)?;
        if contratado.acompanhando {
            return Some(DestinoOlheiro::Escolhidos);
        }
        Some(match (contratado.em_missao, contratado.relatorio_atual) {
            (false, _) => DestinoOlheiro::NovaMissao(id),
            (true, Some(relatorio)) => DestinoOlheiro::Relatorio(relatorio),
            (true, None) => DestinoOlheiro::Missoes,
        })
    }

    /// Visão da aba Olheiros salva para a carreira (Cards por padrão).
    pub fn densidade_olheiros(&self) -> Densidade {
        self.estado_ativo().map_or(Densidade::Cards, |e| e.ler(|d| d.ui_prefs.densidade_olheiros))
    }

    pub fn definir_densidade_olheiros(&mut self, densidade: Densidade) {
        let Some(estado) = self.estado_ativo() else {
            return;
        };
        if estado.ler(|d| d.ui_prefs.densidade_olheiros) == densidade {
            return;
        }
        if let Err(err) = estado.mutar(|d| d.ui_prefs.densidade_olheiros = densidade) {
            tracing::warn!("[scout::state] Visão dos Olheiros não foi salva: {err:?}");
        }
    }

    /// "Contratar" clicado: abre a confirmação para essa oferta, com o nome
    /// gerado (que o jogador pode trocar).
    pub fn preparar_contratacao(&mut self, oferta: OfertaOlheiro) {
        let nome = oferta.olheiro.nome.clone();
        self.contratacao = Some(Contratacao { oferta, nome, erro: None });
    }

    /// O nome digitado no modal (aplicado na confirmação).
    pub fn definir_nome_da_contratacao(&mut self, nome: &str) {
        if let Some(c) = self.contratacao.as_mut() {
            c.nome = nome.chars().take(40).collect();
        }
    }

    pub fn cancelar_contratacao(&mut self) {
        self.contratacao = None;
    }

    /// Dados do modal, ou `None` se não há contratação aberta (ou a
    /// carreira deixou de estar pronta — o modal deve fechar).
    pub fn previa_contratacao(&self) -> Option<PreviaContratacao> {
        let contratacao = self.contratacao.clone()?;
        let orcamento_atual = self.orcamento()?;
        let custo = contratacao.custo();
        let faltam = (orcamento_atual < custo).then(|| custo.saturating_sub(orcamento_atual));
        Some(PreviaContratacao {
            orcamento_apos: orcamento_atual.saturating_sub(custo),
            orcamento_atual,
            faltam,
            contratacao,
        })
    }

    /// Botão confirmar do modal. `true` = contratado (o modal fecha); em
    /// falha o erro fica em `Contratacao::erro` e o modal continua aberto.
    pub fn confirmar_contratacao(&mut self) -> bool {
        let Some(contratacao) = self.contratacao.clone() else {
            return false;
        };
        let oferta = contratacao.oferta.clone();
        let olheiro = Olheiro {
            id: Uuid::new_v4(),
            contratado_em: match &self.status {
                CarreiraStatus::Pronta(c) => Some(c.data_atual),
                _ => None,
            },
            nome: crate::scout::nomes::limpar(&contratacao.nome, &oferta.olheiro.nome),
            oferta_id: Some(oferta.id),
            acompanhando_desde: None,
            ..oferta.olheiro.clone()
        };
        let novo = olheiro.clone();
        let custo = contratacao.custo();
        match self.comprar(custo, move |dados| {
            dados.olheiros.push(novo);
            if !dados.ofertas_contratadas.contains(&oferta.id) {
                dados.ofertas_contratadas.push(oferta.id);
            }
        }) {
            Ok(()) => {
                tracing::info!(
                    "[scout::state] Olheiro contratado: {} ({}, foco {} {}★) por {}, id {}.",
                    olheiro.nome_exibicao(),
                    olheiro.tier.nome(),
                    olheiro.especializacao.nome(),
                    olheiro.perfil().principal().texto(),
                    custo,
                    olheiro.id
                );
                self.contratacao = None;
                // O cabeçalho mostra o saldo relido do jogo, não uma conta.
                self.reler();
                true
            }
            Err(erro) => {
                tracing::warn!("[scout::state] Contratação não concluída: {erro:?}");
                if matches!(erro, ErroCompra::OrcamentoMudou { .. }) {
                    self.reler();
                }
                if let Some(aberta) = self.contratacao.as_mut() {
                    aberta.erro = Some(erro);
                }
                false
            }
        }
    }

    /// Debita `custo` (compare-and-write + releitura) e, só então, grava
    /// a compra write-through (`gravar`). Se gravar falhar, desfaz o
    /// débito: nenhum caminho deixa algo salvo sem débito confirmado nem
    /// diz que comprou sem ter comprado (ACs das Stories 1.5 e 2.2).
    fn comprar(
        &mut self,
        custo: i32,
        gravar: impl FnOnce(&mut persistence::ScoutStateFile),
    ) -> Result<(), ErroCompra> {
        let anterior = self.orcamento().ok_or(ErroCompra::SemCarreira)?;
        if anterior < custo {
            return Err(ErroCompra::OrcamentoInsuficiente { faltam: custo.saturating_sub(anterior) });
        }
        let estado = self.estado_ativo().cloned().ok_or(ErroCompra::SemCarreira)?;
        if !estado.gravavel() {
            return Err(ErroCompra::EstadoNaoSalvavel);
        }

        let debitado = self.fonte.write_transfer_budget(anterior, anterior - custo).map_err(|err| match err {
            SaveRepoError::OrcamentoMudou(atual) => ErroCompra::OrcamentoMudou { atual },
            _ => ErroCompra::EscritaFalhou,
        })?;

        if let Err(err) = estado.mutar(gravar) {
            tracing::warn!("[scout::state] Compra não foi salva ({err:?}); desfazendo o débito.");
            return match self.fonte.write_transfer_budget(debitado, anterior) {
                Ok(_) => Err(ErroCompra::NaoSalvo),
                Err(err) => {
                    tracing::warn!("[scout::state] Não deu para desfazer o débito: {err:?}");
                    Err(ErroCompra::DebitadoSemSalvar { debitado: custo })
                }
            };
        }
        Ok(())
    }

    // -----------------------------------------------------------------
    // Nova Missão (Story 2.2)
    // -----------------------------------------------------------------

    /// Nova Missão com o Olheiro já escolhido (passo 1 da tela, ou clique
    /// nele na aba Olheiros): o formulário abre com os filtros ideais da
    /// Especialização dele (`quality::filtros_ideais`). Olheiro em Missão
    /// ou inexistente: nada abre.
    pub fn abrir_nova_missao(&mut self, olheiro_id: Uuid) {
        let Some(contratado) = self.olheiros_contratados().into_iter().find(|c| c.olheiro.id == olheiro_id && !c.ocupado())
        else {
            return;
        };
        self.detalhes_da_missao = false;
        self.rascunho_missao = Some(RascunhoMissao {
            olheiro_id: Some(olheiro_id),
            filtros: quality::filtros_ideais(contratado.olheiro.perfil().foco()),
            modo: ModoBusca::Rapida,
            continua: false,
            investimento: quality::Investimento::Padrao,
            erro: None,
        });
    }

    /// Volta os filtros do formulário aos ideais do Olheiro escolhido.
    pub fn restaurar_filtros_ideais(&mut self) {
        let especializacao = self.previa_missao().and_then(|p| {
            let id = p.rascunho.olheiro_id?;
            p.olheiros.into_iter().find(|c| c.olheiro.id == id).map(|c| c.olheiro.perfil().foco())
        });
        if let (Some(e), Some(r)) = (especializacao, self.rascunho_missao.as_mut()) {
            r.filtros = quality::filtros_ideais(e);
            r.erro = None;
        }
    }

    pub fn cancelar_nova_missao(&mut self) {
        self.rascunho_missao = None;
    }

    pub fn tem_nova_missao(&self) -> bool {
        self.rascunho_missao.is_some()
    }

    /// Botões − / + das faixas (sempre dentro dos limites do campo).
    pub fn ajustar_faixa_da_missao(&mut self, campo: CampoFaixa, delta: i32) {
        let Some(r) = self.rascunho_missao.as_mut() else {
            return;
        };
        let valor = match campo {
            CampoFaixa::OverallMin => &mut r.filtros.overall.min,
            CampoFaixa::OverallMax => &mut r.filtros.overall.max,
            CampoFaixa::PotencialMin => &mut r.filtros.potencial.min,
            CampoFaixa::PotencialMax => &mut r.filtros.potencial.max,
            CampoFaixa::IdadeMin => &mut r.filtros.idade.min,
            CampoFaixa::IdadeMax => &mut r.filtros.idade.max,
            CampoFaixa::ContratoMin => &mut r.filtros.contrato.min,
            CampoFaixa::ContratoMax => &mut r.filtros.contrato.max,
            CampoFaixa::DribleMin => &mut r.filtros.estrelas_drible.min,
            CampoFaixa::DribleMax => &mut r.filtros.estrelas_drible.max,
        };
        let (menor, maior) = campo.limites();
        let novo = (i32::from(*valor) + delta).clamp(i32::from(menor), i32::from(maior));
        *valor = u8::try_from(novo).unwrap_or(*valor);
        r.erro = None;
    }

    /// Nações do mapa, ou `None` enquanto carregam (a primeira chamada
    /// dispara a leitura em background; uma falha é tentada de novo). O
    /// filtro geográfico passou a usar as ligas (`listar_ligas`); o
    /// cartograma de nações fica para o Sonar (Épico 4).
    pub fn nacoes(&self) -> Option<Arc<Vec<Nacao>>> {
        match self.carga_nacoes() {
            Carga::Pronto(nacoes) => Some(nacoes),
            _ => None,
        }
    }

    /// As nações como `Carga`: uma falha fica como erro (a próxima abertura
    /// do painel tenta de novo), sem disparar uma leitura por frame.
    fn carga_nacoes(&self) -> Carga<Arc<Vec<Nacao>>> {
        match self.tarefa_nacoes.poll() {
            TaskState::Done(nacoes) => Carga::Pronto(nacoes),
            TaskState::Running => Carga::Carregando,
            TaskState::Failed(_) => Carga::Erro,
            TaskState::Idle => {
                let fonte = Arc::clone(&self.fonte);
                self.tarefa_nacoes.start(move || fonte.read_nations().map(Arc::new));
                Carga::Carregando
            }
        }
    }

    /// Nome de uma nação (mercados dos Olheiros), se as nações já foram lidas.
    pub fn nome_da_nacao(&self, id: u16) -> Option<String> {
        self.nacoes()?.iter().find(|n| n.id == id).map(|n| n.nome.clone())
    }

    /// Ligas com clubes da carreira pronta (filtro geográfico). A primeira
    /// chamada dispara a leitura em background (lê o `DATA`).
    pub fn listar_ligas(&self) -> Carga<Arc<Vec<Liga>>> {
        let Some(id_save) = self.save_ativo.clone() else {
            return Carga::Erro;
        };
        match self.tarefa_ligas.poll() {
            TaskState::Done((dono, ligas)) if dono == id_save => Carga::Pronto(ligas),
            TaskState::Running => Carga::Carregando,
            TaskState::Failed(_) => Carga::Erro,
            TaskState::Idle | TaskState::Done(_) => {
                self.tarefa_ligas.reset();
                let fonte = Arc::clone(&self.fonte);
                self.tarefa_ligas.start(move || Ok((id_save, Arc::new(fonte.read_leagues()?))));
                Carga::Carregando
            }
        }
    }

    /// "Tentar novamente" do filtro geográfico.
    pub fn reler_ligas(&mut self) {
        self.tarefa_ligas.reset();
    }

    fn ligas_carregadas(&self) -> Arc<Vec<Liga>> {
        match self.listar_ligas() {
            Carga::Pronto(ligas) => ligas,
            _ => Arc::new(Vec::new()),
        }
    }

    /// Amplitude da geografia escolhida (a liga sem dados carregados conta
    /// como "um lugar à parte").
    fn amplitude_da_geografia(&self, filtros: &FiltrosMissao) -> quality::AmplitudeGeografica {
        let ligas = self.ligas_carregadas();
        let continente_do_pais = |id: u16| {
            ligas.iter().find(|l| l.pais == Some(id)).map_or(Confederacao::Outras, |l| l.continente)
        };
        let escopos: Vec<quality::EscopoGeografico> = filtros
            .continentes
            .iter()
            .map(|&c| quality::EscopoGeografico::Continente(c))
            .chain(filtros.paises_dos_clubes.iter().map(|&p| quality::EscopoGeografico::Pais(continente_do_pais(p), p)))
            .chain(filtros.ligas.iter().map(|&id| match ligas.iter().find(|l| l.id == id) {
                Some(l) => quality::EscopoGeografico::Liga { continente: l.continente, pais: l.pais, liga: id },
                None => quality::EscopoGeografico::Liga { continente: Confederacao::Outras, pais: None, liga: id },
            }))
            .collect();
        quality::amplitude_da_geografia(&escopos)
    }

    /// Continente inteiro entra/sai. Entrar tira os países e ligas dele já
    /// escolhidos (o continente já os inclui).
    pub fn alternar_continente_da_missao(&mut self, continente: Confederacao) {
        let ligas = self.ligas_carregadas();
        if let Some(r) = self.rascunho_missao.as_mut() {
            let f = &mut r.filtros;
            if let Some(i) = f.continentes.iter().position(|c| *c == continente) {
                f.continentes.remove(i);
            } else {
                f.continentes.push(continente);
                let do_continente = |l: &&Liga| l.continente == continente;
                f.paises_dos_clubes.retain(|p| !ligas.iter().filter(do_continente).any(|l| l.pais == Some(*p)));
                f.ligas.retain(|id| !ligas.iter().filter(do_continente).any(|l| l.id == *id));
            }
            r.erro = None;
        }
    }

    /// País (todas as ligas dele) entra/sai. Já incluído pelo continente:
    /// nada muda. Entrar tira as ligas dele já escolhidas.
    pub fn alternar_pais_do_clube_da_missao(&mut self, pais: u16) {
        let ligas = self.ligas_carregadas();
        let continente = ligas.iter().find(|l| l.pais == Some(pais)).map(|l| l.continente);
        if let Some(r) = self.rascunho_missao.as_mut() {
            let f = &mut r.filtros;
            if continente.is_some_and(|c| f.continentes.contains(&c)) {
                return;
            }
            if let Some(i) = f.paises_dos_clubes.iter().position(|p| *p == pais) {
                f.paises_dos_clubes.remove(i);
            } else {
                f.paises_dos_clubes.push(pais);
                f.ligas.retain(|id| !ligas.iter().any(|l| l.id == *id && l.pais == Some(pais)));
            }
            r.erro = None;
        }
    }

    /// Liga entra/sai. Já incluída pelo país ou continente: nada muda.
    pub fn alternar_liga_da_missao(&mut self, liga: u32) {
        let ligas = self.ligas_carregadas();
        let Some(dados) = ligas.iter().find(|l| l.id == liga) else {
            return;
        };
        if let Some(r) = self.rascunho_missao.as_mut() {
            let f = &mut r.filtros;
            let incluida = f.continentes.contains(&dados.continente) || dados.pais.is_some_and(|p| f.paises_dos_clubes.contains(&p));
            if incluida {
                return;
            }
            match f.ligas.iter().position(|id| *id == liga) {
                Some(i) => {
                    f.ligas.remove(i);
                }
                None => f.ligas.push(liga),
            }
            r.erro = None;
        }
    }

    /// Ritmo de trabalho entra/sai do filtro (`ataque` = de ataque).
    pub fn alternar_ritmo_da_missao(&mut self, ataque: bool, ritmo: RitmoTrabalho) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            let lista = if ataque { &mut r.filtros.ritmo_ataque } else { &mut r.filtros.ritmo_defesa };
            match lista.iter().position(|x| *x == ritmo) {
                Some(i) => {
                    lista.remove(i);
                }
                None => {
                    lista.push(ritmo);
                    lista.sort();
                }
            }
            r.erro = None;
        }
    }

    /// Nível em relação ao elenco (`None` = qualquer).
    pub fn definir_nivel_da_missao(&mut self, nivel: Option<NivelEquipe>) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.nivel_elenco = nivel;
            r.erro = None;
        }
    }

    /// Limite de orçamento (`salario`: o de salário; senão o de valor).
    pub fn definir_limite_da_missao(&mut self, salario: bool, limite: Limite) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            if salario {
                r.filtros.limite_salario = limite;
            } else {
                r.filtros.limite_valor = limite;
            }
            r.erro = None;
        }
    }

    /// − / + do limite: um degrau da escala 1-2-5 a partir do teto atual
    /// (ou do que o clube tem, se estava sem limite).
    pub fn ajustar_limite_da_missao(&mut self, salario: bool, direcao: i32) {
        let Some(previa) = self.previa_missao() else {
            return;
        };
        let (limite, do_clube) = if salario {
            (previa.rascunho.filtros.limite_salario, previa.folha_disponivel)
        } else {
            (previa.rascunho.filtros.limite_valor, previa.orcamento_apos_missao)
        };
        let atual = limite.teto(do_clube).or(do_clube).unwrap_or(1_000_000);
        self.definir_limite_da_missao(salario, Limite::Ate(quality::degrau_dinheiro(atual, direcao)));
    }

    /// Posição entra/sai do filtro (vazio = todas).
    pub fn alternar_posicao_da_missao(&mut self, perfil: Perfil) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            let lista = &mut r.filtros.posicoes;
            match lista.iter().position(|p| *p == perfil) {
                Some(i) => {
                    lista.remove(i);
                }
                None => {
                    lista.push(perfil);
                    lista.sort();
                }
            }
            r.erro = None;
        }
    }

    /// Folha salarial semanal disponível do clube (`dqXv.wagebudget` vivo).
    pub fn folha_salarial(&self) -> Option<i32> {
        match &self.status {
            CarreiraStatus::Pronta(c) => c.folha_salarial,
            _ => None,
        }
    }

    /// Atalho de filtro (mantém a geografia e o teto).
    pub fn aplicar_atalho_da_missao(&mut self, atalho: Atalho) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros = atalho.aplicar(&r.filtros);
            r.erro = None;
        }
    }

    /// Titulares do elenco por perfil de posição (resumo do formulário);
    /// `None` enquanto o elenco carrega.
    pub fn nivel_do_elenco(&self) -> Option<quality::NivelElenco> {
        match self.listar_elenco_atual() {
            Carga::Pronto(elenco) => {
                let pares: Vec<(u8, u8)> = elenco.iter().map(|j| (j.posicao, j.overall)).collect();
                Some(quality::NivelElenco::de(&pares))
            }
            _ => None,
        }
    }

    /// Pé preferido do filtro (`None` = qualquer).
    pub fn definir_pe_da_missao(&mut self, pe: Option<FiltroPe>) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.pe = pe;
            r.erro = None;
        }
    }

    /// Aba esperando confirmação para descartar a Nova Missão.
    pub fn troca_de_aba_pendente(&self) -> Option<Aba> {
        self.troca_de_aba_pendente
    }

    pub fn definir_troca_de_aba_pendente(&mut self, aba: Option<Aba>) {
        self.troca_de_aba_pendente = aba;
    }

    /// "Mostrar detalhes" do formulário Nova Missão.
    pub fn detalhes_da_missao_abertos(&self) -> bool {
        self.detalhes_da_missao
    }

    pub fn definir_detalhes_da_missao_abertos(&mut self, abertos: bool) {
        self.detalhes_da_missao = abertos;
    }

    /// Rolagem do analógico direito neste frame (ver `Scout::frame`).
    pub fn rolagem(&self) -> f32 {
        self.rolagem
    }

    pub fn definir_rolagem(&mut self, pixels: f32) {
        self.rolagem = pixels;
    }

    /// Onde a árvore do filtro geográfico está aberta.
    pub fn foco_geografico(&self) -> FocoGeografico {
        self.foco_geografico
    }

    pub fn focar_geografia(&mut self, foco: FocoGeografico) {
        self.foco_geografico = foco;
    }

    /// B na árvore: sobe um nível. `false` = já estava no topo (a tela
    /// fecha).
    pub fn subir_foco_geografico(&mut self) -> bool {
        self.foco_geografico = match self.foco_geografico {
            FocoGeografico::Continentes => return false,
            FocoGeografico::Continente(_) => FocoGeografico::Continentes,
            FocoGeografico::Pais(c, _) => FocoGeografico::Continente(c),
        };
        true
    }

    /// "Limpar": volta ao mundo todo.
    pub fn limpar_geografia_da_missao(&mut self) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.continentes.clear();
            r.filtros.paises_dos_clubes.clear();
            r.filtros.ligas.clear();
            r.erro = None;
        }
    }

    /// Atributo dominante entra/sai (até `quality::MAX_DOMINANTES`).
    pub fn alternar_atributo_da_missao(&mut self, atributo: Atributo) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            let lista = &mut r.filtros.atributos_dominantes;
            match lista.iter().position(|a| *a == atributo) {
                Some(i) => {
                    lista.remove(i);
                }
                None if lista.len() < quality::MAX_DOMINANTES => lista.push(atributo),
                None => return,
            }
            r.erro = None;
        }
    }

    /// "Qualquer um": sem atributos dominantes.
    pub fn limpar_atributos_da_missao(&mut self) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.atributos_dominantes.clear();
            r.erro = None;
        }
    }

    /// Fit Posicional escolhido no painel de campo (Story 3.4); `None` =
    /// sem esse filtro.
    pub fn definir_fit_da_missao(&mut self, alvo: Option<PosicaoAlvo>) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.fit_posicional = alvo;
            r.erro = None;
        }
    }

    /// Jogador de Referência escolhido no seletor de elenco (Story 3.3);
    /// `None` = sem esse filtro. Guarda a foto dos atributos dele agora.
    pub fn definir_referencia_da_missao(&mut self, player_id: Option<u32>) {
        let referencia = player_id.and_then(|id| self.jogador_do_elenco(id)).map(|j| j.como_referencia());
        if player_id.is_some() && referencia.is_none() {
            return;
        }
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.referencia = referencia;
            r.erro = None;
        }
    }

    /// Prazo fixo ou contínua (Story 2.10).
    pub fn definir_continua_da_missao(&mut self, continua: bool) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.continua = continua;
            r.erro = None;
        }
    }

    pub fn definir_modo_da_missao(&mut self, modo: ModoBusca) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.modo = modo;
            r.erro = None;
        }
    }

    /// Verba da viagem (Épico 5, item 6).
    pub fn definir_investimento_da_missao(&mut self, investimento: quality::Investimento) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.investimento = investimento;
            r.erro = None;
        }
    }

    /// Os lugares do filtro geográfico, para os mercados dos Olheiros: uma
    /// liga vale pelo país dela (ou pelo continente, se não tem país).
    fn regioes_dos_filtros(filtros: &FiltrosMissao, ligas: &[Liga]) -> Vec<quality::Regiao> {
        let continente_do_pais = |id: u16| ligas.iter().find(|l| l.pais == Some(id)).map_or(Confederacao::Outras, |l| l.continente);
        let mut regioes: Vec<quality::Regiao> = filtros.continentes.iter().map(|&c| quality::Regiao::Continente(c)).collect();
        regioes.extend(filtros.paises_dos_clubes.iter().map(|&p| quality::Regiao::Pais { id: p, continente: continente_do_pais(p) }));
        for id in &filtros.ligas {
            match ligas.iter().find(|l| l.id == *id) {
                Some(Liga { pais: Some(p), continente, .. }) => regioes.push(quality::Regiao::Pais { id: *p, continente: *continente }),
                Some(l) => regioes.push(quality::Regiao::Continente(l.continente)),
                None => {}
            }
        }
        regioes.dedup();
        regioes
    }

    /// O que o Olheiro já trabalhou até `hoje` (as Missões dele): a
    /// experiência que adapta a mercados e tipos novos (Épico 5).
    fn trabalhos_do_olheiro(dados: &persistence::ScoutStateFile, olheiro: Uuid, hoje: Date, ligas: &[Liga]) -> Vec<quality::TrabalhoPassado> {
        dados
            .missoes
            .iter()
            .filter(|m| m.olheiro_id == olheiro && m.criada_em <= hoje)
            .map(|m| {
                let fim = hoje.min(m.prazo_estimado);
                let regioes = Self::regioes_dos_filtros(&m.filtros, ligas);
                quality::TrabalhoPassado {
                    tipo: m.tipo,
                    regioes: if regioes.is_empty() { vec![quality::Regiao::Mundo] } else { regioes },
                    dias: u32::try_from((fim.day_number() - m.criada_em.day_number()).max(0)).unwrap_or(0),
                }
            })
            .collect()
    }

    /// Penalidade de mercado e de fora do foco de um Olheiro numa Missão com
    /// estes filtros, hoje (e a pior distância de mercado, para a tela).
    fn penalidade_para(&self, olheiro: &Olheiro, filtros: &FiltrosMissao, tipo: quality::TipoMissao, hoje: Date) -> (quality::Penalidade, u8) {
        // Olheiro de antes das estrelas: segue a tabela do v1, sem mercado nem foco
        if olheiro.perfil.is_none() {
            return (quality::Penalidade::default(), 0);
        }
        let ligas = self.ligas_carregadas();
        let regioes = Self::regioes_dos_filtros(filtros, &ligas);
        let trabalhos = self
            .estado_ativo()
            .map(|e| e.ler(|d| Self::trabalhos_do_olheiro(d, olheiro.id, hoje, &ligas)))
            .unwrap_or_default();
        let penalidade = quality::penalidade_missao(&olheiro.perfil(), &olheiro.mercados, &regioes, tipo, &trabalhos);
        let alvo: &[quality::Regiao] = if regioes.is_empty() { &[quality::Regiao::Mundo] } else { &regioes };
        let distancia = alvo.iter().map(|&r| quality::distancia_mercado(&olheiro.mercados, r)).max().unwrap_or(0);
        (penalidade, distancia)
    }

    /// O formulário inteiro, ou `None` se ele não está aberto (ou a
    /// carreira deixou de estar pronta — o formulário deve fechar).
    pub fn previa_missao(&self) -> Option<PreviaMissao> {
        let rascunho = self.rascunho_missao.clone()?;
        let (orcamento_atual, data_atual) = match &self.status {
            CarreiraStatus::Pronta(c) => (c.orcamento_transferencias, c.data_atual),
            _ => return None,
        };
        let olheiros = self.olheiros_contratados();
        let escolhido = rascunho
            .olheiro_id
            .and_then(|id| olheiros.iter().find(|c| c.olheiro.id == id && !c.ocupado()))
            .map(|c| c.olheiro.clone());
        let tipo = quality::tipo_por_filtros(&rascunho.filtros);
        let amplitude = self.amplitude_da_geografia(&rascunho.filtros);
        let (penalidade, distancia_mercado) =
            escolhido.as_ref().map_or_else(Default::default, |o| self.penalidade_para(o, &rascunho.filtros, tipo, data_atual));
        let pedido = |o: &Olheiro, investimento| quality::PedidoMissao {
            perfil: o.perfil(),
            modo: rascunho.modo,
            tipo,
            amplitude,
            penalidade,
            investimento,
        };
        let estimativa = escolhido.as_ref().map(|o| quality::estimar_missao(&pedido(o, rascunho.investimento)));
        let custos_por_verba = escolhido
            .as_ref()
            .map(|o| quality::Investimento::TODOS.iter().map(|&i| (i, quality::estimar_missao(&pedido(o, i)).custo)).collect())
            .unwrap_or_default();
        let fora_do_foco = escolhido.as_ref().is_some_and(|o| {
            let foco = o.perfil().foco();
            tipo != quality::TipoMissao::Geral && foco != Especializacao::Generalista && !quality::combina(foco, tipo)
        });
        let bloqueio = if escolhido.is_none() {
            Some(BloqueioMissao::SemOlheiroDisponivel)
        } else if !rascunho.filtros.overall.valida() {
            Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::OverallMin })
        } else if !rascunho.filtros.potencial.valida() {
            Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::PotencialMin })
        } else if !rascunho.filtros.idade.valida() {
            Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::IdadeMin })
        } else if !rascunho.filtros.contrato.valida() {
            Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::ContratoMin })
        } else if !rascunho.filtros.estrelas_drible.valida() {
            Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::DribleMin })
        } else {
            estimativa
                .filter(|e| orcamento_atual < e.custo)
                .map(|e| BloqueioMissao::OrcamentoInsuficiente { faltam: e.custo.saturating_sub(orcamento_atual) })
        };
        let custo = estimativa.map_or(0, |e| e.custo);
        let orcamento_apos_missao = Some(i64::from(orcamento_atual) - i64::from(custo));
        let folha_disponivel = self.folha_salarial().map(i64::from);
        Some(PreviaMissao {
            teto: rascunho.filtros.limite_valor.teto(orcamento_apos_missao),
            teto_salario: rascunho.filtros.limite_salario.teto(folha_disponivel),
            orcamento_apos_missao,
            folha_disponivel,
            combina: escolhido.as_ref().is_some_and(|o| quality::combina(o.perfil().foco(), tipo)),
            penalidade,
            distancia_mercado,
            fora_do_foco,
            custos_por_verba,
            rascunho,
            olheiros,
            tipo,
            amplitude,
            estimativa,
            orcamento_atual,
            data_atual,
            bloqueio,
        })
    }

    /// Confirmar do formulário. `true` = Missão encomendada (o formulário
    /// fecha). Debita com as garantias da Story 1.5 e grava a Missão como
    /// `Pendente` — nenhuma busca roda agora (AD-8).
    pub fn confirmar_nova_missao(&mut self) -> bool {
        let Some(previa) = self.previa_missao() else {
            return false;
        };
        if previa.bloqueio.is_some() {
            return false;
        }
        let (Some(olheiro_id), Some(estimativa), Some(prazo)) = (previa.rascunho.olheiro_id, previa.estimativa, previa.prazo())
        else {
            return false;
        };
        let missao = Missao {
            id: Uuid::new_v4(),
            olheiro_id,
            status: StatusMissao::Pendente,
            criada_em: previa.data_atual,
            prazo_estimado: prazo,
            filtros: FiltrosMissao { teto_valor: previa.teto, teto_salario: previa.teto_salario, ..previa.rascunho.filtros.clone() },
            modo_busca: previa.rascunho.modo,
            tipo: previa.tipo,
            amplitude: previa.amplitude,
            estimativa,
            continua: previa.rascunho.continua,
            blocos: 1,
            blocos_buscados: 0,
            renovacoes: Vec::new(),
            investimento: previa.rascunho.investimento,
        };
        let nova = missao.clone();
        match self.comprar(estimativa.custo, move |dados| dados.missoes.push(nova)) {
            Ok(()) => {
                tracing::info!(
                    "[scout::state] Missão encomendada: {:?} {:?} ({}), prazo {}, id {}.",
                    missao.tipo,
                    missao.modo_busca,
                    estimativa.custo,
                    missao.prazo_estimado.0,
                    missao.id
                );
                self.rascunho_missao = None;
                self.reler();
                true
            }
            Err(erro) => {
                tracing::warn!("[scout::state] Missão não encomendada: {erro:?}");
                if matches!(erro, ErroCompra::OrcamentoMudou { .. }) {
                    self.reler();
                }
                if let Some(r) = self.rascunho_missao.as_mut() {
                    r.erro = Some(erro);
                }
                false
            }
        }
    }

    /// Missões desta carreira, mais novas primeiro, com o Olheiro e o
    /// progresso de cada uma (aba Missões). Com erro de leitura, a lista da
    /// última carreira pronta continua aparecendo, sem progresso.
    pub fn missoes(&self) -> Vec<MissaoNaLista> {
        let (estado, hoje) = match &self.status {
            CarreiraStatus::Pronta(_) => (self.estado_ativo(), self.data_progresso),
            CarreiraStatus::ErroLeitura => (self.ultimo_save.as_deref().and_then(|id| self.estados.get(id)), None),
            _ => (None, None),
        };
        let Some(estado) = estado else {
            return Vec::new();
        };
        estado.ler(|dados| {
            dados
                .missoes
                .iter()
                .rev()
                .filter(|m| !dados.relatorios.iter().any(|r| r.missao_id == m.id && r.arquivado))
                .map(|m| {
                    let relatorio = dados.relatorios.iter().find(|r| r.missao_id == m.id);
                    let revelados = relatorio.map_or(0, |r| m.revelados(r.jogadores.len(), hoje));
                    MissaoNaLista {
                        missao: m.clone(),
                        olheiro: dados.olheiros.iter().find(|o| o.id == m.olheiro_id).cloned(),
                        progresso: hoje.map(|data| progresso_missao(m.criada_em, m.prazo_estimado, data)),
                        falha: self.falhas_busca.get(&m.id).cloned(),
                        relatorio_id: relatorio.filter(|_| revelados > 0 || m.status == StatusMissao::Concluida).map(|r| r.id),
                        relatorio_novo: relatorio.is_some_and(|r| revelados > usize::from(r.vistos) || (!r.aberto && revelados > 0)),
                        revelados,
                        previstos: m.alvo_total(),
                    }
                })
                .collect()
        })
    }

    // -----------------------------------------------------------------
    // Voltar no tempo (bug relatado pelo Felipe em 2026-10-01; trazido da
    // branch `claude/relatorio-ficha`)
    // -----------------------------------------------------------------

    /// A carreira acabou de ficar ativa a partir de um save gravado em
    /// `data_do_save`. Se o jogador avançou, usou o Scout e saiu SEM
    /// salvar, o jogo voltou para esse save — o orçamento gasto volta
    /// sozinho (estava só na memória), mas o arquivo do Scout não. Aqui o
    /// Scout volta junto: desfaz tudo o que aconteceu DEPOIS da data do
    /// save (o que foi feito no mesmo dia fica — não há como saber se foi
    /// antes ou depois de salvar):
    /// - Olheiros contratados depois → saem;
    /// - Missões encomendadas depois → saem, com os Relatórios;
    /// - renovações de Missão contínua depois → desfeitas;
    /// - Missão de prazo fixo concluída num prazo depois → volta a correr
    ///   (o Relatório parcial continua, revelado pela data).
    ///
    /// Devolve quantas coisas foram desfeitas.
    fn voltar_no_tempo(&mut self, data_do_save: Date) -> usize {
        let Some(estado) = self.estado_ativo().cloned() else {
            return 0;
        };
        let depois = |d: Date| d > data_do_save;
        let desfeitos = estado.ler(|dados| {
            dados.olheiros.iter().filter(|o| o.contratado_em.is_some_and(depois) || o.acompanhando_desde.is_some_and(depois)).count()
                + dados.escolhidos.iter().filter(|e| depois(e.adicionado_em) || depois(e.observado_em)).count()
                + dados
                    .missoes
                    .iter()
                    .filter(|m| {
                        depois(m.criada_em)
                            || m.renovacoes.iter().any(|(d, _)| depois(*d))
                            || (!m.continua && m.status == StatusMissao::Concluida && depois(m.prazo_estimado))
                    })
                    .count()
        });
        if desfeitos == 0 {
            return 0;
        }
        let resultado = estado.mutar(|dados| {
            // Olheiros contratados depois saem, e a oferta deles volta ao mercado.
            let saem: Vec<Uuid> =
                dados.olheiros.iter().filter(|o| o.contratado_em.is_some_and(depois)).filter_map(|o| o.oferta_id).collect();
            dados.ofertas_contratadas.retain(|id| !saem.contains(id));
            dados.olheiros.retain(|o| !o.contratado_em.is_some_and(depois));
            for o in dados.olheiros.iter_mut().filter(|o| o.acompanhando_desde.is_some_and(depois)) {
                o.acompanhando_desde = None;
            }
            // Escolhidos: os adicionados depois saem; uma observação feita
            // depois (atualização do acompanhamento) passa a valer da data
            // do save, com os dias a mais descontados — não há como refazer
            // a observação daquele dia, só não deixar o tempo andar sozinho.
            dados.escolhidos.retain(|e| !depois(e.adicionado_em));
            for e in dados.escolhidos.iter_mut().filter(|e| depois(e.observado_em)) {
                let excesso = u32::try_from(e.observado_em.day_number() - data_do_save.day_number()).unwrap_or(0);
                e.observado_em = data_do_save;
                if let Some(a) = e.acompanhamento.as_mut() {
                    a.dias = a.dias.saturating_sub(excesso);
                }
            }
            let removidas: Vec<Uuid> = dados.missoes.iter().filter(|m| depois(m.criada_em)).map(|m| m.id).collect();
            dados.missoes.retain(|m| !removidas.contains(&m.id));
            dados.relatorios.retain(|r| !removidas.contains(&r.missao_id));
            for m in dados.missoes.iter_mut() {
                // renovações desfeitas da mais nova para a mais antiga
                while let Some(&(quando, prazo_anterior)) = m.renovacoes.last() {
                    if !depois(quando) {
                        break;
                    }
                    m.renovacoes.pop();
                    m.blocos = m.blocos.saturating_sub(1).max(1);
                    m.blocos_buscados = m.blocos_buscados.min(m.blocos);
                    m.prazo_estimado = prazo_anterior;
                }
                if !m.continua && m.status == StatusMissao::Concluida && depois(m.prazo_estimado) {
                    m.status = StatusMissao::Pendente;
                }
            }
            // Relatórios com mais jogadores que os blocos pagos agora
            for r in dados.relatorios.iter_mut() {
                if let Some(m) = dados.missoes.iter().find(|m| m.id == r.missao_id) {
                    r.jogadores.truncate(m.alvo_total());
                    let teto = u16::try_from(r.jogadores.len()).unwrap_or(u16::MAX);
                    r.vistos = r.vistos.min(teto);
                    r.notificados = r.notificados.min(teto);
                }
            }
        });
        match resultado {
            Ok(()) => {
                tracing::info!(
                    "[scout::state] Save de {} carregado: {desfeitos} item(ns) do Scout feitos depois dele foram desfeitos.",
                    data_do_save.0
                );
                self.avisar(TipoAviso::VoltouNoTempo { data: data_do_save, desfeitos });
                desfeitos
            }
            Err(err) => {
                tracing::warn!("[scout::state] Não deu para voltar o Scout para {}: {err:?}", data_do_save.0);
                0
            }
        }
    }

    // -----------------------------------------------------------------
    // Busca das Missões vencidas (Story 2.4, AD-8/AD-9)
    // -----------------------------------------------------------------

    /// Borda fechado→aberto do painel (nunca por polling — AD-8, com a
    /// mudança da Story 2.10):
    /// - Missão `Pendente` que ainda não buscou para os blocos pagos vira
    ///   `EmExecucao` ANTES de entrar na fila — logo na primeira abertura
    ///   depois de criada (ou renovada), para o Relatório parcial existir;
    ///   reabrir o painel não a despacha de novo;
    /// - Missão de prazo fixo com a busca feita e o prazo cumprido vira
    ///   `Concluida` (a contínua espera renovação ou encerramento).
    ///
    /// Ordem da fila: `prazo_estimado`, depois `criada_em` (AD-9).
    fn despachar_missoes(&mut self, hoje: Date) {
        let (Some(id_save), Some(estado)) = (self.save_ativo.clone(), self.estado_ativo().cloned()) else {
            return;
        };
        let (mut a_buscar, a_concluir): (Vec<(Missao, Vec<u32>)>, Vec<Uuid>) = estado.ler(|dados| {
            let pendentes = dados.missoes.iter().filter(|m| m.status == StatusMissao::Pendente);
            let buscar = pendentes
                .clone()
                .filter(|m| m.blocos_buscados < m.blocos.max(1))
                .map(|m| {
                    let ja = dados
                        .relatorios
                        .iter()
                        .find(|r| r.missao_id == m.id)
                        .map(|r| r.jogadores.iter().map(|j| j.player_id).collect())
                        .unwrap_or_default();
                    (m.clone(), ja)
                })
                .collect();
            let concluir = pendentes
                .filter(|m| !m.continua && m.blocos_buscados >= m.blocos.max(1) && hoje >= m.prazo_estimado)
                .map(|m| m.id)
                .collect();
            (buscar, concluir)
        });
        if !a_concluir.is_empty() {
            match estado.mutar(|dados| {
                for m in dados.missoes.iter_mut().filter(|m| a_concluir.contains(&m.id)) {
                    m.status = StatusMissao::Concluida;
                }
            }) {
                Ok(()) => tracing::info!("[scout::state] {} Missão(ões) concluída(s) no prazo.", a_concluir.len()),
                Err(err) => tracing::warn!("[scout::state] Missões não foram concluídas: {err:?}"),
            }
        }
        if a_buscar.is_empty() {
            return;
        }
        a_buscar.sort_by_key(|(m, _)| (m.prazo_estimado, m.criada_em));
        let ids: Vec<Uuid> = a_buscar.iter().map(|(m, _)| m.id).collect();
        let marcou = estado.mutar(|dados| {
            for m in dados.missoes.iter_mut().filter(|m| ids.contains(&m.id)) {
                m.status = StatusMissao::EmExecucao;
            }
        });
        if let Err(err) = marcou {
            // Sem gravar `EmExecucao` não há garantia contra despacho
            // duplicado: a Missão espera a próxima abertura.
            tracing::warn!("[scout::state] Não deu para marcar Missões em execução: {err:?}");
            return;
        }
        for (mut missao, ja) in a_buscar {
            missao.status = StatusMissao::EmExecucao;
            let quantos = missao.alvo_total().saturating_sub(ja.len());
            tracing::info!("[scout::state] Missão {} na fila de busca ({quantos} jogadores).", missao.id);
            self.falhas_busca.remove(&missao.id);
            self.fila_busca.push_back(BuscaNaFila {
                id_save: id_save.clone(),
                missao,
                hoje,
                excluir: ja.into_iter().collect(),
                quantos,
            });
        }
    }

    /// Roda a cada frame (painel aberto ou fechado): trata a busca que
    /// terminou e dispara a próxima da fila. Uma de cada vez (AD-9).
    fn processar_buscas(&mut self) {
        match self.tarefa_busca.poll() {
            TaskState::Running => return,
            TaskState::Done(encontrados) => {
                if let Some(busca) = self.busca_atual.take() {
                    self.concluir_busca(busca, encontrados);
                }
            }
            TaskState::Failed(err) => {
                if let Some(busca) = self.busca_atual.take() {
                    self.falhar_busca(busca, &err);
                }
            }
            TaskState::Idle => {}
        }
        let Some(proxima) = self.fila_busca.pop_front() else {
            return;
        };
        let fonte = Arc::clone(&self.fonte);
        let (missao, hoje, excluir, quantos) =
            (proxima.missao.clone(), proxima.hoje, proxima.excluir.clone(), proxima.quantos);
        if self
            .tarefa_busca
            .start(move || search::executar_missao(&missao, fonte.as_ref(), hoje, &excluir, quantos))
        {
            self.busca_atual = Some(proxima);
        } else {
            self.fila_busca.push_front(proxima);
        }
    }

    /// Junta os jogadores encontrados ao Relatório da Missão (cria se não
    /// existe) no arquivo da carreira DONA da busca (pode não ser a ativa).
    /// Prazo fixo já cumprido → `Concluida` (o Olheiro fica livre); senão a
    /// Missão segue `Pendente`, com o Relatório parcial crescendo.
    fn concluir_busca(&mut self, busca: BuscaNaFila, encontrados: Vec<JogadorEncontrado>) {
        let Some(estado) = self.estados.get(&busca.id_save).cloned() else {
            return;
        };
        let id = busca.missao.id;
        let novos = encontrados.len();
        let concluida = !busca.missao.continua && busca.hoje >= busca.missao.prazo_estimado;
        let base = search::relatorio_vazio(&busca.missao, busca.hoje);
        let mut total = 0;
        let gravou = estado.mutar(|dados| {
            if let Some(m) = dados.missoes.iter_mut().find(|m| m.id == id) {
                m.status = if concluida { StatusMissao::Concluida } else { StatusMissao::Pendente };
                m.blocos_buscados = m.blocos.max(1);
            }
            let indice = match dados.relatorios.iter().position(|r| r.missao_id == id) {
                Some(i) => i,
                None => {
                    dados.relatorios.push(base);
                    dados.relatorios.len() - 1
                }
            };
            if let Some(r) = dados.relatorios.get_mut(indice) {
                r.jogadores.extend(encontrados);
                r.gerado_em = Some(busca.hoje);
                total = r.jogadores.len();
            }
        });
        match gravou {
            Ok(()) => {
                tracing::info!("[scout::state] Busca da Missão {id}: +{novos} jogadores (total {total}).");
                if concluida {
                    self.avisar(TipoAviso::RelatorioPronto { tipo: busca.missao.tipo, jogadores: total });
                }
            }
            Err(err) => {
                tracing::warn!("[scout::state] Relatório da Missão {id} não foi salvo: {err:?}");
                self.falhar_busca(busca, &SaveRepoError::Interno(format!("{err:?}")));
            }
        }
    }

    /// A busca falhou: a Missão volta a `Pendente` (roda de novo na próxima
    /// abertura do painel) e a falha aparece — nunca em silêncio.
    fn falhar_busca(&mut self, busca: BuscaNaFila, err: &SaveRepoError) {
        tracing::warn!("[scout::state] Busca da Missão {} falhou: {err:?}", busca.missao.id);
        let id = busca.missao.id;
        if let Some(estado) = self.estados.get(&busca.id_save) {
            if let Err(e) = estado.mutar(|dados| {
                for m in dados.missoes.iter_mut().filter(|m| m.id == id && m.status == StatusMissao::EmExecucao) {
                    m.status = StatusMissao::Pendente;
                }
            }) {
                tracing::warn!("[scout::state] Não deu para devolver a Missão {id} a Pendente: {e:?}");
            }
        }
        self.falhas_busca.insert(id, err.to_string());
        self.avisar(TipoAviso::BuscaFalhou);
    }


    // -----------------------------------------------------------------
    // Relatórios (Story 2.5)
    // -----------------------------------------------------------------

    fn montar_relatorio_na_lista(dados: &persistence::ScoutStateFile, r: &Relatorio, hoje: Option<Date>) -> RelatorioNaLista {
        let missao = dados.missoes.iter().find(|m| m.id == r.missao_id).cloned();
        let olheiro = missao.as_ref().and_then(|m| dados.olheiros.iter().find(|o| o.id == m.olheiro_id).cloned());
        let revelados = missao.as_ref().map_or(r.jogadores.len(), |m| m.revelados(r.jogadores.len(), hoje));
        let mut relatorio = r.clone();
        relatorio.jogadores.truncate(revelados);
        let parcial = missao.as_ref().is_some_and(|m| m.status != StatusMissao::Concluida);
        // Relatório parcial: quem acabou de aparecer ainda está sendo
        // observado (mercado → salário → atributos).
        if let (Some(m), Some(hoje)) = (missao.as_ref(), hoje) {
            let fracao = progresso_missao(m.criada_em, m.prazo_estimado, hoje).fracao;
            for (indice, j) in relatorio.jogadores.iter_mut().enumerate() {
                j.observacao = quality::observacao(fracao, indice, m.alvo_total(), !parcial);
                if !j.atributos_observados() {
                    j.atributos.clear();
                    j.similaridade = None;
                    j.fit = None;
                    j.variacao_overall = None;
                }
            }
        }
        RelatorioNaLista {
            novo: revelados > usize::from(r.vistos) || (!r.aberto && revelados > 0),
            previstos: missao.as_ref().map_or(revelados, Missao::alvo_total),
            parcial,
            relatorio,
            missao,
            olheiro,
        }
    }

    /// Relatórios da carreira pronta, mais novos primeiro. `arquivados`:
    /// a lista principal (`false`) ou o filtro "Arquivados" (Story 2.7).
    pub fn relatorios(&self, arquivados: bool) -> Vec<RelatorioNaLista> {
        let Some(estado) = self.estado_ativo() else {
            return Vec::new();
        };
        estado.ler(|dados| {
            dados
                .relatorios
                .iter()
                .rev()
                .filter(|r| r.arquivado == arquivados)
                .map(|r| Self::montar_relatorio_na_lista(dados, r, self.data_progresso))
                .collect()
        })
    }

    /// Abre um Relatório: a tela passa a mostrá-lo e o "novo" some
    /// (gravado, para valer depois de reiniciar o jogo).
    pub fn abrir_relatorio(&mut self, id: Uuid) {
        let Some(estado) = self.estado_ativo() else {
            return;
        };
        let existe = estado.ler(|dados| dados.relatorios.iter().any(|r| r.id == id));
        if !existe {
            return;
        }
        // vistos = o que está revelado agora (o "novo" some até crescer)
        let revelados = estado
            .ler(|dados| dados.relatorios.iter().find(|r| r.id == id).map(|r| Self::montar_relatorio_na_lista(dados, r, self.data_progresso)))
            .map_or(0, |item| item.relatorio.jogadores.len());
        let vistos = u16::try_from(revelados).unwrap_or(u16::MAX);
        let mudou = estado.ler(|dados| dados.relatorios.iter().any(|r| r.id == id && (!r.aberto || r.vistos < vistos)));
        if mudou {
            if let Err(err) = estado.mutar(|dados| {
                for r in dados.relatorios.iter_mut().filter(|r| r.id == id) {
                    r.aberto = true;
                    r.vistos = r.vistos.max(vistos);
                    r.notificados = r.notificados.max(vistos);
                }
            }) {
                tracing::warn!("[scout::state] Relatório aberto, mas o \"novo\" não foi salvo: {err:?}");
            }
        }
        self.relatorio_aberto = Some(id);
    }

    /// Rosto do jogador para a visão Cards (pede o carregamento se preciso).
    pub fn rosto(&self, player_id: u32) -> Rosto {
        self.minifaces.rosto(player_id)
    }

    pub fn minifaces(&self) -> &Minifaces {
        &self.minifaces
    }

    /// Data da carreira usada nas telas (a da abertura do painel).
    pub fn data_da_carreira(&self) -> Option<Date> {
        self.data_progresso
    }

    pub fn vendo_arquivados(&self) -> bool {
        self.vendo_arquivados
    }

    pub fn ver_arquivados(&mut self, arquivados: bool) {
        self.vendo_arquivados = arquivados;
    }

    /// Pode arquivar: já foi aberto ao menos uma vez e a Missão terminou
    /// (Story 2.7). Arquivar nunca apaga nada.
    pub fn pode_arquivar(item: &RelatorioNaLista) -> bool {
        item.relatorio.aberto
            && !item.relatorio.arquivado
            && item.missao.as_ref().is_none_or(|m| m.status == StatusMissao::Concluida)
    }

    /// "Arquivar": some da lista principal (e a Missão some da aba
    /// Missões), aparece em "Arquivados". `false` = não permitido/não salvo.
    pub fn arquivar_relatorio(&mut self, id: Uuid) -> bool {
        let permitido = self.relatorios(false).iter().any(|item| item.relatorio.id == id && Self::pode_arquivar(item));
        permitido && self.marcar_arquivado(id, true)
    }

    /// "Restaurar": volta para a lista principal.
    pub fn restaurar_relatorio(&mut self, id: Uuid) -> bool {
        let arquivado = self.relatorios(true).iter().any(|item| item.relatorio.id == id);
        arquivado && self.marcar_arquivado(id, false)
    }

    fn marcar_arquivado(&mut self, id: Uuid, arquivado: bool) -> bool {
        let Some(estado) = self.estado_ativo() else {
            return false;
        };
        match estado.mutar(|dados| {
            for r in dados.relatorios.iter_mut().filter(|r| r.id == id) {
                r.arquivado = arquivado;
            }
        }) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!("[scout::state] Relatório não foi (des)arquivado: {err:?}");
                false
            }
        }
    }

    pub fn fechar_relatorio(&mut self) {
        self.relatorio_aberto = None;
        self.fechar_ficha();
    }

    /// O Relatório na tela, ou `None` (a tela deve fechar).
    pub fn relatorio_aberto(&self) -> Option<RelatorioNaLista> {
        let id = self.relatorio_aberto?;
        let estado = self.estado_ativo()?;
        estado.ler(|dados| {
            dados.relatorios.iter().find(|r| r.id == id).map(|r| Self::montar_relatorio_na_lista(dados, r, self.data_progresso))
        })
    }

    // -----------------------------------------------------------------
    // Relatório parcial, Missão contínua e aviso (Story 2.10)
    // -----------------------------------------------------------------

    /// Com o painel FECHADO e a data da carreira mudando: se um Relatório
    /// ganhou jogadores desde o último anúncio, o banner do canto avisa
    /// ("Relatório atualizado: +2 jogadores · Missão Jovens"). Barato: só
    /// quando a data muda (a leitura de 1 s já acontece de todo jeito).
    fn avisar_jogadores_novos(&mut self) {
        let CarreiraStatus::Pronta(carreira) = &self.status else {
            return;
        };
        let hoje = carreira.data_atual;
        if self.painel_aberto || self.data_avisos == Some(hoje) {
            return;
        }
        self.data_avisos = Some(hoje);
        let Some(estado) = self.estado_ativo().cloned() else {
            return;
        };
        let novidades: Vec<(Uuid, u16, quality::TipoMissao, usize)> = estado.ler(|dados| {
            dados
                .relatorios
                .iter()
                .filter(|r| !r.arquivado)
                .filter_map(|r| {
                    let m = dados.missoes.iter().find(|m| m.id == r.missao_id)?;
                    let revelados = m.revelados(r.jogadores.len(), Some(hoje));
                    let novos = revelados.checked_sub(usize::from(r.notificados)).filter(|n| *n > 0)?;
                    Some((r.id, u16::try_from(revelados).unwrap_or(u16::MAX), m.tipo, novos))
                })
                .collect()
        });
        let Some(&(_, _, tipo, _)) = novidades.first() else {
            return;
        };
        let total: usize = novidades.iter().map(|(_, _, _, n)| n).sum();
        let marcas: Vec<(Uuid, u16)> = novidades.iter().map(|(id, n, _, _)| (*id, *n)).collect();
        if let Err(err) = estado.mutar(|dados| {
            for r in dados.relatorios.iter_mut() {
                if let Some((_, n)) = marcas.iter().find(|(id, _)| *id == r.id) {
                    r.notificados = r.notificados.max(*n);
                }
            }
        }) {
            tracing::warn!("[scout::state] Aviso de jogadores novos não foi salvo: {err:?}");
        }
        tracing::info!("[scout::state] Relatório(s) atualizado(s): +{total} jogadores.");
        self.avisar(TipoAviso::RelatorioAtualizado { tipo, novos: total });
    }

    /// Custo de mais um bloco de uma Missão contínua (o mesmo do primeiro).
    pub fn custo_do_bloco(missao: &Missao) -> i32 {
        missao.estimativa.custo
    }

    /// O bloco pago de uma Missão contínua acabou (na data do painel).
    pub fn bloco_encerrado(&self, missao: &Missao) -> bool {
        missao.continua
            && missao.status == StatusMissao::Pendente
            && self.data_progresso.is_some_and(|hoje| hoje >= missao.prazo_estimado)
    }

    /// "Renovar": paga mais um bloco de `DIAS_BLOCO_CONTINUO` dias, com
    /// as garantias de toda compra (débito confirmado + gravação; nada
    /// automático — FR-3/NFR1). O novo bloco busca mais jogadores na
    /// próxima abertura do painel.
    pub fn renovar_missao(&mut self, id: Uuid) -> bool {
        let Some(missao) = self.estado_ativo().and_then(|e| e.ler(|d| d.missoes.iter().find(|m| m.id == id).cloned())) else {
            return false;
        };
        if !self.bloco_encerrado(&missao) {
            return false;
        }
        let hoje = self.data_progresso.unwrap_or(missao.prazo_estimado);
        let resultado = self.comprar(Self::custo_do_bloco(&missao), move |dados| {
            if let Some(m) = dados.missoes.iter_mut().find(|m| m.id == id) {
                m.renovacoes.push((hoje, m.prazo_estimado));
                m.blocos = m.blocos.saturating_add(1);
                // o bloco novo começa hoje (ou no fim do anterior, se antes)
                let inicio = hoje.max(m.prazo_estimado);
                m.prazo_estimado = inicio.mais_dias(quality::DIAS_BLOCO_CONTINUO);
            }
        });
        match resultado {
            Ok(()) => {
                tracing::info!("[scout::state] Missão contínua {id} renovada por mais um bloco.");
                self.erros_missao.remove(&id);
                self.reler();
                // a busca do bloco novo roda já (o painel está aberto)
                if let Some(hoje) = self.data_progresso {
                    self.despachar_missoes(hoje);
                }
                true
            }
            Err(erro) => {
                tracing::warn!("[scout::state] Renovação não concluída: {erro:?}");
                if matches!(erro, ErroCompra::OrcamentoMudou { .. }) {
                    self.reler();
                }
                self.erros_missao.insert(id, erro);
                false
            }
        }
    }

    /// "Encerrar" uma Missão contínua: o Olheiro fica livre e os jogadores
    /// JÁ REVELADOS viram o Relatório final (os que ainda não tinham
    /// aparecido são descartados). Não mexe no orçamento.
    pub fn encerrar_missao(&mut self, id: Uuid) -> bool {
        let Some(estado) = self.estado_ativo().cloned() else {
            return false;
        };
        let hoje = self.data_progresso;
        let pode = estado.ler(|d| d.missoes.iter().any(|m| m.id == id && m.continua && m.status == StatusMissao::Pendente));
        if !pode {
            return false;
        }
        match estado.mutar(|dados| {
            let revelados = dados.missoes.iter().find(|m| m.id == id).map(|m| {
                let encontrados = dados.relatorios.iter().find(|r| r.missao_id == id).map_or(0, |r| r.jogadores.len());
                m.revelados(encontrados, hoje)
            });
            if let Some(r) = dados.relatorios.iter_mut().find(|r| r.missao_id == id) {
                r.jogadores.truncate(revelados.unwrap_or(0));
            }
            if let Some(m) = dados.missoes.iter_mut().find(|m| m.id == id) {
                m.status = StatusMissao::Concluida;
                m.prazo_estimado = hoje.map_or(m.prazo_estimado, |h| h.min(m.prazo_estimado));
            }
        }) {
            Ok(()) => {
                tracing::info!("[scout::state] Missão contínua {id} encerrada.");
                self.erros_missao.remove(&id);
                true
            }
            Err(err) => {
                tracing::warn!("[scout::state] Missão não foi encerrada: {err:?}");
                self.erros_missao.insert(id, ErroCompra::NaoSalvo);
                false
            }
        }
    }

    /// Falha da última renovação desta Missão (o card mostra).
    pub fn erro_da_missao(&self, id: Uuid) -> Option<&ErroCompra> {
        self.erros_missao.get(&id)
    }

    /// Aba que a navegação deve assumir porque uma carreira acabou de
    /// ficar ativa (devolve uma vez só). Cliques em abas feitos antes da
    /// carreira ficar pronta não contam: vale a aba salva da carreira.
    pub fn tomar_aba_restaurada(&mut self) -> Option<Aba> {
        self.aba_restaurada.take()
    }

    /// O usuário trocou de aba: persiste em `ui_prefs` (AD-7). Sem
    /// carreira pronta não há arquivo para gravar e a troca é só visual.
    pub fn definir_aba_ativa(&mut self, aba: Aba) {
        let Some(estado) = self.estado_ativo() else {
            return;
        };
        if estado.ler(|dados| dados.ui_prefs.aba_ativa) == aba {
            return;
        }
        if let Err(err) = estado.mutar(|dados| dados.ui_prefs.aba_ativa = aba) {
            tracing::warn!("[scout::state] Aba ativa não foi salva: {err:?}");
        }
    }
}

/// Elenco do técnico para o seletor: por função (goleiro → defesa → meio →
/// ataque), depois Overall.
fn elenco_de(pool: crate::save_repo::PlayerPool, hoje: Date) -> Vec<JogadorElenco> {
    let mut elenco: Vec<JogadorElenco> = pool
        .jogadores
        .into_iter()
        .map(|j| JogadorElenco {
            player_id: j.player_id,
            idade: j.idade(hoje),
            posicao: j.posicao,
            overall: j.overall,
            potencial: j.potencial,
            atributos: j.atributos.to_vec(),
            nome: j.nome,
        })
        .collect();
    elenco.sort_by(|a, b| {
        let grupo = |j: &JogadorElenco| crate::save_repo::funcao_da_posicao(j.posicao) as u8;
        grupo(a).cmp(&grupo(b)).then(b.overall.cmp(&a.overall)).then(a.nome.cmp(&b.nome))
    });
    elenco
}

/// Ao carregar o arquivo de uma carreira (uma vez por sessão): Missão em
/// `EmExecucao` aqui é sobra de uma busca interrompida (jogo fechado no
/// meio) — volta a `Pendente` para não ficar presa (Story 2.4).
fn destravar_missoes(estado: &EstadoPersistido) {
    let presas = estado.ler(|dados| dados.missoes.iter().filter(|m| m.status == StatusMissao::EmExecucao).count());
    if presas == 0 {
        return;
    }
    match estado.mutar(|dados| {
        for m in dados.missoes.iter_mut().filter(|m| m.status == StatusMissao::EmExecucao) {
            m.status = StatusMissao::Pendente;
        }
    }) {
        Ok(()) => tracing::info!("[scout::state] {presas} Missão(ões) interrompida(s) voltaram a Pendente."),
        Err(err) => tracing::warn!("[scout::state] Missões presas em execução não foram destravadas: {err:?}"),
    }
}

fn status_de_erro(err: &SaveRepoError) -> CarreiraStatus {
    match err {
        SaveRepoError::NaoLocalizado | SaveRepoError::CarreiraNaoCarregada => CarreiraStatus::SemCarreira,
        outro => {
            tracing::warn!("[scout::state] Falha ao ler o save ativo: {outro:?}");
            CarreiraStatus::ErroLeitura
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::persistence::tests::PastaTemporaria;
    use crate::save_repo::PlayerPool;
    use crate::scout::search::tests::{jogador, pool};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// Fonte falsa: devolve as leituras de uma fila (a última se repete) e
    /// conta as localizações, que terminam com `resultado_localizacao`.
    struct FonteFalsa {
        leituras: Mutex<Vec<Result<CareerSnapshot, SaveRepoError>>>,
        resultado_localizacao: Result<(), SaveRepoError>,
        localizacoes: Arc<AtomicUsize>,
        /// Resposta do sinal "há carreira carregada?", controlada pelo teste.
        sinal: Arc<Mutex<bool>>,
        /// Resultados das próximas escritas de orçamento (vazio = sucesso).
        resultados_escrita: Mutex<Vec<Result<i32, SaveRepoError>>>,
        /// `(anterior, novo)` de cada escrita pedida.
        escritas: Arc<Mutex<Vec<(i32, i32)>>>,
        /// Resposta de `read_all_players` (a busca de uma Missão).
        jogadores: Arc<Mutex<Result<PlayerPool, SaveRepoError>>>,
        /// Quantas buscas leram os jogadores.
        buscas: Arc<AtomicUsize>,
        /// Enquanto `false`, a busca fica "rodando" (para testar o despacho
        /// com a tarefa em andamento).
        liberar_busca: Arc<std::sync::atomic::AtomicBool>,
    }

    impl CareerSource for FonteFalsa {
        fn write_transfer_budget(&self, anterior: i32, novo: i32) -> Result<i32, SaveRepoError> {
            self.escritas.lock().unwrap_or_else(|p| p.into_inner()).push((anterior, novo));
            let mut fila = self.resultados_escrita.lock().unwrap_or_else(|p| p.into_inner());
            if fila.is_empty() {
                Ok(novo)
            } else {
                fila.remove(0)
            }
        }

        fn read_nations(&self) -> Result<Vec<Nacao>, SaveRepoError> {
            Ok(vec![
                Nacao { id: 52, nome: "Argentina".to_string(), iso: "AR".to_string(), confederacao: Confederacao::AmericaDoSul },
                Nacao { id: 54, nome: "Brazil".to_string(), iso: "BR".to_string(), confederacao: Confederacao::AmericaDoSul },
                Nacao { id: 14, nome: "England".to_string(), iso: "GB".to_string(), confederacao: Confederacao::Europa },
            ])
        }

        fn read_all_players(&self) -> Result<PlayerPool, SaveRepoError> {
            self.buscas.fetch_add(1, Ordering::SeqCst);
            let inicio = Instant::now();
            while !self.liberar_busca.load(Ordering::SeqCst) && inicio.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(1));
            }
            self.jogadores.lock().unwrap_or_else(|p| p.into_inner()).clone()
        }

        fn start_career_probe(&self, task: &AsyncTask<bool>) -> bool {
            let aceso = *self.sinal.lock().unwrap_or_else(|p| p.into_inner());
            task.start(move || Ok(aceso))
        }

        fn start_locating(&self, task: &AsyncTask<()>) -> bool {
            self.localizacoes.fetch_add(1, Ordering::SeqCst);
            let resultado = self.resultado_localizacao.clone();
            task.start(move || resultado)
        }

        fn read_snapshot(&self) -> Result<CareerSnapshot, SaveRepoError> {
            let mut fila = self.leituras.lock().unwrap_or_else(|p| p.into_inner());
            if fila.len() > 1 {
                fila.remove(0)
            } else {
                fila.first().cloned().unwrap_or(Err(SaveRepoError::NaoLocalizado))
            }
        }
    }

    fn snapshot() -> CareerSnapshot {
        CareerSnapshot {
            orcamento_transferencias: 63_999_988,
            data_atual: Date(20260703),
            tecnico: "Senhor Manager".to_string(),
            id_save: ID_A.to_string(),
            data_do_save: Date(20260703),
            folha_salarial: Some(165_000),
        }
    }

    const ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn snapshot_de(id_save: &str) -> CareerSnapshot {
        CareerSnapshot { id_save: id_save.to_string(), ..snapshot() }
    }

    /// Sem pasta de estado: persistência só em memória (testes antigos).
    fn estado(
        leituras: Vec<Result<CareerSnapshot, SaveRepoError>>,
        resultado_localizacao: Result<(), SaveRepoError>,
    ) -> (ScoutState, Arc<AtomicUsize>) {
        estado_em(leituras, resultado_localizacao, None)
    }

    fn estado_em(
        leituras: Vec<Result<CareerSnapshot, SaveRepoError>>,
        resultado_localizacao: Result<(), SaveRepoError>,
        diretorio: Option<PathBuf>,
    ) -> (ScoutState, Arc<AtomicUsize>) {
        estado_com_sinal(leituras, resultado_localizacao, diretorio, Arc::new(Mutex::new(false)))
    }

    fn estado_com_sinal(
        leituras: Vec<Result<CareerSnapshot, SaveRepoError>>,
        resultado_localizacao: Result<(), SaveRepoError>,
        diretorio: Option<PathBuf>,
        sinal: Arc<Mutex<bool>>,
    ) -> (ScoutState, Arc<AtomicUsize>) {
        let localizacoes = Arc::new(AtomicUsize::new(0));
        let fonte = FonteFalsa {
            leituras: Mutex::new(leituras),
            resultado_localizacao,
            localizacoes: Arc::clone(&localizacoes),
            sinal,
            resultados_escrita: Mutex::new(Vec::new()),
            escritas: Arc::new(Mutex::new(Vec::new())),
            jogadores: Arc::new(Mutex::new(Ok(pool_de_teste()))),
            buscas: Arc::new(AtomicUsize::new(0)),
            liberar_busca: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        (ScoutState::com_fonte(Box::new(fonte), diretorio), localizacoes)
    }

    type Escritas = Arc<Mutex<Vec<(i32, i32)>>>;

    /// Carreira pronta com `orcamento`; depois de contratar, a releitura
    /// devolve `orcamento_depois`. Escritas seguem `resultados`.
    fn estado_contratacao(
        orcamento: i32,
        orcamento_depois: i32,
        diretorio: Option<PathBuf>,
        resultados: Vec<Result<i32, SaveRepoError>>,
    ) -> (ScoutState, Escritas) {
        let antes = CareerSnapshot { orcamento_transferencias: orcamento, ..snapshot() };
        let depois = CareerSnapshot { orcamento_transferencias: orcamento_depois, ..snapshot() };
        let escritas: Escritas = Arc::new(Mutex::new(Vec::new()));
        let fonte = FonteFalsa {
            leituras: Mutex::new(vec![Ok(antes), Ok(depois)]),
            resultado_localizacao: Ok(()),
            localizacoes: Arc::new(AtomicUsize::new(0)),
            sinal: Arc::new(Mutex::new(false)),
            resultados_escrita: Mutex::new(resultados),
            escritas: Arc::clone(&escritas),
            jogadores: Arc::new(Mutex::new(Ok(pool_de_teste()))),
            buscas: Arc::new(AtomicUsize::new(0)),
            liberar_busca: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        let mut st = ScoutState::com_fonte(Box::new(fonte), diretorio);
        st.ao_abrir_painel();
        (st, escritas)
    }

    fn escritas_de(escritas: &Escritas) -> Vec<(i32, i32)> {
        escritas.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn erro_da_contratacao(st: &ScoutState) -> Option<ErroCompra> {
        st.contratacao.as_ref().and_then(|c| c.erro.clone())
    }

    /// Força uma sondagem agora e trata a resposta (painel fechado).
    fn sondar(st: &mut ScoutState) {
        st.proximo_sinal = None;
        st.tick();
        let inicio = Instant::now();
        while matches!(st.tarefa_sinal.poll(), TaskState::Running) {
            assert!(inicio.elapsed() < Duration::from_secs(5), "sinal falso travou");
            std::thread::sleep(Duration::from_millis(2));
        }
        st.tick();
    }

    fn acender(sinal: &Arc<Mutex<bool>>, aceso: bool) {
        *sinal.lock().unwrap_or_else(|p| p.into_inner()) = aceso;
    }

    /// Roda `tick` até a localização em background terminar.
    fn ticks_ate_terminar(estado: &mut ScoutState) {
        let inicio = Instant::now();
        while matches!(estado.tarefa_localizar.poll(), TaskState::Running) {
            assert!(inicio.elapsed() < Duration::from_secs(5), "localização falsa travou");
            std::thread::sleep(Duration::from_millis(2));
        }
        estado.tick();
    }

    #[test]
    fn opening_with_a_located_career_shows_it_without_scanning() {
        let (mut st, localizacoes) = estado(vec![Ok(snapshot())], Ok(()));
        st.ao_abrir_painel();
        assert_eq!(st.status(), &CarreiraStatus::Pronta(snapshot()));
        assert_eq!(localizacoes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn first_opening_locates_once_then_shows_the_career() {
        let (mut st, localizacoes) = estado(vec![Err(SaveRepoError::NaoLocalizado), Ok(snapshot())], Ok(()));
        st.ao_abrir_painel();
        assert_eq!(st.status(), &CarreiraStatus::Localizando);
        ticks_ate_terminar(&mut st);
        assert_eq!(st.status(), &CarreiraStatus::Pronta(snapshot()));
        assert_eq!(localizacoes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_locating_shows_no_career_and_does_not_loop() {
        let (mut st, localizacoes) =
            estado(vec![Err(SaveRepoError::NaoLocalizado)], Err(SaveRepoError::CarreiraNaoCarregada));
        st.ao_abrir_painel();
        ticks_ate_terminar(&mut st);
        assert_eq!(st.status(), &CarreiraStatus::SemCarreira);

        // muitos frames depois, ainda sem novas varreduras
        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        for _ in 0..10 {
            st.tick();
        }
        assert_eq!(st.status(), &CarreiraStatus::SemCarreira);
        assert_eq!(localizacoes.load(Ordering::SeqCst), 1);

        // reabrir o painel permite uma nova tentativa
        st.ao_abrir_painel();
        assert_eq!(localizacoes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn read_failure_shows_error_and_retry_relocates() {
        let (mut st, localizacoes) = estado(
            vec![Err(SaveRepoError::ProcessoInacessivel), Ok(snapshot())],
            Ok(()),
        );
        st.ao_abrir_painel();
        assert_eq!(st.status(), &CarreiraStatus::ErroLeitura);
        assert_eq!(localizacoes.load(Ordering::SeqCst), 0);

        // sem releitura automática no estado de erro
        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        st.tick();
        assert_eq!(st.status(), &CarreiraStatus::ErroLeitura);

        st.tentar_novamente();
        assert_eq!(st.status(), &CarreiraStatus::Localizando);
        ticks_ate_terminar(&mut st);
        assert_eq!(st.status(), &CarreiraStatus::Pronta(snapshot()));
        assert_eq!(localizacoes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn periodic_reread_follows_the_game_and_the_menu() {
        let mut depois = snapshot();
        depois.orcamento_transferencias = 50_000_000;
        let (mut st, localizacoes) = estado(
            vec![Ok(snapshot()), Ok(depois.clone()), Err(SaveRepoError::CarreiraNaoCarregada), Ok(depois.clone())],
            Ok(()),
        );
        st.ao_abrir_painel();
        assert_eq!(st.status(), &CarreiraStatus::Pronta(snapshot()));

        // antes do intervalo: não relê
        st.tick();
        assert_eq!(st.status(), &CarreiraStatus::Pronta(snapshot()));

        let vencer = |st: &mut ScoutState| st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        vencer(&mut st);
        st.tick();
        assert_eq!(st.status(), &CarreiraStatus::Pronta(depois.clone()));

        // voltou ao menu (auto-localização já gasta nesta abertura)
        vencer(&mut st);
        st.tick();
        assert_eq!(st.status(), &CarreiraStatus::SemCarreira);

        // voltou para a carreira: a releitura em SemCarreira recupera
        vencer(&mut st);
        st.tick();
        assert_eq!(st.status(), &CarreiraStatus::Pronta(depois));
        assert_eq!(localizacoes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn build_mismatch_is_a_read_error() {
        assert_eq!(status_de_erro(&SaveRepoError::TabelaNaoEncontrada), CarreiraStatus::ErroLeitura);
        assert_eq!(status_de_erro(&SaveRepoError::Interno("x".into())), CarreiraStatus::ErroLeitura);
        assert_eq!(status_de_erro(&SaveRepoError::CarreiraNaoCarregada), CarreiraStatus::SemCarreira);
    }

    fn aba_no_arquivo(pasta: &PastaTemporaria, id_save: &str) -> serde_json::Value {
        let bytes = std::fs::read(pasta.0.join(format!("{id_save}.json"))).unwrap_or_default();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        json["ui_prefs"]["aba_ativa"].clone()
    }

    fn vencer_releitura(st: &mut ScoutState) {
        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
    }

    #[test]
    fn ready_career_creates_its_file_and_the_saved_tab_survives_a_restart() {
        let pasta = PastaTemporaria::nova();
        let (mut st, _) = estado_em(vec![Ok(snapshot_de(ID_A))], Ok(()), Some(pasta.0.clone()));
        st.ao_abrir_painel();
        assert!(pasta.0.join(format!("{ID_A}.json")).is_file(), "arquivo criado no primeiro uso");
        assert_eq!(st.tomar_aba_restaurada(), Some(Aba::Olheiros));
        assert_eq!(st.tomar_aba_restaurada(), None, "restaura uma vez só");

        st.definir_aba_ativa(Aba::Sonar);
        assert_eq!(aba_no_arquivo(&pasta, ID_A), "sonar");

        // releitura periódica da mesma carreira não restaura de novo
        vencer_releitura(&mut st);
        st.tick();
        assert_eq!(st.tomar_aba_restaurada(), None);

        // "reiniciar o jogo": novo estado, carreira ainda não localizada
        drop(st);
        let (mut st, _) =
            estado_em(vec![Err(SaveRepoError::NaoLocalizado), Ok(snapshot_de(ID_A))], Ok(()), Some(pasta.0.clone()));
        st.ao_abrir_painel();
        assert_eq!(st.status(), &CarreiraStatus::Localizando);
        assert_eq!(st.tomar_aba_restaurada(), None);
        ticks_ate_terminar(&mut st);
        assert_eq!(st.tomar_aba_restaurada(), Some(Aba::Sonar));
    }

    #[test]
    fn two_careers_each_read_and_write_only_their_own_file() {
        let pasta = PastaTemporaria::nova();
        let (mut st, _) = estado_em(
            vec![Ok(snapshot_de(ID_A)), Ok(snapshot_de(ID_B)), Ok(snapshot_de(ID_A))],
            Ok(()),
            Some(pasta.0.clone()),
        );
        st.ao_abrir_painel();
        assert_eq!(st.tomar_aba_restaurada(), Some(Aba::Olheiros));
        st.definir_aba_ativa(Aba::Sonar);

        // trocou para a carreira B
        vencer_releitura(&mut st);
        st.tick();
        assert_eq!(st.tomar_aba_restaurada(), Some(Aba::Olheiros), "B tem a própria aba");
        st.definir_aba_ativa(Aba::Relatorios);
        assert_eq!(aba_no_arquivo(&pasta, ID_A), "sonar");
        assert_eq!(aba_no_arquivo(&pasta, ID_B), "relatorios");

        // voltou para A: mesmo estado (mesmo mutex), aba de A
        vencer_releitura(&mut st);
        st.tick();
        assert_eq!(st.tomar_aba_restaurada(), Some(Aba::Sonar));
        assert_eq!(st.estados.len(), 2);
    }

    #[test]
    fn tab_changes_without_a_ready_career_are_not_saved() {
        let pasta = PastaTemporaria::nova();
        let (mut st, _) = estado_em(
            vec![Ok(snapshot_de(ID_A)), Err(SaveRepoError::CarreiraNaoCarregada), Ok(snapshot_de(ID_A))],
            Ok(()),
            Some(pasta.0.clone()),
        );
        st.ao_abrir_painel();
        st.definir_aba_ativa(Aba::Missoes);

        // voltou ao menu com o painel aberto: troca de aba é só visual
        vencer_releitura(&mut st);
        st.tick();
        assert_eq!(st.status(), &CarreiraStatus::SemCarreira);
        st.definir_aba_ativa(Aba::Sonar);
        assert_eq!(aba_no_arquivo(&pasta, ID_A), "missoes");

        // voltou à carreira: a navegação volta para a aba salva
        vencer_releitura(&mut st);
        st.tick();
        assert_eq!(st.tomar_aba_restaurada(), Some(Aba::Missoes));
    }

    #[test]
    fn without_a_data_dir_the_scout_still_works() {
        let (mut st, _) = estado(vec![Ok(snapshot())], Ok(()));
        st.ao_abrir_painel();
        assert_eq!(st.status(), &CarreiraStatus::Pronta(snapshot()));
        st.definir_aba_ativa(Aba::Sonar); // só avisa no log
        assert_eq!(st.tomar_aba_restaurada(), Some(Aba::Olheiros));
    }

    #[test]
    fn first_tick_announces_the_injection_for_three_seconds() {
        let (mut st, _) = estado(vec![Err(SaveRepoError::NaoLocalizado)], Ok(()));
        assert_eq!(st.aviso_visivel(Instant::now()), None);
        st.tick();
        let agora = Instant::now();
        assert_eq!(st.aviso_visivel(agora), Some(&TipoAviso::Injetado));
        assert_eq!(st.aviso_visivel(agora + DURACAO_AVISO), None);
    }

    #[test]
    fn signal_locates_the_career_without_opening_the_panel() {
        let sinal = Arc::new(Mutex::new(true));
        let (mut st, localizacoes) = estado_com_sinal(
            vec![Err(SaveRepoError::NaoLocalizado), Ok(snapshot())],
            Ok(()),
            None,
            Arc::clone(&sinal),
        );
        sondar(&mut st);
        assert_eq!(st.status(), &CarreiraStatus::Localizando);
        // "Carregando" fica enquanto localiza, mesmo passado o tempo do aviso
        let depois = Instant::now() + DURACAO_AVISO * 5;
        assert_eq!(st.aviso_visivel(depois), Some(&TipoAviso::Carregando));

        ticks_ate_terminar(&mut st);
        assert_eq!(st.status(), &CarreiraStatus::Pronta(snapshot()));
        let agora = Instant::now();
        assert_eq!(st.aviso_visivel(agora), Some(&TipoAviso::Pronta(snapshot())));
        assert_eq!(st.aviso_visivel(agora + DURACAO_AVISO), None);
        assert_eq!(localizacoes.load(Ordering::SeqCst), 1);

        // com a carreira pronta o sinal não é mais consultado
        st.proximo_sinal = None;
        st.tick();
        assert_eq!(st.proximo_sinal, None);
    }

    #[test]
    fn failed_locating_is_announced_and_the_signal_only_rearms_after_going_off() {
        let sinal = Arc::new(Mutex::new(true));
        let (mut st, localizacoes) = estado_com_sinal(
            vec![Err(SaveRepoError::NaoLocalizado)],
            Err(SaveRepoError::CarreiraNaoCarregada),
            None,
            Arc::clone(&sinal),
        );
        sondar(&mut st);
        ticks_ate_terminar(&mut st);
        assert_eq!(st.status(), &CarreiraStatus::SemCarreira);
        assert_eq!(st.aviso_visivel(Instant::now()), Some(&TipoAviso::Falhou));

        // sinal continua aceso: nada de loop
        for _ in 0..3 {
            sondar(&mut st);
        }
        assert_eq!(localizacoes.load(Ordering::SeqCst), 1);

        // voltou ao menu (apagou) e entrou de novo (acendeu): nova tentativa
        acender(&sinal, false);
        sondar(&mut st);
        acender(&sinal, true);
        sondar(&mut st);
        assert_eq!(localizacoes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn signal_off_never_locates() {
        let (mut st, localizacoes) = estado(vec![Err(SaveRepoError::NaoLocalizado)], Ok(()));
        for _ in 0..3 {
            sondar(&mut st);
        }
        assert_eq!(st.status(), &CarreiraStatus::SemCarreira);
        assert_eq!(localizacoes.load(Ordering::SeqCst), 0);
    }

    /// Uma oferta de Olheiro do v1 (Especialização × Tier) com o custo da
    /// tabela antiga (os testes de compra conferem valores exatos).
    fn oferta_v1(especializacao: Especializacao, tier: Tier) -> OfertaOlheiro {
        let custo = match (especializacao, tier) {
            (Especializacao::Generalista, Tier::Junior) => 300_000,
            (Especializacao::Tatico, Tier::Elite) => 5_200_000,
            _ => 1_500_000,
        };
        let id = Uuid::new_v4();
        let olheiro = Olheiro {
            id,
            especializacao,
            tier,
            nome: "Carlos Teste".to_string(),
            perfil: Some(quality::PerfilOlheiro::v1(especializacao, tier)),
            oferta_id: Some(id),
            ..Default::default()
        };
        OfertaOlheiro { id, olheiro, custo, faltam: None }
    }

    fn candidato(indice: u32, foco: u8, pais: u16) -> quality::CandidatoOlheiro {
        let e = quality::Estrelas(foco);
        quality::CandidatoOlheiro {
            indice,
            semente: u64::from(indice) * 7919,
            perfil: quality::PerfilOlheiro { jovens: e, medalhoes: e.menos(2), tatico: e.menos(2), generalista: e.menos(2), rede: quality::Estrelas(5) },
            pais: Some(pais),
            mercados: vec![quality::Mercado::Pais { id: pais, continente: Confederacao::AmericaDoSul }],
        }
    }

    #[test]
    fn offers_get_a_name_a_nation_a_cost_and_leave_the_list_once_hired() {
        let nacoes = vec![
            Nacao { id: 54, nome: "Brazil".to_string(), iso: "BR".to_string(), confederacao: Confederacao::AmericaDoSul },
            Nacao { id: 52, nome: "Argentina".to_string(), iso: "AR".to_string(), confederacao: Confederacao::AmericaDoSul },
        ];
        let candidatos = [candidato(0, 9, 54), candidato(1, 5, 52), candidato(2, 7, 54)];
        let ofertas = montar_ofertas(&candidatos, 24_400, 42, &nacoes, &[], Some(1_000_000));
        assert_eq!(ofertas.len(), 3);
        assert!(ofertas.windows(2).all(|w| w[0].custo <= w[1].custo), "do mais barato ao mais caro");
        let barato = &ofertas[0];
        assert_eq!(barato.olheiro.tier, Tier::Junior);
        assert_eq!(barato.olheiro.nacao.as_ref().map(|n| n.nome.as_str()), Some("Argentina"));
        assert!(!barato.olheiro.nome.is_empty());
        assert_eq!(barato.faltam, None, "Júnior cabe em 1 M");
        let elite = ofertas.iter().find(|o| o.olheiro.tier == Tier::Elite).expect("elite");
        assert_eq!(elite.faltam, Some(elite.custo - 1_000_000));
        assert_eq!(elite.olheiro.perfil().foco(), Especializacao::CacadorDeJovens);
        // mesma carreira e mês: mesmos ids; contratada sai da lista
        let de_novo = montar_ofertas(&candidatos, 24_400, 42, &nacoes, &[elite.id], Some(1_000_000));
        assert_eq!(de_novo.len(), 2);
        assert!(de_novo.iter().all(|o| o.id != elite.id));
        assert_eq!(semente_da_carreira(ID_A), 0xaaaa_aaaa_aaaa_aaaa);
    }

    #[test]
    fn the_market_loads_in_background_and_hiring_from_it_saves_the_name_and_removes_the_offer() {
        let pasta = PastaTemporaria::nova();
        let (mut st, escritas) = estado_contratacao(63_999_988, 63_000_000, Some(pasta.0.clone()), vec![]);
        let inicio = Instant::now();
        let mercado = loop {
            match st.mercado_de_olheiros() {
                Carga::Pronto(m) => break m,
                _ => {
                    assert!(inicio.elapsed() < Duration::from_secs(5), "mercado não carregou");
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        };
        // a fonte falsa não dá o clube: atratividade média
        assert_eq!(mercado.clube, None);
        assert!(mercado.ofertas.len() >= 4);
        assert_eq!(mercado.renova_em, Date(20260801));
        let oferta = mercado.ofertas[0].clone();
        st.preparar_contratacao(oferta.clone());
        st.definir_nome_da_contratacao("  Zé Olheiro  ");
        assert!(st.confirmar_contratacao());
        assert_eq!(escritas_de(&escritas), [(63_999_988, 63_999_988 - oferta.custo)]);
        let contratado = st.olheiros_contratados().remove(0).olheiro;
        assert_eq!(contratado.nome, "Zé Olheiro");
        assert_ne!(contratado.id, oferta.id, "o id de verdade é novo");
        assert_eq!((contratado.oferta_id, contratado.perfil, contratado.mercados.clone()), (Some(oferta.id), oferta.olheiro.perfil, oferta.olheiro.mercados.clone()));
        let depois = match st.mercado_de_olheiros() {
            Carga::Pronto(m) => m,
            outro => panic!("{outro:?}"),
        };
        assert!(depois.ofertas.iter().all(|o| o.id != oferta.id), "a oferta contratada saiu do mês");
    }

    #[test]
    fn hired_olheiros_come_from_the_career_file_with_their_mission_status() {
        let pasta = PastaTemporaria::nova();
        let (mut st, _) = estado_em(vec![Ok(snapshot())], Ok(()), Some(pasta.0.clone()));
        assert!(st.olheiros_contratados().is_empty(), "sem carreira, nada");
        st.ao_abrir_painel();
        assert!(st.olheiros_contratados().is_empty());
        assert_eq!(st.orcamento(), Some(63_999_988));

        let ocupado = Olheiro { id: Uuid::new_v4(), especializacao: Especializacao::Tatico, tier: Tier::Experiente, ..Default::default() };
        let livre = Olheiro { id: Uuid::new_v4(), especializacao: Especializacao::Generalista, tier: Tier::Junior, ..Default::default() };
        let missao = |olheiro: &Olheiro, status| Missao::de_teste(olheiro.id, status);
        let missoes = vec![missao(&ocupado, StatusMissao::Pendente), missao(&livre, StatusMissao::Concluida)];
        let (o1, o2) = (ocupado.clone(), livre.clone());
        st.estado_ativo()
            .map(|e| e.mutar(move |d| {
                d.olheiros = vec![o1, o2];
                d.missoes = missoes;
            }))
            .expect("carreira ativa")
            .expect("gravou");

        type Resumo = (Uuid, bool, Option<StatusMissao>, Option<Uuid>, usize);
        let lista = st.olheiros_contratados();
        let resumo: Vec<Resumo> =
            lista.iter().map(|c| (c.olheiro.id, c.em_missao, c.missao.as_ref().map(|m| m.status), c.relatorio_atual, c.relatorios)).collect();
        assert_eq!(
            resumo,
            vec![(ocupado.id, true, Some(StatusMissao::Pendente), None, 0), (livre.id, false, None, None, 0)],
            "o ocupado traz a Missão em andamento; o livre, nenhuma"
        );
        // clique: livre → Nova Missão com ele; ocupado sem Relatório → aba Missões
        assert_eq!(st.destino_do_olheiro(livre.id), Some(DestinoOlheiro::NovaMissao(livre.id)));
        assert_eq!(st.destino_do_olheiro(ocupado.id), Some(DestinoOlheiro::Missoes));
        assert_eq!(st.destino_do_olheiro(Uuid::new_v4()), None);
    }

    #[test]
    fn hiring_debits_the_budget_then_saves_the_olheiro_and_shows_the_reread_balance() {
        let pasta = PastaTemporaria::nova();
        let (mut st, escritas) = estado_contratacao(63_999_988, 63_699_988, Some(pasta.0.clone()), vec![]);
        st.preparar_contratacao(oferta_v1(Especializacao::Generalista, Tier::Junior));
        let previa = st.previa_contratacao().expect("modal aberto");
        assert_eq!(
            (previa.contratacao.custo(), previa.orcamento_atual, previa.orcamento_apos),
            (300_000, 63_999_988, 63_699_988)
        );
        assert_eq!(previa.faltam, None);

        assert!(st.confirmar_contratacao());
        assert_eq!(escritas_de(&escritas), [(63_999_988, 63_699_988)]);
        assert_eq!(st.previa_contratacao(), None, "modal fecha");
        assert_eq!(st.orcamento(), Some(63_699_988), "saldo relido do jogo");
        let contratados = st.olheiros_contratados();
        assert_eq!(contratados.len(), 1);
        let olheiro = &contratados[0].olheiro;
        assert_eq!((olheiro.especializacao, olheiro.tier), (Especializacao::Generalista, Tier::Junior));
        assert!(!contratados[0].em_missao, "Disponível");

        // persistido (write-through) no arquivo da carreira
        let bytes = std::fs::read(pasta.0.join(format!("{ID_A}.json"))).unwrap_or_default();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        assert_eq!(json["olheiros"][0]["id"], olheiro.id.to_string());
        assert_eq!(json["olheiros"][0]["especializacao"], "generalista");
    }

    #[test]
    fn insufficient_budget_never_writes() {
        let pasta = PastaTemporaria::nova();
        let (mut st, escritas) = estado_contratacao(100_000, 100_000, Some(pasta.0.clone()), vec![]);
        st.preparar_contratacao(oferta_v1(Especializacao::Tatico, Tier::Elite));
        assert_eq!(st.previa_contratacao().and_then(|p| p.faltam), Some(5_100_000));
        assert!(!st.confirmar_contratacao());
        assert_eq!(erro_da_contratacao(&st), Some(ErroCompra::OrcamentoInsuficiente { faltam: 5_100_000 }));
        assert!(escritas_de(&escritas).is_empty());
        assert!(st.olheiros_contratados().is_empty());
    }

    #[test]
    fn failed_or_unconfirmed_writes_hire_nobody() {
        for (resultado, esperado) in [
            (Err(SaveRepoError::ProcessoInacessivel), ErroCompra::EscritaFalhou),
            (Err(SaveRepoError::Interno("releitura".into())), ErroCompra::EscritaFalhou),
            (Err(SaveRepoError::OrcamentoMudou(60_000_000)), ErroCompra::OrcamentoMudou { atual: 60_000_000 }),
        ] {
            let pasta = PastaTemporaria::nova();
            let (mut st, escritas) =
                estado_contratacao(63_999_988, 63_999_988, Some(pasta.0.clone()), vec![resultado]);
            st.preparar_contratacao(oferta_v1(Especializacao::Generalista, Tier::Junior));
            assert!(!st.confirmar_contratacao());
            assert_eq!(erro_da_contratacao(&st), Some(esperado));
            assert_eq!(escritas_de(&escritas).len(), 1);
            assert!(st.olheiros_contratados().is_empty(), "nenhum Olheiro salvo");
            assert!(st.previa_contratacao().is_some(), "modal continua aberto para tentar de novo");
        }
    }

    #[test]
    fn without_a_writable_state_file_the_budget_is_not_touched() {
        let (mut st, escritas) = estado_contratacao(63_999_988, 63_999_988, None, vec![]);
        st.preparar_contratacao(oferta_v1(Especializacao::Generalista, Tier::Junior));
        assert!(!st.confirmar_contratacao());
        assert_eq!(erro_da_contratacao(&st), Some(ErroCompra::EstadoNaoSalvavel));
        assert!(escritas_de(&escritas).is_empty());
    }

    #[test]
    fn if_saving_the_olheiro_fails_the_debit_is_undone() {
        for (desfazer, esperado) in [
            (Ok(63_999_988), ErroCompra::NaoSalvo),
            (
                Err(SaveRepoError::ProcessoInacessivel),
                ErroCompra::DebitadoSemSalvar { debitado: 300_000 },
            ),
        ] {
            let pasta = PastaTemporaria::nova();
            let dir = pasta.0.join("scout");
            let (mut st, escritas) =
                estado_contratacao(63_999_988, 63_699_988, Some(dir.clone()), vec![Ok(63_699_988), desfazer]);
            // a pasta de estado vira um arquivo: gravar o Olheiro falha
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::write(&dir, b"x");

            st.preparar_contratacao(oferta_v1(Especializacao::Generalista, Tier::Junior));
            assert!(!st.confirmar_contratacao());
            assert_eq!(erro_da_contratacao(&st), Some(esperado));
            assert_eq!(escritas_de(&escritas), [(63_999_988, 63_699_988), (63_699_988, 63_999_988)]);
            assert!(st.olheiros_contratados().is_empty());
        }
    }

    #[test]
    fn cancelling_closes_the_preview_without_writing() {
        let (mut st, escritas) = estado_contratacao(63_999_988, 63_999_988, None, vec![]);
        st.preparar_contratacao(oferta_v1(Especializacao::Generalista, Tier::Junior));
        st.cancelar_contratacao();
        assert_eq!(st.previa_contratacao(), None);
        assert!(escritas_de(&escritas).is_empty());
    }

    /// Carreira pronta (orçamento `orcamento`, data 03/07/2026) com estes
    /// Olheiros já no arquivo; depois de comprar, a releitura devolve
    /// `orcamento_depois`.
    fn estado_com_olheiros(
        orcamento: i32,
        orcamento_depois: i32,
        olheiros: Vec<Olheiro>,
        missoes: Vec<Missao>,
        pasta: &PastaTemporaria,
    ) -> (ScoutState, Escritas) {
        let (st, escritas) = estado_contratacao(orcamento, orcamento_depois, Some(pasta.0.clone()), vec![]);
        st.estado_ativo()
            .map(|e| {
                e.mutar(move |d| {
                    d.olheiros = olheiros;
                    d.missoes = missoes;
                })
            })
            .expect("carreira ativa")
            .expect("gravou");
        (st, escritas)
    }

    fn olheiro(especializacao: Especializacao, tier: Tier) -> Olheiro {
        Olheiro { id: Uuid::new_v4(), especializacao, tier, ..Default::default() }
    }

    #[test]
    fn new_missao_form_opens_with_the_chosen_olheiro_and_his_ideal_filters() {
        let pasta = PastaTemporaria::nova();
        let ocupado = olheiro(Especializacao::Tatico, Tier::Elite);
        let livre = olheiro(Especializacao::CacadorDeJovens, Tier::Elite);
        let missoes = vec![Missao::de_teste(ocupado.id, StatusMissao::Pendente)];
        let (mut st, _) = estado_com_olheiros(63_999_988, 0, vec![ocupado.clone(), livre.clone()], missoes, &pasta);

        st.abrir_nova_missao(ocupado.id);
        assert!(!st.tem_nova_missao(), "Olheiro em Missão não abre o formulário");
        st.abrir_nova_missao(livre.id);
        let previa = st.previa_missao().expect("formulário aberto");
        assert_eq!(previa.rascunho.olheiro_id, Some(livre.id));
        assert_eq!(previa.rascunho.filtros, quality::filtros_ideais(Especializacao::CacadorDeJovens));
        assert_eq!(previa.tipo, quality::TipoMissao::Jovens, "os filtros ideais já combinam");
        assert!(previa.combina);
        assert_eq!(previa.bloqueio, None);
        let antes = previa.estimativa.expect("estimativa");

        // Completa: revela mais
        st.definir_modo_da_missao(ModoBusca::Completa);
        let previa = st.previa_missao().expect("formulário aberto");
        let completa = previa.estimativa.expect("estimativa");
        assert!(completa.atributos_revelados > antes.atributos_revelados);
        assert_eq!(completa.qualidade, Qualidade::Alta);
        assert_eq!(previa.prazo(), Some(Date(20260703).mais_dias(completa.duracao_dias)));

        // tirar a cara de "jovens" perde o bônus; "Restaurar sugestão" volta
        st.ajustar_faixa_da_missao(CampoFaixa::IdadeMax, 10);
        st.definir_nivel_da_missao(None);
        let geral = st.previa_missao().expect("formulário aberto");
        assert_eq!(geral.tipo, quality::TipoMissao::Geral);
        assert!(!geral.combina);
        st.restaurar_filtros_ideais();
        assert_eq!(st.previa_missao().map(|p| p.tipo), Some(quality::TipoMissao::Jovens));
    }

    #[test]
    fn ranges_are_clamped_and_an_inverted_range_blocks_confirmation() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, escritas) = estado_com_olheiros(63_999_988, 0, vec![o.clone()], vec![], &pasta);
        st.abrir_nova_missao(o.id);
        st.ajustar_faixa_da_missao(CampoFaixa::IdadeMin, -100);
        st.ajustar_faixa_da_missao(CampoFaixa::IdadeMax, 100);
        st.ajustar_faixa_da_missao(CampoFaixa::ContratoMax, 10);
        let f = st.previa_missao().map(|p| p.rascunho.filtros).expect("aberto");
        assert_eq!((f.idade.min, f.idade.max, f.contrato.max), (quality::IDADE_MENOR, quality::IDADE_MAIOR, quality::CONTRATO_MAIOR));
        st.ajustar_faixa_da_missao(CampoFaixa::OverallMax, 50);
        assert_eq!(st.previa_missao().map(|p| p.rascunho.filtros.overall.max), Some(99), "não passa de 99");
        st.ajustar_faixa_da_missao(CampoFaixa::OverallMin, -100);
        assert_eq!(st.previa_missao().map(|p| p.rascunho.filtros.overall.min), Some(1), "não passa de 1");

        st.ajustar_faixa_da_missao(CampoFaixa::PotencialMin, 60); // 50 -> 99... limitado
        st.ajustar_faixa_da_missao(CampoFaixa::PotencialMax, -40); // 99 -> 59
        let previa = st.previa_missao().expect("aberto");
        assert_eq!(previa.bloqueio, Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::PotencialMin }));
        assert!(!st.confirmar_nova_missao());
        assert!(escritas_de(&escritas).is_empty());
    }

    #[test]
    fn without_an_available_olheiro_confirmation_is_blocked() {
        let pasta = PastaTemporaria::nova();
        let (mut st, _) = estado_com_olheiros(63_999_988, 0, vec![], vec![], &pasta);
        st.abrir_nova_missao(Uuid::new_v4());
        assert!(!st.tem_nova_missao(), "sem Olheiro, não há formulário");
        assert_eq!(st.previa_missao(), None);
        assert!(!st.confirmar_nova_missao());
    }

    #[test]
    fn confirming_debits_and_saves_a_pending_missao_and_the_olheiro_becomes_busy() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, escritas) = estado_com_olheiros(63_999_988, 63_849_988, vec![o.clone()], vec![], &pasta);
        st.abrir_nova_missao(o.id);
        let estimativa = st.previa_missao().and_then(|p| p.estimativa).expect("estimativa");
        assert_eq!(estimativa.custo, 450_000, "Júnior, Rápida, mundo");

        assert!(st.confirmar_nova_missao());
        assert_eq!(escritas_de(&escritas), [(63_999_988, 63_999_988 - 450_000)]);
        assert!(!st.tem_nova_missao(), "o formulário fecha");
        assert_eq!(st.orcamento(), Some(63_849_988), "saldo relido do jogo");

        let missoes = st.missoes();
        assert_eq!(missoes.len(), 1);
        let (m, dono) = (&missoes[0].missao, &missoes[0].olheiro);
        assert_eq!(m.status, StatusMissao::Pendente, "nenhuma busca roda agora");
        assert_eq!(m.olheiro_id, o.id);
        assert_eq!(dono.as_ref().map(|d| d.id), Some(o.id));
        assert_eq!(m.criada_em, Date(20260703));
        assert_eq!(m.prazo_estimado, Date(20260703).mais_dias(estimativa.duracao_dias));
        assert_eq!(m.estimativa, estimativa, "guarda o que o jogador pagou");
        assert!(st.olheiros_contratados().iter().all(|c| c.em_missao), "Olheiro agora Em Missão");

        // gravado no arquivo da carreira
        let bytes = std::fs::read(pasta.0.join(format!("{ID_A}.json"))).unwrap_or_default();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        assert_eq!(json["missoes"][0]["status"], "Pendente");
        assert_eq!(json["missoes"][0]["modo_busca"], "rapida");
        assert_eq!(json["missoes"][0]["estimativa"]["custo"], 450_000);

        // o mesmo Olheiro, agora em Missão, não abre outro formulário
        st.abrir_nova_missao(o.id);
        assert!(!st.tem_nova_missao());
    }

    #[test]
    fn insufficient_budget_blocks_with_the_exact_shortfall() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, escritas) = estado_com_olheiros(100_000, 100_000, vec![o.clone()], vec![], &pasta);
        st.abrir_nova_missao(o.id);
        assert_eq!(
            st.previa_missao().and_then(|p| p.bloqueio),
            Some(BloqueioMissao::OrcamentoInsuficiente { faltam: 350_000 })
        );
        assert!(!st.confirmar_nova_missao());
        assert!(escritas_de(&escritas).is_empty());
        assert!(st.missoes().is_empty());
    }

    #[test]
    fn a_failed_debit_saves_no_missao_and_keeps_the_form_open() {
        let pasta = PastaTemporaria::nova();
        let (st, _) = estado_contratacao(63_999_988, 63_999_988, Some(pasta.0.clone()), vec![Err(SaveRepoError::ProcessoInacessivel)]);
        let mut st = st;
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let id = o.id;
        st.estado_ativo().map(|e| e.mutar(move |d| d.olheiros = vec![o])).expect("ativa").expect("gravou");
        st.abrir_nova_missao(id);
        assert!(!st.confirmar_nova_missao());
        assert_eq!(st.previa_missao().and_then(|p| p.rascunho.erro), Some(ErroCompra::EscritaFalhou));
        assert!(st.missoes().is_empty());
        assert!(st.olheiros_contratados().iter().all(|c| !c.em_missao));
    }

    #[test]
    fn progress_is_clamped_between_creation_and_deadline() {
        let (criada, prazo) = (Date(20260701), Date(20260711)); // 10 dias
        let p = |hoje| progresso_missao(criada, prazo, hoje);
        assert_eq!(p(Date(20260701)), ProgressoMissao { fracao: 0.0, dias_restantes: 10, prazo_atingido: false });
        assert_eq!(p(Date(20260706)), ProgressoMissao { fracao: 0.5, dias_restantes: 5, prazo_atingido: false });
        assert_eq!(p(Date(20260711)), ProgressoMissao { fracao: 1.0, dias_restantes: 0, prazo_atingido: true });
        // depois do prazo: nada acima de 100% nem dias negativos
        assert_eq!(p(Date(20260901)), ProgressoMissao { fracao: 1.0, dias_restantes: 0, prazo_atingido: true });
        // antes da criação (save mais antigo carregado): 0%, nada negativo
        let antes = p(Date(20260620));
        assert_eq!((antes.fracao, antes.prazo_atingido), (0.0, false));
        assert_eq!(antes.dias_restantes, 21);
        // prazo no mesmo dia da criação não divide por zero
        assert_eq!(progresso_missao(criada, criada, criada).fracao, 1.0);
        // atravessa mês/ano
        let virada = progresso_missao(Date(20281220), Date(20290109), Date(20281230));
        assert_eq!((virada.fracao, virada.dias_restantes), (0.5, 10));
    }

    #[test]
    fn progress_uses_the_date_from_when_the_panel_opened() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let mut m = Missao::de_teste(o.id, StatusMissao::Pendente);
        m.criada_em = Date(20260701);
        m.prazo_estimado = Date(20260711);
        let dia = |d| CareerSnapshot { data_atual: Date(d), ..snapshot() };
        let fonte = FonteFalsa {
            leituras: Mutex::new(vec![Ok(dia(20260706)), Ok(dia(20260709)), Ok(dia(20260709))]),
            resultado_localizacao: Ok(()),
            localizacoes: Arc::new(AtomicUsize::new(0)),
            sinal: Arc::new(Mutex::new(false)),
            resultados_escrita: Mutex::new(Vec::new()),
            escritas: Arc::new(Mutex::new(Vec::new())),
            jogadores: Arc::new(Mutex::new(Ok(pool_de_teste()))),
            buscas: Arc::new(AtomicUsize::new(0)),
            liberar_busca: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        let mut st = ScoutState::com_fonte(Box::new(fonte), Some(pasta.0.clone()));
        st.ao_abrir_painel();
        let (om, mm) = (o.clone(), m.clone());
        st.estado_ativo()
            .map(|e| e.mutar(move |d| { d.olheiros = vec![om]; d.missoes = vec![mm]; }))
            .expect("ativa")
            .expect("gravou");
        let fracao = |st: &ScoutState| st.missoes().first().and_then(|l| l.progresso).map(|p| p.fracao);
        assert_eq!(fracao(&st), Some(0.5));

        // a releitura periódica traz um dia novo, mas o progresso não anda
        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        st.tick();
        assert_eq!(st.orcamento(), Some(63_999_988));
        assert_eq!(fracao(&st), Some(0.5), "sem polling com o painel aberto");

        // reabrir o painel recalcula
        st.ao_abrir_painel();
        assert_eq!(fracao(&st), Some(0.8));
    }

    #[test]
    fn a_read_error_keeps_the_missao_list_without_progress() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = Missao::de_teste(o.id, StatusMissao::Pendente);
        let fonte = FonteFalsa {
            leituras: Mutex::new(vec![Ok(snapshot()), Err(SaveRepoError::ProcessoInacessivel)]),
            resultado_localizacao: Ok(()),
            localizacoes: Arc::new(AtomicUsize::new(0)),
            sinal: Arc::new(Mutex::new(false)),
            resultados_escrita: Mutex::new(Vec::new()),
            escritas: Arc::new(Mutex::new(Vec::new())),
            jogadores: Arc::new(Mutex::new(Ok(pool_de_teste()))),
            buscas: Arc::new(AtomicUsize::new(0)),
            liberar_busca: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        let mut st = ScoutState::com_fonte(Box::new(fonte), Some(pasta.0.clone()));
        st.ao_abrir_painel();
        let (om, mm) = (o.clone(), m.clone());
        st.estado_ativo()
            .map(|e| e.mutar(move |d| { d.olheiros = vec![om]; d.missoes = vec![mm]; }))
            .expect("ativa")
            .expect("gravou");
        assert!(st.missoes().first().is_some_and(|l| l.progresso.is_some()));

        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        st.tick();
        assert_eq!(st.status(), &CarreiraStatus::ErroLeitura);
        let lista = st.missoes();
        assert_eq!(lista.len(), 1, "a lista não some");
        assert_eq!(lista[0].missao.id, m.id);
        assert_eq!(lista[0].progresso, None, "sem data, sem progresso");
    }

    // -----------------------------------------------------------------
    // Story 2.4: busca das Missões vencidas
    // -----------------------------------------------------------------

    fn pool_de_teste() -> PlayerPool {
        pool((1..=60).map(|i| jogador(i, 55 + (i % 40) as u8, 60 + (i % 39) as u8, (i % 27) as u8)).collect())
    }

    /// Pedaços da fonte que os testes de busca controlam.
    struct Busca {
        buscas: Arc<AtomicUsize>,
        liberar: Arc<AtomicBool>,
        jogadores: Arc<Mutex<Result<PlayerPool, SaveRepoError>>>,
    }

    /// Carreira pronta em `hoje`, com um Olheiro e as `missoes` gravadas.
    fn estado_com_missoes(pasta: &PastaTemporaria, hoje: i32, missoes: Vec<Missao>, olheiro: &Olheiro) -> (ScoutState, Busca) {
        let busca = Busca {
            buscas: Arc::new(AtomicUsize::new(0)),
            liberar: Arc::new(AtomicBool::new(true)),
            jogadores: Arc::new(Mutex::new(Ok(pool_de_teste()))),
        };
        let fonte = FonteFalsa {
            leituras: Mutex::new(vec![Ok(CareerSnapshot { data_atual: Date(hoje), ..snapshot() })]),
            resultado_localizacao: Ok(()),
            localizacoes: Arc::new(AtomicUsize::new(0)),
            sinal: Arc::new(Mutex::new(false)),
            resultados_escrita: Mutex::new(Vec::new()),
            escritas: Arc::new(Mutex::new(Vec::new())),
            jogadores: Arc::clone(&busca.jogadores),
            buscas: Arc::clone(&busca.buscas),
            liberar_busca: Arc::clone(&busca.liberar),
        };
        // grava o arquivo antes de a carreira ficar pronta
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        let o = olheiro.clone();
        estado.mutar(move |d| { d.olheiros = vec![o]; d.missoes = missoes; }).expect("gravou");
        let st = ScoutState::com_fonte(Box::new(fonte), Some(pasta.0.clone()));
        (st, busca)
    }

    fn missao_com_prazo(olheiro: &Olheiro, criada: i32, prazo: i32) -> Missao {
        let mut m = Missao::de_teste(olheiro.id, StatusMissao::Pendente);
        m.criada_em = Date(criada);
        m.prazo_estimado = Date(prazo);
        m
    }

    fn status_de(st: &ScoutState, id: Uuid) -> Option<StatusMissao> {
        st.estado_ativo()?.ler(|d| d.missoes.iter().find(|m| m.id == id).map(|m| m.status))
    }

    /// `tick` até a fila esvaziar e a última busca ser tratada.
    fn ticks_ate_buscar(st: &mut ScoutState) {
        let inicio = Instant::now();
        loop {
            st.tick();
            if st.fila_busca.is_empty() && st.busca_atual.is_none() {
                return;
            }
            assert!(inicio.elapsed() < Duration::from_secs(5), "busca falsa travou");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn a_due_missao_runs_on_panel_open_and_produces_a_saved_report() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let vencida = missao_com_prazo(&o, 20260701, 20260710);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![vencida.clone()], &o);
        busca.liberar.store(false, Ordering::SeqCst);

        st.ao_abrir_painel();
        // EmExecucao gravado ANTES de a busca rodar (AD-8)
        assert_eq!(status_de(&st, vencida.id), Some(StatusMissao::EmExecucao));
        assert!(st.olheiros_contratados()[0].em_missao);
        st.tick();
        assert!(matches!(st.tarefa_busca.poll(), TaskState::Running));

        // fechar e reabrir com a busca rodando não despacha de novo
        st.ao_fechar_painel();
        st.ao_abrir_painel();
        st.ao_fechar_painel();
        busca.liberar.store(true, Ordering::SeqCst);
        ticks_ate_buscar(&mut st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 1, "uma busca só");

        // concluiu com o painel FECHADO: Relatório gravado, Olheiro livre
        assert_eq!(status_de(&st, vencida.id), Some(StatusMissao::Concluida));
        assert!(!st.olheiros_contratados()[0].em_missao);
        let relatorios = st.estado_ativo().map(|e| e.ler(|d| d.relatorios.clone())).unwrap_or_default();
        assert_eq!(relatorios.len(), 1);
        assert_eq!(relatorios[0].missao_id, vencida.id);
        assert_eq!(relatorios[0].jogadores.len(), usize::from(vencida.estimativa.alvo_jogadores));
        assert!(!relatorios[0].aberto);
        assert!(matches!(st.aviso_visivel(Instant::now()), Some(TipoAviso::RelatorioPronto { .. })));

        // reabrir depois de concluída não roda nada
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 1);
        // e o Relatório sobrevive a reiniciar o jogo
        let relido = EstadoPersistido::carregar(Some(&pasta.0), ID_A).ler(|d| d.relatorios.clone());
        assert_eq!(relido, relatorios);
    }

    #[test]
    fn a_missao_before_its_deadline_searches_early_and_stays_pending() {
        // Story 2.10 mudou o AD-8: a busca roda na primeira abertura depois
        // de criada, para o Relatório parcial existir.
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = missao_com_prazo(&o, 20260701, 20260720);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![m.clone()], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Pendente));
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 1);
        assert!(st.olheiros_contratados()[0].em_missao, "o Olheiro segue na Missão");
        // reabrir não busca de novo
        st.ao_fechar_painel();
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn due_missoes_run_one_at_a_time_in_deadline_then_creation_order() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let tardia = missao_com_prazo(&o, 20260601, 20260710);
        let cedo_b = missao_com_prazo(&o, 20260605, 20260705);
        let cedo_a = missao_com_prazo(&o, 20260602, 20260705);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![tardia.clone(), cedo_b.clone(), cedo_a.clone()], &o);
        st.ao_abrir_painel();
        let ordem: Vec<Uuid> = st.fila_busca.iter().map(|b| b.missao.id).collect();
        assert_eq!(ordem, vec![cedo_a.id, cedo_b.id, tardia.id]);
        ticks_ate_buscar(&mut st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 3);
        let relatorios: Vec<Uuid> = st.estado_ativo().map(|e| e.ler(|d| d.relatorios.iter().map(|r| r.missao_id).collect())).unwrap_or_default();
        assert_eq!(relatorios, vec![cedo_a.id, cedo_b.id, tardia.id], "uma de cada vez, na ordem da fila");
    }

    #[test]
    fn a_failed_search_goes_back_to_pendente_and_says_so() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = missao_com_prazo(&o, 20260701, 20260710);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![m.clone()], &o);
        *busca.jogadores.lock().unwrap_or_else(|p| p.into_inner()) = Err(SaveRepoError::ProcessoInacessivel);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Pendente));
        assert_eq!(st.missoes()[0].falha.as_deref(), Some("Não foi possível ler o save ativo."));
        assert!(matches!(st.aviso_visivel(Instant::now()), Some(TipoAviso::BuscaFalhou)));

        // na próxima abertura roda de novo (e agora funciona)
        *busca.jogadores.lock().unwrap_or_else(|p| p.into_inner()) = Ok(pool_de_teste());
        st.ao_fechar_painel();
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Concluida));
        assert_eq!(st.missoes()[0].falha, None);
    }

    #[test]
    fn a_missao_stuck_in_execution_is_reset_when_the_state_loads() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let mut presa = missao_com_prazo(&o, 20260701, 20260720);
        presa.status = StatusMissao::EmExecucao;
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![presa.clone()], &o);
        // ao carregar o arquivo ela volta a Pendente; a abertura do painel
        // (mesmo frame) a despacha de novo, uma vez
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 1);
        assert_eq!(status_de(&st, presa.id), Some(StatusMissao::Pendente));
    }

    #[test]
    fn opening_a_report_clears_new_and_persists() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = missao_com_prazo(&o, 20260701, 20260710);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, vec![m.clone()], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        let linha = st.missoes().into_iter().next().expect("missão");
        assert!(linha.relatorio_novo);
        let id = linha.relatorio_id.expect("relatório");
        let lista = st.relatorios(false);
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].missao.as_ref().map(|x| x.id), Some(m.id));
        assert_eq!(lista[0].olheiro.as_ref().map(|x| x.id), Some(o.id));
        assert!(st.relatorios(true).is_empty());

        st.abrir_relatorio(id);
        assert_eq!(st.relatorio_aberto().map(|r| r.relatorio.id), Some(id));
        assert!(!st.missoes()[0].relatorio_novo);
        let relido = EstadoPersistido::carregar(Some(&pasta.0), ID_A).ler(|d| d.relatorios[0].aberto);
        assert!(relido, "o \"novo\" some também depois de reiniciar");

        // fechar o painel fecha a tela do Relatório
        st.ao_fechar_painel();
        assert_eq!(st.relatorio_aberto(), None);
        // id que não existe não abre nada
        st.abrir_relatorio(Uuid::new_v4());
        assert_eq!(st.relatorio_aberto(), None);
    }

    #[test]
    fn opened_reports_can_be_archived_and_restored_never_deleted() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = missao_com_prazo(&o, 20260701, 20260710);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, vec![m.clone()], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        let id = st.relatorios(false)[0].relatorio.id;

        // ainda não aberto: Arquivar não é oferecido nem aceito
        assert!(!ScoutState::pode_arquivar(&st.relatorios(false)[0]));
        assert!(!st.arquivar_relatorio(id));

        st.abrir_relatorio(id);
        st.fechar_relatorio();
        assert!(ScoutState::pode_arquivar(&st.relatorios(false)[0]));
        assert!(st.arquivar_relatorio(id));
        assert!(st.relatorios(false).is_empty());
        assert_eq!(st.relatorios(true).len(), 1);
        assert!(st.missoes().is_empty(), "a Missão sai da aba Missões com o Relatório arquivado");
        let arquivado = EstadoPersistido::carregar(Some(&pasta.0), ID_A).ler(|d| (d.relatorios.len(), d.relatorios[0].arquivado));
        assert_eq!(arquivado, (1, true), "nunca apagado");

        assert!(st.restaurar_relatorio(id));
        assert_eq!(st.relatorios(false).len(), 1);
        assert!(st.relatorios(true).is_empty());
        assert_eq!(st.missoes().len(), 1);
        assert!(!st.restaurar_relatorio(id), "já está na lista principal");
    }

    /// Espera as ligas carregarem em background.
    fn esperar_ligas(st: &ScoutState) -> Arc<Vec<Liga>> {
        let inicio = Instant::now();
        loop {
            match st.listar_ligas() {
                Carga::Pronto(ligas) => return ligas,
                Carga::Erro => panic!("ligas falsas falharam"),
                Carga::Carregando => {
                    assert!(inicio.elapsed() < Duration::from_secs(5), "ligas falsas travaram");
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }

    #[test]
    fn choosing_leagues_countries_and_continents_changes_breadth_and_the_estimate_live() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &o);
        st.ao_abrir_painel();
        assert_eq!(esperar_ligas(&st).len(), 4, "ligas do pool de teste");
        st.abrir_nova_missao(o.id);
        let mundo = st.previa_missao().expect("formulário");
        assert_eq!(mundo.amplitude, quality::AmplitudeGeografica::Mundo);

        st.alternar_liga_da_missao(13); // Premier League (England)
        let pais = st.previa_missao().expect("formulário");
        assert_eq!(pais.amplitude, quality::AmplitudeGeografica::Pais);
        let (m, p) = (mundo.estimativa.expect("estimativa"), pais.estimativa.expect("estimativa"));
        assert!(p.precisao_mais_menos < m.precisao_mais_menos, "mais estreito = mais preciso");
        st.alternar_pais_do_clube_da_missao(45); // Spain inteira
        assert_eq!(st.previa_missao().map(|x| x.amplitude), Some(quality::AmplitudeGeografica::VariosPaises));

        // o país inteiro engole a liga dele; liga incluída não sai sozinha
        st.alternar_pais_do_clube_da_missao(14);
        let f = st.previa_missao().map(|x| x.rascunho.filtros).expect("formulário");
        assert_eq!((f.paises_dos_clubes.clone(), f.ligas.clone()), (vec![45, 14], vec![]));
        st.alternar_liga_da_missao(13);
        assert_eq!(st.previa_missao().map(|x| x.rascunho.filtros.ligas), Some(vec![]), "incluída pelo país: nada muda");

        // o continente engole os países dele
        st.alternar_continente_da_missao(Confederacao::Europa);
        let f = st.previa_missao().map(|x| x.rascunho.filtros).expect("formulário");
        assert_eq!((f.continentes.clone(), f.paises_dos_clubes.clone()), (vec![Confederacao::Europa], vec![]));
        assert_eq!(st.previa_missao().map(|x| x.amplitude), Some(quality::AmplitudeGeografica::Continente));
        st.alternar_pais_do_clube_da_missao(54); // Brazil: outro continente
        assert_eq!(st.previa_missao().map(|x| x.amplitude), Some(quality::AmplitudeGeografica::Mundo));
        st.limpar_geografia_da_missao();
        let f = st.previa_missao().map(|x| x.rascunho.filtros).expect("formulário");
        assert!(!f.tem_geografia());
    }

    #[test]
    fn up_to_three_dominant_attributes_toggle_on_and_off() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &o);
        st.ao_abrir_painel();
        st.abrir_nova_missao(o.id);
        for a in [Atributo::Velocidade, Atributo::Drible, Atributo::Finalizacao, Atributo::Forca] {
            st.alternar_atributo_da_missao(a);
        }
        let lista = st.previa_missao().map(|p| p.rascunho.filtros.atributos_dominantes).expect("formulário");
        assert_eq!(lista, vec![Atributo::Velocidade, Atributo::Drible, Atributo::Finalizacao], "o 4º não entra");
        assert_eq!(st.previa_missao().map(|p| p.tipo), Some(quality::TipoMissao::Tatica));
        st.alternar_atributo_da_missao(Atributo::Drible);
        assert_eq!(
            st.previa_missao().map(|p| p.rascunho.filtros.atributos_dominantes),
            Some(vec![Atributo::Velocidade, Atributo::Finalizacao])
        );
        st.limpar_atributos_da_missao();
        assert_eq!(st.previa_missao().map(|p| p.rascunho.filtros.atributos_dominantes), Some(vec![]));
    }

    #[test]
    fn missions_saved_with_one_dominant_attribute_still_load() {
        let antigo = r#"{"overall":{"min":50,"max":99},"potencial":{"min":50,"max":99},"atributo_dominante":"drible"}"#;
        let f: FiltrosMissao = serde_json::from_str(antigo).expect("filtro antigo");
        assert_eq!(f.atributos_dominantes, vec![Atributo::Drible]);
        assert_eq!((f.idade, f.contrato), (FiltrosMissao::default().idade, FiltrosMissao::default().contrato));
        let nulo = r#"{"overall":{"min":50,"max":99},"potencial":{"min":50,"max":99},"atributo_dominante":null}"#;
        assert!(serde_json::from_str::<FiltrosMissao>(nulo).expect("nulo").atributos_dominantes.is_empty());
        let json = serde_json::to_string(&f).expect("serializa");
        assert!(json.contains("\"atributos_dominantes\":[\"drible\"]"), "{json}");
    }

    // -----------------------------------------------------------------
    // Story 2.10: Relatório parcial, Missão contínua, aviso
    // -----------------------------------------------------------------

    /// Carreira que vai lendo `datas` (uma por leitura; a última se repete).
    fn estado_com_datas(pasta: &PastaTemporaria, datas: &[i32], missoes: Vec<Missao>, olheiro: &Olheiro) -> (ScoutState, Busca, Escritas) {
        let busca = Busca {
            buscas: Arc::new(AtomicUsize::new(0)),
            liberar: Arc::new(AtomicBool::new(true)),
            jogadores: Arc::new(Mutex::new(Ok(pool_de_teste()))),
        };
        let escritas: Escritas = Arc::new(Mutex::new(Vec::new()));
        let fonte = FonteFalsa {
            leituras: Mutex::new(datas.iter().map(|&d| Ok(CareerSnapshot { data_atual: Date(d), ..snapshot() })).collect()),
            resultado_localizacao: Ok(()),
            localizacoes: Arc::new(AtomicUsize::new(0)),
            sinal: Arc::new(Mutex::new(false)),
            resultados_escrita: Mutex::new(Vec::new()),
            escritas: Arc::clone(&escritas),
            jogadores: Arc::clone(&busca.jogadores),
            buscas: Arc::clone(&busca.buscas),
            liberar_busca: Arc::clone(&busca.liberar),
        };
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        let o = olheiro.clone();
        estado.mutar(move |d| { d.olheiros = vec![o]; d.missoes = missoes; }).expect("gravou");
        (ScoutState::com_fonte(Box::new(fonte), Some(pasta.0.clone())), busca, escritas)
    }

    fn reabrir(st: &mut ScoutState) {
        st.ao_fechar_painel();
        st.ao_abrir_painel();
        ticks_ate_buscar(st);
    }

    #[test]
    fn a_partial_report_grows_with_the_career_days_until_the_deadline() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = missao_com_prazo(&o, 20260701, 20260711);
        let alvo = m.alvo_total();
        let (mut st, _busca, _) = estado_com_datas(&pasta, &[20260706, 20260709, 20260711], vec![m.clone()], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);

        // metade do caminho: metade dos jogadores (arredondado para cima)
        let linha = st.missoes().into_iter().next().expect("missão");
        assert_eq!(linha.revelados, alvo.div_ceil(2));
        assert_eq!(linha.previstos, alvo);
        assert!(linha.relatorio_novo);
        let id = linha.relatorio_id.expect("Relatório parcial");
        st.abrir_relatorio(id);
        let aberto = st.relatorio_aberto().expect("aberto");
        assert!(aberto.parcial);
        assert_eq!(aberto.relatorio.jogadores.len(), alvo.div_ceil(2), "só os revelados");
        let primeiros: Vec<u32> = aberto.relatorio.jogadores.iter().map(|j| j.player_id).collect();
        assert!(!st.missoes()[0].relatorio_novo, "visto");

        // dias depois: aparecem mais, os primeiros continuam lá, o "novo" volta
        reabrir(&mut st);
        let linha = st.missoes().into_iter().next().expect("missão");
        assert_eq!(linha.revelados, (alvo * 8).div_ceil(10));
        assert!(linha.relatorio_novo);
        st.abrir_relatorio(id);
        let depois: Vec<u32> = st.relatorio_aberto().expect("aberto").relatorio.jogadores.iter().map(|j| j.player_id).collect();
        assert_eq!(&depois[..primeiros.len()], &primeiros[..]);

        // prazo: Concluída com todos, Olheiro livre
        reabrir(&mut st);
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Concluida));
        assert_eq!(st.missoes()[0].revelados, alvo);
        assert!(!st.olheiros_contratados()[0].em_missao);
    }

    #[test]
    fn a_continuous_missao_renews_by_explicit_purchase_and_can_be_ended() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let mut m = missao_com_prazo(&o, 20260701, 20260731);
        m.continua = true;
        let alvo = m.alvo_total();
        let (mut st, busca, escritas) = estado_com_datas(&pasta, &[20260801, 20260816], vec![m.clone()], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);

        // fim do bloco: segue Pendente (sem cobrança automática), todos do bloco à mostra
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Pendente));
        let atual = st.estado_ativo().and_then(|e| e.ler(|d| d.missoes.first().cloned())).expect("missão");
        assert!(st.bloco_encerrado(&atual));
        assert_eq!(st.missoes()[0].revelados, alvo);
        assert!(escritas_de(&escritas).is_empty(), "nada cobrado sem confirmar");

        // renovar = compra confirmada; o bloco novo busca mais jogadores
        assert!(st.renovar_missao(m.id));
        assert_eq!(escritas_de(&escritas), vec![(63_999_988, 63_999_988 - m.estimativa.custo)]);
        ticks_ate_buscar(&mut st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 2);
        let (blocos, prazo, encontrados) = st
            .estado_ativo()
            .map(|e| e.ler(|d| (d.missoes[0].blocos, d.missoes[0].prazo_estimado, d.relatorios[0].jogadores.len())))
            .expect("estado");
        assert_eq!((blocos, prazo), (2, Date(20260831)));
        assert_eq!(encontrados, 2 * alvo, "sem repetir jogadores");
        let ids: std::collections::HashSet<u32> =
            st.estado_ativo().map(|e| e.ler(|d| d.relatorios[0].jogadores.iter().map(|j| j.player_id).collect())).unwrap_or_default();
        assert_eq!(ids.len(), 2 * alvo);

        // no meio do segundo bloco, encerrar: fica só o que já apareceu
        reabrir(&mut st);
        let revelados = st.missoes()[0].revelados;
        assert!(revelados > alvo && revelados < 2 * alvo, "{revelados}");
        assert!(st.encerrar_missao(m.id));
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Concluida));
        assert!(!st.olheiros_contratados()[0].em_missao);
        let final_ = st.estado_ativo().map(|e| e.ler(|d| d.relatorios[0].jogadores.len())).expect("estado");
        assert_eq!(final_, revelados);
        assert!(!st.renovar_missao(m.id), "encerrada não renova");
    }

    #[test]
    fn new_players_are_announced_on_the_banner_with_the_panel_closed() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = missao_com_prazo(&o, 20260701, 20260711);
        let (mut st, _busca, _) = estado_com_datas(&pasta, &[20260702, 20260702, 20260706], vec![m.clone()], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        st.abrir_relatorio(st.missoes()[0].relatorio_id.expect("parcial"));
        st.ao_fechar_painel();

        // mesma data: nada a anunciar
        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        st.aviso = None;
        st.tick();
        assert_eq!(st.aviso_visivel(Instant::now()), None);

        // dias depois, painel fechado: banner com os novos
        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        st.tick();
        let alvo = m.alvo_total();
        let antes = crate::scout::quality::revelados(0.1, alvo, alvo);
        match st.aviso_visivel(Instant::now()) {
            Some(TipoAviso::RelatorioAtualizado { novos, .. }) => assert_eq!(*novos, alvo.div_ceil(2) - antes),
            outro => panic!("esperava o aviso de Relatório atualizado, veio {outro:?}"),
        }
        // anunciado uma vez só
        st.aviso = None;
        st.data_avisos = None;
        st.ultima_leitura = Some(Instant::now() - INTERVALO_RELEITURA * 2);
        st.tick();
        assert_eq!(st.aviso_visivel(Instant::now()), None);
    }

    // -----------------------------------------------------------------
    // Épico 3: elenco, Jogador de Referência, Fit Posicional, Ficha
    // -----------------------------------------------------------------

    /// `pool_de_teste` com os jogadores 1, 2 e 3 no clube do técnico.
    fn pool_com_elenco() -> PlayerPool {
        let mut p = pool_de_teste();
        for j in p.jogadores.iter_mut().take(3) {
            j.clube_id = Some(241);
        }
        p
    }

    fn trocar_jogadores(busca: &Busca, pool: Result<PlayerPool, SaveRepoError>) {
        *busca.jogadores.lock().unwrap_or_else(|p| p.into_inner()) = pool;
    }

    /// Espera a leitura do elenco terminar.
    fn esperar_elenco(st: &ScoutState) -> EstadoElenco {
        let inicio = Instant::now();
        loop {
            match st.listar_elenco_atual() {
                EstadoElenco::Carregando => {
                    assert!(inicio.elapsed() < Duration::from_secs(5), "elenco falso travou");
                    std::thread::sleep(Duration::from_millis(2));
                }
                pronto_ou_erro => return pronto_ou_erro,
            }
        }
    }

    fn elenco_pronto(st: &ScoutState) -> Arc<Vec<JogadorElenco>> {
        match esperar_elenco(st) {
            EstadoElenco::Pronto(elenco) => elenco,
            outro => panic!("elenco não ficou pronto: {outro:?}"),
        }
    }

    #[test]
    fn the_squad_is_read_in_background_once_per_panel_opening() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Tatico, Tier::Experiente);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![], &o);
        trocar_jogadores(&busca, Ok(pool_com_elenco()));
        st.ao_abrir_painel();
        assert_eq!(st.listar_elenco_atual(), EstadoElenco::Carregando, "nunca lê no thread de render");
        let elenco = elenco_pronto(&st);
        let ids: Vec<u32> = elenco.iter().map(|j| j.player_id).collect();
        assert_eq!(ids.len(), 3);
        assert!(elenco.iter().all(|j| j.atributos.len() == 33));
        // por função (posições 1, 2, 3 são todas de defesa), depois Overall
        assert!(elenco.windows(2).all(|par| par[0].overall >= par[1].overall));
        let lidas = busca.buscas.load(Ordering::SeqCst);
        elenco_pronto(&st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), lidas, "guardado durante a abertura");
        st.ao_fechar_painel();
        st.ao_abrir_painel();
        elenco_pronto(&st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), lidas + 1, "relido na abertura seguinte");
    }

    #[test]
    fn a_squad_read_failure_shows_an_error_and_retry_reads_again() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Tatico, Tier::Experiente);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![], &o);
        trocar_jogadores(&busca, Err(SaveRepoError::ProcessoInacessivel));
        st.ao_abrir_painel();
        assert_eq!(esperar_elenco(&st), EstadoElenco::Erro);
        trocar_jogadores(&busca, Ok(pool_com_elenco()));
        st.reler_elenco();
        assert_eq!(elenco_pronto(&st).len(), 3);
    }

    #[test]
    fn reference_and_fit_make_a_tactical_missao_and_are_saved_with_it() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Tatico, Tier::Experiente);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![], &o);
        trocar_jogadores(&busca, Ok(pool_com_elenco()));
        st.ao_abrir_painel();
        st.abrir_nova_missao(o.id);
        // o Tático já abre com atributos dominantes; sem eles, Missão Geral
        st.limpar_atributos_da_missao();
        let antes = st.previa_missao().expect("formulário");
        assert_ne!(antes.tipo, quality::TipoMissao::Tatica);
        elenco_pronto(&st);

        st.definir_referencia_da_missao(Some(2));
        st.definir_fit_da_missao(Some(PosicaoAlvo::Volante));
        let previa = st.previa_missao().expect("formulário");
        assert_eq!(previa.tipo, quality::TipoMissao::Tatica);
        assert!(previa.combina, "Tático combina com Missão de perfil");
        let referencia = previa.rascunho.filtros.referencia.clone().expect("referência");
        assert_eq!((referencia.player_id, referencia.nome.as_str()), (2, "Jogador 2"));
        assert_eq!(referencia.atributos.len(), 33, "foto dos atributos");
        // fora do elenco: ignorado; "Nenhum" tira
        st.definir_referencia_da_missao(Some(999));
        assert_eq!(st.previa_missao().and_then(|p| p.rascunho.filtros.referencia).map(|r| r.player_id), Some(2));
        st.definir_referencia_da_missao(None);
        assert_eq!(st.previa_missao().and_then(|p| p.rascunho.filtros.referencia), None);
        st.definir_referencia_da_missao(Some(2));

        assert!(st.confirmar_nova_missao());
        let salva = st.estado_ativo().and_then(|e| e.ler(|d| d.missoes.first().cloned())).expect("Missão salva");
        assert_eq!(salva.tipo, quality::TipoMissao::Tatica);
        assert_eq!(salva.filtros.fit_posicional, Some(PosicaoAlvo::Volante));
        assert_eq!(salva.filtros.referencia.map(|r| r.player_id), Some(2));
        let relida = EstadoPersistido::carregar(Some(&pasta.0), ID_A).ler(|d| d.missoes.first().cloned());
        assert_eq!(relida.and_then(|m| m.filtros.referencia).map(|r| r.nome), Some("Jogador 2".to_string()));
    }

    #[test]
    fn the_ficha_opens_from_the_open_report_and_overlays_a_squad_player() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let vencida = missao_com_prazo(&o, 20260701, 20260710);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![vencida], &o);
        trocar_jogadores(&busca, Ok(pool_com_elenco()));
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        let item = st.relatorios(false).into_iter().next().expect("Relatório");
        let primeiro = item.relatorio.jogadores.first().map(|j| j.player_id).expect("jogador");
        assert!(item.relatorio.jogadores.iter().all(|j| j.pe.is_some()), "pé preferido no Relatório novo");

        // sem Relatório aberto, não há Ficha
        st.abrir_ficha(primeiro);
        assert_eq!(st.ficha_aberta(), None);
        st.abrir_relatorio(item.relatorio.id);
        st.abrir_ficha(999_999);
        assert_eq!(st.ficha_aberta(), None, "jogador fora do Relatório");
        st.abrir_ficha(primeiro);
        let ficha = st.ficha_aberta().expect("Ficha");
        assert_eq!(ficha.jogador.player_id, primeiro);
        assert_eq!(ficha.comparacao, None);

        elenco_pronto(&st);
        st.comparar_com(Some(1));
        assert_eq!(st.ficha_aberta().and_then(|f| f.comparacao).map(|c| c.player_id), Some(1));
        st.comparar_com(None);
        assert_eq!(st.ficha_aberta().and_then(|f| f.comparacao), None);

        // abrir outra Ficha começa sem comparação; fechar o Relatório fecha a Ficha
        st.comparar_com(Some(1));
        st.abrir_ficha(primeiro);
        assert_eq!(st.ficha_aberta().and_then(|f| f.comparacao), None);
        st.fechar_relatorio();
        assert_eq!(st.ficha_aberta(), None);
    }

    #[test]
    fn reports_and_filters_saved_before_epic_3_still_load() {
        let jogador: JogadorEncontrado = serde_json::from_str(
            r#"{"player_id":1,"nome":"A","idade":20,"posicao":24,"nacao_id":54,"nacao":"Brazil","clube":"C",
                "overall":{"min":70,"max":74},"potencial":{"min":80,"max":84},"atributos":[]}"#,
        )
        .expect("Relatório antigo");
        assert_eq!((jogador.pe, jogador.similaridade, jogador.fit), (None, None, None));
        let filtros: FiltrosMissao =
            serde_json::from_str(r#"{"overall":{"min":50,"max":99},"potencial":{"min":50,"max":99}}"#).expect("filtros antigos");
        assert_eq!((filtros.fit_posicional, filtros.referencia), (None, None));
        let json = serde_json::to_string(&FiltrosMissao { fit_posicional: Some(PosicaoAlvo::MeiaAtacante), ..FiltrosMissao::default() })
            .expect("serializa");
        assert!(json.contains("\"fit_posicional\":\"meia_atacante\""), "{json}");
    }

    #[test]
    fn loading_an_older_save_rolls_the_scout_back_to_its_date() {
        let pasta = PastaTemporaria::nova();
        // feito ANTES do save (02/07): fica
        let mut antigo = olheiro(Especializacao::Generalista, Tier::Junior);
        antigo.contratado_em = Some(Date(20260702));
        let mut missao_antiga = missao_com_prazo(&antigo, 20260702, 20260712);
        missao_antiga.status = StatusMissao::Concluida;
        missao_antiga.blocos_buscados = 1;
        let mut continua = missao_com_prazo(&antigo, 20260601, 20260731);
        continua.continua = true;
        continua.blocos = 2;
        continua.blocos_buscados = 2;
        continua.renovacoes = vec![(Date(20260705), Date(20260701))];
        // feito DEPOIS do save: sai
        let mut novo = olheiro(Especializacao::Tatico, Tier::Elite);
        novo.contratado_em = Some(Date(20260706));
        let missao_nova = missao_com_prazo(&novo, 20260706, 20260716);
        let (missao_antiga_id, nova_id, continua_id) = (missao_antiga.id, missao_nova.id, continua.id);

        // o jogo recarregou o save de 03/07 (a data viva ainda mostra 02/07)
        let (mut st, _busca, _) = estado_com_datas(&pasta, &[20260702], vec![missao_antiga, continua, missao_nova], &antigo);
        let (n, r1, r2) = (novo.clone(), Relatorio::de_teste(missao_antiga_id), Relatorio::de_teste(nova_id));
        EstadoPersistido::carregar(Some(&pasta.0), ID_A)
            .mutar(move |d| {
                d.olheiros.push(n);
                d.relatorios = vec![r1, r2];
            })
            .expect("gravou");
        st.ao_abrir_painel();

        let (olheiros, missoes, relatorios) = st
            .estado_ativo()
            .map(|e| e.ler(|d| (d.olheiros.clone(), d.missoes.clone(), d.relatorios.clone())))
            .expect("estado");
        assert_eq!(olheiros.iter().map(|o| o.id).collect::<Vec<_>>(), vec![antigo.id], "o Olheiro de 06/07 saiu");
        assert!(missoes.iter().all(|m| m.id != nova_id), "a MissÃ£o de 06/07 saiu");
        assert!(relatorios.iter().all(|r| r.missao_id != nova_id), "com o RelatÃ³rio");
        let antiga = missoes.iter().find(|m| m.id == missao_antiga_id).expect("antiga");
        assert_eq!(antiga.status, StatusMissao::Pendente, "concluÃ­da em 12/07 volta a correr");
        let c = missoes.iter().find(|m| m.id == continua_id).expect("contÃ­nua");
        assert_eq!((c.blocos, c.prazo_estimado, c.renovacoes.len()), (1, Date(20260701), 0), "renovaÃ§Ã£o de 05/07 desfeita");
        assert!(matches!(st.aviso_visivel(Instant::now()), Some(TipoAviso::VoltouNoTempo { desfeitos: 4, .. })));

        // nada mais a desfazer: reativar nÃ£o mexe
        assert_eq!(st.voltar_no_tempo(Date(20260703)), 0);
    }

    // -----------------------------------------------------------------
    // Épico 5: Olheiros com mercado e foco; Épico 6: Lista de Escolhidos
    // -----------------------------------------------------------------

    /// Um Olheiro novo (Épico 5): perfil em estrelas e mercado no Brasil.
    fn olheiro_brasileiro(foco: Especializacao, meias: u8) -> Olheiro {
        let e = quality::Estrelas(meias);
        let mut perfil = quality::PerfilOlheiro { jovens: e.menos(3), medalhoes: e.menos(3), tatico: e.menos(3), generalista: e.menos(3), rede: quality::Estrelas(6) };
        match foco {
            Especializacao::CacadorDeJovens => perfil.jovens = e,
            Especializacao::CacadorDeMedalhoes => perfil.medalhoes = e,
            Especializacao::Tatico => perfil.tatico = e,
            Especializacao::Generalista => perfil.generalista = e,
        }
        Olheiro {
            id: Uuid::new_v4(),
            especializacao: perfil.foco(),
            tier: perfil.tier(),
            nome: "Paulo Medeiros".to_string(),
            nacao: Some(NacaoOlheiro { id: 54, nome: "Brazil".to_string(), continente: Confederacao::AmericaDoSul }),
            perfil: Some(perfil),
            mercados: vec![quality::Mercado::Pais { id: 54, continente: Confederacao::AmericaDoSul }],
            ..Default::default()
        }
    }

    #[test]
    fn the_preview_charges_market_distance_until_the_olheiro_adapts_and_offers_three_budgets() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro_brasileiro(Especializacao::Generalista, 7);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &o);
        st.ao_abrir_painel();
        esperar_ligas(&st);
        st.abrir_nova_missao(o.id);
        // Brasileirão (liga 7): o mercado dele
        st.alternar_liga_da_missao(7);
        let em_casa = st.previa_missao().expect("formulário");
        assert_eq!((em_casa.distancia_mercado, em_casa.penalidade), (0, quality::Penalidade::default()));
        // Premier League (liga 13, Inglaterra): outro continente
        st.limpar_geografia_da_missao();
        st.alternar_liga_da_missao(13);
        let longe = st.previa_missao().expect("formulário");
        assert_eq!(longe.distancia_mercado, 2);
        assert_eq!(longe.penalidade, quality::Penalidade { qualidade: 2, velocidade: 4 });
        let (e_casa, e_longe) = (em_casa.estimativa.expect("estimativa"), longe.estimativa.expect("estimativa"));
        assert!(e_longe.duracao_dias > e_casa.duracao_dias);
        assert!(e_longe.atributos_revelados < e_casa.atributos_revelados || e_longe.qualidade < e_casa.qualidade);
        // três verbas, do mais barato ao mais caro; a escolhida vai para a Missão
        let custos: Vec<i32> = longe.custos_por_verba.iter().map(|(_, c)| *c).collect();
        assert_eq!(custos.len(), 3);
        assert!(custos[0] < custos[1] && custos[1] < custos[2], "{custos:?}");
        st.definir_investimento_da_missao(quality::Investimento::Reforcada);
        let reforcada = st.previa_missao().expect("formulário");
        assert_eq!(reforcada.estimativa.map(|e| e.custo), Some(custos[2]));
        assert!(st.confirmar_nova_missao());
        let gravada = st.estado_ativo().map(|e| e.ler(|d| d.missoes[0].clone())).expect("missão");
        assert_eq!(gravada.investimento, quality::Investimento::Reforcada);

        // seis meses depois, com a Missão inglesa no currículo, a penalidade some
        st.data_progresso = Some(Date(20270115));
        let livre = st.estado_ativo().cloned().expect("estado");
        livre.mutar(|d| d.missoes[0].status = StatusMissao::Concluida).expect("gravou");
        livre.mutar(|d| d.missoes[0].prazo_estimado = Date(20270110)).expect("gravou");
        let (penalidade, distancia) = st.penalidade_para(&o, &gravada.filtros, gravada.tipo, Date(20270115));
        assert_eq!(distancia, 2, "o mercado de origem não muda");
        assert_eq!(penalidade, quality::Penalidade::default(), "adaptado");
    }

    #[test]
    fn a_specialist_out_of_focus_is_warned_in_the_preview() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro_brasileiro(Especializacao::CacadorDeJovens, 7);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &o);
        st.ao_abrir_painel();
        esperar_ligas(&st);
        st.abrir_nova_missao(o.id);
        st.alternar_liga_da_missao(7);
        assert!(!st.previa_missao().expect("formulário").fora_do_foco, "abre com os filtros dele (Jovens)");
        st.aplicar_atalho_da_missao(Atalho::MudaPatamar);
        let previa = st.previa_missao().expect("formulário");
        assert_eq!(previa.tipo, quality::TipoMissao::Medalhoes);
        assert!(previa.fora_do_foco);
        assert_eq!(previa.penalidade.qualidade, 2, "1 estrela a menos até se habituar");
    }

    /// `tick` até a atualização dos Escolhidos terminar.
    fn ticks_ate_acompanhar(st: &mut ScoutState) {
        let inicio = Instant::now();
        loop {
            st.tick();
            if !st.atualizacao_escolhidos_pendente {
                return;
            }
            assert!(inicio.elapsed() < Duration::from_secs(5), "acompanhamento falso travou");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn escolhido_de_teste(player_id: u32, observado: i32) -> Escolhido {
        let mut jogador = search::revelar(
            &Missao::de_teste(Uuid::new_v4(), StatusMissao::Concluida),
            &pool_de_teste(),
            Date(observado),
            pool_de_teste().jogadores.iter().find(|j| j.player_id == player_id).expect("jogador do pool"),
        );
        jogador.atributos.truncate(6);
        Escolhido {
            jogador,
            adicionado_em: Date(observado),
            observado_em: Date(observado),
            precisao: 10,
            prioridade: false,
            acompanhamento: None,
            alvo: None,
            referencia: None,
            relatorio_id: None,
        }
    }

    #[test]
    fn a_player_added_from_the_ficha_ages_until_a_new_analysis_is_needed() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let vencida = missao_com_prazo(&o, 20260701, 20260710);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, vec![vencida], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        let item = st.relatorios(false).into_iter().next().expect("Relatório");
        let pid = item.relatorio.jogadores[0].player_id;
        st.abrir_relatorio(item.relatorio.id);
        st.abrir_ficha(pid);
        assert_eq!(st.ficha_aberta().and_then(|f| f.escolhido), None);
        assert!(st.adicionar_escolhido_da_ficha());
        assert!(!st.adicionar_escolhido_da_ficha(), "já está na lista");
        let ficha = st.ficha_aberta().expect("Ficha");
        assert!(ficha.escolhido.is_some() && !ficha.da_lista);
        assert!(st.esta_nos_escolhidos(pid));

        let lista = st.escolhidos();
        assert_eq!(lista.len(), 1);
        let e = &lista[0];
        assert_eq!((e.frescor, e.acompanhado), (quality::Frescor::Atualizado, false));
        assert_eq!(e.escolhido.precisao, item.relatorio.precisao_mais_menos);
        let observados = e.jogador.atributos.len();
        assert!(observados > 0);

        // 8 meses: envelhecendo, faixas mais largas
        st.data_progresso = Some(Date(20270312));
        let e = st.escolhidos().remove(0);
        assert!(matches!(e.frescor, quality::Frescor::Envelhecendo { .. }), "{:?}", e.frescor);
        assert!(e.precisao > e.escolhido.precisao);
        assert!(e.jogador.overall.max - e.jogador.overall.min > e.escolhido.jogador.overall.max - e.escolhido.jogador.overall.min);
        // 13 meses: desatualizado; 19 meses: vencido, sem atributos
        st.data_progresso = Some(Date(20270812));
        assert!(matches!(st.escolhidos()[0].frescor, quality::Frescor::Desatualizado { .. }));
        st.data_progresso = Some(Date(20280212));
        let vencido = st.escolhidos().remove(0);
        assert_eq!(vencido.frescor, quality::Frescor::Vencido);
        assert!(vencido.jogador.atributos.is_empty());
        assert_eq!(vencido.escolhido.jogador.atributos.len(), observados, "o arquivo guarda a última observação");

        // a Ficha também abre da lista
        st.fechar_relatorio();
        st.abrir_ficha_de_escolhido(pid);
        let ficha = st.ficha_aberta().expect("Ficha do Escolhido");
        assert!(ficha.da_lista);
        assert_eq!(ficha.item.relatorio.qualidade, Qualidade::Baixa, "faixas largas");
        assert!(st.remover_escolhido(pid));
        assert!(st.escolhidos().is_empty());
        assert_eq!(st.ficha_aberta(), None, "sem o Escolhido, a Ficha fecha");
    }

    #[test]
    fn designated_generalists_keep_their_vacancies_updated_until_exact() {
        let pasta = PastaTemporaria::nova();
        let g = olheiro(Especializacao::Generalista, Tier::Junior); // 2,5★: 5 vagas
        let t = olheiro(Especializacao::Tatico, Tier::Elite);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &g);
        st.ao_abrir_painel();
        let estado = st.estado_ativo().cloned().expect("estado");
        let (t2, lista) = (t.clone(), (1..=6).map(|i| escolhido_de_teste(i, 20260601 + i as i32)).collect::<Vec<_>>());
        estado.mutar(move |d| { d.olheiros.push(t2); d.escolhidos = lista; }).expect("gravou");
        assert_eq!(st.resumo_acompanhamento().vagas, 0);

        // só Generalista livre pode ser designado
        assert!(!st.designar_acompanhamento(t.id, true), "Tático não acompanha");
        assert!(st.designar_acompanhamento(g.id, true));
        assert!(!st.designar_acompanhamento(g.id, true), "já designado");
        ticks_ate_acompanhar(&mut st);
        let resumo = st.resumo_acompanhamento();
        assert_eq!((resumo.generalistas, resumo.vagas, resumo.jogadores, resumo.acompanhados), (1, 5, 6, 5));
        let lista = st.escolhidos();
        assert_eq!(lista.iter().filter(|e| e.acompanhado).count(), 5);
        assert!(!lista[5].acompanhado && lista[5].escolhido.jogador.player_id == 6, "o mais novo fica sem vaga");
        let primeiro = &lista[0];
        let a = primeiro.escolhido.acompanhamento.expect("acompanhamento começou");
        assert_eq!(primeiro.escolhido.observado_em, Date(20260712));
        assert_eq!(a.dias, 0);
        assert!(primeiro.dias_para_exato.is_some_and(|d| d > 0));

        // designado não aceita Missão; o clique leva à aba Escolhidos
        st.abrir_nova_missao(g.id);
        assert!(!st.tem_nova_missao());
        assert_eq!(st.destino_do_olheiro(g.id), Some(DestinoOlheiro::Escolhidos));

        // depois do tempo todo, exato e com todos os atributos
        let fim = Date(20260712).mais_dias(a.dias_para_exato);
        st.data_progresso = Some(fim);
        st.atualizar_escolhidos(fim);
        ticks_ate_acompanhar(&mut st);
        let exato = st.escolhidos().remove(0);
        assert_eq!((exato.precisao, exato.dias_para_exato), (0, Some(0)));
        assert_eq!(exato.jogador.overall.min, exato.jogador.overall.max, "valor exato");
        assert_eq!(exato.jogador.atributos.len(), 28);
        let real = pool_de_teste().jogadores.into_iter().find(|j| j.player_id == exato.escolhido.jogador.player_id).expect("real");
        assert_eq!(exato.jogador.overall.min, real.overall);

        // prioridade: o 6º toma a vaga do 5º, que deixa de ser acompanhado
        st.alternar_prioridade_escolhido(6);
        ticks_ate_acompanhar(&mut st);
        let lista = st.escolhidos();
        assert_eq!(lista[0].escolhido.jogador.player_id, 6);
        assert!(lista[0].acompanhado && lista[0].escolhido.acompanhamento.is_some());
        let quinto = lista.iter().find(|e| e.escolhido.jogador.player_id == 5).expect("5");
        assert!(!quinto.acompanhado && quinto.escolhido.acompanhamento.is_none());

        // liberar: ninguém mais é acompanhado e ele volta a aceitar Missão
        assert!(st.designar_acompanhamento(g.id, false));
        assert!(st.escolhidos().iter().all(|e| !e.acompanhado && e.escolhido.acompanhamento.is_none()));
        st.abrir_nova_missao(g.id);
        assert!(st.tem_nova_missao());
    }

    #[test]
    fn a_retired_player_keeps_the_last_observation_without_rereading_forever() {
        let e = escolhido_de_teste(3, 20260601);
        let sem_ele = pool(Vec::new());
        let hoje = Date(20260712);
        let depois = avancar_escolhido(&e, &sem_ele, hoje);
        assert_eq!(depois.jogador, e.jogador, "nada novo a observar");
        assert_eq!(depois.observado_em, hoje, "não entra de novo na fila");
        assert!(depois.acompanhamento.is_some());
        let de_novo = avancar_escolhido(&depois, &sem_ele, hoje.mais_dias(10));
        assert_eq!(de_novo.acompanhamento.map(|a| a.dias), Some(10));
    }

    #[test]
    fn a_false_positive_shows_up_once_the_player_is_exact() {
        let hoje = Date(20260712);
        let mut e = escolhido_de_teste(3, 20260712);
        e.jogador.falso_positivo = true;
        assert!(!escolhido_em(&e, hoje, false).fora_do_filtro, "com faixas, ninguém sabe");
        let exato = Escolhido { precisao: 0, ..e };
        assert!(escolhido_em(&exato, hoje, false).fora_do_filtro);
    }

    #[test]
    fn loading_an_older_save_rolls_back_escolhidos_designations_and_hired_offers() {
        let pasta = PastaTemporaria::nova();
        let mut g = olheiro(Especializacao::Generalista, Tier::Experiente);
        g.acompanhando_desde = Some(Date(20260701));
        let mut g2 = olheiro(Especializacao::Generalista, Tier::Junior);
        g2.acompanhando_desde = Some(Date(20260706));
        let mut novo = olheiro_brasileiro(Especializacao::Tatico, 7);
        novo.contratado_em = Some(Date(20260706));
        let oferta = Uuid::new_v4();
        novo.oferta_id = Some(oferta);
        let (mut st, _busca, _) = estado_com_datas(&pasta, &[20260702], Vec::new(), &g);
        let mut atualizado = escolhido_de_teste(1, 20260601);
        atualizado.observado_em = Date(20260708);
        atualizado.acompanhamento =
            Some(Acompanhamento { inicio: Date(20260601), precisao_inicial: 10, atributos_iniciais: 6, dias_para_exato: 100, dias: 37 });
        let (n, g2c, lista) = (novo.clone(), g2.clone(), vec![atualizado, escolhido_de_teste(2, 20260601), escolhido_de_teste(3, 20260707)]);
        EstadoPersistido::carregar(Some(&pasta.0), ID_A)
            .mutar(move |d| {
                d.olheiros.push(g2c);
                d.olheiros.push(n);
                d.escolhidos = lista;
                d.ofertas_contratadas = vec![oferta];
            })
            .expect("gravou");
        st.ao_abrir_painel();
        let (olheiros, escolhidos, ofertas) = st
            .estado_ativo()
            .map(|e| e.ler(|d| (d.olheiros.clone(), d.escolhidos.clone(), d.ofertas_contratadas.clone())))
            .expect("estado");
        assert_eq!(olheiros.len(), 2, "o contratado em 06/07 saiu");
        assert_eq!(olheiros[0].acompanhando_desde, Some(Date(20260701)), "designado antes do save: fica");
        assert_eq!(olheiros[1].acompanhando_desde, None, "a designação de 06/07 foi desfeita");
        assert!(ofertas.is_empty(), "a oferta dele volta ao mercado");
        assert_eq!(escolhidos.iter().map(|e| e.jogador.player_id).collect::<Vec<_>>(), vec![1, 2], "o de 07/07 saiu");
        let e = &escolhidos[0];
        assert_eq!(e.observado_em, Date(20260703), "a data do save");
        assert_eq!(e.acompanhamento.map(|a| a.dias), Some(32), "5 dias a menos");
    }

    // -----------------------------------------------------------------
    // Correções da revisão (2026-10-04)
    // -----------------------------------------------------------------

    #[test]
    fn a_legacy_olheiro_gets_no_market_or_focus_penalty() {
        let pasta = PastaTemporaria::nova();
        let antigo = olheiro(Especializacao::CacadorDeJovens, Tier::Elite);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &antigo);
        st.ao_abrir_painel();
        esperar_ligas(&st);
        st.abrir_nova_missao(antigo.id);
        st.alternar_liga_da_missao(13);
        st.aplicar_atalho_da_missao(Atalho::MudaPatamar);
        let previa = st.previa_missao().expect("formulário");
        assert_eq!(previa.tipo, quality::TipoMissao::Medalhoes);
        assert_eq!((previa.penalidade, previa.distancia_mercado), (quality::Penalidade::default(), 0));
        let v1 = quality::estimar_missao(&quality::PedidoMissao::v1(
            Tier::Elite,
            Especializacao::CacadorDeJovens,
            previa.rascunho.modo,
            previa.tipo,
            previa.amplitude,
        ));
        assert_eq!(previa.estimativa, Some(v1), "os mesmos números da tabela do v1");
    }

    #[test]
    fn without_a_writable_state_file_tracking_never_reads_the_save() {
        let pasta = PastaTemporaria::nova();
        let mut o = olheiro(Especializacao::Generalista, Tier::Junior);
        o.acompanhando_desde = Some(Date(20260701));
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &o);
        EstadoPersistido::carregar(Some(&pasta.0), ID_A).mutar(|d| d.escolhidos = vec![escolhido_de_teste(1, 20260601)]).expect("gravou");
        // gravado por uma DLL mais nova: lido, mas somente leitura
        let arquivo = pasta.0.join(format!("{ID_A}.json"));
        let texto = std::fs::read_to_string(&arquivo).unwrap_or_default().replacen("\"versao\": 2", "\"versao\": 3", 1);
        std::fs::write(&arquivo, texto).expect("gravou");
        st.ao_abrir_painel();
        assert_eq!(st.resumo_acompanhamento().acompanhados, 1, "um jogador com vaga");
        assert!(!st.atualizacao_escolhidos_pendente, "sem leitura do save que não teria onde gravar");
    }

    #[test]
    fn the_market_keeps_its_attractiveness_for_the_whole_month() {
        let pasta = PastaTemporaria::nova();
        let (st, _) = estado_contratacao(63_999_988, 63_999_988, Some(pasta.0.clone()), vec![]);
        let estado = st.estado_ativo().cloned().expect("estado");
        let periodo = quality::periodo_do_mercado(2026, 7);
        estado.mutar(|d| d.mercado_do_mes = Some((periodo, 100))).expect("gravou");
        let mercado = loop {
            if let Carga::Pronto(m) = st.mercado_de_olheiros() {
                break m;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(mercado.atratividade, 100, "vale a do mês, mesmo com o clube desconhecido agora");
        assert_eq!(mercado.ofertas.len(), quality::quantas_ofertas(100) as usize);
        // mês passado não vale
        estado.mutar(|d| d.mercado_do_mes = Some((periodo - 1, 100))).expect("gravou");
        if let Carga::Pronto(m) = st.mercado_de_olheiros() {
            assert_ne!(m.atratividade, 100);
        }
    }

    #[test]
    fn the_observation_date_follows_the_block_where_the_player_appeared() {
        let mut m = Missao::de_teste(Uuid::new_v4(), StatusMissao::Pendente);
        m.criada_em = Date(20260101);
        m.prazo_estimado = Date(20260115);
        let hoje = Date(20260601);
        assert_eq!(data_da_observacao(Some(&m), Some(Date(20260102)), 0, hoje), Date(20260115), "prazo fixo: no prazo");
        assert_eq!(data_da_observacao(Some(&m), None, 0, Date(20260110)), Date(20260110), "antes do prazo: hoje");
        m.continua = true;
        let por_bloco = usize::from(m.estimativa.alvo_jogadores);
        assert_eq!(data_da_observacao(Some(&m), None, 0, hoje), Date(20260131), "1º bloco");
        assert_eq!(data_da_observacao(Some(&m), None, por_bloco, hoje), Date(20260302), "2º bloco");
        assert_eq!(data_da_observacao(None, Some(Date(20260201)), 3, hoje), Date(20260201));
    }
}
