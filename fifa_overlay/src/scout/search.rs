//! `scout::search` — orquestrador de leitura do Scout (AD-3).
//!
//! Pelo AD-1, só esta camada do domínio conversa com o `save_repo`; as
//! telas falam com `scout::state`, que pede os dados aqui. Por enquanto
//! (Story 1.2) só existe a leitura do estado da carreira para o cabeçalho
//! e para os estados vazios do painel. Os 5 filtros de Missão (FR-4)
//! chegam no Épico 2.

use crate::async_task::AsyncTask;
use crate::save_repo::{self, Date, SaveRepoError};

/// O que o painel mostra da carreira ativa.
#[derive(Debug, Clone, PartialEq)]
pub struct CareerSnapshot {
    pub orcamento_transferencias: i32,
    pub data_atual: Date,
    /// "Nome Sobrenome" do técnico (`mPrV`), para o jogador reconhecer a carreira.
    pub tecnico: String,
    /// SHA-256 da identidade do save (AD-11): nome do arquivo de estado.
    pub id_save: String,
}

/// De onde vem o estado da carreira. Em produção é o `save_repo`; nos
/// testes de `scout::state` é uma fonte falsa (sem varrer memória).
pub trait CareerSource: Send + Sync {
    /// Dispara a localização (varredura pesada) no `AsyncTask` (AD-4).
    fn start_locating(&self, task: &AsyncTask<()>) -> bool;
    /// Leitura barata (poucos bytes) do estado vivo da carreira localizada.
    fn read_snapshot(&self) -> Result<CareerSnapshot, SaveRepoError>;
}

/// Fonte real: o `save_repo` da Story 1.1.
pub struct SaveRepoSource;

impl CareerSource for SaveRepoSource {
    fn start_locating(&self, task: &AsyncTask<()>) -> bool {
        save_repo::start_locating(task)
    }

    fn read_snapshot(&self) -> Result<CareerSnapshot, SaveRepoError> {
        let identidade = save_repo::read_career_identity()?;
        Ok(CareerSnapshot {
            orcamento_transferencias: save_repo::read_transfer_budget()?,
            data_atual: save_repo::read_current_date()?,
            tecnico: format!("{} {}", identidade.first_name, identidade.surname)
                .trim()
                .to_string(),
            id_save: identidade.hash(),
        })
    }
}
