//! Pointer scan reverso IN-PROCESS: dado um endereço alvo conhecido
//! (ex: onde confirmamos que "Strength" de um jogador realmente
//! escreve/persiste), varre a memória do processo procurando
//! ponteiros de 8 bytes que apontem para esse endereço, subindo
//! níveis até (idealmente) achar uma âncora dentro de um módulo
//! carregado (fifa16.exe ou fifa16.bin) — que seria uma variável
//! global/estática, estável entre execuções do jogo.
//!
//! Diferente do "Pointer scan for this address" do Cheat Engine (que
//! suspende todas as threads do processo para tirar um snapshot
//! consistente — e isso já causou um crash do FIFA por reação do
//! anti-tamper), esta implementação roda DENTRO do processo, lê
//! memória via `ReadProcessMemory` protegido (sem suspender nada), e
//! portanto não deveria disparar o mesmo tipo de detecção.

use crate::memscan::{enumerate_private_committed_regions, read_region_bytes, Region};

#[derive(Debug, Clone)]
pub struct ModuleRange {
    pub name: String,
    pub base: usize,
    pub size: usize,
}

impl ModuleRange {
    pub fn contains(&self, addr: usize) -> bool {
        addr >= self.base && addr < self.base + self.size
    }

    pub fn offset_of(&self, addr: usize) -> usize {
        addr - self.base
    }
}

/// Enumera os módulos carregados no processo atual (equivalente
/// in-process de `modules.py::list_modules`, mas usando
/// `EnumProcessModules` sem precisar de handle externo).
pub fn enumerate_modules() -> Vec<ModuleRange> {
    use windows::Win32::Foundation::{HMODULE, MAX_PATH};
    use windows::Win32::System::ProcessStatus::{
        EnumProcessModules, GetModuleFileNameExW, GetModuleInformation, MODULEINFO,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut modules = Vec::new();
    let process = unsafe { GetCurrentProcess() };

    // Primeira tentativa com um buffer razoável; a maioria dos
    // processos tem bem menos que 1024 módulos carregados.
    let mut buf: Vec<HMODULE> = vec![HMODULE::default(); 1024];
    let mut needed: u32 = 0;

    let ok = unsafe {
        EnumProcessModules(
            process,
            buf.as_mut_ptr(),
            (buf.len() * std::mem::size_of::<HMODULE>()) as u32,
            &mut needed,
        )
    };

    if ok.is_err() {
        return modules;
    }

    let count = (needed as usize) / std::mem::size_of::<HMODULE>();

    for i in 0..count.min(buf.len()) {
        let hmodule = buf[i];

        let mut name_buf = [0u16; MAX_PATH as usize];
        let name_len = unsafe { GetModuleFileNameExW(Some(process), Some(hmodule), &mut name_buf) };
        let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);

        let mut info = MODULEINFO::default();
        let info_ok = unsafe {
            GetModuleInformation(
                process,
                hmodule,
                &mut info,
                std::mem::size_of::<MODULEINFO>() as u32,
            )
        };

        if info_ok.is_ok() {
            modules.push(ModuleRange {
                name,
                base: info.lpBaseOfDll as usize,
                size: info.SizeOfImage as usize,
            });
        }
    }

    modules
}

#[derive(Debug, Clone)]
pub struct PointerHit {
    pub pointer_addr: usize,
    pub pointer_value: usize,
}

#[derive(Debug, Clone)]
pub struct AnchorFound {
    pub module_name: String,
    pub module_offset: usize,
    pub pointer_addr: usize,
    pub pointer_value: usize,
}

/// Varre todas as regiões PRIVATE+COMMIT procurando ponteiros de 8
/// bytes (alinhados a 8) cujo valor caia em [target, target+tolerance]
/// para algum alvo em `targets`.
fn scan_pointers_to_targets(targets: &[usize], tolerance: usize) -> Vec<PointerHit> {
    if targets.is_empty() {
        return Vec::new();
    }

    let mut sorted_targets = targets.to_vec();
    sorted_targets.sort_unstable();

    let regions = enumerate_private_committed_regions();
    let mut hits = Vec::new();

    for region in regions {
        let Some(bytes) = read_region_bytes(&region) else { continue };
        let usable_len = bytes.len() - (bytes.len() % 8);

        for chunk_start in (0..usable_len).step_by(8) {
            let chunk = &bytes[chunk_start..chunk_start + 8];
            let value = usize::from_le_bytes(chunk.try_into().unwrap());

            if value == 0 {
                continue;
            }

            // busca binária pelo alvo mais próximo (<=  value)
            let idx = sorted_targets.partition_point(|&t| t <= value);

            let mut matched = false;
            if idx > 0 {
                let candidate = sorted_targets[idx - 1];
                if value >= candidate && value - candidate <= tolerance {
                    matched = true;
                }
            }
            if !matched && idx < sorted_targets.len() {
                let candidate = sorted_targets[idx];
                if candidate >= value && candidate - value <= tolerance {
                    // esse caso só interessa se tolerance permitir
                    // ponteiro apontando LIGEIRAMENTE ANTES do alvo
                    // (não é o padrão comum, mas mantemos por simetria
                    // com a implementação Python original).
                }
            }

            if matched {
                hits.push(PointerHit {
                    pointer_addr: region.base + chunk_start,
                    pointer_value: value,
                });
            }
        }
    }

    hits
}

