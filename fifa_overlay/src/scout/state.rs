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

use std::time::{Duration, Instant};

use crate::async_task::{AsyncTask, TaskState};
use crate::save_repo::SaveRepoError;

use super::search::{CareerSnapshot, CareerSource, SaveRepoSource};

/// Com o painel aberto, relê o estado vivo nesse intervalo.
const INTERVALO_RELEITURA: Duration = Duration::from_secs(1);

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
}

impl ScoutState {
    pub fn new() -> Self {
        Self::com_fonte(Box::new(SaveRepoSource))
    }

    pub fn com_fonte(fonte: Box<dyn CareerSource>) -> Self {
        ScoutState {
            fonte,
            tarefa_localizar: AsyncTask::new(),
            status: CarreiraStatus::SemCarreira,
            localizacao_automatica_disponivel: false,
            ultima_leitura: None,
        }
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

    /// Chamado a cada frame com o painel aberto. Barato: um `poll` do
    /// `AsyncTask` e, no máximo, uma leitura de alguns bytes por segundo.
    pub fn tick(&mut self) {
        match self.tarefa_localizar.poll() {
            TaskState::Running => {
                self.status = CarreiraStatus::Localizando;
                return;
            }
            TaskState::Done(()) if self.status == CarreiraStatus::Localizando => {
                self.reler();
                return;
            }
            TaskState::Failed(err) if self.status == CarreiraStatus::Localizando => {
                self.status = status_de_erro(&err);
                self.ultima_leitura = Some(Instant::now());
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
    }

    fn reler(&mut self) {
        self.ultima_leitura = Some(Instant::now());
        match self.fonte.read_snapshot() {
            Ok(snapshot) => self.status = CarreiraStatus::Pronta(snapshot),
            Err(SaveRepoError::NaoLocalizado | SaveRepoError::CarreiraNaoCarregada)
                if self.localizacao_automatica_disponivel =>
            {
                self.iniciar_localizacao();
            }
            Err(err) => self.status = status_de_erro(&err),
        }
    }

    fn iniciar_localizacao(&mut self) {
        self.localizacao_automatica_disponivel = false;
        if self.fonte.start_locating(&self.tarefa_localizar) {
            tracing::info!("[scout::state] Localizando a carreira em background.");
            self.status = CarreiraStatus::Localizando;
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
    use crate::save_repo::Date;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// Fonte falsa: devolve as leituras de uma fila (a última se repete) e
    /// conta as localizações, que terminam com `resultado_localizacao`.
    struct FonteFalsa {
        leituras: Mutex<Vec<Result<CareerSnapshot, SaveRepoError>>>,
        resultado_localizacao: Result<(), SaveRepoError>,
        localizacoes: Arc<AtomicUsize>,
    }

    impl CareerSource for FonteFalsa {
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
        }
    }

    fn estado(
        leituras: Vec<Result<CareerSnapshot, SaveRepoError>>,
        resultado_localizacao: Result<(), SaveRepoError>,
    ) -> (ScoutState, Arc<AtomicUsize>) {
        let localizacoes = Arc::new(AtomicUsize::new(0));
        let fonte = FonteFalsa {
            leituras: Mutex::new(leituras),
            resultado_localizacao,
            localizacoes: Arc::clone(&localizacoes),
        };
        (ScoutState::com_fonte(Box::new(fonte)), localizacoes)
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
}
