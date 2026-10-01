//! Injetor da DLL fifa_overlay no processo fifa16.exe.
//!
//! Uso:
//!     fifa_injector.exe [caminho_para_fifa_overlay.dll]
//!         Injeta agora (o jogo já tem de estar aberto).
//!
//!     fifa_injector.exe --aguardar [--enquanto-pid <PID>] [caminho_para_dll]
//!         Fica esperando o jogo abrir e injeta quando ele estiver pronto
//!         (Story 1.7). Se o jogo fechar, volta a esperar a próxima
//!         abertura. Com `--enquanto-pid`, termina quando esse processo
//!         (o servidor do FIFA Friends) fechar.
//!
//! Se o caminho não for informado, procura `fifa_overlay.dll` no
//! mesmo diretório do executável do injetor (útil quando ambos os
//! binários são compilados e copiados juntos).
//!
//! O FIFA roda elevado: o injetor tem de rodar como Administrador.
//!
//! "Pronto" = o processo já carregou o `d3d11.dll` e tem uma janela
//! visível, e continua assim por `CARENCIA`. Injetar antes disso (com o
//! executável ainda se desempacotando — ver `PROJECT_MEMORY.md`, sessão 3)
//! é pedir crash; o hook do `Present` só precisa do DirectX carregado.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use hudhook::inject::Process;
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW, MODULEENTRY32W,
    PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};

const TARGET_PROCESS: &str = "fifa16.exe";
const DEFAULT_DLL_NAME: &str = "fifa_overlay.dll";

/// Intervalo entre verificações no modo `--aguardar`.
const INTERVALO: Duration = Duration::from_secs(1);
/// Quanto tempo o jogo precisa ficar "pronto" antes de injetarmos.
const CARENCIA: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    let mut aguardar = false;
    let mut enquanto_pid: Option<u32> = None;
    let mut dll_arg: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--aguardar" => aguardar = true,
            "--enquanto-pid" => match args.next().and_then(|v| v.parse().ok()) {
                Some(pid) => enquanto_pid = Some(pid),
                None => {
                    eprintln!("[fifa_injector] --enquanto-pid precisa de um número.");
                    return ExitCode::FAILURE;
                }
            },
            _ => dll_arg = Some(PathBuf::from(arg)),
        }
    }

    let Some(dll_path) = resolver_dll(dll_arg) else {
        return ExitCode::FAILURE;
    };
    println!("[fifa_injector] DLL: {}", dll_path.display());

    if aguardar {
        aguardar_e_injetar(&dll_path, enquanto_pid)
    } else {
        injetar_agora(&dll_path)
    }
}

fn resolver_dll(arg: Option<PathBuf>) -> Option<PathBuf> {
    let candidato = match arg {
        Some(path) => path,
        None => {
            let mut dir = std::env::current_exe().ok()?;
            dir.pop();
            dir.join(DEFAULT_DLL_NAME)
        }
    };
    match candidato.canonicalize() {
        Ok(path) => Some(path),
        Err(err) => {
            eprintln!("[fifa_injector] Não encontrei a DLL em {}: {err}", candidato.display());
            None
        }
    }
}

fn injetar_agora(dll_path: &Path) -> ExitCode {
    println!("[fifa_injector] Procurando processo '{TARGET_PROCESS}'...");
    match injetar(dll_path) {
        Ok(()) => {
            println!("[fifa_injector] DLL injetada com sucesso.");
            println!("[fifa_injector] Log da DLL: {}\\fifa_overlay.log", std::env::temp_dir().display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("[fifa_injector] {err}");
            ExitCode::FAILURE
        }
    }
}

fn injetar(dll_path: &Path) -> Result<(), String> {
    let process =
        Process::by_name(TARGET_PROCESS).map_err(|e| format!("Processo '{TARGET_PROCESS}' não encontrado: {e}"))?;
    process.inject(dll_path.to_path_buf()).map_err(|e| format!("Falha ao injetar DLL: {e}"))
}

/// Em que pé está o jogo, a cada verificação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Jogo {
    Fechado,
    /// Aberto, mas ainda sem DirectX ou sem janela visível.
    Iniciando(u32),
    Pronto(u32),
    /// A DLL do overlay já está carregada (injeção anterior ou manual).
    ComOverlay(u32),
    /// Não foi possível listar os módulos do jogo (injetor sem
    /// privilégio de Administrador — o FIFA roda elevado).
    SemAcesso(u32),
}

/// O que fazer depois de olhar o jogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Acao {
    Esperar,
    Injetar(u32),
}

/// Máquina de estados do modo `--aguardar`, separada das chamadas ao
/// Windows para poder ser testada.
#[derive(Debug, Default)]
struct Vigia {
    /// Desde quando o processo `pid` está pronto.
    pronto_desde: Option<(u32, Instant)>,
    /// Último pid em que já tentamos injetar (uma tentativa por abertura).
    tentado: Option<u32>,
}

