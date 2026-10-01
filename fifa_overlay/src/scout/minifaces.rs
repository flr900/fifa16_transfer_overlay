//! Rostos dos jogadores (minifaces) para a visão Cards (Story 2.6).
//!
//! Carregamento preguiçoso e com cache, sem engasgar o jogo:
//! - a tela pede o rosto só dos cards VISÍVEIS (`rosto`); o pedido entra
//!   numa fila;
//! - um `AsyncTask` lê e decodifica os `.dds` em lotes, fora do thread de
//!   render (AD-4);
//! - no `before_render` (o único lugar com acesso ao renderizador), no
//!   máximo `ENVIOS_POR_FRAME` imagens sobem para a GPU por frame;
//! - até `LIMITE_TEXTURAS` texturas ficam vivas; acima disso a menos usada
//!   recentemente é reaproveitada (`replace_texture`), sem criar outra.
//!
//! Sem arquivo para o `playerid` (jogador gerado pelo jogo, regen…), o
//! rosto é `Ausente` e a tela desenha uma silhueta neutra.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use imgui::TextureId;

use crate::async_task::{AsyncTask, TaskState};
use crate::dds::Imagem;

/// Quantos arquivos um lote lê de uma vez.
const LOTE: usize = 12;
/// Quantas imagens sobem para a GPU por frame (cada uma tem 64 KB).
const ENVIOS_POR_FRAME: usize = 4;
/// Texturas vivas no máximo (~64 KB de GPU cada).
pub const LIMITE_TEXTURAS: usize = 300;

/// O que a tela desenha no lugar do rosto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rosto {
    Pronto(TextureId),
    /// Ainda lendo: só o fundo do quadro.
    Carregando,
    /// Não há arquivo: silhueta.
    Ausente,
}

#[derive(Debug)]
enum Estado {
    NaFila,
    Lendo,
    /// Decodificado, esperando subir para a GPU.
    Decodificado(Imagem),
    Pronto { textura: TextureId, uso: u64 },
    Ausente,
}

type Leitor = fn(u32) -> Option<Imagem>;

struct Interno {
    estados: HashMap<u32, Estado>,
    fila: VecDeque<u32>,
    tarefa: AsyncTask<Vec<(u32, Option<Imagem>)>>,
    /// O lote em andamento ainda não foi tratado.
    lote_pendente: bool,
    relogio: u64,
}

pub struct Minifaces {
    interno: Mutex<Interno>,
    leitor: Leitor,
}

impl Minifaces {
    pub fn new(leitor: Leitor) -> Self {
        Minifaces {
            interno: Mutex::new(Interno {
                estados: HashMap::new(),
                fila: VecDeque::new(),
                tarefa: AsyncTask::new(),
                lote_pendente: false,
                relogio: 0,
            }),
            leitor,
        }
    }

    fn travar(&self) -> std::sync::MutexGuard<'_, Interno> {
        self.interno.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Rosto de `player_id` agora; pede o carregamento se ainda não pediu.
    pub fn rosto(&self, player_id: u32) -> Rosto {
        let mut interno = self.travar();
        interno.relogio += 1;
        let agora = interno.relogio;
        match interno.estados.get_mut(&player_id) {
            Some(Estado::Pronto { textura, uso }) => {
                *uso = agora;
                Rosto::Pronto(*textura)
            }
            Some(Estado::Ausente) => Rosto::Ausente,
            Some(_) => Rosto::Carregando,
            None => {
                interno.estados.insert(player_id, Estado::NaFila);
                interno.fila.push_back(player_id);
                Rosto::Carregando
            }
        }
    }

    /// A cada frame: trata o lote que terminou e dispara o próximo.
    pub fn tick(&self) {
        let mut interno = self.travar();
        if interno.lote_pendente {
            match interno.tarefa.poll() {
                TaskState::Running => return,
                TaskState::Done(lidos) => {
                    interno.lote_pendente = false;
                    for (id, imagem) in lidos {
                        let estado = match imagem {
                            Some(img) => Estado::Decodificado(img),
                            None => Estado::Ausente,
                        };
                        interno.estados.insert(id, estado);
                    }
                }
                TaskState::Failed(_) | TaskState::Idle => {
                    interno.lote_pendente = false;
                    // não deveria acontecer (o leitor não falha): marca o lote
                    // como sem rosto em vez de deixá-lo "lendo" para sempre
                    for estado in interno.estados.values_mut().filter(|e| matches!(e, Estado::Lendo)) {
                        *estado = Estado::Ausente;
                    }
                }
            }
        }
        if interno.fila.is_empty() {
            return;
        }
        let quantos = interno.fila.len().min(LOTE);
        let lote: Vec<u32> = interno.fila.drain(..quantos).collect();
        let leitor = self.leitor;
        let ids = lote.clone();
        if interno.tarefa.start(move || Ok(ids.into_iter().map(|id| (id, leitor(id))).collect())) {
            interno.lote_pendente = true;
            for id in lote {
                interno.estados.insert(id, Estado::Lendo);
            }
        } else {
            for id in lote.into_iter().rev() {
                interno.fila.push_front(id);
            }
        }
    }

