//! `AsyncTask<T>` — encapsula o padrão "thread de background + resultado
//! compartilhado via `Arc<Mutex<..>>` + flag `AtomicBool`" que antes era
//! repetido à mão 3× em `lib.rs` (`ScanState`, `PointerScanState`,
//! `ValueScanState`).
//!
//! Regras (AD-4 da Architecture Spine):
//! - Qualquer varredura pesada (memória inteira / `CZUM` inteira) roda
//!   por aqui, NUNCA dentro de `ImguiRenderLoop::render` (bloqueia o
//!   render do jogo inteiro — ver `PROJECT_MEMORY.md`, sessão 5, Bug 1).
//! - `Failed` é uma variante IRMÃ de `Done`; o erro nunca vai embutido
//!   em `T`.
//! - `poll()` NÃO consome o estado: o ImGui é modo imediato e chama o
//!   render todo frame, então a tela precisa poder ler o resultado
//!   quantas vezes quiser.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::save_repo::SaveRepoError;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskState<T> {
    Idle,
    Running,
    Done(T),
    Failed(SaveRepoError),
}

pub struct AsyncTask<T> {
    state: Arc<Mutex<TaskState<T>>>,
    in_progress: Arc<AtomicBool>,
}

impl<T: Send + 'static> AsyncTask<T> {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(TaskState::Idle)),
            in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Dispara `job` numa thread própria. Retorna `false` (e não faz
    /// nada) se já houver uma execução em andamento — evita o scan
    /// duplicado (AD-8/AD-9 dependem disso).
    pub fn start<F>(&self, job: F) -> bool
    where
        F: FnOnce() -> Result<T, SaveRepoError> + Send + 'static,
    {
        if self
            .in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }

        set_state(&self.state, TaskState::Running);

        let state = Arc::clone(&self.state);
        let in_progress = Arc::clone(&self.in_progress);

        std::thread::spawn(move || {
            // Um panic Rust dentro do job não pode deixar o estado preso em
            // `Running` para sempre. (Exceções de hardware, tipo access
            // violation, NÃO são capturadas aqui — por isso toda leitura
            // de memória usa `ReadProcessMemory`, ver `memscan.rs`.)
            let outcome = match catch_unwind(AssertUnwindSafe(job)) {
                Ok(Ok(value)) => TaskState::Done(value),
                Ok(Err(err)) => TaskState::Failed(err),
                Err(_) => TaskState::Failed(SaveRepoError::Interno(
                    "a tarefa em background entrou em pânico".to_string(),
                )),
            };
            set_state(&state, outcome);
            in_progress.store(false, Ordering::SeqCst);
        });

        true
    }

    /// Leitura não destrutiva e idempotente do estado atual.
    pub fn poll(&self) -> TaskState<T>
    where
        T: Clone,
    {
        match self.state.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.in_progress.load(Ordering::SeqCst)
    }

    /// Volta para `Idle` (ex.: antes de re-localizar a carreira). No-op
    /// enquanto houver execução em andamento.
    pub fn reset(&self) {
        if !self.is_running() {
            set_state(&self.state, TaskState::Idle);
        }
    }
}

impl<T: Send + 'static> Default for AsyncTask<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn set_state<T>(state: &Arc<Mutex<TaskState<T>>>, new_state: TaskState<T>) {
    match state.lock() {
        Ok(mut guard) => *guard = new_state,
        Err(poisoned) => *poisoned.into_inner() = new_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_until_finished(task: &AsyncTask<u32>) -> TaskState<u32> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = task.poll();
            if !matches!(state, TaskState::Running | TaskState::Idle) {
                return state;
            }
            assert!(Instant::now() < deadline, "tarefa não terminou a tempo");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn starts_idle() {
        let task: AsyncTask<u32> = AsyncTask::new();
        assert_eq!(task.poll(), TaskState::Idle);
        assert!(!task.is_running());
    }

    #[test]
    fn done_path_and_poll_is_non_destructive() {
        let task = AsyncTask::new();
        assert!(task.start(|| Ok(42u32)));
        assert_eq!(wait_until_finished(&task), TaskState::Done(42));
        // segunda leitura devolve o mesmo resultado (não consumiu)
        assert_eq!(task.poll(), TaskState::Done(42));
        assert!(!task.is_running());
    }

    #[test]
    fn failed_is_a_sibling_of_done() {
        let task: AsyncTask<u32> = AsyncTask::new();
        assert!(task.start(|| Err(SaveRepoError::CarreiraNaoCarregada)));
        assert_eq!(
            wait_until_finished(&task),
            TaskState::Failed(SaveRepoError::CarreiraNaoCarregada)
        );
    }

    #[test]
    fn second_start_while_running_is_ignored() {
        let task = AsyncTask::new();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        assert!(task.start(move || {
            let _ = release_rx.recv();
            Ok(1u32)
        }));
        assert!(task.is_running());
        assert!(!task.start(|| Ok(2u32)), "segundo start deveria ser ignorado");
        release_tx.send(()).unwrap();
        assert_eq!(wait_until_finished(&task), TaskState::Done(1));
    }

    #[test]
    fn panic_in_job_becomes_failed_not_stuck_running() {
        let task: AsyncTask<u32> = AsyncTask::new();
        assert!(task.start(|| panic!("boom")));
        assert!(matches!(
            wait_until_finished(&task),
            TaskState::Failed(SaveRepoError::Interno(_))
        ));
        assert!(!task.is_running());
    }

    #[test]
    fn can_restart_after_finishing_and_reset_goes_idle() {
        let task = AsyncTask::new();
        assert!(task.start(|| Ok(1u32)));
        assert_eq!(wait_until_finished(&task), TaskState::Done(1));
        task.reset();
        assert_eq!(task.poll(), TaskState::Idle);
        assert!(task.start(|| Ok(2u32)));
        assert_eq!(wait_until_finished(&task), TaskState::Done(2));
    }
}