impl Vigia {
    fn decidir(&mut self, jogo: Jogo, agora: Instant) -> Acao {
        match jogo {
            Jogo::Fechado | Jogo::Iniciando(_) | Jogo::ComOverlay(_) | Jogo::SemAcesso(_) => {
                self.pronto_desde = None;
                if let Jogo::ComOverlay(pid) = jogo {
                    self.tentado = Some(pid);
                }
                Acao::Esperar
            }
            Jogo::Pronto(pid) => {
                if self.tentado == Some(pid) {
                    return Acao::Esperar;
                }
                let desde = match self.pronto_desde {
                    Some((p, desde)) if p == pid => desde,
                    _ => {
                        self.pronto_desde = Some((pid, agora));
                        agora
                    }
                };
                if agora.saturating_duration_since(desde) >= CARENCIA {
                    self.tentado = Some(pid);
                    self.pronto_desde = None;
                    Acao::Injetar(pid)
                } else {
                    Acao::Esperar
                }
            }
        }
    }
}

fn aguardar_e_injetar(dll_path: &Path, enquanto_pid: Option<u32>) -> ExitCode {
    println!("[fifa_injector] Aguardando o {TARGET_PROCESS} abrir (Ctrl+C para sair)...");
    let mut vigia = Vigia::default();
    let mut ultimo: Option<Jogo> = None;

    loop {
        let processos = listar_processos();
        if let Some(pid) = enquanto_pid {
            if !processos.iter().any(|(p, _)| *p == pid) {
                println!("[fifa_injector] O servidor (pid {pid}) fechou. Saindo.");
                return ExitCode::SUCCESS;
            }
        }

        let jogo = examinar_jogo(&processos, dll_path);
        if ultimo != Some(jogo) {
            match jogo {
                Jogo::Fechado if ultimo.is_some() => println!("[fifa_injector] Jogo fechado. Aguardando a próxima abertura..."),
                Jogo::Fechado => {}
                Jogo::Iniciando(pid) => println!("[fifa_injector] Jogo abrindo (pid {pid}), esperando o DirectX..."),
                Jogo::Pronto(pid) => println!(
                    "[fifa_injector] Jogo pronto (pid {pid}); injetando em {} s...",
                    CARENCIA.as_secs()
                ),
                Jogo::ComOverlay(pid) => println!("[fifa_injector] Central de Scout carregada no jogo (pid {pid})."),
                Jogo::SemAcesso(pid) => eprintln!(
                    "[fifa_injector] Sem acesso ao jogo (pid {pid}): rode este terminal como Administrador."
                ),
            }
            ultimo = Some(jogo);
        }

        if let Acao::Injetar(pid) = vigia.decidir(jogo, Instant::now()) {
            println!("[fifa_injector] Injetando no pid {pid}...");
            match injetar(dll_path) {
                Ok(()) => println!("[fifa_injector] DLL injetada. Log: {}\\fifa_overlay.log", std::env::temp_dir().display()),
                Err(err) => eprintln!("[fifa_injector] {err} (não tento de novo nesta abertura do jogo)"),
            }
        }

        std::thread::sleep(INTERVALO);
    }
}

fn examinar_jogo(processos: &[(u32, String)], dll_path: &Path) -> Jogo {
    let Some(pid) = processos
        .iter()
        .find(|(_, nome)| nome.eq_ignore_ascii_case(TARGET_PROCESS))
        .map(|(pid, _)| *pid)
    else {
        return Jogo::Fechado;
    };
    let Some(modulos) = listar_modulos(pid) else {
        return Jogo::SemAcesso(pid);
    };
    let nome_dll = dll_path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    classificar(pid, &modulos, &nome_dll, tem_janela_visivel(pid))
}

/// Regra pura: `modulos` em minúsculas.
fn classificar(pid: u32, modulos: &[String], nome_dll: &str, janela_visivel: bool) -> Jogo {
    let tem = |nome: &str| modulos.iter().any(|m| m == nome);
    if (!nome_dll.is_empty() && tem(nome_dll)) || modulos.iter().any(|m| m.starts_with("fifa_overlay")) {
        Jogo::ComOverlay(pid)
    } else if tem("d3d11.dll") && janela_visivel {
        Jogo::Pronto(pid)
    } else {
        Jogo::Iniciando(pid)
    }
}

/// Fecha o handle do snapshot ao sair de escopo.
struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn nome_utf16(buf: &[u16]) -> String {
    let fim = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(buf.get(..fim).unwrap_or_default())
}

/// `(pid, nome do executável)` de todos os processos.
fn listar_processos() -> Vec<(u32, String)> {
    let mut lista = Vec::new();
    let Ok(handle) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return lista;
    };
    let snapshot = Snapshot(handle);
    let mut entrada = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entrada) }.is_ok();
    while ok {
        lista.push((entrada.th32ProcessID, nome_utf16(&entrada.szExeFile)));
        ok = unsafe { Process32NextW(snapshot.0, &mut entrada) }.is_ok();
    }
    lista
}

