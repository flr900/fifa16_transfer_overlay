//! Controle (XInput) para a Central de Scout — Story 1.6.
//!
//! Duas tarefas:
//! 1. **Ler o controle** para o overlay (navegação do ImGui, atalho de
//!    abrir/fechar, LB/RB, B).
//! 2. **Esconder o controle do jogo enquanto o painel está aberto.** O
//!    FIFA lê o controle direto pelo XInput, sem passar pelas mensagens de
//!    janela que o hudhook filtra (`MessageFilter`). Sem isso, um "A" no
//!    painel também seria um "A" no menu do jogo. Por isso interceptamos
//!    `XInputGetState` nas DLLs de XInput já carregadas no processo
//!    (`fifa16.exe` referencia `xinput1_4`/`1_3`/`9_1_0`…) com o MinHook
//!    que o hudhook já embute. Com o bloqueio ligado, o jogo recebe o
//!    controle "parado" (nenhum botão, analógicos no centro).
//!
//! O overlay lê pelo TRAMPOLIM do gancho (a função original), então o
//! bloqueio nunca esconde o controle do próprio overlay.
//!
//! Se nenhuma DLL de XInput estiver carregada na hora (o jogo pode
//! carregá-la depois), `instalar_bloqueio` é tentado de novo
//! periodicamente; enquanto isso o overlay lê pelo `xinput1_4` direto e o
//! jogo NÃO é bloqueado (registrado no log).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use hudhook::mh::{MhHook, MH_DisableHook, MH_EnableHook, MH_Initialize, MH_STATUS};
use imgui::{Io, Key};
use windows::core::{s, w, PCWSTR};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Input::XboxController::{XINPUT_GAMEPAD, XINPUT_STATE};

type FnGetState = unsafe extern "system" fn(u32, *mut XINPUT_STATE) -> u32;

const ERROR_SUCCESS: u32 = 0;
const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;

/// DLLs de XInput que o jogo pode ter carregado (nomes do `fifa16.exe`).
const MODULOS: [PCWSTR; 5] = [
    w!("xinput1_4.dll"),
    w!("xinput1_3.dll"),
    w!("xinput9_1_0.dll"),
    w!("xinput1_2.dll"),
    w!("xinput1_1.dll"),
];

/// Bits de `wButtons` (XInput).
pub mod botao {
    pub const DPAD_CIMA: u16 = 0x0001;
    pub const DPAD_BAIXO: u16 = 0x0002;
    pub const DPAD_ESQUERDA: u16 = 0x0004;
    pub const DPAD_DIREITA: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const L3: u16 = 0x0040;
    pub const R3: u16 = 0x0080;
    pub const LB: u16 = 0x0100;
    pub const RB: u16 = 0x0200;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;
}

/// Zona morta do analógico esquerdo e limiar dos gatilhos (valores do SDK).
const ZONA_MORTA_ANALOGICO: i32 = 7849;
const LIMIAR_GATILHO: u8 = 30;

/// Foto do controle num frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EstadoControle {
    pub botoes: u16,
    pub lx: i16,
    pub ly: i16,
    pub lt: u8,
    pub rt: u8,
    /// Analógico direito: rola a tela do Scout (2026-10-03).
    pub rx: i16,
    pub ry: i16,
}

impl EstadoControle {
    fn de(gamepad: &XINPUT_GAMEPAD) -> Self {
        EstadoControle {
            botoes: gamepad.wButtons.0,
            lx: gamepad.sThumbLX,
            ly: gamepad.sThumbLY,
            lt: gamepad.bLeftTrigger,
            rt: gamepad.bRightTrigger,
            rx: gamepad.sThumbRX,
            ry: gamepad.sThumbRY,
        }
    }

    pub fn segura(&self, mascara: u16) -> bool {
        self.botoes & mascara == mascara
    }

    /// Nada apertado (botões e gatilhos). Os analógicos não contam: soltos
    /// eles voltam ao centro sozinhos e o jogo não "dispara" nada com eles.
    pub fn solto(&self) -> bool {
        self.botoes == 0 && self.lt < LIMIAR_GATILHO && self.rt < LIMIAR_GATILHO
    }
}

