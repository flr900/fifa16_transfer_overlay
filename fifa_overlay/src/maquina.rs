//! Máquina de estados do modo carreira do FIFA (2026-10-08, só para desenvolvimento).
//!
//! A análise da imagem do `fifa16.exe` (ver `integracao-negociacao.md`) mostrou que
//! as telas (menu do jogador, compra, empréstimo, contrato...) são Actions de uma
//! máquina de estados cujo objeto fica no HEAP. Para saber se há um campo de "estado
//! atual" ou de "ação pendente" (que um dia dê para escrever, como o orçamento), este
//! módulo:
//!
//! 1. **acha o objeto**: varre o heap, em blocos e numa thread própria, pelo
//!    PONTEIRO DO NOME da Action de compra (`[Action+8]` aponta para a string
//!    `ActionEnterTransferOfferActionPopup`, que mora na imagem). Cada Action tem a
//!    vtable sobrescrita pela do seu tipo logo depois de construída, então a vtable
//!    não serve de prova; os nomes dos vizinhos (empréstimo e negociação de
//!    contrato) servem;
//! 2. **observa**: com o gravador ligado, lê os primeiros 0x2000 bytes do objeto a cada
//!    frame e registra no log (`[maquina]`) o que muda, com o nome da ação quando um
//!    valor aponta para um token.
//!
//! SÓ LEITURA. Pedido: criar `%TEMP%\fifa_achar_maquina.pedido` (o overlay o apaga).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::memscan::{enumerate_private_committed_regions, read_region_bytes, read_region_into, Region};
use crate::pointer_scan::enumerate_modules;

/// RVA da vtable da máquina de estados do modo carreira (build 16.0.2904053); só
/// informativa (a classe derivada pode sobrescrevê-la).
const RVA_VTABLE_MAQUINA: usize = 0x306_8590;
/// RVAs das strings com o nome das Actions (ficam na imagem, sem ASLR).
const RVA_NOME_COMPRA: usize = 0x304_B0B0; // ActionEnterTransferOfferActionPopup
const RVA_NOME_EMPRESTIMO: usize = 0x304_B0E8; // ActionEnterLoanOfferActionPopup
const RVA_NOME_NEGOCIACAO: usize = 0x304_B118; // ActionEnterPlayerContractNegotiationFromActionPopup
/// Onde ficam essas Actions no objeto (o nome é o 2º campo: `+8`).
const DESLOC_ACAO_COMPRA: usize = 0xE98;
const DESLOC_ACAO_EMPRESTIMO: usize = 0xEB0;
const DESLOC_ACAO_NEGOCIACAO: usize = 0xEC8;
/// As Actions ficam no objeto entre estes deslocamentos (a grade não é uniforme: a
/// conferência é a vtable da Action no endereço, não o alinhamento).
const INICIO_DAS_ACOES: usize = 0x98;
const FIM_DAS_ACOES: usize = 0x1400;
/// Quanto do objeto se despeja ao achá-lo, e quanto se observa por frame.
const TAMANHO_DO_DESPEJO: usize = 0x2000;
/// O objeto tem 204 Actions (constantes) de `+0x28` a `+0x1348`; os campos de estado vêm depois.
const TAMANHO_OBSERVADO: usize = 0x2000;
const TAMANHO_DO_BLOCO: usize = 8 * 1024 * 1024;
const MAX_ACHADOS: usize = 64;
/// Um campo que muda mais que isto no log é ruído (contador de frames).
const MAX_REGISTROS_POR_CAMPO: u32 = 40;
const MAX_LINHAS_POR_QUADRO: usize = 40;
const INTERVALO_CHECAGEM: Duration = Duration::from_secs(1);

/// Posições (múltiplas de 8) de `valor` num bloco de bytes.
fn achar_valor_alinhado(bloco: &[u8], valor: u64) -> Vec<usize> {
    let alvo = valor.to_le_bytes();
    bloco
        .chunks_exact(8)
        .enumerate()
        .filter(|(_, c)| *c == alvo)
        .map(|(i, _)| i * 8)
        .collect()
}