    /// No `before_render`: sobe até `ENVIOS_POR_FRAME` imagens. `carregar`
    /// recebe a imagem e, se houver, a textura a reaproveitar; devolve a
    /// textura usada (ou `None` se o renderizador falhou).
    pub fn enviar(&self, carregar: &mut dyn FnMut(&Imagem, Option<TextureId>) -> Option<TextureId>) {
        let mut interno = self.travar();
        let prontos: Vec<u32> = interno
            .estados
            .iter()
            .filter(|(_, e)| matches!(e, Estado::Decodificado(_)))
            .map(|(id, _)| *id)
            .take(ENVIOS_POR_FRAME)
            .collect();
        for id in prontos {
            let vivas = interno.estados.values().filter(|e| matches!(e, Estado::Pronto { .. })).count();
            // Acima do limite: reaproveita a textura menos usada.
            let reuso = if vivas >= LIMITE_TEXTURAS {
                let menos_usada = interno
                    .estados
                    .iter()
                    .filter_map(|(k, e)| match e {
                        Estado::Pronto { textura, uso } => Some((*uso, *k, *textura)),
                        _ => None,
                    })
                    .min_by_key(|(uso, k, _)| (*uso, *k));
                menos_usada.map(|(_, k, textura)| {
                    interno.estados.remove(&k);
                    textura
                })
            } else {
                None
            };
            let Some(Estado::Decodificado(imagem)) = interno.estados.remove(&id) else {
                continue;
            };
            let uso = interno.relogio;
            match carregar(&imagem, reuso) {
                Some(textura) => {
                    interno.estados.insert(id, Estado::Pronto { textura, uso });
                }
                None => {
                    tracing::warn!("[scout::minifaces] Falha ao criar a textura do rosto {id}.");
                    interno.estados.insert(id, Estado::Ausente);
                }
            }
        }
    }

    /// Esquece tudo (ex.: o renderizador foi recriado). As texturas antigas
    /// não são mais referenciadas.
    #[allow(dead_code)]
    pub fn limpar(&self) {
        let mut interno = self.travar();
        interno.estados.clear();
        interno.fila.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn leitor_falso(id: u32) -> Option<Imagem> {
        (id % 2 == 0).then(|| Imagem { largura: 1, altura: 1, rgba: vec![id as u8, 0, 0, 255] })
    }

    fn ate_ler(m: &Minifaces) {
        let inicio = Instant::now();
        loop {
            m.tick();
            let interno = m.travar();
            if interno.fila.is_empty() && !interno.lote_pendente {
                return;
            }
            drop(interno);
            assert!(inicio.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn faces_load_lazily_and_missing_files_become_a_silhouette() {
        let m = Minifaces::new(leitor_falso);
        assert_eq!(m.rosto(2), Rosto::Carregando);
        assert_eq!(m.rosto(3), Rosto::Carregando);
        ate_ler(&m);
        assert_eq!(m.rosto(3), Rosto::Ausente);
        assert_eq!(m.rosto(2), Rosto::Carregando, "decodificado, ainda não subiu");
        let mut proxima = 0usize;
        m.enviar(&mut |_, reuso| {
            assert_eq!(reuso, None);
            proxima += 1;
            Some(TextureId::new(proxima))
        });
        assert_eq!(m.rosto(2), Rosto::Pronto(TextureId::new(1)));
    }

    #[test]
    fn at_most_a_few_uploads_per_frame() {
        let m = Minifaces::new(leitor_falso);
        for id in (0..40).map(|i| i * 2) {
            m.rosto(id);
        }
        ate_ler(&m);
        let mut envios = 0;
        m.enviar(&mut |_, _| {
            envios += 1;
            Some(TextureId::new(envios))
        });
        assert_eq!(envios, ENVIOS_POR_FRAME);
    }

    #[test]
    fn beyond_the_limit_the_least_recently_used_texture_is_reused() {
        let m = Minifaces::new(leitor_falso);
        let total = LIMITE_TEXTURAS + 1;
        let mut criadas = 0usize;
        let mut reusos = Vec::new();
        for id in (0..total as u32).map(|i| i * 2) {
            m.rosto(id);
            ate_ler(&m);
            m.enviar(&mut |_, reuso| match reuso {
                Some(t) => {
                    reusos.push(t);
                    Some(t)
                }
                None => {
                    criadas += 1;
                    Some(TextureId::new(criadas))
                }
            });
        }
        assert_eq!(criadas, LIMITE_TEXTURAS, "nunca mais que o limite");
        assert_eq!(reusos, vec![TextureId::new(1)], "a primeira (menos usada) foi reaproveitada");
        assert_eq!(m.rosto(0), Rosto::Carregando, "o rosto despejado volta a ser pedido");
    }
}