// ---------------------------------------------------------------------
// Gancho do XInputGetState
// ---------------------------------------------------------------------

static BLOQUEAR_JOGO: AtomicBool = AtomicBool::new(false);
/// Trampolim (função original) de cada DLL de `MODULOS` já enganchada.
static TRAMPOLINS: [AtomicUsize; 5] =
    [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];
/// Função que o OVERLAY usa para ler o controle (um trampolim, ou o
/// `xinput1_4` direto enquanto não houver gancho).
static LEITOR: AtomicUsize = AtomicUsize::new(0);

/// Liga/desliga o "controle parado" para o jogo.
pub fn bloquear_jogo(bloquear: bool) {
    BLOQUEAR_JOGO.store(bloquear, Ordering::Relaxed);
}

/// O que o jogo recebe: o estado real, ou o controle parado se bloqueado.
unsafe fn desviar(indice_modulo: usize, usuario: u32, estado: *mut XINPUT_STATE) -> u32 {
    let original = TRAMPOLINS.get(indice_modulo).map_or(0, |t| t.load(Ordering::Acquire));
    if original == 0 {
        return ERROR_DEVICE_NOT_CONNECTED;
    }
    // SAFETY: `original` é o trampolim criado pelo MinHook para uma
    // função com esta assinatura (`XInputGetState`).
    let original: FnGetState = unsafe { std::mem::transmute::<usize, FnGetState>(original) };
    let resultado = unsafe { original(usuario, estado) };
    if resultado == ERROR_SUCCESS && BLOQUEAR_JOGO.load(Ordering::Relaxed) && !estado.is_null() {
        // SAFETY: ponteiro não nulo que o jogo passou e o original preencheu.
        unsafe { (*estado).Gamepad = XINPUT_GAMEPAD::default() };
    }
    resultado
}

// Uma função de desvio por DLL: cada uma chama o próprio trampolim.
unsafe extern "system" fn desvio_0(u: u32, e: *mut XINPUT_STATE) -> u32 {
    unsafe { desviar(0, u, e) }
}
unsafe extern "system" fn desvio_1(u: u32, e: *mut XINPUT_STATE) -> u32 {
    unsafe { desviar(1, u, e) }
}
unsafe extern "system" fn desvio_2(u: u32, e: *mut XINPUT_STATE) -> u32 {
    unsafe { desviar(2, u, e) }
}
unsafe extern "system" fn desvio_3(u: u32, e: *mut XINPUT_STATE) -> u32 {
    unsafe { desviar(3, u, e) }
}
unsafe extern "system" fn desvio_4(u: u32, e: *mut XINPUT_STATE) -> u32 {
    unsafe { desviar(4, u, e) }
}
const DESVIOS: [FnGetState; 5] = [desvio_0, desvio_1, desvio_2, desvio_3, desvio_4];

