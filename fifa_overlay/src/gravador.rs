//! Gravador de controle (2026-10-08, só para desenvolvimento).
//!
//! Para a Central abrir a tela de negociação do jogo é preciso saber, para
//! cada caminho (comprar, empréstimo, contrato), a sequência exata de botões
//! e o tempo entre eles. Em vez de adivinhar, o gravador registra no log
//! (`%TEMP%\\fifa_overlay.log`, prefixo `[gravador]`) cada aperto e cada
//! soltura do controle e cada vez que o jogador em foco no jogo muda.
//!
//! Também registra o **componente de tela** que o jogo carregou por último
//! (`[gravador] tela: ...`): o ponteiro `fifa16.exe+0x3357378` aponta para um
//! buffer pequeno que o jogo reescreve a cada widget de interface (CareerHub,
//! FluxTile_*, ...). Nas investigações antigas (`MEMORY_INVESTIGATION_NOTES.md`,
//! §6) ele NÃO se mostrou o "estado da tela", mas a sequência de nomes pode
//! ainda marcar as transições entre hub, lista e negociações. Lido de dentro
//! do processo por `ReadProcessMemory` protegido, a cada 100 ms; só quando o
//! gravador está ligado.
//!
//! Liga e desliga sozinho: existe enquanto o arquivo
//! `%TEMP%\\fifa_gravar_controle.pedido` existir (checado a cada segundo).
//! Desligado, não faz nada além dessa checagem. Só LÊ o controle (o mesmo
//! estado que o overlay já lê); nunca escreve nada no jogo.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::gamepad::{botao, EstadoControle, LIMIAR_GATILHO};
use crate::memscan::{read_region_bytes, Region};
use crate::pointer_scan::enumerate_modules;

/// Deslocamento, dentro do `fifa16.exe`, do ponteiro para o buffer de
/// componentes de interface (build 16.0.2904053; achado em 2026-09).
const OFFSET_BUFFER_DE_TELA: usize = 0x335_7378;
/// O buffer é um anel de entradas de 64 bytes; o ponteiro marca a entrada
/// escrita por último. Lê-se uma janela em volta (antes e depois dela).
const TAMANHO_DA_ENTRADA: usize = 64;
const JANELA_ANTES: usize = 1024;
const JANELA_TOTAL: usize = 2048;
/// Textos menores que isto (como `p+Z.`) são lixo binário, não nome de evento.
const MIN_TEXTO_DA_TELA: usize = 6;
/// Uma leitura por frame: o jogo reescreve o buffer em poucos milissegundos
/// e leituras a cada 100 ms perdiam nomes de evento inteiros.
const INTERVALO_TELA: Duration = Duration::ZERO;

const INTERVALO_CHECAGEM: Duration = Duration::from_secs(1);
/// Quanto o analógico esquerdo precisa passar para contar como um aperto
/// numa direção (os menus do FIFA também navegam por ele).
const LIMIAR_ANALOGICO: i16 = 16_000;

/// Os botões que o gravador nomeia, na ordem em que aparecem no log.
const NOMES: [(u16, &str); 14] = [
    (botao::DPAD_CIMA, "D-pad ↑"),
    (botao::DPAD_BAIXO, "D-pad ↓"),
    (botao::DPAD_ESQUERDA, "D-pad ←"),
    (botao::DPAD_DIREITA, "D-pad →"),
    (botao::A, "A"),
    (botao::B, "B"),
    (botao::X, "X"),
    (botao::Y, "Y"),
    (botao::LB, "LB"),
    (botao::RB, "RB"),
    (botao::START, "Start"),
    (botao::BACK, "Select"),
    (botao::L3, "L3"),
    (botao::R3, "R3"),
];

/// Tudo o que conta como "apertado" num instante: botões, gatilhos e as
/// quatro direções do analógico esquerdo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Apertos {
    botoes: u16,
    lt: bool,
    rt: bool,
    analogico_cima: bool,
    analogico_baixo: bool,
    analogico_esquerda: bool,
    analogico_direita: bool,
}

