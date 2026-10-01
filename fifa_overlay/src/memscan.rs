//! Varredura de memória IN-PROCESS (de dentro da própria DLL injetada).
//!
//! Diferente das ferramentas externas usadas nas sessões anteriores
//! (Python com `ReadProcessMemory`/`OpenProcess`, Cheat Engine), aqui
//! rodamos DENTRO do processo do fifa16.exe. Isso significa:
//! - Não precisamos de `OpenProcess` com direitos elevados — usamos o
//!   pseudo-handle do processo atual (`GetCurrentProcess()`).
//! - Não precisamos suspender threads (o que motivou o crash da
//!   sessão anterior ao usar "Pointer scan" do Cheat Engine).
//!
//! ## Por que `ReadProcessMemory` em vez de desreferenciar ponteiros direto?
//!
//! A primeira versão deste módulo lia memória via
//! `std::slice::from_raw_parts` sobre um ponteiro cru, validado
//! apenas uma vez por `VirtualQuery` no início do scan. Isso causou
//! um crash real (access violation / SEH exception) quando o scan
//! passou a rodar numa thread separada (para não travar a UI): como
//! o jogo continua rodando concorrentemente enquanto escaneamos, uma
//! página de memória pode ser desmapeada/liberada pelo próprio motor
//! do FIFA entre o momento em que `VirtualQuery` confirma que ela
//! existe e o momento em que efetivamente a lemos — gerando um
//! `STATUS_ACCESS_VIOLATION` que o Rust não consegue capturar (é uma
//! exceção de hardware, não um panic Rust; `catch_unwind` não pega).
//!
//! `ReadProcessMemory`, mesmo usada no próprio processo via
//! `GetCurrentProcess()`, faz essa cópia de forma protegida
//! internamente pelo kernel/runtime do Windows: se a página não
//! estiver mais acessível no momento exato da cópia, a função
//! simplesmente retorna `FALSE` (erro), em vez de crashar o processo
//! inteiro. É a mesma técnica seguindo o mesmo princípio de robustez
//! que já usávamos nas ferramentas Python externas — só que aqui
//! aplicada a nós mesmos.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Memory::{
    VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_PRIVATE, PAGE_GUARD, PAGE_NOACCESS,
};
use windows::Win32::System::Threading::GetCurrentProcess;

/// Tamanho máximo de uma única região que aceitamos varrer (mesmo
/// limite usado nas ferramentas Python anteriores, para evitar
/// regiões monstruosas de allocator genérico).
const MAX_REGION_SIZE: usize = 64 * 1024 * 1024;

/// Assinatura de cabeçalho de database FIFA (t3db v8), a mesma usada
/// em `fifa16_db_parser.py` (`DB_SIGNATURE`).
pub const DB_SIGNATURE: &[u8] = b"DB\x00\x08\x00\x00\x00\x00";

#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub base: usize,
    pub size: usize,
}