/// Engancha `XInputGetState` em cada DLL de XInput já carregada e ainda
/// não enganchada. Devolve quantas DLLs estão enganchadas no total.
/// Barato (alguns `GetModuleHandle`): pode ser chamado de novo.
pub fn instalar_bloqueio() -> usize {
    // O hudhook já inicializou o MinHook; uma segunda chamada só devolve
    // "já inicializado".
    let _ = unsafe { MH_Initialize() };
    for (indice, nome) in MODULOS.iter().enumerate() {
        let (Some(trampolim), Some(desvio)) = (TRAMPOLINS.get(indice), DESVIOS.get(indice)) else {
            continue;
        };
        if trampolim.load(Ordering::Acquire) != 0 {
            continue;
        }
        let Ok(modulo) = (unsafe { GetModuleHandleW(*nome) }) else {
            continue;
        };
        let Some(alvo) = (unsafe { GetProcAddress(modulo, s!("XInputGetState")) }) else {
            continue;
        };
        let alvo = alvo as *mut c_void;
        // SAFETY: `alvo` é a exportação `XInputGetState` e o desvio tem a
        // mesma assinatura e convenção de chamada.
        let gancho = match unsafe { MhHook::new(alvo, *desvio as *mut c_void) } {
            Ok(gancho) => gancho,
            Err(status) => {
                tracing::warn!("[gamepad] Não deu para enganchar XInputGetState #{indice}: {status:?}");
                continue;
            }
        };
        // Trampolim publicado ANTES de ligar o gancho: o desvio nunca roda
        // sem saber para onde chamar.
        trampolim.store(gancho.trampoline() as usize, Ordering::Release);
        let _ = LEITOR.compare_exchange(0, gancho.trampoline() as usize, Ordering::AcqRel, Ordering::Acquire);
        match unsafe { MH_EnableHook(alvo) } {
            MH_STATUS::MH_OK => tracing::info!("[gamepad] XInputGetState enganchado (DLL #{indice})."),
            status => tracing::warn!("[gamepad] MH_EnableHook falhou para a DLL #{indice}: {status:?}"),
        }
        // `MhHook` não desfaz o gancho ao sair de escopo: ele vive até o
        // processo acabar (a DLL do produto não é descarregada).
    }
    TRAMPOLINS.iter().filter(|t| t.load(Ordering::Acquire) != 0).count()
}

/// Desliga os ganchos antes de a DLL ser descarregada (recarga de
/// desenvolvimento, `recarregar_dev.ps1`): o jogo não pode chamar um
/// desvio que mora numa DLL que está saindo da memória.
pub fn remover_ganchos() {
    bloquear_jogo(false);
    LEITOR.store(0, Ordering::Release);
    for (indice, nome) in MODULOS.iter().enumerate() {
        let Some(trampolim) = TRAMPOLINS.get(indice) else {
            continue;
        };
        if trampolim.load(Ordering::Acquire) == 0 {
            continue;
        }
        let Ok(modulo) = (unsafe { GetModuleHandleW(*nome) }) else {
            continue;
        };
        if let Some(alvo) = unsafe { GetProcAddress(modulo, s!("XInputGetState")) } {
            match unsafe { MH_DisableHook(alvo as *mut c_void) } {
                MH_STATUS::MH_OK => tracing::info!("[gamepad] Gancho do XInputGetState removido (DLL #{indice})."),
                status => tracing::warn!("[gamepad] MH_DisableHook falhou para a DLL #{indice}: {status:?}"),
            }
        }
    }
}

/// Função de leitura do overlay. Sem gancho, carrega o `xinput1_4` direto.
fn leitor() -> Option<FnGetState> {
    let mut endereco = LEITOR.load(Ordering::Acquire);
    if endereco == 0 {
        let modulo = unsafe { LoadLibraryW(w!("xinput1_4.dll")) }.ok()?;
        let funcao = unsafe { GetProcAddress(modulo, s!("XInputGetState")) }?;
        endereco = funcao as usize;
    }
    // SAFETY: endereço de `XInputGetState` (ou do trampolim dela).
    Some(unsafe { std::mem::transmute::<usize, FnGetState>(endereco) })
}

/// Lê o primeiro controle conectado. Consultar um índice sem controle é
/// lento no XInput, então os 4 índices só são varridos a cada
/// `INTERVALO_BUSCA` enquanto nenhum responde.
pub struct Controle {
    indice: Option<u32>,
    proxima_busca: Instant,
    proxima_instalacao: Instant,
    ganchos: usize,
    avisou_sem_xinput: bool,
}

const INTERVALO_BUSCA: Duration = Duration::from_secs(2);
const INTERVALO_INSTALACAO: Duration = Duration::from_secs(3);

impl Controle {
    pub fn new() -> Self {
        let agora = Instant::now();
        Controle { indice: None, proxima_busca: agora, proxima_instalacao: agora, ganchos: 0, avisou_sem_xinput: false }
    }