impl Apertos {
    fn de(estado: &EstadoControle) -> Self {
        Apertos {
            botoes: estado.botoes,
            lt: estado.lt >= LIMIAR_GATILHO,
            rt: estado.rt >= LIMIAR_GATILHO,
            analogico_cima: estado.ly >= LIMIAR_ANALOGICO,
            analogico_baixo: estado.ly <= -LIMIAR_ANALOGICO,
            analogico_esquerda: estado.lx <= -LIMIAR_ANALOGICO,
            analogico_direita: estado.lx >= LIMIAR_ANALOGICO,
        }
    }

    /// Cada entrada: nome e se está apertada.
    fn lista(&self) -> Vec<(&'static str, bool)> {
        let mut lista: Vec<(&'static str, bool)> = NOMES.iter().map(|(mascara, nome)| (*nome, self.botoes & mascara != 0)).collect();
        lista.push(("LT", self.lt));
        lista.push(("RT", self.rt));
        lista.push(("Analógico ↑", self.analogico_cima));
        lista.push(("Analógico ↓", self.analogico_baixo));
        lista.push(("Analógico ←", self.analogico_esquerda));
        lista.push(("Analógico →", self.analogico_direita));
        lista
    }
}

/// O que mudou entre dois instantes: `+Nome` (apertou) e `-Nome` (soltou).
fn diferencas(antes: &Apertos, depois: &Apertos) -> Vec<String> {
    antes
        .lista()
        .into_iter()
        .zip(depois.lista())
        .filter(|((_, a), (_, d))| a != d)
        .map(|((nome, _), (_, d))| format!("{}{nome}", if d { '+' } else { '-' }))
        .collect()
}

/// Textos imprimíveis (4+ caracteres ASCII) de um pedaço de memória.
fn textos_imprimiveis(bytes: &[u8]) -> Vec<String> {
    textos_com_minimo(bytes, 4)
}

fn textos_com_minimo(bytes: &[u8], minimo: usize) -> Vec<String> {
    let mut textos = Vec::new();
    let mut atual = Vec::new();
    for &b in bytes.iter().chain(std::iter::once(&0u8)) {
        if (0x20..=0x7E).contains(&b) {
            atual.push(b);
        } else {
            if atual.len() >= minimo {
                textos.push(String::from_utf8_lossy(&atual).into_owned());
            }
            atual.clear();
        }
    }
    textos
}

/// Lê a janela do anel em volta do ponteiro de escrita:
/// `(ponteiro, endereço do início da janela, bytes)`. Se a janela grande
/// passa de memória legível, tenta janelas menores.
fn ler_janela(base_exe: usize) -> Option<(usize, usize, Vec<u8>)> {
    let bruto = read_region_bytes(&Region { base: base_exe.checked_add(OFFSET_BUFFER_DE_TELA)?, size: 8 })?;
    let ponteiro = usize::try_from(u64::from_le_bytes(bruto.get(..8)?.try_into().ok()?)).ok()?;
    if ponteiro == 0 {
        return None;
    }
    let alinhado = ponteiro & !(TAMANHO_DA_ENTRADA - 1);
    [(JANELA_ANTES, JANELA_TOTAL), (JANELA_ANTES / 2, JANELA_TOTAL / 2), (0, TAMANHO_DA_ENTRADA * 4)].into_iter().find_map(|(antes, total)| {
        let inicio = alinhado.checked_sub(antes)?;
        read_region_bytes(&Region { base: inicio, size: total }).map(|bytes| (ponteiro, inicio, bytes))
    })
}

/// O anel de entradas como o gravador o viu por último, para registrar só o
/// que muda (a cada entrada nova que o jogo escreve).
#[derive(Default)]
struct Anel {
    entradas: HashMap<usize, [u8; TAMANHO_DA_ENTRADA]>,
}

/// Uma entrada que mudou: endereço, se é a que o ponteiro marca, textos.
#[derive(Debug, PartialEq, Eq)]
struct Novidade {
    endereco: usize,
    atual: bool,
    textos: Vec<String>,
}

impl Anel {
    /// Compara a janela com o que já se viu e devolve as entradas que mudaram
    /// e têm texto (as sem texto só atualizam a memória do anel).
    fn novidades(&mut self, ponteiro: usize, inicio: usize, bytes: &[u8]) -> Vec<Novidade> {
        let mut novas = Vec::new();
        for (i, pedaco) in bytes.chunks_exact(TAMANHO_DA_ENTRADA).enumerate() {
            let endereco = inicio + i * TAMANHO_DA_ENTRADA;
            let Ok(entrada) = <[u8; TAMANHO_DA_ENTRADA]>::try_from(pedaco) else { continue };
            if self.entradas.get(&endereco) == Some(&entrada) {
                continue;
            }
            self.entradas.insert(endereco, entrada);
            let textos = textos_com_minimo(&entrada, MIN_TEXTO_DA_TELA);
            if !textos.is_empty() {
                novas.push(Novidade { endereco, atual: ponteiro & !(TAMANHO_DA_ENTRADA - 1) == endereco, textos });
            }
        }
        novas
    }
}

/// Os textos numa linha de log: separados por `|`, cada um cortado em 90.
fn texto_para_log(textos: &[String]) -> String {
    if textos.is_empty() {
        return "(sem texto)".to_string();
    }
    textos.iter().map(|t| t.chars().take(90).collect::<String>()).collect::<Vec<_>>().join(" | ")
}

pub struct Gravador {
    arquivo: PathBuf,
    proxima_checagem: Instant,
    ligado_desde: Option<Instant>,
    anterior: Apertos,
    ultimo_foco: Option<u32>,
    base_exe: Option<usize>,
    proxima_leitura_de_tela: Instant,
    anel: Anel,
}

impl Gravador {
    /// Desde quando o gravador está ligado (para alinhar outros registros ao tempo dele).
    pub fn ligado_desde(&self) -> Option<Instant> {
        self.ligado_desde
    }