/// Nomes (minúsculos) dos módulos carregados em `pid`. `None` se não der
/// para abrir o processo (injetor sem privilégio de Administrador).
fn listar_modulos(pid: u32) -> Option<Vec<String>> {
    let mut lista = Vec::new();
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) }.ok()?;
    let snapshot = Snapshot(handle);
    let mut entrada = MODULEENTRY32W { dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
    let mut ok = unsafe { Module32FirstW(snapshot.0, &mut entrada) }.is_ok();
    while ok {
        lista.push(nome_utf16(&entrada.szModule).to_ascii_lowercase());
        ok = unsafe { Module32NextW(snapshot.0, &mut entrada) }.is_ok();
    }
    Some(lista)
}

struct BuscaJanela {
    pid: u32,
    achou: bool,
}

unsafe extern "system" fn procurar_janela(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` aponta para o `BuscaJanela` de `tem_janela_visivel`,
    // vivo durante toda a chamada síncrona de `EnumWindows`.
    let busca = unsafe { &mut *(lparam.0 as *mut BuscaJanela) };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == busca.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
        busca.achou = true;
        return BOOL(0); // para a enumeração
    }
    BOOL(1)
}

fn tem_janela_visivel(pid: u32) -> bool {
    let mut busca = BuscaJanela { pid, achou: false };
    // `EnumWindows` devolve erro quando o callback interrompe: o que vale é `achou`.
    let _ = unsafe { EnumWindows(Some(procurar_janela), LPARAM(&mut busca as *mut BuscaJanela as isize)) };
    busca.achou
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modulos(nomes: &[&str]) -> Vec<String> {
        nomes.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn classification_needs_directx_and_a_visible_window() {
        let dll = "fifa_overlay_dev.dll";
        assert_eq!(classificar(7, &modulos(&["fifa16.exe", "kernel32.dll"]), dll, true), Jogo::Iniciando(7));
        assert_eq!(classificar(7, &modulos(&["fifa16.exe", "d3d11.dll"]), dll, false), Jogo::Iniciando(7));
        assert_eq!(classificar(7, &modulos(&["fifa16.exe", "d3d11.dll"]), dll, true), Jogo::Pronto(7));
        assert_eq!(classificar(7, &modulos(&["d3d11.dll", "fifa_overlay_dev.dll"]), dll, true), Jogo::ComOverlay(7));
        // overlay injetado à mão com outro nome de cópia também conta
        assert_eq!(classificar(7, &modulos(&["d3d11.dll", "fifa_overlay.dll"]), dll, true), Jogo::ComOverlay(7));
    }

    #[test]
    fn injects_once_after_the_grace_period() {
        let mut vigia = Vigia::default();
        let t0 = Instant::now();
        assert_eq!(vigia.decidir(Jogo::Fechado, t0), Acao::Esperar);
        assert_eq!(vigia.decidir(Jogo::Iniciando(7), t0), Acao::Esperar);
        assert_eq!(vigia.decidir(Jogo::Pronto(7), t0), Acao::Esperar);
        assert_eq!(vigia.decidir(Jogo::Pronto(7), t0 + CARENCIA / 2), Acao::Esperar);
        assert_eq!(vigia.decidir(Jogo::Pronto(7), t0 + CARENCIA), Acao::Injetar(7));
        // não tenta de novo no mesmo processo (nem se a injeção falhou)
        assert_eq!(vigia.decidir(Jogo::Pronto(7), t0 + CARENCIA * 3), Acao::Esperar);
    }

    #[test]
    fn grace_restarts_if_the_game_stops_being_ready() {
        let mut vigia = Vigia::default();
        let t0 = Instant::now();
        vigia.decidir(Jogo::Pronto(7), t0);
        // janela sumiu (troca de modo de vídeo) no meio da carência
        vigia.decidir(Jogo::Iniciando(7), t0 + CARENCIA / 2);
        assert_eq!(vigia.decidir(Jogo::Pronto(7), t0 + CARENCIA), Acao::Esperar);
        assert_eq!(vigia.decidir(Jogo::Pronto(7), t0 + CARENCIA * 2), Acao::Injetar(7));
    }

    #[test]
    fn already_injected_or_new_game_process() {
        let mut vigia = Vigia::default();
        let t0 = Instant::now();
        // overlay já estava lá: nunca injeta nesse processo
        vigia.decidir(Jogo::ComOverlay(7), t0);
        assert_eq!(vigia.decidir(Jogo::Pronto(7), t0 + CARENCIA * 2), Acao::Esperar);
        // jogo reaberto (outro pid): injeta de novo depois da carência
        vigia.decidir(Jogo::Fechado, t0 + CARENCIA * 3);
        vigia.decidir(Jogo::Pronto(8), t0 + CARENCIA * 4);
        assert_eq!(vigia.decidir(Jogo::Pronto(8), t0 + CARENCIA * 5), Acao::Injetar(8));
    }
}