    /// Estado do controle neste frame (`None` sem controle).
    pub fn ler(&mut self) -> Option<EstadoControle> {
        let agora = Instant::now();
        if self.ganchos == 0 && agora >= self.proxima_instalacao {
            self.proxima_instalacao = agora + INTERVALO_INSTALACAO;
            self.ganchos = instalar_bloqueio();
            if self.ganchos == 0 && !self.avisou_sem_xinput {
                self.avisou_sem_xinput = true;
                tracing::info!("[gamepad] Nenhuma DLL de XInput carregada ainda; o jogo não é bloqueado por enquanto.");
            }
        }
        let ler = leitor()?;
        let consultar = |indice: u32| -> Option<EstadoControle> {
            let mut estado = XINPUT_STATE::default();
            // SAFETY: `estado` é um XINPUT_STATE válido na pilha.
            (unsafe { ler(indice, &mut estado) } == ERROR_SUCCESS).then(|| EstadoControle::de(&estado.Gamepad))
        };
        if let Some(indice) = self.indice {
            if let Some(estado) = consultar(indice) {
                return Some(estado);
            }
            tracing::info!("[gamepad] Controle {indice} desconectado.");
            self.indice = None;
        }
        if agora < self.proxima_busca {
            return None;
        }
        self.proxima_busca = agora + INTERVALO_BUSCA;
        let (indice, estado) = (0..4).find_map(|i| consultar(i).map(|e| (i, e)))?;
        tracing::info!("[gamepad] Controle conectado no índice {indice}.");
        self.indice = Some(indice);
        Some(estado)
    }
}

// ---------------------------------------------------------------------
// Para o ImGui
// ---------------------------------------------------------------------

/// Analógico → 0..1 depois da zona morta.
pub fn normalizar_eixo(valor: i16, zona_morta: i32) -> f32 {
    let valor = i32::from(valor).abs();
    if valor <= zona_morta {
        return 0.0;
    }
    ((valor - zona_morta) as f32 / (32767 - zona_morta) as f32).clamp(0.0, 1.0)
}

/// Eventos de tecla de gamepad do ImGui para um estado (`None` = tudo
/// solto). `(tecla, apertada, valor analógico)`.
pub fn eventos_imgui(estado: Option<EstadoControle>) -> Vec<(Key, bool, f32)> {
    let e = estado.unwrap_or_default();
    let digital = |mascara: u16| -> (bool, f32) {
        let apertado = e.botoes & mascara != 0;
        (apertado, if apertado { 1.0 } else { 0.0 })
    };
    let mut eventos = Vec::with_capacity(20);
    for (tecla, mascara) in [
        (Key::GamepadDpadUp, botao::DPAD_CIMA),
        (Key::GamepadDpadDown, botao::DPAD_BAIXO),
        (Key::GamepadDpadLeft, botao::DPAD_ESQUERDA),
        (Key::GamepadDpadRight, botao::DPAD_DIREITA),
        (Key::GamepadFaceDown, botao::A),
        (Key::GamepadFaceRight, botao::B),
        (Key::GamepadFaceLeft, botao::X),
        (Key::GamepadFaceUp, botao::Y),
        (Key::GamepadL1, botao::LB),
        (Key::GamepadR1, botao::RB),
        (Key::GamepadStart, botao::START),
        (Key::GamepadBack, botao::BACK),
        (Key::GamepadL3, botao::L3),
        (Key::GamepadR3, botao::R3),
    ] {
        let (apertado, valor) = digital(mascara);
        eventos.push((tecla, apertado, valor));
    }
    let x = normalizar_eixo(e.lx, ZONA_MORTA_ANALOGICO);
    let y = normalizar_eixo(e.ly, ZONA_MORTA_ANALOGICO);
    for (tecla, ativo, valor) in [
        (Key::GamepadLStickLeft, e.lx < 0, x),
        (Key::GamepadLStickRight, e.lx > 0, x),
        // XInput: Y positivo é para CIMA.
        (Key::GamepadLStickUp, e.ly > 0, y),
        (Key::GamepadLStickDown, e.ly < 0, y),
    ] {
        let valor = if ativo { valor } else { 0.0 };
        eventos.push((tecla, valor > 0.1, valor));
    }
    for (tecla, gatilho) in [(Key::GamepadL2, e.lt), (Key::GamepadR2, e.rt)] {
        eventos.push((tecla, gatilho >= LIMIAR_GATILHO, f32::from(gatilho) / 255.0));
    }
    eventos
}