    pub fn new() -> Self {
        Gravador {
            arquivo: std::env::temp_dir().join("fifa_gravar_controle.pedido"),
            proxima_checagem: Instant::now(),
            ligado_desde: None,
            anterior: Apertos::default(),
            ultimo_foco: None,
            base_exe: None,
            proxima_leitura_de_tela: Instant::now(),
            anel: Anel::default(),
        }
    }

    /// Chamado a cada frame com o controle de agora e o jogador em foco no
    /// jogo (`(id, nome)`), se a Central o leu.
    pub fn registrar(&mut self, controle: Option<EstadoControle>, foco: Option<(u32, &str)>) {
        let agora = Instant::now();
        if agora >= self.proxima_checagem {
            self.proxima_checagem = agora + INTERVALO_CHECAGEM;
            let pedido = self.arquivo.exists();
            match (pedido, self.ligado_desde) {
                (true, None) => {
                    self.ligado_desde = Some(agora);
                    self.anterior = Apertos::default();
                    self.ultimo_foco = None;
                    self.anel = Anel::default();
                    tracing::info!("[gravador] ligado: aperte os botões como de costume; apague {} para parar.", self.arquivo.display());
                }
                (false, Some(_)) => {
                    self.ligado_desde = None;
                    tracing::info!("[gravador] desligado.");
                }
                _ => {}
            }
        }
        let Some(inicio) = self.ligado_desde else { return };
        let ms = agora.duration_since(inicio).as_millis();
        if let Some(estado) = controle {
            let apertos = Apertos::de(&estado);
            let mudancas = diferencas(&self.anterior, &apertos);
            if !mudancas.is_empty() {
                tracing::info!("[gravador] t={ms}ms {}", mudancas.join(" "));
            }
            self.anterior = apertos;
        }
        if agora >= self.proxima_leitura_de_tela {
            self.proxima_leitura_de_tela = agora + INTERVALO_TELA;
            let base = *self.base_exe.get_or_insert_with(|| {
                enumerate_modules().into_iter().find(|m| m.name.to_ascii_lowercase().ends_with("fifa16.exe")).map_or(0, |m| m.base)
            });
            if base != 0 {
                if let Some((ponteiro, inicio_janela, bytes)) = ler_janela(base) {
                    for n in self.anel.novidades(ponteiro, inicio_janela, &bytes) {
                        let marca = if n.atual { "*" } else { " " };
                        tracing::info!("[gravador] t={ms}ms tela{marca} 0x{:X}: {}", n.endereco, texto_para_log(&n.textos));
                    }
                }
            }
        }
        let foco_id = foco.map(|(id, _)| id);
        if foco_id != self.ultimo_foco {
            self.ultimo_foco = foco_id;
            if let Some((id, nome)) = foco {
                tracing::info!("[gravador] t={ms}ms foco no jogo: {id} \"{nome}\"");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estado(botoes: u16) -> EstadoControle {
        EstadoControle { botoes, ..Default::default() }
    }

    #[test]
    fn every_printable_text_of_the_buffer_goes_to_the_log_in_memory_order() {
        let mut bytes = b"\x00\x01ab\x00EnterTransferOfferFromActionPopup\x00\xff".to_vec();
        bytes.extend_from_slice(b"xx\x00game/x/Widget.swf\x00");
        let textos = textos_imprimiveis(&bytes);
        assert_eq!(textos, ["EnterTransferOfferFromActionPopup", "game/x/Widget.swf"]);
        assert_eq!(texto_para_log(&textos), "EnterTransferOfferFromActionPopup | game/x/Widget.swf");
        assert_eq!(texto_para_log(&[]), "(sem texto)");
        assert_eq!(texto_para_log(&["a".repeat(120)]).chars().count(), 90, "cada texto é cortado em 90");
        assert_eq!(textos_imprimiveis(b"abcd\x00efgh"), ["abcd", "efgh"]);
        assert!(textos_imprimiveis(b"\x00ab\x00\x01").is_empty(), "menos de 4 letras não é texto");
    }

    fn entrada(texto: &str) -> Vec<u8> {
        let mut e = vec![0u8; TAMANHO_DA_ENTRADA];
        e[..texto.len()].copy_from_slice(texto.as_bytes());
        e
    }

    #[test]
    fn the_ring_reports_only_entries_that_changed_and_marks_the_one_the_pointer_names() {
        let mut anel = Anel::default();
        let inicio = 0x1000;
        let mut janela: Vec<u8> = [entrada("ActionPopup"), entrada("p+Z."), entrada("ContractOffer"), entrada("")].concat();
        let primeira = anel.novidades(inicio + 2 * TAMANHO_DA_ENTRADA + 5, inicio, &janela);
        assert_eq!(
            primeira,
            [
                Novidade { endereco: inicio, atual: false, textos: vec!["ActionPopup".to_string()] },
                Novidade { endereco: inicio + 2 * TAMANHO_DA_ENTRADA, atual: true, textos: vec!["ContractOffer".to_string()] },
            ],
            "lixo curto e entrada vazia ficam de fora"
        );
        assert!(anel.novidades(inicio, inicio, &janela).is_empty(), "nada mudou");
        // o jogo escreve uma entrada nova na posição 3
        janela[3 * TAMANHO_DA_ENTRADA..4 * TAMANHO_DA_ENTRADA].copy_from_slice(&entrada("TransferOffer"));
        let depois = anel.novidades(inicio + 3 * TAMANHO_DA_ENTRADA, inicio, &janela);
        assert_eq!(depois, [Novidade { endereco: inicio + 3 * TAMANHO_DA_ENTRADA, atual: true, textos: vec!["TransferOffer".to_string()] }]);
        // uma janela deslocada (o ponteiro andou) não repete o que já foi visto
        assert!(anel.novidades(inicio, inicio + TAMANHO_DA_ENTRADA, &janela[TAMANHO_DA_ENTRADA..]).is_empty());
    }

    #[test]
    fn presses_and_releases_are_named_in_the_order_of_the_table() {
        let nada = Apertos::de(&estado(0));
        let a_e_baixo = Apertos::de(&estado(botao::A | botao::DPAD_BAIXO));
        assert_eq!(diferencas(&nada, &a_e_baixo), ["+D-pad ↓", "+A"]);
        assert_eq!(diferencas(&a_e_baixo, &nada), ["-D-pad ↓", "-A"]);
        assert!(diferencas(&a_e_baixo, &a_e_baixo).is_empty(), "nada mudou");
    }

    #[test]
    fn triggers_and_the_left_stick_count_only_past_their_thresholds() {
        let leve = EstadoControle { lt: LIMIAR_GATILHO - 1, ly: LIMIAR_ANALOGICO - 1, ..Default::default() };
        assert!(diferencas(&Apertos::default(), &Apertos::de(&leve)).is_empty());
        let firme = EstadoControle { lt: 255, rt: LIMIAR_GATILHO, ly: -LIMIAR_ANALOGICO, lx: LIMIAR_ANALOGICO, ..Default::default() };
        assert_eq!(diferencas(&Apertos::default(), &Apertos::de(&firme)), ["+LT", "+RT", "+Analógico ↓", "+Analógico →"]);
    }
}
