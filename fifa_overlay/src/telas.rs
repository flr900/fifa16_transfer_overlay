//! Em que tela do FIFA estamos (2026-10-08), lido de dentro do jogo.
//!
//! O ponteiro `fifa16.exe+0x3357378` aponta para uma tabela de 16 entradas
//! fixas de 64 bytes. A entrada dos EVENTOS DE TELA (`MainMenuHub`,
//! `ViewShortlist`, `ActionPopup`, `TransferOffer`, `ContractOffer`...) muda
//! 70 a 200 ms depois do aperto que a causou (mapeado com o gravador de
//! controle; ver `integracao-negociacao.md`). O endereço da tabela muda a
//! cada sessão do jogo, então a entrada é achada pelo VOCABULÁRIO: a que traz
//! um nome de evento conhecido.
//!
//! O texto novo fica no começo da entrada e restos dos anteriores vêm
//! depois, por isso só o PRIMEIRO texto vale. Às vezes o jogo escreve o nome
//! de novo um byte adiante (`ActionPopup` → `ctionPopup`), então as buscas
//! usam pedaços que sobrevivem a isso.

use crate::memscan::{read_region_bytes, Region};
use crate::pointer_scan::enumerate_modules;

/// Deslocamento, dentro do `fifa16.exe`, do ponteiro para a tabela (build
/// 16.0.2904053).
const OFFSET_TABELA_DE_TELA: usize = 0x335_7378;
const TAMANHO_DA_ENTRADA: usize = 64;
const JANELA_ANTES: usize = 1024;
const JANELA_TOTAL: usize = 2048;
/// Textos menores que isto são lixo binário, não nome de evento.
const MIN_TEXTO: usize = 6;

/// A tela, como os eventos a mostram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evento {
    /// Hub da carreira (`MainMenuHub`, `CacheTeamSheet`).
    Hub,
    /// Abrindo a lista de Escolhidos do jogo (`ViewShortlist`).
    ListaCarregando,
    /// Uma tela terminou de carregar (`NotifyScreenLoadedAndRefresh`): a lista
    /// de Escolhidos, quando se sabe que ela está aberta (`Contexto`).
    TelaCarregada,
    /// Menu do jogador (`ActionPopup`).
    Menu,
    /// Negociação de contrato (`EnterPreContractOffer...`, `ContractOffer`).
    Contrato,
    /// Tela de compra e empréstimo (`EnterTransferOffer...`, `TransferOffer`).
    Compra,
    /// Carregando entre telas (`SendReadyOnLoadComplete`).
    Transicao,
    Desconhecido,
}

/// O evento que um texto da entrada de eventos representa. A ordem importa:
/// `EnterPreContractOfferFromActionPopup` traz `ActionPopup` dentro.
pub fn classificar(texto: &str) -> Evento {
    let tem = |trecho: &str| texto.contains(trecho);
    if tem("ContractOffer") || tem("ontractOffer") {
        Evento::Contrato
    } else if tem("TransferOffer") || tem("ransferOffer") {
        Evento::Compra
    } else if tem("ActionPopup") || tem("ctionPopup") {
        Evento::Menu
    } else if tem("ScreenLoadedAndRefresh") {
        Evento::TelaCarregada
    } else if tem("ViewShortlist") || tem("iewShortlist") {
        Evento::ListaCarregando
    } else if tem("MainMenuHub") || tem("CacheTeamSheet") || tem("acheTeamSheet") {
        Evento::Hub
    } else if tem("ReadyOnLoadComplete") {
        Evento::Transicao
    } else {
        Evento::Desconhecido
    }
}

/// Textos imprimíveis (ASCII) de pelo menos `MIN_TEXTO` letras, em ordem.
fn textos(bytes: &[u8]) -> Vec<String> {
    let mut saida = Vec::new();
    let mut atual = Vec::new();
    for &b in bytes.iter().chain(std::iter::once(&0u8)) {
        if (0x20..=0x7E).contains(&b) {
            atual.push(b);
        } else {
            if atual.len() >= MIN_TEXTO {
                saida.push(String::from_utf8_lossy(&atual).into_owned());
            }
            atual.clear();
        }
    }
    saida
}