/// Resultado completo de um pointer scan multi-nível.
pub struct PointerScanOutcome {
    pub anchors: Vec<AnchorFound>,
    pub levels_explored: usize,
    pub last_level_hit_count: usize,
    /// Hits do NÍVEL 1 (ponteiros diretos para o endereço alvo
    /// original) — geralmente o conjunto mais confiável/pequeno,
    /// útil para inspeção manual mesmo se o BFS completo não achar
    /// uma âncora dentro de um módulo.
    pub level1_hits: Vec<PointerHit>,
}

/// Executa o BFS multi-nível a partir de um endereço alvo conhecido,
/// procurando uma âncora estável dentro de um módulo carregado.
///
/// `tolerance`: quantos bytes de "folga" aceitar entre o valor do
/// ponteiro e o alvo (útil porque o ponteiro pode apontar para o
/// início de uma struct maior que contém o campo, não exatamente o
/// campo em si).
pub fn find_pointer_chain(
    target_address: usize,
    max_levels: usize,
    tolerance: usize,
) -> PointerScanOutcome {
    let modules = enumerate_modules();

    let mut current_targets = vec![target_address];
    let mut visited: std::collections::HashSet<usize> = current_targets.iter().copied().collect();

    let mut levels_explored = 0;
    let mut last_level_hit_count = 0;
    let mut level1_hits: Vec<PointerHit> = Vec::new();

    for level in 1..=max_levels {
        levels_explored = level;

        tracing::info!(
            "[pointer-scan] Nível {}: buscando ponteiros para {} alvo(s)...",
            level,
            current_targets.len()
        );

        let hits = scan_pointers_to_targets(&current_targets, tolerance);
        last_level_hit_count = hits.len();

        tracing::info!("[pointer-scan] Nível {}: {} hit(s) encontrados.", level, hits.len());

        if level == 1 {
            level1_hits = hits.clone();
            for hit in &level1_hits {
                tracing::info!(
                    "[pointer-scan] NÍVEL 1 HIT: 0x{:X} -> 0x{:X}",
                    hit.pointer_addr,
                    hit.pointer_value
                );
            }
        }

        if hits.is_empty() {
            break;
        }

        let mut anchors = Vec::new();
        for hit in &hits {
            if let Some(module) = modules.iter().find(|m| m.contains(hit.pointer_addr)) {
                anchors.push(AnchorFound {
                    module_name: module.name.clone(),
                    module_offset: module.offset_of(hit.pointer_addr),
                    pointer_addr: hit.pointer_addr,
                    pointer_value: hit.pointer_value,
                });
            }
        }

        if !anchors.is_empty() {
            return PointerScanOutcome { anchors, levels_explored, last_level_hit_count, level1_hits };
        }

        // Corta o crescimento cedo para evitar explosão combinatória
        // (cada hit deste nível vira um alvo do próximo nível; sem um
        // limite razoável, poucas centenas de hits com tolerância > 0
        // podem virar dezenas de milhares em 2-3 níveis).
        const MAX_TARGETS_PER_LEVEL: usize = 2_000;

        let mut new_targets: Vec<usize> = hits
            .iter()
            .map(|h| h.pointer_addr)
            .filter(|a| !visited.contains(a))
            .collect();

        if new_targets.is_empty() {
            tracing::info!("[pointer-scan] Nível {}: sem novos alvos (todos já visitados). Parando.", level);
            break;
        }

        if new_targets.len() > MAX_TARGETS_PER_LEVEL {
            tracing::warn!(
                "[pointer-scan] Nível {}: cortando {} alvos para {} (evitar explosão).",
                level,
                new_targets.len(),
                MAX_TARGETS_PER_LEVEL
            );
            new_targets.truncate(MAX_TARGETS_PER_LEVEL);
        }

        for t in &new_targets {
            visited.insert(*t);
        }

        current_targets = new_targets;
    }

    PointerScanOutcome { anchors: Vec::new(), levels_explored, last_level_hit_count, level1_hits }
}

/// Lê um valor u32 de um endereço arbitrário via `ReadProcessMemory`
/// protegido (retorna None em vez de crashar se a página não for
/// acessível).
pub fn read_u32_at(address: usize) -> Option<u32> {
    let region = Region { base: address, size: 4 };
    let bytes = read_region_bytes(&region)?;
    if bytes.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
}