/// Palavras de 8 bytes que mudaram entre dois instantes: `(deslocamento, antes, depois)`.
fn palavras_que_mudaram(antes: &[u8], depois: &[u8]) -> Vec<(usize, u64, u64)> {
    antes
        .chunks_exact(8)
        .zip(depois.chunks_exact(8))
        .enumerate()
        .filter(|(_, (a, d))| a != d)
        .map(|(i, (a, d))| {
            (i * 8, u64::from_le_bytes(<[u8; 8]>::try_from(a).unwrap_or([0; 8])), u64::from_le_bytes(<[u8; 8]>::try_from(d).unwrap_or([0; 8])))
        })
        .collect()
}

/// O que um valor "é", para o log: ponteiro para uma Action do objeto, para dentro
/// do objeto, ou para o código/dados da imagem.
fn descrever(valor: u64, instancia: usize, base_exe: usize, tamanho_imagem: usize, nome_da_acao: impl Fn(usize) -> Option<String>) -> String {
    let v = match usize::try_from(valor) {
        Ok(v) if v != 0 => v,
        _ => return String::new(),
    };
    if v >= instancia + INICIO_DAS_ACOES && v < instancia + FIM_DAS_ACOES {
        return match nome_da_acao(v) {
            Some(nome) => format!("  ; ação +0x{:X} \"{nome}\"", v - instancia),
            None => format!("  ; +0x{:X} do objeto", v - instancia),
        };
    }
    if v >= instancia && v < instancia + TAMANHO_DO_DESPEJO {
        return format!("  ; +0x{:X} do objeto", v - instancia);
    }
    if v >= base_exe && v < base_exe + tamanho_imagem {
        return format!("  ; imagem RVA 0x{:X}", v - base_exe);
    }
    String::new()
}

/// Lê um texto ASCII terminado em zero (até 80 bytes).
fn ler_texto(endereco: usize) -> Option<String> {
    let bytes = read_region_bytes(&Region { base: endereco, size: 80 }).or_else(|| read_region_bytes(&Region { base: endereco, size: 16 }))?;
    let fim = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let texto = &bytes[..fim];
    (texto.len() >= 4 && texto.iter().all(|b| (0x20..=0x7E).contains(b))).then(|| String::from_utf8_lossy(texto).into_owned())
}

/// O nome de uma Action (`[acao + 8]` aponta para o texto, que começa com "Action").
fn nome_da_acao(endereco: usize) -> Option<String> {
    let bytes = read_region_bytes(&Region { base: endereco.checked_add(8)?, size: 8 })?;
    let ponteiro = usize::try_from(u64::from_le_bytes(<[u8; 8]>::try_from(bytes.get(..8)?).ok()?)).ok()?;
    ler_texto(ponteiro).filter(|t| t.starts_with("Action"))
}

fn ler_u64(endereco: usize) -> Option<u64> {
    let b = read_region_bytes(&Region { base: endereco, size: 8 })?;
    Some(u64::from_le_bytes(<[u8; 8]>::try_from(b.get(..8)?).ok()?))
}

