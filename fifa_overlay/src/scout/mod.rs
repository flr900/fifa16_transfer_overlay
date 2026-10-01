//! Central de Scout — estado raiz do painel e despacho para as telas.
//!
//! Duas regras da arquitetura moram aqui:
//! - **AD-6**: a tela ativa é uma pilha que nunca fica vazia; `stack[0]` é
//!   sempre uma das 4 abas fixas. Abrir/fechar o painel é um `bool`
//!   separado da pilha; ao fechar, a pilha volta para `[aba ativa]`, então
//!   reabrir sempre cai na aba de topo e nunca dentro de uma tela satélite.
//! - **AD-14**: o atalho (F10) é lido por polling de `GetAsyncKeyState`
//!   dentro de `render()`, com detecção de borda. Segurar a tecla alterna o
//!   painel UMA vez, só na transição solta→pressionada.

pub mod persistence;
pub mod screens;
pub mod search;
pub mod state;

use imgui::Ui;
use serde::{Deserialize, Serialize};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY, VK_F10};

use screens::theme::Fonts;
use state::ScoutState;

/// Atalho do painel. F10 foi escolhido com o Felipe (2026-09-30): o FIFA
/// 16 não usa F10 no modo carreira e o Companion Electron já usa
/// Ctrl+Shift+P.
const ATALHO_PAINEL: VIRTUAL_KEY = VK_F10;

/// As 4 abas fixas, na ordem da barra de abas. Persistida em `ui_prefs`
/// como `"olheiros"`, `"missoes"`, `"relatorios"`, `"sonar"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aba {
    Olheiros,
    Missoes,
    Relatorios,
    Sonar,
}

impl Aba {
    pub const TODAS: [Aba; 4] = [Aba::Olheiros, Aba::Missoes, Aba::Relatorios, Aba::Sonar];

    pub fn rotulo(self) -> &'static str {
        match self {
            Aba::Olheiros => "Olheiros",
            Aba::Missoes => "Missões",
            Aba::Relatorios => "Relatórios",
            Aba::Sonar => "Sonar",
        }
    }
}

/// Telas que abrem POR CIMA de uma aba (nomes do Structural Seed). Ainda
/// sem conteúdo: chegam nas próximas stories.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Satelite {
    ConfirmacaoContratacao,
    NovaMissao,
    SelecaoGeografica,
    FichaJogador,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoutScreen {
    Aba(Aba),
    #[allow(dead_code)]
    Satelite(Satelite),
}

/// Máximo de telas satélite empilhadas sobre a aba. AD-6 fala em
/// "profundidade máxima de 2" e o EXPERIENCE.md cita um caso de dois
/// níveis (Formulário Nova Missão → Painel de Seleção Geográfica), então
/// a pilha tem no máximo `[aba, satélite, satélite]`.
pub const MAX_SATELITES: usize = 2;

/// Pilha de navigação (AD-6). Nunca vazia: `stack[0]` é a aba ativa.
#[derive(Debug, Clone, PartialEq)]
pub struct Navigation {
    stack: Vec<ScoutScreen>,
}

impl Navigation {
    pub fn new(aba: Aba) -> Self {
        Navigation { stack: vec![ScoutScreen::Aba(aba)] }
    }

    pub fn aba_ativa(&self) -> Aba {
        match self.stack.first() {
            Some(ScoutScreen::Aba(aba)) => *aba,
            // Inalcançável pela API (a raiz é sempre uma aba), mas sem panic.
            _ => Aba::Olheiros,
        }
    }

    /// Tela no topo da pilha (a que está visível).
    #[allow(dead_code)]
    pub fn tela_atual(&self) -> ScoutScreen {
        self.stack.last().copied().unwrap_or(ScoutScreen::Aba(Aba::Olheiros))
    }

    #[allow(dead_code)]
    pub fn profundidade(&self) -> usize {
        self.stack.len()
    }

    /// Trocar de aba descarta as telas satélite da aba anterior.
    pub fn trocar_aba(&mut self, aba: Aba) {
        self.stack.clear();
        self.stack.push(ScoutScreen::Aba(aba));
    }