/// Zona morta do analógico direito (valor do SDK).
const ZONA_MORTA_DIREITO: i32 = 8689;
/// Pixels por frame com o analógico direito no fim do curso.
const ROLAGEM_MAXIMA: f32 = 26.0;

/// Pixels a rolar neste frame pelo analógico direito (positivo = para
/// baixo): curva quadrática depois da zona morta, para o começo do curso
/// rolar devagar e dar para ler.
pub fn rolagem_do_analogico(ry: i16) -> f32 {
    let v = normalizar_eixo(ry, ZONA_MORTA_DIREITO);
    if v <= 0.0 {
        return 0.0;
    }
    let pixels = v * v * ROLAGEM_MAXIMA;
    if ry > 0 {
        -pixels
    } else {
        pixels
    }
}

/// Quanto o analógico precisa inclinar (0..1, depois da zona morta) para
/// valer como uma direção do D-pad na navegação.
const LIMIAR_ANALOGICO_NAV: f32 = 0.5;

/// Estado que vai para a NAVEGAÇÃO do ImGui (pedidos do Felipe, 2026-10-01):
/// - o analógico esquerdo move o foco como o D-pad (no ImGui 1.89 ele só
///   rolaria a janela) e deixa de ser repassado como analógico, para não
///   rolar e mover ao mesmo tempo;
/// - ←/→ (D-pad ou analógico) navegam SEMPRE dentro da tela: trocar de
///   aba é só LB/RB (`scout`);
/// - o Y não vai para o ImGui (ali ele abre o modo de janelas): é o botão
///   "Opções" do Scout, lido à parte (`scout::comandos_controle`);
/// - o Select (Back) também não: ele abre as Configurações do Scout.
pub fn para_navegacao(estado: EstadoControle) -> EstadoControle {
    let mut botoes = estado.botoes & !(botao::Y | botao::BACK);
    let x = normalizar_eixo(estado.lx, ZONA_MORTA_ANALOGICO);
    let y = normalizar_eixo(estado.ly, ZONA_MORTA_ANALOGICO);
    if y >= LIMIAR_ANALOGICO_NAV {
        botoes |= if estado.ly > 0 { botao::DPAD_CIMA } else { botao::DPAD_BAIXO };
    }
    if x >= LIMIAR_ANALOGICO_NAV {
        botoes |= if estado.lx > 0 { botao::DPAD_DIREITA } else { botao::DPAD_ESQUERDA };
    }
    EstadoControle { botoes, lx: 0, ly: 0, ..estado }
}