/// Varre o heap pelo ponteiro do nome da Action de compra e devolve os objetos que
/// passam na conferência (os nomes de empréstimo e de negociação nos lugares certos).
fn procurar(base_exe: usize) -> Vec<usize> {
    let nome_compra = (base_exe + RVA_NOME_COMPRA) as u64;
    let mut buffer = vec![0u8; TAMANHO_DO_BLOCO];
    let faixa_do_buffer = buffer.as_ptr() as usize..buffer.as_ptr() as usize + buffer.len();
    let mut candidatos = Vec::new();
    let mut blocos_lidos = 0usize;
    for regiao in enumerate_private_committed_regions() {
        let mut inicio = 0;
        while inicio < regiao.size && candidatos.len() < MAX_ACHADOS {
            let tamanho = TAMANHO_DO_BLOCO.min(regiao.size - inicio);
            let bloco = Region { base: regiao.base + inicio, size: tamanho };
            if let Some(lido) = read_region_into(&bloco, &mut buffer) {
                blocos_lidos += 1;
                for pos in achar_valor_alinhado(&buffer[..lido], nome_compra) {
                    let acao = bloco.base + pos - 8;
                    if !faixa_do_buffer.contains(&(bloco.base + pos)) && acao >= DESLOC_ACAO_COMPRA {
                        candidatos.push(acao - DESLOC_ACAO_COMPRA);
                    }
                }
            }
            inicio += tamanho;
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    tracing::info!("[maquina] Varredura: {blocos_lidos} bloco(s) lidos, {} candidato(s) com o nome da ação de compra.", candidatos.len());
    candidatos
        .into_iter()
        .filter(|c| {
            let emprestimo = ler_u64(c + DESLOC_ACAO_EMPRESTIMO + 8) == Some((base_exe + RVA_NOME_EMPRESTIMO) as u64);
            let negociacao = ler_u64(c + DESLOC_ACAO_NEGOCIACAO + 8) == Some((base_exe + RVA_NOME_NEGOCIACAO) as u64);
            let vtable = ler_u64(*c);
            tracing::info!(
                "[maquina] Candidato 0x{c:X}: empréstimo {}, negociação {}, vtable {:x?} (esperada 0x{:X}).",
                if emprestimo { "ok" } else { "não" },
                if negociacao { "ok" } else { "não" },
                vtable,
                base_exe + RVA_VTABLE_MAQUINA
            );
            emprestimo && negociacao
        })
        .collect()
}

pub struct Maquina {
    pedido: PathBuf,
    proxima_checagem: Instant,
    procurando: Arc<AtomicBool>,
    achados: Arc<Mutex<Option<Vec<usize>>>>,
    instancia: Option<usize>,
    anterior: Vec<u8>,
    base_exe: usize,
    tamanho_imagem: usize,
    registros: HashMap<usize, u32>,
}

impl Maquina {
    pub fn new() -> Self {
        Maquina {
            pedido: std::env::temp_dir().join("fifa_achar_maquina.pedido"),
            proxima_checagem: Instant::now(),
            procurando: Arc::new(AtomicBool::new(false)),
            achados: Arc::new(Mutex::new(None)),
            instancia: None,
            anterior: Vec::new(),
            base_exe: 0,
            tamanho_imagem: 0,
            registros: HashMap::new(),
        }
    }

    fn modulo(&mut self) -> bool {
        if self.base_exe == 0 {
            if let Some(m) = enumerate_modules().into_iter().find(|m| m.name.to_ascii_lowercase().ends_with("fifa16.exe")) {
                self.base_exe = m.base;
                self.tamanho_imagem = m.size;
            }
        }
        self.base_exe != 0
    }

    /// Atende o pedido de busca e recebe o resultado da thread. Chamado a cada frame.
    pub fn verificar(&mut self) {
        let agora = Instant::now();
        if agora < self.proxima_checagem {
            return;
        }
        self.proxima_checagem = agora + INTERVALO_CHECAGEM;
        if !self.procurando.load(Ordering::Relaxed) && self.pedido.exists() && self.modulo() {
            let _ = std::fs::remove_file(&self.pedido);
            self.procurando.store(true, Ordering::Relaxed);
            let (procurando, achados, base) = (Arc::clone(&self.procurando), Arc::clone(&self.achados), self.base_exe);
            tracing::info!("[maquina] Procurando a máquina de estados no heap (vtable 0x{:X}), só leitura...", base + RVA_VTABLE_MAQUINA);
            let iniciou = std::thread::Builder::new().name("maquina".to_string()).spawn(move || {
                let encontrados = procurar(base);
                if let Ok(mut guarda) = achados.lock() {
                    *guarda = Some(encontrados);
                }
                procurando.store(false, Ordering::Relaxed);
            });
            if iniciou.is_err() {
                self.procurando.store(false, Ordering::Relaxed);
            }
        }
        let resultado = self.achados.lock().ok().and_then(|mut g| g.take());
        if let Some(encontrados) = resultado {
            match encontrados.first() {
                Some(&endereco) => self.adotar(endereco, &encontrados),
                None => tracing::warn!("[maquina] Nenhuma máquina de estados achada (o jogo está na carreira?)."),
            }
        }
    }

    fn adotar(&mut self, endereco: usize, todos: &[usize]) {
        tracing::info!("[maquina] Máquina de estados em 0x{endereco:X} ({} confirmada(s): {:x?}).", todos.len(), todos);
        self.instancia = Some(endereco);
        self.registros.clear();
        self.anterior = read_region_bytes(&Region { base: endereco, size: TAMANHO_OBSERVADO }).unwrap_or_default();
        if let Some(bytes) = read_region_bytes(&Region { base: endereco, size: TAMANHO_DO_DESPEJO }) {
            let destino = std::env::temp_dir().join("fifa_maquina.bin");
            match std::fs::write(&destino, &bytes) {
                Ok(()) => tracing::info!("[maquina] Objeto ({} bytes) gravado em {}.", bytes.len(), destino.display()),
                Err(e) => tracing::warn!("[maquina] Não gravou o objeto: {e}"),
            }
        }
        // o estado de partida: o cabeçalho e a região logo depois das Actions (onde ficam os campos de estado)
        for (i, c) in self.anterior.chunks_exact(8).enumerate() {
            let desloc = i * 8;
            if !(desloc < 0x28 || (0x1340..0x1420).contains(&desloc)) {
                continue;
            }
            let v = u64::from_le_bytes(<[u8; 8]>::try_from(c).unwrap_or([0; 8]));
            if v != 0 {
                tracing::info!("[maquina] inicial +0x{:X}: 0x{v:X}{}", i * 8, descrever(v, endereco, self.base_exe, self.tamanho_imagem, nome_da_acao));
            }
        }
    }

    /// Com o gravador ligado: registra o que mudou no objeto (`ms` é o tempo do gravador).
    pub fn observar(&mut self, ms: u128) {
        let Some(endereco) = self.instancia else { return };
        let Some(agora) = read_region_bytes(&Region { base: endereco, size: TAMANHO_OBSERVADO }) else {
            tracing::warn!("[maquina] O objeto em 0x{endereco:X} deixou de ser legível (o jogo recarregou?); observação encerrada.");
            self.instancia = None;
            return;
        };
        if self.anterior.len() != agora.len() {
            self.anterior = agora;
            return;
        }
        let mut linhas = 0;
        for (desloc, antes, depois) in palavras_que_mudaram(&self.anterior, &agora) {
            let n = self.registros.entry(desloc).or_insert(0);
            *n += 1;
            if *n > MAX_REGISTROS_POR_CAMPO || linhas >= MAX_LINHAS_POR_QUADRO {
                continue;
            }
            linhas += 1;
            tracing::info!(
                "[maquina] t={ms}ms +0x{desloc:X}: 0x{antes:X} -> 0x{depois:X}{}",
                descrever(depois, endereco, self.base_exe, self.tamanho_imagem, nome_da_acao)
            );
        }
        self.anterior = agora;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scan_finds_aligned_values_only() {
        let mut bloco = vec![0u8; 64];
        bloco[16..24].copy_from_slice(&0x1_4306_8590u64.to_le_bytes());
        bloco[41..49].copy_from_slice(&0x1_4306_8590u64.to_le_bytes()); // desalinhado: não conta
        assert_eq!(achar_valor_alinhado(&bloco, 0x1_4306_8590), [16]);
        assert!(achar_valor_alinhado(&bloco, 7).is_empty() || achar_valor_alinhado(&bloco, 7).iter().all(|p| p % 8 == 0));
    }

    #[test]
    fn changed_words_are_reported_with_their_offset_and_both_values() {
        let mut antes = vec![0u8; 32];
        let mut depois = antes.clone();
        antes[8..16].copy_from_slice(&5u64.to_le_bytes());
        depois[8..16].copy_from_slice(&6u64.to_le_bytes());
        depois[24..32].copy_from_slice(&0xABu64.to_le_bytes());
        assert_eq!(palavras_que_mudaram(&antes, &depois), [(8, 5, 6), (24, 0, 0xAB)]);
        assert!(palavras_que_mudaram(&depois, &depois).is_empty());
    }

    #[test]
    fn a_value_is_described_as_an_action_of_the_object_or_as_code_or_nothing() {
        let (inst, base, tam) = (0x5000_0000usize, 0x1_4000_0000usize, 0x0952_4000usize);
        let nome = |v: usize| (v == inst + 0xE98).then(|| "ActionEnterTransferOfferActionPopup".to_string());
        assert_eq!(descrever((inst + 0xE98) as u64, inst, base, tam, nome), "  ; ação +0xE98 \"ActionEnterTransferOfferActionPopup\"");
        assert_eq!(descrever((inst + 0xEB0) as u64, inst, base, tam, |_| None), "  ; +0xEB0 do objeto");
        assert_eq!(descrever((inst + 0x40) as u64, inst, base, tam, |_| None), "  ; +0x40 do objeto", "dentro do objeto, fora da grade de ações");
        assert_eq!(descrever((base + 0x30A_8BF8) as u64, inst, base, tam, |_| None), "  ; imagem RVA 0x30A8BF8");
        assert_eq!(descrever(0, inst, base, tam, |_| None), "");
        assert_eq!(descrever(0x7777, inst, base, tam, |_| None), "");
    }
}