/// O evento de uma entrada de 64 bytes: o do primeiro texto.
fn evento_da_entrada(entrada: &[u8]) -> Evento {
    textos(entrada).first().map_or(Evento::Desconhecido, |t| classificar(t))
}

/// Entre as entradas de uma janela de memória, a que traz um nome de evento:
/// o endereço dela. Entradas de eventos reais têm texto de vocabulário
/// conhecido; o resto (widgets `.swf`, noticiário) não.
fn achar_entrada_de_eventos(inicio: usize, janela: &[u8]) -> Option<usize> {
    janela
        .chunks_exact(TAMANHO_DA_ENTRADA)
        .enumerate()
        .find(|(_, e)| evento_da_entrada(e) != Evento::Desconhecido)
        .map(|(i, _)| inicio + i * TAMANHO_DA_ENTRADA)
}

/// Onde o jogo está, na leitura de agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leitura {
    pub evento: Evento,
    /// A lista de Escolhidos do jogo está aberta (ou uma tela dentro dela).
    pub na_lista: bool,
}

#[derive(Default)]
pub struct Telas {
    base_exe: Option<usize>,
    entrada: Option<usize>,
    na_lista: bool,
    ultimo: Option<Evento>,
}

impl Telas {
    pub fn new() -> Self {
        Telas::default()
    }

    /// O contexto muda pelos eventos: `ViewShortlist` entra na lista; o hub a
    /// deixa. As telas de dentro (menu, negociações) seguem na lista.
    fn aplicar(&mut self, evento: Evento) {
        match evento {
            Evento::ListaCarregando => self.na_lista = true,
            Evento::Hub => self.na_lista = false,
            _ => {}
        }
        self.ultimo = Some(evento);
    }

