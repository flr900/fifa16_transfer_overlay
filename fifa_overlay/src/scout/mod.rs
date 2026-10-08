//! Central de Scout — estado raiz do painel e despacho para as telas.
//!
//! Duas regras da arquitetura moram aqui:
//! - **AD-6**: a tela ativa é uma pilha que nunca fica vazia; `stack[0]` é
//!   sempre uma das abas fixas (5 desde o Épico 6, com Escolhidos). Abrir/fechar o painel é um `bool`
//!   separado da pilha; ao fechar, a pilha volta para `[aba ativa]`, então
//!   reabrir sempre cai na aba de topo e nunca dentro de uma tela satélite.
//! - **AD-14**: o atalho (F10) é lido por polling de `GetAsyncKeyState`
//!   dentro de `render()`, com detecção de borda. Segurar a tecla alterna o
//!   painel UMA vez, só na transição solta→pressionada.
//!
//! Controle (Story 1.6, revisto em 2026-10-01 e 2026-10-03): o mesmo vale
//! para o combo `COMBO_PAINEL`. Com o painel aberto, só LB/RB trocam de
//! aba — de qualquer tela; com a Nova Missão aberta, o jogador confirma
//! antes que o rascunho será descartado (`pedir_troca_de_aba`); o
//! analógico direito rola a tela; B volta uma tela (fecha o modal; na
//! raiz, fecha o painel); D-pad/analógico (inclusive ←/→) e A são a navegação do
//! próprio ImGui (`gamepad::para_navegacao`). A barra de abas não recebe
//! foco do controle; ao abrir o painel, trocar de aba ou de tela, o foco
//! vai para o primeiro item da tela (`Navigation::tomar_foco_pendente`). Enquanto o painel está aberto, e depois de fechar até
//! todos os botões serem soltos, o jogo recebe o controle parado
//! (`bloqueia_controle`, aplicado em `crate::gamepad`): sem isso o B ou o
//! START que fechou o painel chegaria ao FIFA ao ser solto.

pub mod lista;
pub mod minifaces;
pub mod nomes;
pub mod persistence;
pub mod quality;
pub mod screens;
pub mod search;
pub mod state;

use imgui::Ui;
use serde::{Deserialize, Serialize};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY, VK_F10};

use crate::gamepad::{botao, EstadoControle};
use screens::theme::Fonts;
use state::ScoutState;

/// Atalho do controle: L3 + START (o mesmo combo validado no Companion
/// Electron: L3+R3 conflita com um atalho do FIFA e os paddles do 8BitDo
/// não emitem XInput — `PROJECT_MEMORY.md`, "Por que o atalho de controle
/// usa LEFT_THUMB+START").
pub const COMBO_PAINEL: u16 = botao::L3 | botao::START;

/// O que o controle pediu neste frame (bordas solto→apertado).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComandosControle {
    pub alternar_painel: bool,
    pub aba_anterior: bool,
    pub proxima_aba: bool,
    pub voltar: bool,
    /// Y: o menu de Opções da tela (filtros, ou o que fazer com o Olheiro
    /// em foco). Cada tela decide o que ele abre (`ScoutState::opcoes_pedidas`).
    pub opcoes: bool,
}

pub fn comandos_controle(anterior: EstadoControle, atual: EstadoControle) -> ComandosControle {
    let borda = |mascara: u16| atual.segura(mascara) && !anterior.segura(mascara);
    ComandosControle {
        alternar_painel: borda(COMBO_PAINEL),
        aba_anterior: borda(botao::LB),
        proxima_aba: borda(botao::RB),
        voltar: borda(botao::B),
        opcoes: borda(botao::Y),
    }
}

/// Atalho do painel. F10 foi escolhido com o Felipe (2026-09-30): o FIFA
/// 16 não usa F10 no modo carreira e o Companion Electron já usa
/// Ctrl+Shift+P.
const ATALHO_PAINEL: VIRTUAL_KEY = VK_F10;

/// As abas fixas, na ordem da barra de abas. Persistida em `ui_prefs`
/// como `"olheiros"`, `"missoes"`, `"relatorios"`, `"escolhidos"`,
/// `"base"` (a Base do Scout, 2026-10-08, tomou o lugar do Sonar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aba {
    Olheiros,
    Missoes,
    Relatorios,
    /// Os jogadores que já passaram pelo scout do clube (2026-10-08).
    Base,
    Escolhidos,
}

