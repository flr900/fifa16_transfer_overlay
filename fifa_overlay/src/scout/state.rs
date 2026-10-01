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
pub use crate::save_repo::{Atributo, Confederacao, Funcao, Nacao};

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
    /// Uma Missão terminou a busca e o Relatório está pronto (Story 2.4).
    RelatorioPronto { tipo: quality::TipoMissao, jogadores: usize },
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Especializacao {
    CacadorDeJovens,
    CacadorDeMedalhoes,
    Tatico,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Junior,
    Experiente,
    Elite,
}

impl Tier {
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

/// Olheiro contratado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Olheiro {
    pub id: Uuid,
    pub especializacao: Especializacao,
    pub tier: Tier,
}

/// Uma das 12 combinações Especialização × Tier à venda (FR-2). Sem
/// limite de vagas no v1: as 12 ficam sempre disponíveis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfertaOlheiro {
    pub especializacao: Especializacao,
    pub tier: Tier,
    pub custo: i32,
    /// Quanto falta no orçamento para contratar (`None` = dá para pagar).
    pub faltam: Option<i32>,
}

/// Olheiro contratado como a aba Olheiros o mostra.
#[derive(Debug, Clone, PartialEq)]
pub struct OlheiroContratado {
    pub olheiro: Olheiro,
    /// Tem Missão ainda não concluída (AD-8): "Em Missão" em vez de "Disponível".
    pub em_missao: bool,
}

/// Contratação em andamento: o usuário clicou "Contratar" e o modal de
/// confirmação está aberto (Story 1.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Contratacao {
    pub especializacao: Especializacao,
    pub tier: Tier,
    pub custo: i32,
    /// Falha da última tentativa de confirmar (o modal mostra e oferece
    /// tentar de novo). `None` = ainda não tentou.
    pub erro: Option<ErroCompra>,
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
/// atributo dominante; geografia (2.9), Fit Posicional e Jogador de
/// Referência (Épico 3) entram com `#[serde(default)]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiltrosMissao {
    pub overall: FaixaAtributo,
    pub potencial: FaixaAtributo,
    /// "O melhor driblador": só entram jogadores que têm este atributo
    /// entre os seus `quality::TOP_DOMINANTE` maiores (Story 2.8).
    #[serde(default)]
    pub atributo_dominante: Option<Atributo>,
    /// Países escolhidos no mapa (`Crbb.nationid`; `search::NACAO_OUTROS` =
    /// quadro "Outros"). Vazio = todos os países (Story 2.9).
    #[serde(default)]
    pub paises: Vec<u16>,
}

impl Default for FiltrosMissao {
    /// Faixas amplas: o formulário abre sem restringir quase nada.
    fn default() -> Self {
        FiltrosMissao {
            overall: FaixaAtributo { min: 50, max: FaixaAtributo::MAIOR },
            potencial: FaixaAtributo { min: 50, max: FaixaAtributo::MAIOR },
            atributo_dominante: None,
            paises: Vec::new(),
        }
    }
}

/// Qual ponta de qual faixa um botão − / + do formulário mexe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoFaixa {
    OverallMin,
    OverallMax,
    PotencialMin,
    PotencialMax,
}

/// Formulário Nova Missão aberto (Story 2.2): o que o jogador escolheu até
/// agora. Nada é persistido antes de confirmar.
#[derive(Debug, Clone, PartialEq)]
pub struct RascunhoMissao {
    pub olheiro_id: Option<Uuid>,
    pub filtros: FiltrosMissao,
    pub modo: ModoBusca,
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
}

impl PreviaMissao {
    /// Data em que a Missão fica pronta, se confirmada agora.
    pub fn prazo(&self) -> Option<Date> {
        self.estimativa.map(|e| self.data_atual.mais_dias(e.duracao_dias))
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
    /// O Relatório ainda não foi aberto: indicador "novo" (UX-DR13).
    pub relatorio_novo: bool,
}

/// Um Relatório como a aba Relatórios e a tela do Relatório o mostram.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatorioNaLista {
    pub relatorio: Relatorio,
    /// `None` se a Missão sumiu do arquivo (não deveria acontecer).
    pub missao: Option<Missao>,
    pub olheiro: Option<Olheiro>,
}

