//! Despejo da imagem do `fifa16.exe` (2026-10-08, só para desenvolvimento).
//!
//! Para abrir telas do jogo direto (sem apertar botões), é preciso achar no
//! código do jogo quem dispara eventos como `EnterTransferOfferActionPopup`.
//! Isso se faz offline, olhando o código. Como o executável tem proteção
//! (packer), o código de verdade só existe na memória do processo em execução:
//! este módulo copia a imagem do módulo, como está agora, para um arquivo.
//!
//! SÓ LEITURA: nada é escrito na memória do jogo, não há depurador nem
//! gancho. Roda uma vez por pedido, numa thread própria (o frame não espera),
//! em blocos pequenos e com uma pausa entre eles para não pesar.
//!
//! Pedido: criar o arquivo `%TEMP%\fifa_despejar_imagem.pedido`. O overlay o
//! apaga, grava `%TEMP%\fifa16_imagem.bin` (a imagem, byte a byte: o deslocamento
//! no arquivo é o RVA) e `%TEMP%\fifa16_imagem.json` (base, tamanho e as
//! faixas que não deu para ler), e registra `[despejo]` no log.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::memscan::{read_region_bytes, Region};
use crate::pointer_scan::enumerate_modules;

const TAMANHO_DA_PAGINA: usize = 4096;
const TAMANHO_DO_BLOCO: usize = 256 * 1024;
const PAUSA_ENTRE_BLOCOS: Duration = Duration::from_millis(1);
const INTERVALO_CHECAGEM: Duration = Duration::from_secs(1);

/// Copia `tamanho` bytes para `saida`, lendo com `ler(deslocamento, tamanho)`
/// em blocos. Bloco que não dá para ler inteiro é refeito página por página;
/// página ilegível vira zeros. Devolve as faixas `(início, fim)` ilegíveis
/// (deslocamentos, fim exclusivo), já unidas.
pub fn copiar_com_buracos(
    saida: &mut impl Write,
    tamanho: usize,
    mut ler: impl FnMut(usize, usize) -> Option<Vec<u8>>,
    mut pausa: impl FnMut(),
) -> io::Result<Vec<(usize, usize)>> {
    let mut ruins: Vec<(usize, usize)> = Vec::new();
    let mut marcar = |inicio: usize, fim: usize| match ruins.last_mut() {
        Some(ultima) if ultima.1 == inicio => ultima.1 = fim,
        _ => ruins.push((inicio, fim)),
    };
    let mut posicao = 0;
    while posicao < tamanho {
        let bloco = TAMANHO_DO_BLOCO.min(tamanho - posicao);
        match ler(posicao, bloco) {
            Some(bytes) if bytes.len() == bloco => saida.write_all(&bytes)?,
            _ => {
                // refaz por página: só as realmente ilegíveis viram buraco
                let mut feito = 0;
                while feito < bloco {
                    let pagina = TAMANHO_DA_PAGINA.min(bloco - feito);
                    let inicio = posicao + feito;
                    match ler(inicio, pagina) {
                        Some(bytes) if bytes.len() == pagina => saida.write_all(&bytes)?,
                        _ => {
                            saida.write_all(&vec![0u8; pagina])?;
                            marcar(inicio, inicio + pagina);
                        }
                    }
                    feito += pagina;
                }
            }
        }
        posicao += bloco;
        pausa();
    }
    saida.flush()?;
    Ok(ruins)
}

fn json_do_despejo(modulo: &str, base: usize, tamanho: usize, ruins: &[(usize, usize)]) -> String {
    let faixas: Vec<String> = ruins.iter().map(|(a, b)| format!("[{a},{b}]")).collect();
    format!(
        "{{\"modulo\":{:?},\"base\":{base},\"tamanho\":{tamanho},\"build\":{:?},\"ilegiveis\":[{}]}}\n",
        modulo,
        crate::BUILD_TAG,
        faixas.join(",")
    )
}

fn despejar(destino_bin: &PathBuf, destino_json: &PathBuf) -> Result<String, String> {
    let modulo = enumerate_modules()
        .into_iter()
        .find(|m| m.name.to_ascii_lowercase().ends_with("fifa16.exe"))
        .ok_or("o módulo fifa16.exe não foi encontrado")?;
    let inicio = Instant::now();
    let arquivo = std::fs::File::create(destino_bin).map_err(|e| format!("não criou {}: {e}", destino_bin.display()))?;
    let mut saida = io::BufWriter::new(arquivo);
    let ruins = copiar_com_buracos(
        &mut saida,
        modulo.size,
        |deslocamento, tamanho| read_region_bytes(&Region { base: modulo.base + deslocamento, size: tamanho }),
        || std::thread::sleep(PAUSA_ENTRE_BLOCOS),
    )
    .map_err(|e| format!("falha ao gravar: {e}"))?;
    std::fs::write(destino_json, json_do_despejo(&modulo.name, modulo.base, modulo.size, &ruins))
        .map_err(|e| format!("não gravou {}: {e}", destino_json.display()))?;
    let ilegiveis: usize = ruins.iter().map(|(a, b)| b - a).sum();
    Ok(format!(
        "{} bytes de {} (base 0x{:X}) em {} ms; {} faixa(s) ilegível(is), {} KB.",
        modulo.size,
        modulo.name,
        modulo.base,
        inicio.elapsed().as_millis(),
        ruins.len(),
        ilegiveis / 1024
    ))
}