/// Repassa o controle ao ImGui (eventos repetidos são descartados por ele).
pub fn alimentar_imgui(io: &mut Io, estado: Option<EstadoControle>) {
    for (tecla, apertada, valor) in eventos_imgui(estado) {
        io.add_key_analog_event(tecla, apertada, valor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_has_a_dead_zone_and_is_normalised() {
        assert_eq!(normalizar_eixo(0, ZONA_MORTA_ANALOGICO), 0.0);
        assert_eq!(normalizar_eixo(7000, ZONA_MORTA_ANALOGICO), 0.0);
        assert_eq!(normalizar_eixo(-7849, ZONA_MORTA_ANALOGICO), 0.0);
        assert_eq!(normalizar_eixo(32767, ZONA_MORTA_ANALOGICO), 1.0);
        assert_eq!(normalizar_eixo(i16::MIN, ZONA_MORTA_ANALOGICO), 1.0, "sem overflow no -32768");
        let meio = normalizar_eixo(20_000, ZONA_MORTA_ANALOGICO);
        assert!(meio > 0.4 && meio < 0.55, "{meio}");
    }

    fn evento(eventos: &[(Key, bool, f32)], tecla: Key) -> (bool, f32) {
        eventos.iter().find(|(k, _, _)| *k == tecla).map(|(_, a, v)| (*a, *v)).unwrap_or((false, -1.0))
    }

    #[test]
    fn buttons_and_stick_become_imgui_gamepad_keys() {
        let estado = EstadoControle { botoes: botao::A | botao::DPAD_BAIXO | botao::RB, lx: -32767, ly: 32767, lt: 0, rt: 255, rx: 0, ry: 0 };
        let eventos = eventos_imgui(Some(estado));
        assert_eq!(evento(&eventos, Key::GamepadFaceDown), (true, 1.0));
        assert_eq!(evento(&eventos, Key::GamepadDpadDown), (true, 1.0));
        assert_eq!(evento(&eventos, Key::GamepadR1), (true, 1.0));
        assert_eq!(evento(&eventos, Key::GamepadFaceRight), (false, 0.0));
        assert_eq!(evento(&eventos, Key::GamepadLStickLeft), (true, 1.0));
        assert_eq!(evento(&eventos, Key::GamepadLStickRight), (false, 0.0));
        assert_eq!(evento(&eventos, Key::GamepadLStickUp), (true, 1.0), "Y positivo = cima");
        assert_eq!(evento(&eventos, Key::GamepadR2), (true, 1.0));
        assert_eq!(evento(&eventos, Key::GamepadL2), (false, 0.0));
    }

    #[test]
    fn select_never_reaches_imgui_as_a_cancel() {
        let estado = EstadoControle { botoes: botao::BACK | botao::Y | botao::A, ..Default::default() };
        assert_eq!(para_navegacao(estado).botoes, botao::A, "Select e Y são do Scout; A segue para o ImGui");
    }

    #[test]
    fn stick_moves_focus_like_the_dpad_and_left_right_always_navigate() {
        let cima = EstadoControle { ly: 30_000, ..Default::default() };
        assert_eq!(para_navegacao(cima).botoes, botao::DPAD_CIMA);
        let baixo = EstadoControle { ly: -30_000, ..Default::default() };
        assert_eq!(para_navegacao(baixo).botoes, botao::DPAD_BAIXO);
        // pouco inclinado: nada
        assert_eq!(para_navegacao(EstadoControle { ly: 12_000, ..Default::default() }).botoes, 0);
        // o analógico some (não rola a janela junto)
        let convertido = para_navegacao(EstadoControle { lx: -30_000, ly: 30_000, ..Default::default() });
        assert_eq!((convertido.lx, convertido.ly), (0, 0));
        assert_eq!(convertido.botoes, botao::DPAD_CIMA | botao::DPAD_ESQUERDA);
        // ←/→ vão para o ImGui (trocar de aba é só LB/RB)
        let direita = EstadoControle { botoes: botao::DPAD_DIREITA | botao::A, lx: 30_000, ..Default::default() };
        assert_eq!(para_navegacao(direita).botoes, botao::DPAD_DIREITA | botao::A);
    }

    #[test]
    fn no_controller_releases_everything() {
        assert!(eventos_imgui(None).iter().all(|(_, apertada, valor)| !apertada && *valor == 0.0));
    }

    #[test]
    fn released_ignores_sticks_but_not_triggers() {
        assert!(EstadoControle { lx: 30_000, ..Default::default() }.solto());
        assert!(!EstadoControle { rt: 200, ..Default::default() }.solto());
        assert!(!EstadoControle { botoes: botao::B, ..Default::default() }.solto());
        assert!(EstadoControle { botoes: botao::L3 | botao::START, ..Default::default() }.segura(botao::L3 | botao::START));
    }

    #[test]
    fn y_never_reaches_the_imgui_navigation_but_the_other_buttons_do() {
        let estado = EstadoControle { botoes: botao::Y | botao::A | botao::DPAD_BAIXO, ..Default::default() };
        let nav = para_navegacao(estado);
        assert_eq!(nav.botoes, botao::A | botao::DPAD_BAIXO, "o Y é do Scout, não da navegação");
        let eventos = eventos_imgui(Some(nav));
        assert!(eventos.iter().all(|(tecla, apertada, _)| !(*tecla == Key::GamepadFaceUp && *apertada)));
    }
}