    /// Lê o evento de agora (barato: 8 + 64 bytes por frame; a janela inteira
    /// só quando a entrada de eventos precisa ser achada de novo). `None` se o
    /// jogo ainda não tem a tabela.
    pub fn ler(&mut self) -> Option<Leitura> {
        let base = *self.base_exe.get_or_insert_with(|| {
            enumerate_modules().into_iter().find(|m| m.name.to_ascii_lowercase().ends_with("fifa16.exe")).map_or(0, |m| m.base)
        });
        if base == 0 {
            return None;
        }
        if let Some(endereco) = self.entrada {
            if let Some(bytes) = read_region_bytes(&Region { base: endereco, size: TAMANHO_DA_ENTRADA }) {
                let evento = evento_da_entrada(&bytes);
                if evento != Evento::Desconhecido {
                    self.aplicar(evento);
                    return Some(Leitura { evento, na_lista: self.na_lista });
                }
            }
            // a entrada guardada deixou de falar de eventos: o jogo mudou de lugar
            self.entrada = None;
        }
        let ponteiro = read_region_bytes(&Region { base: base.checked_add(OFFSET_TABELA_DE_TELA)?, size: 8 })
            .and_then(|b| b.get(..8).and_then(|b| <[u8; 8]>::try_from(b).ok()))
            .and_then(|b| usize::try_from(u64::from_le_bytes(b)).ok())
            .filter(|p| *p != 0)?;
        let alinhado = ponteiro & !(TAMANHO_DA_ENTRADA - 1);
        let (inicio, janela) = [(JANELA_ANTES, JANELA_TOTAL), (JANELA_ANTES / 2, JANELA_TOTAL / 2)]
            .into_iter()
            .find_map(|(antes, total)| {
                let inicio = alinhado.checked_sub(antes)?;
                read_region_bytes(&Region { base: inicio, size: total }).map(|b| (inicio, b))
            })?;
        let endereco = achar_entrada_de_eventos(inicio, &janela)?;
        tracing::info!("[telas] Entrada de eventos de tela em 0x{endereco:X}.");
        self.entrada = Some(endereco);
        let evento = evento_da_entrada(janela.get(endereco - inicio..endereco - inicio + TAMANHO_DA_ENTRADA)?);
        self.aplicar(evento);
        Some(Leitura { evento, na_lista: self.na_lista })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada(textos: &[&str]) -> Vec<u8> {
        let mut e = vec![0u8; TAMANHO_DA_ENTRADA];
        let mut pos = 0;
        for t in textos {
            e[pos..pos + t.len()].copy_from_slice(t.as_bytes());
            pos += t.len() + 1;
        }
        e
    }

    #[test]
    fn the_events_seen_in_the_recordings_map_to_the_right_screen() {
        use Evento::*;
        let casos = [
            ("MainMenuHub", Hub),
            ("CacheTeamSheet", Hub),
            ("acheTeamSheet", Hub),
            ("ViewShortlist", ListaCarregando),
            ("iewShortlist", ListaCarregando),
            ("NotifyScreenLoadedAndRefresh", TelaCarregada),
            ("otifyScreenLoadedAndRefresh", TelaCarregada),
            ("ActionPopup", Menu),
            ("ctionPopup", Menu),
            ("EnterTransferOfferActionPopup", Compra),
            ("TransferOffer", Compra),
            ("ransferOffer", Compra),
            ("EnterPreContractOfferFromActionPopup", Contrato),
            ("ContractOffer", Contrato),
            ("ontractOffer", Contrato),
            ("SendReadyOnLoadComplete", Transicao),
            ("endReadyOnLoadComplete", Transicao),
            ("game/components/FluxTiles/career/FluxTile_Advance.swf", Desconhecido),
            ("ecentemente do Villarreal CF por $ 15.000.000", Desconhecido),
        ];
        for (texto, esperado) in casos {
            assert_eq!(classificar(texto), esperado, "{texto}");
        }
    }

    #[test]
    fn only_the_first_text_of_an_entry_counts_because_the_rest_is_what_was_there_before() {
        assert_eq!(evento_da_entrada(&entrada(&["ActionPopup", "LoadedAndRefresh", "nPopup"])), Evento::Menu);
        assert_eq!(evento_da_entrada(&entrada(&["NotifyScreenLoadedAndRefresh", "nPopup"])), Evento::TelaCarregada);
        assert_eq!(evento_da_entrada(&entrada(&["p+Z.", "ActionPopup"])), Evento::Menu, "lixo curto não conta como texto");
        assert_eq!(evento_da_entrada(&entrada(&[])), Evento::Desconhecido);
    }

    #[test]
    fn the_event_entry_is_the_one_with_known_vocabulary_and_the_widget_entries_are_ignored() {
        let janela: Vec<u8> = [
            entrada(&["qualidades, cla"]),
            entrada(&["game/components/FluxTiles/career/FluxTile_Advance.swf"]),
            entrada(&["ecentemente do Be", "JK por $ 34.000.000"]),
            entrada(&["ViewShortlist", "dedAndRefresh"]),
            entrada(&["game/components/CareerComponents/CareerHubWidget.swf"]),
        ]
        .concat();
        assert_eq!(achar_entrada_de_eventos(0x2000, &janela), Some(0x2000 + 3 * TAMANHO_DA_ENTRADA));
        let sem_eventos: Vec<u8> = [entrada(&["qualidades, cla"]), entrada(&[])].concat();
        assert_eq!(achar_entrada_de_eventos(0x2000, &sem_eventos), None);
    }

    #[test]
    fn the_list_context_starts_with_the_shortlist_survives_the_inner_screens_and_ends_at_the_hub() {
        let mut t = Telas::new();
        assert!(!t.na_lista);
        t.aplicar(Evento::TelaCarregada);
        assert!(!t.na_lista, "uma tela qualquer carregada não prova que é a lista");
        t.aplicar(Evento::ListaCarregando);
        t.aplicar(Evento::TelaCarregada);
        assert!(t.na_lista);
        for dentro in [Evento::Menu, Evento::Compra, Evento::Contrato, Evento::Transicao, Evento::Desconhecido] {
            t.aplicar(dentro);
            assert!(t.na_lista, "{dentro:?}");
        }
        t.aplicar(Evento::Hub);
        assert!(!t.na_lista);
    }
}