/// Atende o pedido de despejo. Chamado a cada frame; só olha o arquivo de
/// pedido a cada segundo.
pub struct Despejo {
    pedido: PathBuf,
    proxima_checagem: Instant,
    rodando: Arc<AtomicBool>,
}

impl Despejo {
    pub fn new() -> Self {
        Despejo { pedido: std::env::temp_dir().join("fifa_despejar_imagem.pedido"), proxima_checagem: Instant::now(), rodando: Arc::new(AtomicBool::new(false)) }
    }

    pub fn verificar(&mut self) {
        let agora = Instant::now();
        if agora < self.proxima_checagem {
            return;
        }
        self.proxima_checagem = agora + INTERVALO_CHECAGEM;
        if self.rodando.load(Ordering::Relaxed) || !self.pedido.exists() {
            return;
        }
        let _ = std::fs::remove_file(&self.pedido);
        self.rodando.store(true, Ordering::Relaxed);
        let rodando = Arc::clone(&self.rodando);
        let pasta = std::env::temp_dir();
        let (bin, json) = (pasta.join("fifa16_imagem.bin"), pasta.join("fifa16_imagem.json"));
        tracing::info!("[despejo] Copiando a imagem do fifa16.exe para {} (só leitura).", bin.display());
        let iniciou = std::thread::Builder::new().name("despejo".to_string()).spawn(move || {
            match despejar(&bin, &json) {
                Ok(resumo) => tracing::info!("[despejo] Pronto: {resumo}"),
                Err(erro) => tracing::warn!("[despejo] Falhou: {erro}"),
            }
            rodando.store(false, Ordering::Relaxed);
        });
        if iniciou.is_err() {
            tracing::warn!("[despejo] Não foi possível iniciar a thread.");
            self.rodando.store(false, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma "memória" de 600 KB com padrão conhecido e páginas ilegíveis.
    fn memoria(tamanho: usize, ruins: &'static [usize]) -> impl FnMut(usize, usize) -> Option<Vec<u8>> {
        move |de, n| {
            let paginas = (de / TAMANHO_DA_PAGINA)..=((de + n - 1) / TAMANHO_DA_PAGINA);
            if paginas.into_iter().any(|p| ruins.contains(&p)) || de + n > tamanho {
                return None;
            }
            Some((de..de + n).map(|i| (i % 251) as u8).collect())
        }
    }

    #[test]
    fn it_copies_everything_and_reports_no_holes_when_all_is_readable() {
        let mut saida = Vec::new();
        let ruins = copiar_com_buracos(&mut saida, 600 * 1024, memoria(600 * 1024, &[]), || {}).expect("copiou");
        assert!(ruins.is_empty());
        assert_eq!(saida.len(), 600 * 1024);
        assert!(saida.iter().enumerate().all(|(i, b)| *b == (i % 251) as u8));
    }

    #[test]
    fn an_unreadable_page_becomes_zeros_and_only_that_page_is_a_hole() {
        let mut saida = Vec::new();
        // páginas 3 e 4 (vizinhas, no 1º bloco de 64 páginas) e 100 (no 2º bloco)
        let ruins = copiar_com_buracos(&mut saida, 600 * 1024, memoria(600 * 1024, &[3, 4, 100]), || {}).expect("copiou");
        assert_eq!(ruins, [(3 * 4096, 5 * 4096), (100 * 4096, 101 * 4096)], "vizinhas se unem");
        assert_eq!(saida.len(), 600 * 1024, "o arquivo mantém o tamanho: o deslocamento é o RVA");
        assert!(saida[3 * 4096..5 * 4096].iter().all(|b| *b == 0));
        assert_eq!(saida[2 * 4096 + 7], ((2 * 4096 + 7) % 251) as u8, "o resto do bloco foi lido");
        assert_eq!(saida[101 * 4096], ((101 * 4096) % 251) as u8);
    }

    #[test]
    fn the_last_partial_block_and_the_pause_between_blocks_are_handled() {
        let mut saida = Vec::new();
        let mut pausas = 0;
        let tamanho = TAMANHO_DO_BLOCO * 2 + 1234;
        let ruins = copiar_com_buracos(&mut saida, tamanho, memoria(tamanho, &[]), || pausas += 1).expect("copiou");
        assert!(ruins.is_empty());
        assert_eq!(saida.len(), tamanho);
        assert_eq!(pausas, 3, "uma por bloco");
    }

    #[test]
    fn the_json_names_the_module_the_base_and_the_holes() {
        let json = json_do_despejo("C:\\Jogo\\fifa16.exe", 0x1_4000_0000, 4096, &[(0, 4096)]);
        assert!(json.contains("\"base\":5368709120") && json.contains("\"tamanho\":4096") && json.contains("[[0,4096]]"), "{json}");
        assert!(json.contains("fifa16.exe") && json.contains("\"build\""));
    }
}