/// As 12 ofertas na ordem da tela (Tier crescente, depois a ordem do PRD),
/// com o que falta para cada uma diante de `orcamento`.
pub fn ofertas_de_olheiros(orcamento: Option<i32>) -> Vec<OfertaOlheiro> {
    Tier::TODOS
        .iter()
        .flat_map(|&tier| Especializacao::TODAS.iter().map(move |&especializacao| (especializacao, tier)))
        .map(|(especializacao, tier)| {
            let custo = quality::custo_contratacao(especializacao, tier);
            let faltam = orcamento.and_then(|saldo| (saldo < custo).then(|| custo.saturating_sub(saldo)));
            OfertaOlheiro { especializacao, tier, custo, faltam }
        })
        .collect()
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
    pub estimativa: quality::EstimativaMissao,
}

#[cfg(test)]
impl Missao {
    /// Missão mínima para testes.
    pub fn de_teste(olheiro_id: Uuid, status: StatusMissao) -> Missao {
        let filtros = FiltrosMissao::default();
        let pedido = quality::PedidoMissao {
            tier: Tier::Junior,
            especializacao: Especializacao::Generalista,
            modo: ModoBusca::Rapida,
            tipo: quality::TipoMissao::Geral,
            amplitude: quality::AmplitudeGeografica::Mundo,
        };
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
    pub overall: FaixaAtributo,
    pub potencial: FaixaAtributo,
    /// Na ordem em que o Olheiro observou (a função do jogador primeiro).
    pub atributos: Vec<AtributoRevelado>,
}

impl JogadorEncontrado {
    pub fn atributo(&self, atributo: Atributo) -> Option<FaixaAtributo> {
        self.atributos.iter().find(|a| a.atributo == atributo).map(|a| a.valor)
    }
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
    tarefa_busca: AsyncTask<Relatorio>,
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
        if matches!(self.tarefa_localizar.poll(), TaskState::Running) {
            return;
        }
        self.localizacao_automatica_disponivel = true;
        self.reler();
        self.localizacao_automatica_disponivel = false;
        if let CarreiraStatus::Pronta(carreira) = &self.status {
            let hoje = carreira.data_atual;
            self.data_progresso = Some(hoje);
            self.despachar_missoes_vencidas(hoje);
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
            // Carreira ficou pronta COM o painel aberto: vale como a
            // abertura (AD-8) — senão a Missão vencida esperaria fechar e
            // abrir de novo.
            if self.painel_aberto {
                self.despachar_missoes_vencidas(snapshot.data_atual);
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

    /// Lista de contratação (FR-2) contra o orçamento atual.
    pub fn olheiros_disponiveis(&self) -> Vec<OfertaOlheiro> {
        ofertas_de_olheiros(self.orcamento())
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
                .map(|olheiro| OlheiroContratado {
                    olheiro: olheiro.clone(),
                    em_missao: dados
                        .missoes
                        .iter()
                        .any(|m| m.olheiro_id == olheiro.id && m.status != StatusMissao::Concluida),
                })
                .collect()
        })
    }

    /// "Contratar" clicado: abre a confirmação para essa combinação.
    pub fn preparar_contratacao(&mut self, especializacao: Especializacao, tier: Tier) {
        let custo = quality::custo_contratacao(especializacao, tier);
        self.contratacao = Some(Contratacao { especializacao, tier, custo, erro: None });
    }

    pub fn cancelar_contratacao(&mut self) {
        self.contratacao = None;
    }