    /// Abre uma tela satélite. Recusa (com aviso) além de `MAX_SATELITES`.
    #[allow(dead_code)]
    pub fn push(&mut self, satelite: Satelite) -> bool {
        if self.stack.len() > MAX_SATELITES {
            tracing::warn!(
                "[scout] push de {:?} recusado: pilha já tem {} telas (máximo: aba + {} satélites).",
                satelite,
                self.stack.len(),
                MAX_SATELITES
            );
            return false;
        }
        self.stack.push(ScoutScreen::Satelite(satelite));
        true
    }

    /// Volta uma tela. Com só a aba na pilha, não faz nada.
    #[allow(dead_code)]
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Ao fechar o painel: fica só a aba ativa.
    pub fn reset_para_aba(&mut self) {
        self.stack.truncate(1);
    }
}

/// Estado raiz da Central de Scout.
pub struct Scout {
    painel_aberto: bool,
    tecla_estava_pressionada: bool,
    nav: Navigation,
    state: ScoutState,
}

impl Scout {
    pub fn new() -> Self {
        Scout {
            painel_aberto: false,
            tecla_estava_pressionada: false,
            nav: Navigation::new(Aba::Olheiros),
            state: ScoutState::new(),
        }
    }

    pub fn painel_aberto(&self) -> bool {
        self.painel_aberto
    }

    /// Detecção de borda do atalho (AD-14): só alterna na transição
    /// solta→pressionada. Devolve `true` quando alternou.
    pub fn atualizar_atalho(&mut self, pressionada: bool) -> bool {
        let borda = pressionada && !self.tecla_estava_pressionada;
        self.tecla_estava_pressionada = pressionada;
        if borda {
            self.alternar_painel();
        }
        borda
    }

    fn alternar_painel(&mut self) {
        self.painel_aberto = !self.painel_aberto;
        if self.painel_aberto {
            tracing::info!("[scout] Painel aberto (aba {:?}).", self.nav.aba_ativa());
            self.state.ao_abrir_painel();
        } else {
            tracing::info!("[scout] Painel fechado.");
            self.nav.reset_para_aba();
        }
    }

    /// Chamado a cada frame pelo `render()` do overlay.
    pub fn frame(&mut self, ui: &Ui, fonts: Option<&Fonts>) {
        self.atualizar_atalho(tecla_pressionada(ATALHO_PAINEL));
        if !self.painel_aberto {
            return;
        }
        self.state.tick();
        self.aplicar_aba_restaurada();
        screens::render_painel(ui, fonts, &mut self.nav, &mut self.state);
    }

    /// Carreira acabou de ficar ativa: a navegação vai para a aba salva
    /// dela (Story 1.3). Chamado logo após o `tick`, antes de desenhar.
    fn aplicar_aba_restaurada(&mut self) {
        if let Some(aba) = self.state.tomar_aba_restaurada() {
            if aba != self.nav.aba_ativa() {
                tracing::info!("[scout] Restaurando a aba salva da carreira: {aba:?}.");
                self.nav.trocar_aba(aba);
            }
        }
    }
}