impl Aba {
    pub const TODAS: [Aba; 5] = [Aba::Olheiros, Aba::Missoes, Aba::Relatorios, Aba::Base, Aba::Escolhidos];

    /// Aba vizinha na barra (LB/RB), dando a volta nas pontas.
    pub fn vizinha(self, passo: isize) -> Aba {
        let n = Aba::TODAS.len() as isize;
        let atual = Aba::TODAS.iter().position(|&a| a == self).unwrap_or(0) as isize;
        let indice = (atual + passo).rem_euclid(n) as usize;
        Aba::TODAS.get(indice).copied().unwrap_or(self)
    }

    pub fn rotulo(self) -> &'static str {
        match self {
            Aba::Olheiros => "Olheiros",
            Aba::Missoes => "Missões",
            Aba::Relatorios => "Relatórios",
            Aba::Base => "Base do Scout",
            Aba::Escolhidos => "Escolhidos",
        }
    }
}

/// Telas que abrem POR CIMA de uma aba (nomes do Structural Seed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Satelite {
    ConfirmacaoContratacao,
    /// As 12 ofertas de Olheiro (aba Olheiros → "Contratar Olheiro").
    ContratarOlheiro,
    /// Nova Missão, passo 1: escolher o Olheiro (2026-10-03).
    EscolherOlheiro,
    NovaMissao,
    SelecaoGeografica,
    /// Ficha de um jogador do Relatório aberto (Story 3.1).
    FichaJogador,
    /// Relatório aberto (Story 2.5), a partir de Missões ou Relatórios.
    Relatorio,
    /// Painel de campo "Atributo dominante" sobre o formulário (Story 2.8).
    CampoAtributo,
    /// Painel de campo "Fit Posicional" sobre o formulário (Story 3.4).
    CampoFit,
    /// O seletor de elenco, um só para os dois papéis (AD-13).
    SeletorElenco(ContextoSeletor),
    /// Designar Generalistas para a Lista de Escolhidos (Épico 6).
    AcompanhamentoOlheiros,
}

/// Para que o seletor de elenco foi aberto (AD-13): o rótulo da tela diz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextoSeletor {
    /// Jogador de Referência do formulário Nova Missão (Story 3.3).
    FiltroMissao,
    /// Jogador a sobrepor no Radar da Ficha (Story 3.2).
    ComparacaoFicha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoutScreen {
    Aba(Aba),
    Satelite(Satelite),
}

/// Máximo de telas satélite empilhadas sobre a aba. AD-6 fala em
/// "profundidade máxima de 2" (Formulário Nova Missão → Painel de Seleção
/// Geográfica); o Épico 3 precisa de 3 — Relatório → Ficha → seletor de
/// elenco (emenda do AD-6, Story 3.2). A pilha tem no máximo `[aba,
/// satélite, satélite, satélite]`.
pub const MAX_SATELITES: usize = 3;

/// Pilha de navigação (AD-6). Nunca vazia: `stack[0]` é a aba ativa.
#[derive(Debug, Clone, PartialEq)]
pub struct Navigation {
    stack: Vec<ScoutScreen>,
    /// A tela visível mudou: o foco do controle vai para o primeiro item
    /// dela no próximo frame (senão ficava "perdido" e só o analógico para
    /// cima o trazia de volta — Felipe, 2026-10-01).
    foco_pendente: bool,
}

impl Navigation {
    pub fn new(aba: Aba) -> Self {
        Navigation { stack: vec![ScoutScreen::Aba(aba)], foco_pendente: true }
    }

    /// Pede o foco no primeiro item da tela (ex.: o painel acabou de abrir).
    pub fn pedir_foco(&mut self) {
        self.foco_pendente = true;
    }

    /// Consome o pedido de foco (devolve se havia).
    pub fn tomar_foco_pendente(&mut self) -> bool {
        std::mem::take(&mut self.foco_pendente)
    }

    pub fn aba_ativa(&self) -> Aba {
        match self.stack.first() {
            Some(ScoutScreen::Aba(aba)) => *aba,
            // Inalcançável pela API (a raiz é sempre uma aba), mas sem panic.
            _ => Aba::Olheiros,
        }
    }