/// Enumera todas as regiões de memória PRIVATE + COMMIT do processo
/// atual (o mesmo filtro usado por `memory.py::filter_dynamic_regions`),
/// usando `VirtualQuery` local (sem `VirtualQueryEx`/handle externo).
pub fn enumerate_private_committed_regions() -> Vec<Region> {
    let mut regions = Vec::new();
    let mut address: usize = 0;
    const MAX_ADDRESS: usize = 0x7FFF_FFF0_0000;

    while address < MAX_ADDRESS {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let written = unsafe {
            VirtualQuery(
                Some(address as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };

        if written == 0 {
            break;
        }

        let region_size = mbi.RegionSize;
        if region_size == 0 {
            break;
        }

        let is_committed = mbi.State == MEM_COMMIT;
        let is_private = mbi.Type == MEM_PRIVATE;
        let is_guarded = (mbi.Protect.0 & PAGE_GUARD.0) != 0;
        let is_noaccess = mbi.Protect == PAGE_NOACCESS;

        if is_committed
            && is_private
            && !is_guarded
            && !is_noaccess
            && (region_size as usize) <= MAX_REGION_SIZE
        {
            regions.push(Region {
                base: mbi.BaseAddress as usize,
                size: region_size as usize,
            });
        }

        address = (mbi.BaseAddress as usize).wrapping_add(region_size as usize);

        if address == 0 {
            break;
        }
    }

    regions
}

/// Lê os bytes de uma região de forma SEGURA usando `ReadProcessMemory`
/// com o pseudo-handle do processo atual. Retorna `None` se a leitura
/// falhar (página liberada/protegida entre o VirtualQuery e agora —
/// isso é esperado e tratado como situação normal, não um erro fatal).
///
/// Diferente da primeira versão, retorna um `Vec<u8>` OWNED (cópia),
/// não uma referência para a memória original — isso é necessário
/// porque `ReadProcessMemory` sempre copia para um buffer nosso, e
/// também é mais seguro (não há mais um `&'static [u8]` apontando pra
/// memória que pode virar lixo a qualquer momento).
pub fn read_region_bytes(region: &Region) -> Option<Vec<u8>> {
    if region.base == 0 || region.size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; region.size];
    let mut bytes_read: usize = 0;

    let current_process: HANDLE = unsafe { GetCurrentProcess() };

    let ok = unsafe {
        ReadProcessMemory(
            current_process,
            region.base as *const _,
            buffer.as_mut_ptr() as *mut _,
            region.size,
            Some(&mut bytes_read),
        )
    };

    if ok.is_err() || bytes_read == 0 {
        return None;
    }

    buffer.truncate(bytes_read);
    Some(buffer)
}

/// Escreve bytes num endereço arbitrário via `WriteProcessMemory`
/// protegido (com o pseudo-handle do processo atual). Mesmo princípio
/// de segurança de `read_region_bytes`: se a página não for
/// gravável/acessível no momento exato, retorna `false` em vez de
/// crashar o processo.
pub fn write_bytes_at(address: usize, data: &[u8]) -> bool {
    if address == 0 || data.is_empty() {
        return false;
    }

    let current_process: HANDLE = unsafe { GetCurrentProcess() };
    let mut bytes_written: usize = 0;

    let ok = unsafe {
        WriteProcessMemory(
            current_process,
            address as *const _,
            data.as_ptr() as *const _,
            data.len(),
            Some(&mut bytes_written),
        )
    };

    ok.is_ok() && bytes_written == data.len()
}

/// Procura todas as ocorrências de `needle` dentro de `haystack`,
/// retornando os offsets relativos ao início do haystack. Usa
/// `memchr::memmem` (SIMD quando disponível) — bem mais rápido que um
/// loop byte-a-byte manual (a versão ingênua original levava minutos
/// para varrer a memória completa do processo).
fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    memchr::memmem::find_iter(haystack, needle).collect()
}

/// Resultado de uma database encontrada em memória: endereço
/// absoluto de início e tamanho da região que a contém, MAIS os
/// bytes já lidos (para não precisar reler a região de novo depois).
#[derive(Debug, Clone)]
pub struct DbLocation {
    pub region_base: usize,
    pub region_size: usize,
    pub offset_in_region: usize,
    pub region_bytes: Vec<u8>,
}

/// Varre todas as regiões PRIVATE+COMMIT procurando ocorrências de um
/// valor i32 exato, alinhado a 4 bytes. Equivalente in-process de
/// `scan_value_live.py` — útil para localizar candidatos "vivos" de
/// um atributo de jogador (ex: Strength) sem depender de endereços de
/// heap fixos de uma sessão anterior (que mudam entre execuções).
pub fn scan_for_i32_value(target_value: i32) -> Vec<usize> {
    let needle = target_value.to_le_bytes();
    let regions = enumerate_private_committed_regions();
    let mut found = Vec::new();

    for region in regions {
        let Some(bytes) = read_region_bytes(&region) else { continue };
        for offset in find_all(&bytes, &needle) {
            if offset % 4 == 0 {
                found.push(region.base + offset);
            }
        }
    }

    found
}

/// Filtra uma lista de endereços já conhecidos (de um scan anterior),
/// mantendo apenas aqueles cujo valor ATUAL bate com `target_value`.
/// Equivalente ao "Next Scan" do Cheat Engine — permite reduzir
/// progressivamente uma lista grande de candidatos trocando o
/// contexto no jogo (ex: selecionar outro jogador) entre scans.
pub fn filter_addresses_by_i32_value(addresses: &[usize], target_value: i32) -> Vec<usize> {
    addresses
        .iter()
        .copied()
        .filter(|&addr| {
            let region = Region { base: addr, size: 4 };
            match read_region_bytes(&region) {
                Some(bytes) if bytes.len() == 4 => {
                    i32::from_le_bytes(bytes.try_into().unwrap()) == target_value
                }
                _ => false,
            }
        })
        .collect()
}

/// Varre todas as regiões PRIVATE+COMMIT do processo procurando a
/// assinatura de database FIFA. Equivalente in-process de
/// `find_db_in_memory.py`, mas usando leitura protegida via
/// `ReadProcessMemory` (ver documentação do módulo).
pub fn find_databases_in_memory() -> Vec<DbLocation> {
    let regions = enumerate_private_committed_regions();
    let mut found = Vec::new();

    for region in regions {
        let Some(bytes) = read_region_bytes(&region) else {
            continue;
        };

        for offset in find_all(&bytes, DB_SIGNATURE) {
            found.push(DbLocation {
                region_base: region.base,
                region_size: region.size,
                offset_in_region: offset,
                region_bytes: bytes.clone(),
            });
        }
    }

    found
}