/// Estado físico da tecla agora (bit mais alto de `GetAsyncKeyState`).
fn tecla_pressionada(tecla: VIRTUAL_KEY) -> bool {
    let estado = unsafe { GetAsyncKeyState(i32::from(tecla.0)) };
    (estado as u16) & 0x8000 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_starts_on_the_given_tab_and_never_empties() {
        let mut nav = Navigation::new(Aba::Missoes);
        assert_eq!(nav.aba_ativa(), Aba::Missoes);
        assert_eq!(nav.profundidade(), 1);
        nav.pop();
        nav.pop();
        assert_eq!(nav.profundidade(), 1, "pop com só a aba é no-op");
        assert_eq!(nav.tela_atual(), ScoutScreen::Aba(Aba::Missoes));
    }

    #[test]
    fn push_allows_two_satellites_and_refuses_the_third() {
        let mut nav = Navigation::new(Aba::Missoes);
        assert!(nav.push(Satelite::NovaMissao));
        assert!(nav.push(Satelite::SelecaoGeografica));
        assert!(!nav.push(Satelite::FichaJogador));
        assert_eq!(nav.profundidade(), 3);
        assert_eq!(nav.tela_atual(), ScoutScreen::Satelite(Satelite::SelecaoGeografica));
        nav.pop();
        assert_eq!(nav.tela_atual(), ScoutScreen::Satelite(Satelite::NovaMissao));
        assert_eq!(nav.aba_ativa(), Aba::Missoes);
    }

    #[test]
    fn switching_tab_drops_satellites_and_reset_keeps_only_the_tab() {
        let mut nav = Navigation::new(Aba::Olheiros);
        nav.push(Satelite::ConfirmacaoContratacao);
        nav.trocar_aba(Aba::Relatorios);
        assert_eq!(nav, Navigation::new(Aba::Relatorios));

        nav.push(Satelite::FichaJogador);
        nav.reset_para_aba();
        assert_eq!(nav, Navigation::new(Aba::Relatorios));
    }

    #[test]
    fn shortcut_toggles_once_per_press_even_when_held() {
        let mut scout = Scout::new();
        assert!(!scout.painel_aberto());

        // segurou por vários frames: abre uma vez só
        assert!(scout.atualizar_atalho(true));
        for _ in 0..30 {
            assert!(!scout.atualizar_atalho(true));
        }
        assert!(scout.painel_aberto());

        // soltou: nada muda; apertou de novo: fecha
        assert!(!scout.atualizar_atalho(false));
        assert!(scout.painel_aberto());
        assert!(scout.atualizar_atalho(true));
        assert!(!scout.painel_aberto());
    }

    #[test]
    fn reopening_lands_on_the_last_tab_without_satellites() {
        let mut scout = Scout::new();
        scout.atualizar_atalho(true);
        scout.atualizar_atalho(false);
        scout.nav.trocar_aba(Aba::Sonar);
        scout.nav.push(Satelite::FichaJogador);

        // fecha e reabre
        scout.atualizar_atalho(true);
        scout.atualizar_atalho(false);
        scout.atualizar_atalho(true);
        assert!(scout.painel_aberto());
        assert_eq!(scout.nav, Navigation::new(Aba::Sonar));
    }

    /// Carreira sempre pronta, sem varrer memória.
    struct CarreiraFixa;

    impl search::CareerSource for CarreiraFixa {
        fn start_locating(&self, _task: &crate::async_task::AsyncTask<()>) -> bool {
            false
        }
        fn read_snapshot(&self) -> Result<search::CareerSnapshot, crate::save_repo::SaveRepoError> {
            Ok(search::CareerSnapshot {
                orcamento_transferencias: 1,
                data_atual: crate::save_repo::Date(20260703),
                tecnico: "Senhor Manager".to_string(),
                id_save: "ab".repeat(32),
            })
        }
    }

    #[test]
    fn panel_reopens_on_the_tab_saved_for_the_career_after_a_restart() {
        let pasta = persistence::tests::PastaTemporaria::nova();
        let novo_scout = || Scout {
            state: ScoutState::com_fonte(Box::new(CarreiraFixa), Some(pasta.0.clone())),
            ..Scout::new()
        };
        let abrir = |scout: &mut Scout| {
            scout.atualizar_atalho(false);
            scout.atualizar_atalho(true);
            scout.state.tick();
            scout.aplicar_aba_restaurada();
        };

        let mut scout = novo_scout();
        abrir(&mut scout);
        assert_eq!(scout.nav.aba_ativa(), Aba::Olheiros);
        // o que a barra de abas faz no clique
        scout.nav.trocar_aba(Aba::Sonar);
        scout.state.definir_aba_ativa(Aba::Sonar);

        // jogo reiniciado: Scout novo começa em Olheiros e vai para Sonar
        let mut scout = novo_scout();
        assert_eq!(scout.nav.aba_ativa(), Aba::Olheiros);
        abrir(&mut scout);
        assert_eq!(scout.nav.aba_ativa(), Aba::Sonar);
    }

    #[test]
    fn tab_labels_follow_the_bar_order() {
        let rotulos: Vec<&str> = Aba::TODAS.iter().map(|a| a.rotulo()).collect();
        assert_eq!(rotulos, ["Olheiros", "Missões", "Relatórios", "Sonar"]);
    }
}