    /// Tela no topo da pilha (a que está visível).
    pub fn tela_atual(&self) -> ScoutScreen {
        self.stack.last().copied().unwrap_or(ScoutScreen::Aba(Aba::Olheiros))
    }

    /// Tela logo abaixo do topo (a que aparece por baixo de um modal).
    pub fn tela_abaixo(&self) -> ScoutScreen {
        let n = self.stack.len();
        self.stack.get(n.saturating_sub(2)).copied().unwrap_or(ScoutScreen::Aba(self.aba_ativa()))
    }

    #[allow(dead_code)]
    pub fn profundidade(&self) -> usize {
        self.stack.len()
    }

    /// Trocar de aba descarta as telas satélite da aba anterior.
    pub fn trocar_aba(&mut self, aba: Aba) {
        self.stack.clear();
        self.stack.push(ScoutScreen::Aba(aba));
        self.foco_pendente = true;
    }

    /// Abre uma tela satélite. Recusa (com aviso) além de `MAX_SATELITES`.
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
        self.foco_pendente = true;
        true
    }

    /// Volta uma tela. Com só a aba na pilha, não faz nada.
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.foco_pendente = true;
        }
    }

    /// A tela satélite está em algum lugar da pilha (ex.: o formulário
    /// Nova Missão por baixo de um painel de campo).
    pub fn contem(&self, satelite: Satelite) -> bool {
        self.stack.contains(&ScoutScreen::Satelite(satelite))
    }

    /// Ao fechar o painel: fica só a aba ativa.
    pub fn reset_para_aba(&mut self) {
        self.stack.truncate(1);
        self.foco_pendente = true;
    }
}

/// Estado raiz da Central de Scout.
pub struct Scout {
    painel_aberto: bool,
    tecla_estava_pressionada: bool,
    nav: Navigation,
    state: ScoutState,
    controle_anterior: EstadoControle,
    /// O painel fechou com algum botão do controle apertado: o jogo segue
    /// bloqueado até tudo ser solto.
    esperando_soltar: bool,
    /// Último valor de `bloqueia_controle` que foi para o log (diagnóstico
    /// do "controle travou": só as transições, nunca por frame).
    bloqueio_logado: bool,
}

impl Scout {
    pub fn new() -> Self {
        Scout {
            painel_aberto: false,
            tecla_estava_pressionada: false,
            nav: Navigation::new(Aba::Olheiros),
            controle_anterior: EstadoControle::default(),
            esperando_soltar: false,
            bloqueio_logado: false,
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
            self.nav.pedir_foco();
            self.state.ao_abrir_painel();
        } else {
            tracing::info!("[scout] Painel fechado.");
            self.nav.reset_para_aba();
            self.state.ao_fechar_painel();
        }
    }

    /// Chamado a cada frame pelo `render()` do overlay. O estado anda com
    /// o painel fechado também (o vigia da carreira — Story 1.7); fechado,
    /// só o banner do canto da tela é desenhado, e só quando há aviso.
    pub fn frame(&mut self, ui: &Ui, fonts: Option<&Fonts>, controle: Option<EstadoControle>) {
        let alternou = self.atualizar_atalho(tecla_pressionada(ATALHO_PAINEL));
        self.aplicar_controle(controle.unwrap_or_default(), alternou);
        self.registrar_bloqueio(controle.unwrap_or_default());
        let ry = controle.map_or(0, |c| c.ry);
        let rolagem = if self.painel_aberto { crate::gamepad::rolagem_do_analogico(ry) } else { 0.0 };
        self.state.definir_rolagem(rolagem);
        self.state.tick();
        self.aplicar_aba_restaurada();
        if self.painel_aberto {
            screens::render_painel(ui, fonts, &mut self.nav, &mut self.state);
        } else if let Some(aviso) = self.state.aviso_visivel(std::time::Instant::now()) {
            screens::aviso::render(ui, fonts, aviso);
        }
    }