    /// Dados do modal, ou `None` se não há contratação aberta (ou a
    /// carreira deixou de estar pronta — o modal deve fechar).
    pub fn previa_contratacao(&self) -> Option<PreviaContratacao> {
        let contratacao = self.contratacao.clone()?;
        let orcamento_atual = self.orcamento()?;
        let faltam = (orcamento_atual < contratacao.custo).then(|| contratacao.custo.saturating_sub(orcamento_atual));
        Some(PreviaContratacao {
            orcamento_apos: orcamento_atual.saturating_sub(contratacao.custo),
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
        let olheiro = Olheiro {
            id: Uuid::new_v4(),
            especializacao: contratacao.especializacao,
            tier: contratacao.tier,
        };
        let novo = olheiro.clone();
        match self.comprar(contratacao.custo, move |dados| dados.olheiros.push(novo)) {
            Ok(()) => {
                tracing::info!(
                    "[scout::state] Olheiro contratado: {} {:?} ({}), id {}.",
                    olheiro.especializacao.nome(),
                    olheiro.tier,
                    contratacao.custo,
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

    /// "Nova Missão": abre o formulário com o primeiro Olheiro disponível
    /// já escolhido e faixas amplas.
    pub fn abrir_nova_missao(&mut self) {
        let olheiro_id = self.olheiros_contratados().into_iter().find(|c| !c.em_missao).map(|c| c.olheiro.id);
        self.rascunho_missao =
            Some(RascunhoMissao { olheiro_id, filtros: FiltrosMissao::default(), modo: ModoBusca::Rapida, erro: None });
    }

    pub fn cancelar_nova_missao(&mut self) {
        self.rascunho_missao = None;
    }

    pub fn tem_nova_missao(&self) -> bool {
        self.rascunho_missao.is_some()
    }

    /// Escolhe o Olheiro (só os disponíveis; um "Em Missão" é ignorado).
    pub fn escolher_olheiro_da_missao(&mut self, id: Uuid) {
        let disponivel = self.olheiros_contratados().iter().any(|c| c.olheiro.id == id && !c.em_missao);
        if let (true, Some(r)) = (disponivel, self.rascunho_missao.as_mut()) {
            r.olheiro_id = Some(id);
            r.erro = None;
        }
    }

    /// Botões − / + das faixas (sempre dentro de 1–99).
    pub fn ajustar_faixa_da_missao(&mut self, campo: CampoFaixa, delta: i32) {
        let Some(r) = self.rascunho_missao.as_mut() else {
            return;
        };
        let valor = match campo {
            CampoFaixa::OverallMin => &mut r.filtros.overall.min,
            CampoFaixa::OverallMax => &mut r.filtros.overall.max,
            CampoFaixa::PotencialMin => &mut r.filtros.potencial.min,
            CampoFaixa::PotencialMax => &mut r.filtros.potencial.max,
        };
        let novo = (i32::from(*valor) + delta).clamp(i32::from(FaixaAtributo::MENOR), i32::from(FaixaAtributo::MAIOR));
        *valor = u8::try_from(novo).unwrap_or(*valor);
        r.erro = None;
    }

    /// Nações do mapa, ou `None` enquanto carregam (a primeira chamada
    /// dispara a leitura em background; uma falha é tentada de novo).
    pub fn nacoes(&self) -> Option<Arc<Vec<Nacao>>> {
        match self.tarefa_nacoes.poll() {
            TaskState::Done(nacoes) => Some(nacoes),
            TaskState::Running => None,
            TaskState::Idle | TaskState::Failed(_) => {
                let fonte = Arc::clone(&self.fonte);
                self.tarefa_nacoes.start(move || fonte.read_nations().map(Arc::new));
                None
            }
        }
    }

    /// Amplitude da seleção de países (sem as nações ainda carregadas,
    /// conta só o número de países).
    fn amplitude_dos_paises(&self, paises: &[u16]) -> quality::AmplitudeGeografica {
        let nacoes = match self.tarefa_nacoes.poll() {
            TaskState::Done(nacoes) => nacoes,
            _ => Arc::new(Vec::new()),
        };
        let conf = |id: &u16| nacoes.iter().find(|n| n.id == *id).map_or(Confederacao::Outras, |n| n.confederacao);
        let selecao: Vec<Confederacao> = paises.iter().map(conf).collect();
        let total_da = |c: Confederacao| {
            if nacoes.is_empty() {
                usize::MAX
            } else {
                nacoes.iter().filter(|n| n.confederacao == c).count()
            }
        };
        quality::amplitude_da_selecao(&selecao, total_da)
    }

    /// Clique num país do mapa: entra ou sai da seleção (cumulativo, sem
    /// tecla modificadora — Story 2.9).
    pub fn alternar_pais_da_missao(&mut self, id: u16) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            match r.filtros.paises.iter().position(|p| *p == id) {
                Some(i) => {
                    r.filtros.paises.remove(i);
                }
                None => r.filtros.paises.push(id),
            }
            r.erro = None;
        }
    }

    /// "Limpar": volta a todos os países.
    pub fn limpar_paises_da_missao(&mut self) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.paises.clear();
            r.erro = None;
        }
    }

    /// Atributo dominante escolhido no painel de campo (Story 2.8);
    /// `None` = sem esse filtro.
    pub fn definir_atributo_da_missao(&mut self, atributo: Option<Atributo>) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.filtros.atributo_dominante = atributo;
            r.erro = None;
        }
    }

