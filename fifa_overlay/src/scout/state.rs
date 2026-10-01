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
// Só os campos que o ERD e as ADs já fixam. Especialização/Tier do
// Olheiro chegam nas Stories 1.4/1.5; filtros, modo de busca e Qualidade
// no Épico 2 — cada um com `#[serde(default)]` quando entrar, para os
// arquivos já gravados continuarem válidos.

/// Olheiro contratado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Olheiro {
    pub id: Uuid,
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
    }

    impl CareerSource for FonteFalsa {
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
        };
        (ScoutState::com_fonte(Box::new(fonte), diretorio), localizacoes)
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
}