    /// Comandos do controle (Story 1.6). `ja_alternou`: o F10 já alternou
    /// o painel neste frame (não alternar duas vezes).
    fn aplicar_controle(&mut self, atual: EstadoControle, ja_alternou: bool) {
        let comandos = comandos_controle(self.controle_anterior, atual);
        self.controle_anterior = atual;
        // o Y vale por um frame só, e só sem aviso por cima (as telas leem
        // `opcoes_pedidas` no render deste mesmo frame)
        self.state.definir_opcoes(false);
        // `ja_alternou` com o painel fechado agora = o F10 acabou de fechá-lo
        let estava_aberto = self.painel_aberto || ja_alternou;

        if comandos.alternar_painel && !ja_alternou {
            self.alternar_painel();
        } else if self.painel_aberto {
            if self.state.troca_de_aba_pendente().is_some() {
                // aviso "Sair da Nova Missão?" aberto: B continua editando;
                // A é dos botões do aviso (ImGui)
                if comandos.voltar {
                    self.state.definir_troca_de_aba_pendente(None);
                    self.nav.pedir_foco();
                }
            } else if let Some((_, passo)) = self.state.opcoes_do_olheiro().map(|(c, p)| (c, p)) {
                // janela de Opções do Olheiro aberta: B volta ao menu (na
                // confirmação) ou fecha; A é dos botões
                if comandos.voltar {
                    if passo == state::PassoOpcoes::ConfirmarCancelamento {
                        self.state.definir_passo_das_opcoes(state::PassoOpcoes::Menu);
                    } else {
                        self.state.fechar_opcoes_do_olheiro();
                        self.nav.pedir_foco();
                    }
                }
            } else if self.state.painel_de_filtros().is_some() {
                // painel de filtros (Y) aberto: B fecha; A é dos botões
                if comandos.voltar {
                    self.state.fechar_painel_de_filtros();
                    self.nav.pedir_foco();
                }
            } else if self.state.demissao_pendente().is_some() {
                // aviso "Demitir Olheiro?" aberto: B cancela; A é dos botões
                if comandos.voltar {
                    self.state.cancelar_demissao();
                    self.nav.pedir_foco();
                }
            } else {
                if comandos.opcoes {
                    self.state.definir_opcoes(true);
                }
                if comandos.aba_anterior || comandos.proxima_aba {
                    let passo = if comandos.proxima_aba { 1 } else { -1 };
                    let aba = self.nav.aba_ativa().vizinha(passo);
                    pedir_troca_de_aba(&mut self.nav, &mut self.state, aba);
                }
                if comandos.voltar {
                    self.voltar();
                }
            }
        }

        if self.painel_aberto {
            self.esperando_soltar = false;
        } else if estava_aberto && !atual.solto() {
            // fechou agora, com o botão ainda apertado. Só no fechamento:
            // com o painel já fechado, o B é do jogo (chutar) e não pode
            // bloquear o controle inteiro.
            self.esperando_soltar = true;
        } else if atual.solto() {
            self.esperando_soltar = false;
        }
    }

    /// B do controle: fecha a tela satélite (o modal cancela a
    /// contratação, nada é debitado); na raiz, fecha o painel.
    fn voltar(&mut self) {
        match self.nav.tela_atual() {
            ScoutScreen::Satelite(satelite) => {
                match satelite {
                    Satelite::ConfirmacaoContratacao => self.state.cancelar_contratacao(),
                    Satelite::NovaMissao => self.state.cancelar_nova_missao(),
                    Satelite::Relatorio => self.state.fechar_relatorio(),
                    Satelite::FichaJogador => self.state.fechar_ficha(),
                    // na árvore geográfica, B sobe um nível antes de sair
                    Satelite::SelecaoGeografica if self.state.subir_foco_geografico() => {
                        self.nav.pedir_foco();
                        return;
                    }
                    _ => {}
                }
                self.nav.pop();
            }
            ScoutScreen::Aba(_) => self.alternar_painel(),
        }
    }

    /// No `before_render`: sobe para a GPU os rostos já lidos (Story 2.6).
    pub fn enviar_minifaces(&self, carregar: &mut dyn FnMut(&crate::dds::Imagem, Option<imgui::TextureId>) -> Option<imgui::TextureId>) {
        self.state.minifaces().enviar(carregar);
        self.state.bandeiras().enviar(carregar);
    }