    pub fn definir_modo_da_missao(&mut self, modo: ModoBusca) {
        if let Some(r) = self.rascunho_missao.as_mut() {
            r.modo = modo;
            r.erro = None;
        }
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
            .and_then(|id| olheiros.iter().find(|c| c.olheiro.id == id && !c.em_missao))
            .map(|c| c.olheiro.clone());
        let tipo = quality::tipo_por_filtros(
            rascunho.filtros.overall,
            rascunho.filtros.potencial,
            rascunho.filtros.atributo_dominante,
        );
        let amplitude = self.amplitude_dos_paises(&rascunho.filtros.paises);
        let estimativa = escolhido.as_ref().map(|o| {
            quality::estimar_missao(&quality::PedidoMissao {
                tier: o.tier,
                especializacao: o.especializacao,
                modo: rascunho.modo,
                tipo,
                amplitude,
            })
        });
        let bloqueio = if escolhido.is_none() {
            Some(BloqueioMissao::SemOlheiroDisponivel)
        } else if !rascunho.filtros.overall.valida() {
            Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::OverallMin })
        } else if !rascunho.filtros.potencial.valida() {
            Some(BloqueioMissao::FaixaInvalida { campo: CampoFaixa::PotencialMin })
        } else {
            estimativa
                .filter(|e| orcamento_atual < e.custo)
                .map(|e| BloqueioMissao::OrcamentoInsuficiente { faltam: e.custo.saturating_sub(orcamento_atual) })
        };
        Some(PreviaMissao {
            combina: escolhido.as_ref().is_some_and(|o| quality::combina(o.especializacao, tipo)),
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
        let (Some(olheiro_id), Some(estimativa)) = (previa.rascunho.olheiro_id, previa.estimativa) else {
            return false;
        };
        let missao = Missao {
            id: Uuid::new_v4(),
            olheiro_id,
            status: StatusMissao::Pendente,
            criada_em: previa.data_atual,
            prazo_estimado: previa.data_atual.mais_dias(estimativa.duracao_dias),
            filtros: previa.rascunho.filtros.clone(),
            modo_busca: previa.rascunho.modo,
            tipo: previa.tipo,
            amplitude: previa.amplitude,
            estimativa,
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
                    MissaoNaLista {
                        missao: m.clone(),
                        olheiro: dados.olheiros.iter().find(|o| o.id == m.olheiro_id).cloned(),
                        progresso: hoje.map(|data| progresso_missao(m.criada_em, m.prazo_estimado, data)),
                        falha: self.falhas_busca.get(&m.id).cloned(),
                        relatorio_id: relatorio.map(|r| r.id),
                        relatorio_novo: relatorio.is_some_and(|r| !r.aberto),
                    }
                })
                .collect()
        })
    }

    // -----------------------------------------------------------------
    // Busca das Missões vencidas (Story 2.4, AD-8/AD-9)
    // -----------------------------------------------------------------

    /// Borda fechado→aberto do painel (nunca por polling): toda Missão
    /// `Pendente` com o prazo cumprido vira `EmExecucao` ANTES de entrar na
    /// fila — reabrir o painel não a despacha de novo (AD-8). Ordem da
    /// fila: `prazo_estimado`, depois `criada_em` (AD-9).
    fn despachar_missoes_vencidas(&mut self, hoje: Date) {
        let (Some(id_save), Some(estado)) = (self.save_ativo.clone(), self.estado_ativo().cloned()) else {
            return;
        };
        let mut vencidas: Vec<Missao> = estado.ler(|dados| {
            dados
                .missoes
                .iter()
                .filter(|m| m.status == StatusMissao::Pendente && hoje >= m.prazo_estimado)
                .cloned()
                .collect()
        });
        if vencidas.is_empty() {
            return;
        }
        vencidas.sort_by_key(|m| (m.prazo_estimado, m.criada_em));
        let ids: Vec<Uuid> = vencidas.iter().map(|m| m.id).collect();
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
        for mut missao in vencidas {
            missao.status = StatusMissao::EmExecucao;
            tracing::info!("[scout::state] Missão {} na fila de busca (prazo {}).", missao.id, missao.prazo_estimado.0);
            self.falhas_busca.remove(&missao.id);
            self.fila_busca.push_back(BuscaNaFila { id_save: id_save.clone(), missao, hoje });
        }
    }

    /// Roda a cada frame (painel aberto ou fechado): trata a busca que
    /// terminou e dispara a próxima da fila. Uma de cada vez (AD-9).
    fn processar_buscas(&mut self) {
        match self.tarefa_busca.poll() {
            TaskState::Running => return,
            TaskState::Done(relatorio) => {
                if let Some(busca) = self.busca_atual.take() {
                    self.concluir_busca(busca, relatorio);
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
        let (missao, hoje) = (proxima.missao.clone(), proxima.hoje);
        if self.tarefa_busca.start(move || search::executar_missao(&missao, fonte.as_ref(), hoje)) {
            self.busca_atual = Some(proxima);
        } else {
            self.fila_busca.push_front(proxima);
        }
    }

    /// Grava o Relatório e conclui a Missão no arquivo da carreira DONA da
    /// busca (pode não ser a ativa): o Olheiro volta a ficar disponível.
    fn concluir_busca(&mut self, busca: BuscaNaFila, relatorio: Relatorio) {
        let Some(estado) = self.estados.get(&busca.id_save).cloned() else {
            return;
        };
        let jogadores = relatorio.jogadores.len();
        let id = busca.missao.id;
        let gravou = estado.mutar(move |dados| {
            if let Some(m) = dados.missoes.iter_mut().find(|m| m.id == id) {
                m.status = StatusMissao::Concluida;
            }
            dados.relatorios.retain(|r| r.missao_id != id);
            dados.relatorios.push(relatorio);
        });
        match gravou {
            Ok(()) => {
                tracing::info!("[scout::state] Missão {id} concluída: Relatório com {jogadores} jogadores.");
                self.avisar(TipoAviso::RelatorioPronto { tipo: busca.missao.tipo, jogadores });
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

    /// Falha da última busca desta Missão (texto do `SaveRepoError`).
    pub fn falha_da_busca(&self, missao: Uuid) -> Option<&str> {
        self.falhas_busca.get(&missao).map(String::as_str)
    }

    // -----------------------------------------------------------------
    // Relatórios (Story 2.5)
    // -----------------------------------------------------------------

    fn montar_relatorio_na_lista(dados: &persistence::ScoutStateFile, r: &Relatorio) -> RelatorioNaLista {
        let missao = dados.missoes.iter().find(|m| m.id == r.missao_id).cloned();
        let olheiro = missao.as_ref().and_then(|m| dados.olheiros.iter().find(|o| o.id == m.olheiro_id).cloned());
        RelatorioNaLista { relatorio: r.clone(), missao, olheiro }
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
                .map(|r| Self::montar_relatorio_na_lista(dados, r))
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
        let ja_aberto = estado.ler(|dados| dados.relatorios.iter().any(|r| r.id == id && r.aberto));
        if !ja_aberto {
            if let Err(err) = estado.mutar(|dados| {
                for r in dados.relatorios.iter_mut().filter(|r| r.id == id) {
                    r.aberto = true;
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

    /// Visão dos Relatórios salva para a carreira (Tabular por padrão).
    pub fn densidade(&self) -> Densidade {
        self.estado_ativo().map_or(Densidade::Tabular, |e| e.ler(|d| d.ui_prefs.densidade))
    }

    /// O jogador trocou Tabular/Cards: persiste em `ui_prefs` (AD-7).
    pub fn definir_densidade(&mut self, densidade: Densidade) {
        let Some(estado) = self.estado_ativo() else {
            return;
        };
        if estado.ler(|d| d.ui_prefs.densidade) == densidade {
            return;
        }
        if let Err(err) = estado.mutar(|d| d.ui_prefs.densidade = densidade) {
            tracing::warn!("[scout::state] Visão dos Relatórios não foi salva: {err:?}");
        }
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
    }

    /// O Relatório na tela, ou `None` (a tela deve fechar).
    pub fn relatorio_aberto(&self) -> Option<RelatorioNaLista> {
        let id = self.relatorio_aberto?;
        let estado = self.estado_ativo()?;
        estado.ler(|dados| dados.relatorios.iter().find(|r| r.id == id).map(|r| Self::montar_relatorio_na_lista(dados, r)))
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

    #[test]
    fn offers_list_the_twelve_combinations_in_tier_order_with_the_shortfall() {
        let ofertas = ofertas_de_olheiros(Some(1_000_000));
        assert_eq!(ofertas.len(), 12);
        let primeiras: Vec<_> = ofertas.iter().take(4).map(|o| (o.especializacao, o.tier)).collect();
        assert_eq!(primeiras, Especializacao::TODAS.map(|e| (e, Tier::Junior)).to_vec());
        assert!(ofertas.iter().take(4).all(|o| o.faltam.is_none()), "Júnior cabe em 1 M");
        let exp_generalista = ofertas
            .iter()
            .find(|o| (o.especializacao, o.tier) == (Especializacao::Generalista, Tier::Experiente))
            .map(|o| o.faltam);
        assert_eq!(exp_generalista, Some(Some(200_000)));
        // custo exatamente igual ao saldo: dá para pagar
        let custo = quality::custo_contratacao(Especializacao::Tatico, Tier::Elite);
        let exata = ofertas_de_olheiros(Some(custo));
        assert!(exata.iter().any(|o| o.custo == custo && o.faltam.is_none()));
    }

    #[test]
    fn hired_olheiros_come_from_the_career_file_with_their_mission_status() {
        let pasta = PastaTemporaria::nova();
        let (mut st, _) = estado_em(vec![Ok(snapshot())], Ok(()), Some(pasta.0.clone()));
        assert!(st.olheiros_contratados().is_empty(), "sem carreira, nada");
        st.ao_abrir_painel();
        assert!(st.olheiros_contratados().is_empty());
        assert_eq!(st.orcamento(), Some(63_999_988));

        let ocupado = Olheiro { id: Uuid::new_v4(), especializacao: Especializacao::Tatico, tier: Tier::Experiente };
        let livre = Olheiro { id: Uuid::new_v4(), especializacao: Especializacao::Generalista, tier: Tier::Junior };
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

        assert_eq!(
            st.olheiros_contratados(),
            vec![
                OlheiroContratado { olheiro: ocupado, em_missao: true },
                OlheiroContratado { olheiro: livre, em_missao: false },
            ]
        );
    }

    #[test]
    fn hiring_debits_the_budget_then_saves_the_olheiro_and_shows_the_reread_balance() {
        let pasta = PastaTemporaria::nova();
        let (mut st, escritas) = estado_contratacao(63_999_988, 63_699_988, Some(pasta.0.clone()), vec![]);
        st.preparar_contratacao(Especializacao::Generalista, Tier::Junior);
        let previa = st.previa_contratacao().expect("modal aberto");
        assert_eq!(
            (previa.contratacao.custo, previa.orcamento_atual, previa.orcamento_apos),
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
        st.preparar_contratacao(Especializacao::Tatico, Tier::Elite);
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
            st.preparar_contratacao(Especializacao::Generalista, Tier::Junior);
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
        st.preparar_contratacao(Especializacao::Generalista, Tier::Junior);
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

            st.preparar_contratacao(Especializacao::Generalista, Tier::Junior);
            assert!(!st.confirmar_contratacao());
            assert_eq!(erro_da_contratacao(&st), Some(esperado));
            assert_eq!(escritas_de(&escritas), [(63_999_988, 63_699_988), (63_699_988, 63_999_988)]);
            assert!(st.olheiros_contratados().is_empty());
        }
    }

    #[test]
    fn cancelling_closes_the_preview_without_writing() {
        let (mut st, escritas) = estado_contratacao(63_999_988, 63_999_988, None, vec![]);
        st.preparar_contratacao(Especializacao::Generalista, Tier::Junior);
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
        Olheiro { id: Uuid::new_v4(), especializacao, tier }
    }

    #[test]
    fn new_missao_form_picks_the_first_available_olheiro_and_estimates_live() {
        let pasta = PastaTemporaria::nova();
        let ocupado = olheiro(Especializacao::Tatico, Tier::Elite);
        let livre = olheiro(Especializacao::CacadorDeJovens, Tier::Elite);
        let missoes = vec![Missao::de_teste(ocupado.id, StatusMissao::Pendente)];
        let (mut st, _) = estado_com_olheiros(63_999_988, 0, vec![ocupado.clone(), livre.clone()], missoes, &pasta);

        st.abrir_nova_missao();
        let previa = st.previa_missao().expect("formulário aberto");
        assert_eq!(previa.rascunho.olheiro_id, Some(livre.id), "o ocupado é pulado");
        assert_eq!(previa.olheiros.len(), 2, "o ocupado aparece, apagado");
        assert_eq!(previa.tipo, quality::TipoMissao::Geral);
        assert_eq!(previa.bloqueio, None);
        let antes = previa.estimativa.expect("estimativa");

        // escolher o ocupado é ignorado
        st.escolher_olheiro_da_missao(ocupado.id);
        assert_eq!(st.previa_missao().and_then(|p| p.rascunho.olheiro_id), Some(livre.id));

        // faixas de jovens + Completa: combina e a Qualidade sobe
        st.definir_modo_da_missao(ModoBusca::Completa);
        st.ajustar_faixa_da_missao(CampoFaixa::OverallMax, -29); // 99 -> 70
        st.ajustar_faixa_da_missao(CampoFaixa::PotencialMin, 30); // 50 -> 80
        let previa = st.previa_missao().expect("formulário aberto");
        assert_eq!(previa.tipo, quality::TipoMissao::Jovens);
        assert!(previa.combina);
        let depois = previa.estimativa.expect("estimativa");
        assert!(depois.qualidade > antes.qualidade);
        assert_eq!(depois.qualidade, Qualidade::Alta);
        assert_eq!(previa.prazo(), Some(Date(20260703).mais_dias(depois.duracao_dias)));
    }

    #[test]
    fn ranges_are_clamped_and_an_inverted_range_blocks_confirmation() {
        let pasta = PastaTemporaria::nova();
        let (mut st, escritas) =
            estado_com_olheiros(63_999_988, 0, vec![olheiro(Especializacao::Generalista, Tier::Junior)], vec![], &pasta);
        st.abrir_nova_missao();
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
        st.abrir_nova_missao();
        let previa = st.previa_missao().expect("aberto");
        assert_eq!(previa.bloqueio, Some(BloqueioMissao::SemOlheiroDisponivel));
        assert_eq!(previa.estimativa, None);
        assert!(!st.confirmar_nova_missao());
    }

    #[test]
    fn confirming_debits_and_saves_a_pending_missao_and_the_olheiro_becomes_busy() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, escritas) = estado_com_olheiros(63_999_988, 63_849_988, vec![o.clone()], vec![], &pasta);
        st.abrir_nova_missao();
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

        // um segundo formulário já não tem Olheiro disponível
        st.abrir_nova_missao();
        assert_eq!(st.previa_missao().and_then(|p| p.bloqueio), Some(BloqueioMissao::SemOlheiroDisponivel));
    }

    #[test]
    fn insufficient_budget_blocks_with_the_exact_shortfall() {
        let pasta = PastaTemporaria::nova();
        let (mut st, escritas) =
            estado_com_olheiros(100_000, 100_000, vec![olheiro(Especializacao::Generalista, Tier::Junior)], vec![], &pasta);
        st.abrir_nova_missao();
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
        st.estado_ativo().map(|e| e.mutar(move |d| d.olheiros = vec![o])).expect("ativa").expect("gravou");
        st.abrir_nova_missao();
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
    fn a_missao_before_its_deadline_is_not_dispatched() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let m = missao_com_prazo(&o, 20260701, 20260720);
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![m.clone()], &o);
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Pendente));
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 0);
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
        assert_eq!(st.falha_da_busca(m.id), Some("Não foi possível ler o save ativo."));
        assert!(matches!(st.aviso_visivel(Instant::now()), Some(TipoAviso::BuscaFalhou)));

        // na próxima abertura roda de novo (e agora funciona)
        *busca.jogadores.lock().unwrap_or_else(|p| p.into_inner()) = Ok(pool_de_teste());
        st.ao_fechar_painel();
        st.ao_abrir_painel();
        ticks_ate_buscar(&mut st);
        assert_eq!(status_de(&st, m.id), Some(StatusMissao::Concluida));
        assert_eq!(st.falha_da_busca(m.id), None);
    }

    #[test]
    fn a_missao_stuck_in_execution_is_reset_when_the_state_loads() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let mut presa = missao_com_prazo(&o, 20260701, 20260720);
        presa.status = StatusMissao::EmExecucao;
        let (mut st, busca) = estado_com_missoes(&pasta, 20260712, vec![presa.clone()], &o);
        st.ao_abrir_painel();
        assert_eq!(status_de(&st, presa.id), Some(StatusMissao::Pendente));
        ticks_ate_buscar(&mut st);
        assert_eq!(busca.buscas.load(Ordering::SeqCst), 0, "ainda não venceu");
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
    fn report_density_is_saved_per_career() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &o);
        st.ao_abrir_painel();
        assert_eq!(st.densidade(), Densidade::Tabular);
        st.definir_densidade(Densidade::Cards);
        assert_eq!(st.densidade(), Densidade::Cards);
        let relido = EstadoPersistido::carregar(Some(&pasta.0), ID_A).ler(|d| d.ui_prefs.densidade);
        assert_eq!(relido, Densidade::Cards);
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

    #[test]
    fn choosing_countries_changes_breadth_and_the_estimate_live() {
        let pasta = PastaTemporaria::nova();
        let o = olheiro(Especializacao::Generalista, Tier::Junior);
        let (mut st, _busca) = estado_com_missoes(&pasta, 20260712, Vec::new(), &o);
        st.ao_abrir_painel();
        // nações carregam em background
        let inicio = Instant::now();
        while st.nacoes().is_none() {
            assert!(inicio.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        st.abrir_nova_missao();
        let mundo = st.previa_missao().expect("formulário");
        assert_eq!(mundo.amplitude, quality::AmplitudeGeografica::Mundo);
        st.alternar_pais_da_missao(54);
        let pais = st.previa_missao().expect("formulário");
        assert_eq!(pais.amplitude, quality::AmplitudeGeografica::Pais);
        let (m, p) = (mundo.estimativa.expect("estimativa"), pais.estimativa.expect("estimativa"));
        assert!(p.precisao_mais_menos < m.precisao_mais_menos, "mais estreito = mais preciso");
        st.alternar_pais_da_missao(52);
        assert_eq!(st.previa_missao().map(|x| x.amplitude), Some(quality::AmplitudeGeografica::Continente), "os 2 da América do Sul do teste");
        st.alternar_pais_da_missao(54);
        assert_eq!(st.previa_missao().map(|x| x.rascunho.filtros.paises), Some(vec![52]), "clicar de novo tira");
        st.limpar_paises_da_missao();
        assert_eq!(st.previa_missao().map(|x| x.amplitude), Some(quality::AmplitudeGeografica::Mundo));
    }
}
