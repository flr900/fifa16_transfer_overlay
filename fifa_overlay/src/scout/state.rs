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

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::async_task::{AsyncTask, TaskState};
use crate::save_repo::{Date, SaveRepoError};

use super::persistence::{self, EstadoPersistido};
use super::quality;
use super::search::{CareerSnapshot, CareerSource, SaveRepoSource};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Qualidade {
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

/// Filtros de uma Missão. A Story 2.2 traz Overall e Potencial; geografia
/// (2.9), atributo dominante (2.8), Fit Posicional e Jogador de Referência
/// (Épico 3) entram depois, com `#[serde(default)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiltrosMissao {
    pub overall: FaixaAtributo,
    pub potencial: FaixaAtributo,
}

impl Default for FiltrosMissao {
    /// Faixas amplas: o formulário abre sem restringir quase nada.
    fn default() -> Self {
        FiltrosMissao {
            overall: FaixaAtributo { min: 50, max: FaixaAtributo::MAIOR },
            potencial: FaixaAtributo { min: 50, max: FaixaAtributo::MAIOR },
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
#[allow(dead_code)] // `EmExecucao`/`Concluida` chegam no Épico 2
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relatorio {
    pub id: Uuid,
    pub missao_id: Uuid,
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

pub struct ScoutState {
    fonte: Box<dyn CareerSource>,
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
}

impl ScoutState {
    pub fn new() -> Self {
        Self::com_fonte(Box::new(SaveRepoSource), persistence::diretorio_padrao())
    }

    pub fn com_fonte(fonte: Box<dyn CareerSource>, diretorio_estado: Option<PathBuf>) -> Self {
        ScoutState {
            fonte,
            tarefa_localizar: AsyncTask::new(),
            status: CarreiraStatus::SemCarreira,
            localizacao_automatica_disponivel: false,
            contratacao: None,
            rascunho_missao: None,
            ultima_leitura: None,
            diretorio_estado,
            estados: HashMap::new(),
            save_ativo: None,
            aba_restaurada: None,
            tarefa_sinal: AsyncTask::new(),
            sinal_pendente: false,
            sinal_consumido: false,
            proximo_sinal: None,
            aviso: None,
            aviso_inicial_pendente: true,
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
        if matches!(self.tarefa_localizar.poll(), TaskState::Running) {
            return;
        }
        self.localizacao_automatica_disponivel = true;
        self.reler();
        self.localizacao_automatica_disponivel = false;
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
            self.avisar(TipoAviso::Pronta(snapshot));
        }
    }

    /// A carreira `id_save` ficou pronta: carrega o arquivo dela (só na
    /// primeira vez da sessão) e pede para restaurar a aba salva.
    fn ativar_save(&mut self, id_save: String) {
        let diretorio = self.diretorio_estado.as_deref();
        let estado = self
            .estados
            .entry(id_save.clone())
            .or_insert_with(|| EstadoPersistido::carregar(diretorio, &id_save));
        self.aba_restaurada = Some(estado.ler(|dados| dados.ui_prefs.aba_ativa));
        tracing::info!("[scout::state] Carreira ativa: estado {}…", id_save.get(..8).unwrap_or(&id_save));
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
        let tipo = quality::tipo_por_faixas(rascunho.filtros.overall, rascunho.filtros.potencial);
        let estimativa = escolhido.as_ref().map(|o| {
            quality::estimar_missao(&quality::PedidoMissao {
                tier: o.tier,
                especializacao: o.especializacao,
                modo: rascunho.modo,
                tipo,
                amplitude: quality::AmplitudeGeografica::Mundo,
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
            filtros: previa.rascunho.filtros,
            modo_busca: previa.rascunho.modo,
            tipo: previa.tipo,
            amplitude: quality::AmplitudeGeografica::Mundo,
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

    /// Missões desta carreira, mais novas primeiro, com o Olheiro de cada
    /// uma (a aba Missões; a Story 2.3 acrescenta o progresso).
    pub fn missoes(&self) -> Vec<(Missao, Option<Olheiro>)> {
        let Some(estado) = self.estado_ativo() else {
            return Vec::new();
        };
        estado.ler(|dados| {
            dados
                .missoes
                .iter()
                .rev()
                .map(|m| (m.clone(), dados.olheiros.iter().find(|o| o.id == m.olheiro_id).cloned()))
                .collect()
        })
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
    use std::sync::atomic::{AtomicUsize, Ordering};
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
        let (m, dono) = &missoes[0];
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
}