    /// Loga cada vez que o jogo passa a receber (ou deixa de receber) o
    /// controle parado, com o motivo e o estado dos botões.
    fn registrar_bloqueio(&mut self, atual: EstadoControle) {
        let bloqueado = self.bloqueia_controle();
        if bloqueado == self.bloqueio_logado {
            return;
        }
        self.bloqueio_logado = bloqueado;
        tracing::info!(
            "[scout] Controle do jogo {} (painel_aberto={}, esperando_soltar={}, botoes=0x{:04X}, lt={}, rt={}).",
            if bloqueado { "BLOQUEADO" } else { "liberado" },
            self.painel_aberto,
            self.esperando_soltar,
            atual.botoes,
            atual.lt,
            atual.rt,
        );
    }

    /// O jogo deve receber o controle parado neste frame?
    pub fn bloqueia_controle(&self) -> bool {
        self.painel_aberto || self.esperando_soltar
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

/// Troca de aba pedida por LB/RB ou pela barra de abas, de qualquer tela.
/// Com a Nova Missão aberta (filtros ainda não confirmados), só marca a
/// troca como pendente: o painel pergunta se o jogador quer descartar o
/// rascunho. Senão, troca já.
pub fn pedir_troca_de_aba(nav: &mut Navigation, state: &mut ScoutState, aba: Aba) {
    if nav.contem(Satelite::NovaMissao) {
        state.definir_troca_de_aba_pendente(Some(aba));
        return;
    }
    trocar_aba_agora(nav, state, aba);
}

/// Troca de aba descartando o que estava aberto (contratação, rascunho de
/// Missão, Relatório e Ficha).
pub fn trocar_aba_agora(nav: &mut Navigation, state: &mut ScoutState, aba: Aba) {
    state.definir_troca_de_aba_pendente(None);
    state.fechar_painel_de_filtros();
    state.fechar_opcoes_do_olheiro();
    state.cancelar_contratacao();
    state.cancelar_nova_missao();
    state.fechar_relatorio();
    nav.trocar_aba(aba);
    state.definir_aba_ativa(aba);
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
    fn push_allows_three_satellites_and_refuses_the_fourth() {
        let mut nav = Navigation::new(Aba::Relatorios);
        assert!(nav.push(Satelite::Relatorio));
        assert!(nav.push(Satelite::FichaJogador));
        let seletor = Satelite::SeletorElenco(ContextoSeletor::ComparacaoFicha);
        assert!(nav.push(seletor));
        assert!(!nav.push(Satelite::CampoFit));
        assert_eq!(nav.profundidade(), 4);
        assert_eq!(nav.tela_atual(), ScoutScreen::Satelite(seletor));
        nav.pop();
        assert_eq!(nav.tela_atual(), ScoutScreen::Satelite(Satelite::FichaJogador));
        assert_eq!(nav.aba_ativa(), Aba::Relatorios);
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
        scout.nav.trocar_aba(Aba::Base);
        scout.nav.push(Satelite::FichaJogador);

        // fecha e reabre
        scout.atualizar_atalho(true);
        scout.atualizar_atalho(false);
        scout.atualizar_atalho(true);
        assert!(scout.painel_aberto());
        assert_eq!(scout.nav, Navigation::new(Aba::Base));
    }

    /// Carreira sempre pronta, sem varrer memória.
    struct CarreiraFixa;

    impl search::CareerSource for CarreiraFixa {
        fn start_locating(&self, _task: &crate::async_task::AsyncTask<()>) -> bool {
            false
        }
        fn start_career_probe(&self, _task: &crate::async_task::AsyncTask<bool>) -> bool {
            false
        }
        fn write_transfer_budget(&self, _anterior: i32, novo: i32) -> Result<i32, crate::save_repo::SaveRepoError> {
            Ok(novo)
        }
        fn read_all_players(&self) -> Result<crate::save_repo::PlayerPool, crate::save_repo::SaveRepoError> {
            Err(crate::save_repo::SaveRepoError::NaoLocalizado)
        }
        fn read_nations(&self) -> Result<Vec<crate::save_repo::Nacao>, crate::save_repo::SaveRepoError> {
            Ok(Vec::new())
        }
        fn read_snapshot(&self) -> Result<search::CareerSnapshot, crate::save_repo::SaveRepoError> {
            Ok(search::CareerSnapshot {
                orcamento_transferencias: 1,
                data_atual: crate::save_repo::Date(20260703),
                tecnico: "Senhor Manager".to_string(),
                id_save: "ab".repeat(32),
                data_do_save: crate::save_repo::Date(20260703),
                folha_salarial: None,
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
        scout.nav.trocar_aba(Aba::Base);
        scout.state.definir_aba_ativa(Aba::Base);

        // jogo reiniciado: Scout novo começa em Olheiros e vai para a Base
        let mut scout = novo_scout();
        assert_eq!(scout.nav.aba_ativa(), Aba::Olheiros);
        abrir(&mut scout);
        assert_eq!(scout.nav.aba_ativa(), Aba::Base);
    }

    #[test]
    fn closing_the_panel_during_a_hire_confirmation_cancels_it() {
        let mut scout = Scout { state: ScoutState::com_fonte(Box::new(CarreiraFixa), None), ..Scout::new() };
        scout.atualizar_atalho(true);
        scout.atualizar_atalho(false);
        scout.state.preparar_contratacao(oferta());
        scout.nav.push(Satelite::ConfirmacaoContratacao);
        assert!(scout.state.previa_contratacao().is_some());

        scout.atualizar_atalho(true); // F10 fecha
        assert!(!scout.painel_aberto());
        assert_eq!(scout.state.previa_contratacao(), None);
        assert_eq!(scout.nav.tela_atual(), ScoutScreen::Aba(Aba::Olheiros));
    }

    fn oferta() -> state::OfertaOlheiro {
        let id = uuid::Uuid::new_v4();
        let olheiro = state::Olheiro { id, nome: "Teste".to_string(), ..Default::default() };
        state::OfertaOlheiro { id, olheiro, custo: 300_000, faltam: None }
    }

    fn controle(botoes: u16) -> EstadoControle {
        EstadoControle { botoes, ..Default::default() }
    }

    #[test]
    fn controller_combo_toggles_once_and_the_game_stays_blocked_until_release() {
        let mut scout = Scout::new();
        // L3 primeiro, depois START: abre na borda do combo
        scout.aplicar_controle(controle(botao::L3), false);
        assert!(!scout.painel_aberto());
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        assert!(scout.painel_aberto());
        assert!(scout.bloqueia_controle());
        for _ in 0..10 {
            scout.aplicar_controle(controle(COMBO_PAINEL), false);
        }
        assert!(scout.painel_aberto(), "segurar não alterna de novo");

        // solta e aperta de novo: fecha, mas o jogo só volta a receber o
        // controle quando tudo for solto
        scout.aplicar_controle(controle(0), false);
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        assert!(!scout.painel_aberto());
        assert!(scout.bloqueia_controle());
        scout.aplicar_controle(controle(botao::START), false);
        assert!(scout.bloqueia_controle());
        scout.aplicar_controle(controle(0), false);
        assert!(!scout.bloqueia_controle());
    }

    #[test]
    fn b_during_the_match_never_blocks_the_game() {
        let mut scout = Scout::new();
        // correndo (RT) e chutando (B) com o painel fechado
        let chute = EstadoControle { botoes: botao::B, rt: 255, ly: 30_000, ..Default::default() };
        scout.aplicar_controle(EstadoControle { rt: 255, ..Default::default() }, false);
        scout.aplicar_controle(chute, false);
        assert!(!scout.painel_aberto());
        assert!(!scout.bloqueia_controle(), "B no jogo não é do painel");
        scout.aplicar_controle(EstadoControle { rt: 255, ..Default::default() }, false);
        assert!(!scout.bloqueia_controle());
    }

    #[test]
    fn closing_with_f10_while_holding_a_button_waits_for_release() {
        let mut scout = Scout::new();
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        assert!(scout.painel_aberto());
        // F10 fechou neste frame (`atualizar_atalho`), com A apertado
        scout.painel_aberto = false;
        scout.aplicar_controle(controle(botao::A), true);
        assert!(scout.bloqueia_controle());
        scout.aplicar_controle(controle(0), false);
        assert!(!scout.bloqueia_controle());
    }

    #[test]
    fn f10_and_the_combo_in_the_same_frame_toggle_once() {
        let mut scout = Scout::new();
        scout.aplicar_controle(controle(COMBO_PAINEL), true);
        assert!(!scout.painel_aberto(), "o F10 já alternou; o combo não desfaz");
    }

    #[test]
    fn lb_rb_switch_tabs_from_any_screen_but_ask_before_dropping_a_new_missao() {
        let mut scout = Scout { state: ScoutState::com_fonte(Box::new(CarreiraFixa), None), ..Scout::new() };
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        scout.nav.trocar_aba(Aba::Relatorios);
        scout.nav.push(Satelite::Relatorio);
        scout.nav.push(Satelite::FichaJogador);
        scout.aplicar_controle(controle(botao::RB), false);
        scout.aplicar_controle(controle(0), false);
        assert_eq!(scout.nav.tela_atual(), ScoutScreen::Aba(Aba::Base), "saiu da Ficha");
        assert_eq!(scout.nav.profundidade(), 1);

        // com a Nova Missão aberta: pergunta antes
        scout.nav.trocar_aba(Aba::Missoes);
        scout.nav.push(Satelite::EscolherOlheiro);
        scout.nav.push(Satelite::NovaMissao);
        scout.aplicar_controle(controle(botao::LB), false);
        scout.aplicar_controle(controle(0), false);
        assert_eq!(scout.state.troca_de_aba_pendente(), Some(Aba::Olheiros));
        assert_eq!(scout.nav.tela_atual(), ScoutScreen::Satelite(Satelite::NovaMissao), "ainda no formulário");
        // B: continua editando
        scout.aplicar_controle(controle(botao::B), false);
        scout.aplicar_controle(controle(0), false);
        assert_eq!(scout.state.troca_de_aba_pendente(), None);
        assert_eq!(scout.nav.tela_atual(), ScoutScreen::Satelite(Satelite::NovaMissao));
        // confirmou o aviso: troca
        scout.aplicar_controle(controle(botao::LB), false);
        trocar_aba_agora(&mut scout.nav, &mut scout.state, Aba::Olheiros);
        assert_eq!(scout.nav.tela_atual(), ScoutScreen::Aba(Aba::Olheiros));
        assert_eq!(scout.state.troca_de_aba_pendente(), None);
    }

    #[test]
    fn lb_rb_switch_tabs_with_wraparound() {
        let mut scout = Scout::new();
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        scout.aplicar_controle(controle(botao::RB), false);
        assert_eq!(scout.nav.aba_ativa(), Aba::Missoes);
        scout.aplicar_controle(controle(0), false);
        scout.aplicar_controle(controle(botao::LB), false);
        scout.aplicar_controle(controle(0), false);
        scout.aplicar_controle(controle(botao::LB), false);
        assert_eq!(scout.nav.aba_ativa(), Aba::Escolhidos, "dá a volta");

        // com uma tela satélite aberta, LB/RB também trocam (2026-10-03)
        scout.aplicar_controle(controle(0), false);
        scout.nav.push(Satelite::FichaJogador);
        scout.aplicar_controle(controle(botao::RB), false);
        assert_eq!(scout.nav.aba_ativa(), Aba::Olheiros);
        assert_eq!(scout.nav.profundidade(), 1);
    }

    #[test]
    fn b_closes_the_hire_modal_first_and_then_the_panel() {
        let mut scout = Scout { state: ScoutState::com_fonte(Box::new(CarreiraFixa), None), ..Scout::new() };
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        scout.state.preparar_contratacao(oferta());
        scout.nav.push(Satelite::ConfirmacaoContratacao);

        scout.aplicar_controle(controle(botao::B), false);
        assert!(scout.painel_aberto());
        assert_eq!(scout.nav.profundidade(), 1);
        assert_eq!(scout.state.previa_contratacao(), None, "B cancela a contratação");

        scout.aplicar_controle(controle(0), false);
        scout.aplicar_controle(controle(botao::B), false);
        assert!(!scout.painel_aberto());
        assert!(scout.bloqueia_controle(), "B ainda apertado");
        scout.aplicar_controle(controle(0), false);
        assert!(!scout.bloqueia_controle());
    }

    #[test]
    fn b_on_the_new_missao_form_goes_back_to_the_olheiro_step_without_saving() {
        let mut scout = Scout { state: ScoutState::com_fonte(Box::new(CarreiraFixa), None), ..Scout::new() };
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        scout.state.tick();
        scout.nav.trocar_aba(Aba::Missoes);
        scout.nav.push(Satelite::EscolherOlheiro);
        scout.nav.push(Satelite::NovaMissao);

        scout.aplicar_controle(controle(botao::B), false);
        assert!(scout.painel_aberto(), "B volta uma tela, não fecha o painel");
        assert_eq!(scout.nav.tela_atual(), ScoutScreen::Satelite(Satelite::EscolherOlheiro));
        assert!(!scout.state.tem_nova_missao());
        assert_eq!(scout.nav.tela_abaixo(), ScoutScreen::Aba(Aba::Missoes));
    }

    #[test]
    fn b_goes_back_from_the_squad_selector_to_the_ficha_and_then_to_the_report() {
        let mut scout = Scout { state: ScoutState::com_fonte(Box::new(CarreiraFixa), None), ..Scout::new() };
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        scout.nav.trocar_aba(Aba::Relatorios);
        scout.nav.push(Satelite::Relatorio);
        scout.nav.push(Satelite::FichaJogador);
        scout.nav.push(Satelite::SeletorElenco(ContextoSeletor::ComparacaoFicha));
        for esperado in [Satelite::FichaJogador, Satelite::Relatorio] {
            scout.aplicar_controle(controle(botao::B), false);
            scout.aplicar_controle(controle(0), false);
            assert_eq!(scout.nav.tela_atual(), ScoutScreen::Satelite(esperado));
        }
        assert!(scout.painel_aberto());
    }

    #[test]
    fn dpad_left_right_never_switch_tabs() {
        let mut scout = Scout::new();
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        scout.aplicar_controle(controle(botao::DPAD_DIREITA), false);
        scout.aplicar_controle(controle(0), false);
        scout.aplicar_controle(controle(botao::DPAD_ESQUERDA), false);
        assert_eq!(scout.nav.aba_ativa(), Aba::Olheiros, "←/→ só navegam");
    }

    #[test]
    fn every_screen_change_asks_for_focus_on_its_first_item() {
        let mut nav = Navigation::new(Aba::Olheiros);
        assert!(nav.tomar_foco_pendente(), "primeira tela");
        assert!(!nav.tomar_foco_pendente(), "uma vez só");
        nav.trocar_aba(Aba::Missoes);
        assert!(nav.tomar_foco_pendente());
        nav.push(Satelite::NovaMissao);
        assert!(nav.tomar_foco_pendente());
        nav.pop();
        assert!(nav.tomar_foco_pendente());
        nav.pop();
        assert!(!nav.tomar_foco_pendente(), "pop sem satélite não muda a tela");
    }

    #[test]
    fn y_asks_for_the_options_for_one_frame_only_with_no_warning_on_top() {
        let mut scout = Scout { state: ScoutState::com_fonte(Box::new(CarreiraFixa), None), ..Scout::new() };
        scout.aplicar_controle(controle(COMBO_PAINEL), false);
        scout.aplicar_controle(controle(0), false);
        assert!(!scout.state.opcoes_pedidas());
        scout.aplicar_controle(controle(botao::Y), false);
        assert!(scout.state.opcoes_pedidas(), "borda do Y");
        scout.aplicar_controle(controle(botao::Y), false);
        assert!(!scout.state.opcoes_pedidas(), "segurar não repete");
        scout.aplicar_controle(controle(0), false);
        // com o aviso de demissão aberto, o Y não chega às telas de baixo
        scout.state.definir_troca_de_aba_pendente(Some(Aba::Missoes));
        scout.aplicar_controle(controle(botao::Y), false);
        assert!(!scout.state.opcoes_pedidas());
    }

    #[test]
    fn neighbour_tabs_wrap_around() {
        assert_eq!(Aba::Olheiros.vizinha(-1), Aba::Escolhidos);
        assert_eq!(Aba::Escolhidos.vizinha(1), Aba::Olheiros);
        assert_eq!(Aba::Missoes.vizinha(1), Aba::Relatorios);
    }

    #[test]
    fn tab_labels_follow_the_bar_order() {
        let rotulos: Vec<&str> = Aba::TODAS.iter().map(|a| a.rotulo()).collect();
        assert_eq!(rotulos, ["Olheiros", "Missões", "Relatórios", "Base do Scout", "Escolhidos"]);
    }
}
