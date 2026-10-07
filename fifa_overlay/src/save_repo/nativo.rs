//! Scout nativo do FIFA 16 (o GTN): a lista de escolhidos do jogo e o
//! "conhecimento" que o jogo tem de cada jogador (Épico 7, Story 7.1).
//!
//! Tudo o que está aqui foi mapeado no experimento de 2026-10-06
//! (`_bmad-output/planning-artifacts/integracao-scout-nativo.md`) e
//! conferido em jogo:
//!
//! - **Lista de escolhidos:** um vetor (três ponteiros de 8 bytes numa
//!   estrutura "dona": início, fim, fim da capacidade) de entradas de 28
//!   bytes — `time i32`, `jogador i32`, quatro `i32` que valem `-1` até o
//!   jogo revelar algo e uma marca de 1 byte (`0` ou `1`; os 3 bytes
//!   seguintes são lixo de preenchimento). Capacidade: 100 entradas.
//! - **Conhecimento:** um vetor (mesmo formato de dono) de registros de 20
//!   bytes, ORDENADO por `jogador`: `jogador i32`, `a i32`, `nivel i32`
//!   (0–198; 198 = jogador totalmente conhecido), `data i32` (`aaaammdd` da
//!   última atualização) e `-1`. É o `nivel` que decide o que o jogo mostra
//!   (estimativas, valor e salário a partir de 140, tudo em 198).
//!
//! Esta story só LOCALIZA e LÊ. Nada aqui escreve na memória do jogo
//! (isso é das Stories 7.2 e 7.3).
//!
//! ## Como localizamos sem varrer 1,9 GB
//! Os dados dos dois vetores ficam na mesma região de memória que a data e o
//! orçamento. A sequência de registros de conhecimento tem um padrão forte
//! (ids crescentes, `-1` no fim de cada registro, data plausível, nível até
//! 198); o dono de cada vetor fica numa região vizinha e se reconhece pelos
//! três ponteiros coerentes. Cópias antigas (o buffer do último save, donos
//! liberados) existem, mas só o dono vivo tem fim e capacidade coerentes com
//! o vetor.

use std::sync::{Mutex, MutexGuard, OnceLock};

use std::sync::atomic::{AtomicBool, Ordering};

use super::{i32_at, regions_with_live_date, ByteSink, ByteSource, Date, OwnMemory, ProcessMemory, SaveRepoError, ScrubbedBuffer};
use crate::memscan::{self, Region};

/// Bytes de uma entrada da lista de escolhidos nativa.
pub const ENTRADA_ESCOLHIDO: usize = 28;
/// Bytes de um registro de conhecimento.
pub const REGISTRO_CONHECIMENTO: usize = 20;
/// Capacidade da lista de escolhidos do jogo (entradas).
pub const CAPACIDADE_ESCOLHIDOS: usize = 100;
/// Nível de conhecimento máximo: o jogador é totalmente conhecido.
pub const NIVEL_COMPLETO: i32 = 198;

const MAX_ID_JOGADOR: i32 = 400_000;
const MAX_ID_TIME: i32 = 200_000;
/// Menos registros seguidos que isto não formam uma sequência (evita lixo).
const MIN_REGISTROS_SEQUENCIA: usize = 3;
const MAX_REGISTROS_CONHECIMENTO: usize = 20_000;
/// Até onde, a partir da região dos dados, procuramos primeiro os donos.
const ALCANCE_VIZINHO: usize = 256 * 1024 * 1024;

// ---------------------------------------------------------------------
// Formatos
// ---------------------------------------------------------------------

/// Uma entrada da lista de escolhidos nativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntradaEscolhido {
    /// Time do jogador quando foi adicionado.
    pub time: i32,
    pub jogador: i32,
    /// `-1` ×4 enquanto o jogo não revelou nada.
    pub revelado: [i32; 4],
    /// `0` ou `1` (o jogo grava 1).
    pub marca: u8,
}

impl EntradaEscolhido {
    /// Decodifica 28 bytes; `None` se não parecem uma entrada.
    pub fn de_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != ENTRADA_ESCOLHIDO {
            return None;
        }
        let time = i32_at(bytes, 0)?;
        let jogador = i32_at(bytes, 4)?;
        let marca = u8::try_from(i32_at(bytes, 24)? & 0xFF).ok()?;
        let revelado = [i32_at(bytes, 8)?, i32_at(bytes, 12)?, i32_at(bytes, 16)?, i32_at(bytes, 20)?];
        ((0..=MAX_ID_TIME).contains(&time) && (1..=MAX_ID_JOGADOR).contains(&jogador) && marca <= 1).then_some(EntradaEscolhido {
            time,
            jogador,
            revelado,
            marca,
        })
    }

    /// Os 28 bytes como o jogo grava (preenchimento zerado).
    #[allow(dead_code)] // usado pela Story 7.2 (escrita)
    pub fn para_bytes(&self) -> [u8; ENTRADA_ESCOLHIDO] {
        let mut out = [0u8; ENTRADA_ESCOLHIDO];
        let campos = [self.time, self.jogador, self.revelado[0], self.revelado[1], self.revelado[2], self.revelado[3], i32::from(self.marca)];
        for (i, valor) in campos.iter().enumerate() {
            if let Some(destino) = out.get_mut(i * 4..i * 4 + 4) {
                destino.copy_from_slice(&valor.to_le_bytes());
            }
        }
        out
    }
}

/// Um registro de conhecimento: quanto o jogo sabe de um jogador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistroConhecimento {
    pub jogador: i32,
    /// Campo ainda sem significado conhecido (`65535`, `16<<16 | n`, ...).
    pub a: i32,
    /// Nível de conhecimento (0–198).
    pub nivel: i32,
    /// Data da última atualização.
    pub data: Date,
}

impl RegistroConhecimento {
    /// Decodifica 20 bytes; `None` se não parecem um registro.
    pub fn de_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != REGISTRO_CONHECIMENTO || i32_at(bytes, 16)? != -1 {
            return None;
        }
        let jogador = i32_at(bytes, 0)?;
        let nivel = i32_at(bytes, 8)?;
        let data = Date(i32_at(bytes, 12)?);
        ((1..=MAX_ID_JOGADOR).contains(&jogador) && (0..=NIVEL_COMPLETO).contains(&nivel) && data.is_plausible()).then_some(
            RegistroConhecimento { jogador, a: i32_at(bytes, 4)?, nivel, data },
        )
    }

    /// Os 20 bytes como o jogo grava.
    #[allow(dead_code)] // usado pela Story 7.3 (escrita)
    pub fn para_bytes(&self) -> [u8; REGISTRO_CONHECIMENTO] {
        let mut out = [0u8; REGISTRO_CONHECIMENTO];
        let campos = [self.jogador, self.a, self.nivel, self.data.0, -1];
        for (i, valor) in campos.iter().enumerate() {
            if let Some(destino) = out.get_mut(i * 4..i * 4 + 4) {
                destino.copy_from_slice(&valor.to_le_bytes());
            }
        }
        out
    }
}

/// Os três ponteiros de um vetor do jogo, e onde está a estrutura dona.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VetorVivo {
    /// Endereço da estrutura dona (guarda início, fim e fim da capacidade,
    /// 8 bytes cada).
    pub dono: usize,
    pub inicio: usize,
    pub fim: usize,
    pub fim_capacidade: usize,
}

fn u64_at(bytes: &[u8], pos: usize) -> Option<u64> {
    let chunk: [u8; 8] = bytes.get(pos..pos.checked_add(8)?)?.try_into().ok()?;
    Some(u64::from_le_bytes(chunk))
}

impl VetorVivo {
    /// Lê os três ponteiros da estrutura dona em `dono`.
    fn ler(src: &impl ByteSource, dono: usize) -> Option<Self> {
        let bytes = src.read(dono, 24)?;
        Some(VetorVivo {
            dono,
            inicio: usize::try_from(u64_at(&bytes, 0)?).ok()?,
            fim: usize::try_from(u64_at(&bytes, 8)?).ok()?,
            fim_capacidade: usize::try_from(u64_at(&bytes, 16)?).ok()?,
        })
    }

    fn bytes_usados(&self) -> Option<usize> {
        self.fim.checked_sub(self.inicio)
    }
}

/// Lê a lista de escolhidos de um vetor. `None` se o vetor ou alguma
/// entrada estiver fora do formato (não escolhemos nada duvidoso).
fn ler_escolhidos_de(src: &impl ByteSource, vetor: &VetorVivo) -> Option<Vec<EntradaEscolhido>> {
    let usados = vetor.bytes_usados()?;
    if !usados.is_multiple_of(ENTRADA_ESCOLHIDO) || usados / ENTRADA_ESCOLHIDO > CAPACIDADE_ESCOLHIDOS {
        return None;
    }
    let bruto = if usados == 0 { Vec::new() } else { src.read(vetor.inicio, usados)? };
    let entradas: Option<Vec<EntradaEscolhido>> = bruto.as_chunks::<ENTRADA_ESCOLHIDO>().0.iter().map(|c| EntradaEscolhido::de_bytes(c)).collect();
    let entradas = entradas?;
    let mut ids: Vec<i32> = entradas.iter().map(|e| e.jogador).collect();
    ids.sort_unstable();
    ids.dedup();
    (ids.len() == entradas.len()).then_some(entradas)
}

/// Lê o conhecimento de um vetor: registros válidos e ordenados.
fn ler_conhecimento_de(src: &impl ByteSource, vetor: &VetorVivo) -> Option<Vec<RegistroConhecimento>> {
    let usados = vetor.bytes_usados()?;
    if !usados.is_multiple_of(REGISTRO_CONHECIMENTO) || usados / REGISTRO_CONHECIMENTO > MAX_REGISTROS_CONHECIMENTO {
        return None;
    }
    let bruto = if usados == 0 { Vec::new() } else { src.read(vetor.inicio, usados)? };
    let registros: Option<Vec<RegistroConhecimento>> =
        bruto.as_chunks::<REGISTRO_CONHECIMENTO>().0.iter().map(|c| RegistroConhecimento::de_bytes(c)).collect();
    let registros = registros?;
    registros.windows(2).all(|par| par[0].jogador < par[1].jogador).then_some(registros)
}

// ---------------------------------------------------------------------
// Busca nos bytes de uma região (funções puras, testadas com bytes reais)
// ---------------------------------------------------------------------

/// Uma sequência de registros de conhecimento achada na memória.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequencia {
    /// Endereço do primeiro registro.
    pub inicio: usize,
    pub registros: usize,
}

impl Sequencia {
    fn fim(&self) -> usize {
        self.inicio + self.registros * REGISTRO_CONHECIMENTO
    }
}

/// Sequências de registros válidos, de ids estritamente crescentes, em
/// `bytes` (uma região que começa em `base`). Procura em passos de 4 bytes.
fn sequencias_de_conhecimento(base: usize, bytes: &[u8]) -> Vec<Sequencia> {
    let mut achadas = Vec::new();
    let mut i = 0usize;
    while i + REGISTRO_CONHECIMENTO <= bytes.len() {
        let mut registros = 0usize;
        let mut ultimo = 0i32;
        let mut pos = i;
        while let Some(registro) = bytes.get(pos..pos + REGISTRO_CONHECIMENTO).and_then(RegistroConhecimento::de_bytes) {
            if registros > 0 && registro.jogador <= ultimo {
                break;
            }
            ultimo = registro.jogador;
            registros += 1;
            pos += REGISTRO_CONHECIMENTO;
        }
        if registros >= MIN_REGISTROS_SEQUENCIA {
            achadas.push(Sequencia { inicio: base + i, registros });
            i = pos;
        } else {
            i += 4;
        }
    }
    achadas
}

/// Donos candidatos de cada sequência: onde, em `bytes`, aparecem os três
/// ponteiros `(inicio, fim, fim da capacidade)` coerentes com ela.
fn donos_de_conhecimento(base: usize, bytes: &[u8], sequencias: &[Sequencia]) -> Vec<(Sequencia, usize)> {
    let mut donos = Vec::new();
    for sequencia in sequencias {
        let padrao = (sequencia.inicio as u64).to_le_bytes();
        for pos in memchr::memmem::find_iter(bytes, &padrao).filter(|pos| pos.is_multiple_of(8)) {
            let fim = u64_at(bytes, pos + 8);
            let capacidade = u64_at(bytes, pos + 16);
            let (Some(fim), Some(capacidade)) = (fim, capacidade) else { continue };
            let inicio = sequencia.inicio as u64;
            let reservado = capacidade.saturating_sub(inicio) as usize;
            let coerente = fim == sequencia.fim() as u64
                && capacidade >= fim
                && reservado.is_multiple_of(REGISTRO_CONHECIMENTO)
                && reservado / REGISTRO_CONHECIMENTO <= MAX_REGISTROS_CONHECIMENTO;
            if coerente {
                donos.push((*sequencia, base + pos));
            }
        }
    }
    donos
}

/// Donos candidatos da lista de escolhidos: três ponteiros de 8 bytes
/// `(inicio, fim, fim da capacidade)` com a capacidade do jogo (100
/// entradas), `inicio <= fim <= fim da capacidade` e o uso em múltiplos de
/// uma entrada. Serve também para a lista vazia (sem entradas para casar).
fn donos_de_escolhidos(base: usize, bytes: &[u8]) -> Vec<usize> {
    const RESERVA: u64 = (CAPACIDADE_ESCOLHIDOS * ENTRADA_ESCOLHIDO) as u64;
    let mut donos = Vec::new();
    let mut pos = 0usize;
    while pos + 24 <= bytes.len() {
        if let (Some(inicio), Some(fim), Some(capacidade)) = (u64_at(bytes, pos), u64_at(bytes, pos + 8), u64_at(bytes, pos + 16)) {
            if capacidade.wrapping_sub(inicio) == RESERVA
                && inicio >= 0x10000
                && inicio.is_multiple_of(4)
                && inicio <= fim
                && fim <= capacidade
                && (fim - inicio).is_multiple_of(ENTRADA_ESCOLHIDO as u64)
            {
                donos.push(base + pos);
            }
        }
        pos += 8;
    }
    donos
}

// ---------------------------------------------------------------------
// Localização
// ---------------------------------------------------------------------

/// Onde estão, na memória viva, os dois vetores do scout nativo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalNativo {
    pub escolhidos: Option<VetorVivo>,
    /// Todas as estruturas donas coerentes que apontam para ESSA lista de
    /// escolhidos (o dono se move a cada recarga e cópias velhas podem
    /// continuar íntegras; a escrita atualiza o fim em todas que ainda
    /// batem).
    pub escolhidos_donos: Vec<usize>,
    pub conhecimento: Option<VetorVivo>,
}

/// Escolhe, entre os candidatos, os vetores que de fato validam (leitura
/// dos ponteiros e dos registros). `sequencias_e_donos` vem de
/// `donos_de_conhecimento`; `donos_escolhidos` de `donos_de_escolhidos`.
fn montar_local(
    sequencias_e_donos: &[(Sequencia, usize)],
    donos_escolhidos: &[usize],
    src: &impl ByteSource,
) -> LocalNativo {
    let mut conhecimento: Option<(usize, VetorVivo)> = None;
    for (sequencia, dono) in sequencias_e_donos {
        let Some(vetor) = VetorVivo::ler(src, *dono) else { continue };
        if vetor.inicio != sequencia.inicio || vetor.fim != sequencia.fim() {
            continue;
        }
        let Some(registros) = ler_conhecimento_de(src, &vetor) else { continue };
        if registros.len() == sequencia.registros && conhecimento.as_ref().is_none_or(|(n, _)| registros.len() > *n) {
            conhecimento = Some((registros.len(), vetor));
        }
    }

    let mut validos: Vec<(usize, VetorVivo)> = Vec::new();
    for dono in donos_escolhidos {
        let Some(vetor) = VetorVivo::ler(src, *dono) else { continue };
        if let Some(entradas) = ler_escolhidos_de(src, &vetor) {
            validos.push((entradas.len(), vetor));
        }
    }
    // Ambíguo (mais de um dono coerente): fica com o que tem mais entradas
    // e, em empate, o mais perto dos registros de conhecimento.
    let referencia = conhecimento.map(|(_, v)| v.inicio).unwrap_or(0);
    validos.sort_by_key(|(n, v)| (std::cmp::Reverse(*n), v.inicio.abs_diff(referencia)));
    if validos.len() > 1 {
        let enderecos: Vec<String> = validos.iter().map(|(n, v)| format!("0x{:X} ({n} entradas)", v.dono)).collect();
        tracing::warn!("[nativo] {} donos coerentes da lista de escolhidos ({}); usando o primeiro.", validos.len(), enderecos.join(", "));
    }
    let escolhidos = validos.first().map(|(_, v)| *v);
    let escolhidos_donos = escolhidos
        .map(|principal| {
            validos
                .iter()
                .filter(|(_, v)| (v.inicio, v.fim, v.fim_capacidade) == (principal.inicio, principal.fim, principal.fim_capacidade))
                .map(|(_, v)| v.dono)
                .collect()
        })
        .unwrap_or_default();
    LocalNativo { escolhidos, escolhidos_donos, conhecimento: conhecimento.map(|(_, v)| v) }
}

/// Lê cada região de `regions` para o `buffer` e chama `f(base, bytes)`.
/// Pula o próprio buffer de leitura (a DLL enxerga a memória dela).
fn varrer(regions: &[Region], own: &OwnMemory, buffer: &mut [u8], mut f: impl FnMut(usize, &[u8])) {
    for region in regions {
        if own.overlaps_buffer(region.base, region.size) {
            continue;
        }
        let Some(lido) = memscan::read_region_into(region, buffer) else { continue };
        f(region.base, buffer.get(..lido).unwrap_or_default());
    }
}

/// Localiza os dois vetores na memória do jogo. Pesado (varre regiões):
/// só num `AsyncTask` (AD-4).
fn localizar() -> Result<LocalNativo, SaveRepoError> {
    let inicio = std::time::Instant::now();
    let regions = memscan::enumerate_private_committed_regions();
    let mut buffer = ScrubbedBuffer(vec![0u8; memscan::MAX_REGION_SIZE]);
    let own = OwnMemory::current(&[], &buffer.0);

    // 1) Sequências de conhecimento: primeiro nas regiões com data viva
    //    (onde a carreira mora); se não houver, em todas.
    let rapidas = regions_with_live_date(&regions, &ProcessMemory);
    let mut sequencias: Vec<(usize, Sequencia)> = Vec::new(); // (base da região, sequência)
    for conjunto in [&rapidas[..], &regions[..]] {
        if conjunto.is_empty() {
            continue;
        }
        varrer(conjunto, &own, &mut buffer.0, |base, bytes| {
            sequencias.extend(sequencias_de_conhecimento(base, bytes).into_iter().map(|s| (base, s)));
        });
        if !sequencias.is_empty() {
            break;
        }
    }
    if sequencias.is_empty() {
        tracing::warn!("[nativo] Nenhuma sequência de registros de conhecimento na memória.");
        return Err(SaveRepoError::CarreiraNaoCarregada);
    }
    for (base, s) in &sequencias {
        tracing::info!("[nativo] Sequência de conhecimento: 0x{:X} ({} registros, região 0x{:X}).", s.inicio, s.registros, base);
    }
    let so_sequencias: Vec<Sequencia> = sequencias.iter().map(|(_, s)| *s).collect();
    let ancora = sequencias.iter().map(|(b, _)| *b).next().unwrap_or(0);

    // 2) Donos dos dois vetores: primeiro nas regiões vizinhas, depois em todas.
    let vizinhas: Vec<Region> = regions.iter().copied().filter(|r| r.base.abs_diff(ancora) <= ALCANCE_VIZINHO).collect();
    let mut melhor = LocalNativo::default();
    for conjunto in [&vizinhas[..], &regions[..]] {
        let mut donos_conhecimento = Vec::new();
        let mut donos_lista = Vec::new();
        varrer(conjunto, &own, &mut buffer.0, |base, bytes| {
            donos_conhecimento.extend(donos_de_conhecimento(base, bytes, &so_sequencias));
            donos_lista.extend(donos_de_escolhidos(base, bytes));
        });
        let achado = montar_local(&donos_conhecimento, &donos_lista, &ProcessMemory);
        if melhor.escolhidos.is_none() {
            melhor.escolhidos = achado.escolhidos;
            melhor.escolhidos_donos = achado.escolhidos_donos;
        }
        melhor.conhecimento = melhor.conhecimento.or(achado.conhecimento);
        if melhor.escolhidos.is_some() && melhor.conhecimento.is_some() {
            break;
        }
    }
    tracing::info!("[nativo] Localização em {} ms.", inicio.elapsed().as_millis());
    if melhor.escolhidos.is_none() && melhor.conhecimento.is_none() {
        return Err(SaveRepoError::NaoLocalizado);
    }
    Ok(melhor)
}

// ---------------------------------------------------------------------
// Cache e leituras
// ---------------------------------------------------------------------

static NATIVO: OnceLock<Mutex<Option<LocalNativo>>> = OnceLock::new();

fn lock_cache() -> MutexGuard<'static, Option<LocalNativo>> {
    let mutex = NATIVO.get_or_init(|| Mutex::new(None));
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Localiza e guarda no cache; só registra no log (falhar aqui nunca
/// atrapalha a Central). Chamada de dentro do `AsyncTask` que localiza a
/// carreira, depois que ela foi confirmada.
pub(super) fn localizar_e_guardar() {
    match localizar() {
        Ok(local) => {
            tracing::info!("[nativo] escolhidos: {:?}", local.escolhidos);
            tracing::info!("[nativo] conhecimento: {:?}", local.conhecimento);
            if let Some(v) = local.escolhidos {
                tracing::info!("[nativo] donos da lista de escolhidos: {:X?}", local.escolhidos_donos);
                match ler_escolhidos_de(&ProcessMemory, &v) {
                    Some(lista) => tracing::info!("[nativo] Lista de escolhidos nativa: {} jogador(es) {:?}", lista.len(), lista.iter().map(|e| e.jogador).collect::<Vec<_>>()),
                    None => tracing::warn!("[nativo] Lista de escolhidos ilegível logo depois de localizar."),
                }
            }
            if let Some(v) = local.conhecimento {
                match ler_conhecimento_de(&ProcessMemory, &v) {
                    Some(lista) => tracing::info!("[nativo] Conhecimento nativo: {} registro(s), nível máximo {:?}", lista.len(), lista.iter().map(|r| r.nivel).max()),
                    None => tracing::warn!("[nativo] Conhecimento ilegível logo depois de localizar."),
                }
            }
            *lock_cache() = Some(local);
        }
        Err(erro) => {
            tracing::warn!("[nativo] Scout nativo não localizado: {erro}");
            *lock_cache() = None;
        }
    }
}

/// Lista de escolhidos nativa, lida na hora. O cache é revalidado: se o
/// vetor mudou de lugar (outro save carregado, estrutura liberada), a
/// entrada do cache é descartada e a leitura falha em vez de mentir.
#[allow(dead_code)] // usado pelas Stories 7.2+
pub fn read_native_shortlist() -> Result<Vec<EntradaEscolhido>, SaveRepoError> {
    let mut cache = lock_cache();
    let Some(local) = cache.as_mut() else { return Err(SaveRepoError::NaoLocalizado) };
    let Some(vetor) = local.escolhidos else { return Err(SaveRepoError::NaoLocalizado) };
    let atual = VetorVivo::ler(&ProcessMemory, vetor.dono);
    match atual.filter(|a| a.inicio == vetor.inicio).and_then(|a| ler_escolhidos_de(&ProcessMemory, &a).map(|l| (a, l))) {
        Some((atual, lista)) => {
            local.escolhidos = Some(atual);
            Ok(lista)
        }
        None => {
            local.escolhidos = None;
            Err(SaveRepoError::NaoLocalizado)
        }
    }
}

/// Conhecimento nativo, lido na hora (mesma revalidação).
#[allow(dead_code)] // usado pelas Stories 7.3+
pub fn read_native_knowledge() -> Result<Vec<RegistroConhecimento>, SaveRepoError> {
    let mut cache = lock_cache();
    let Some(local) = cache.as_mut() else { return Err(SaveRepoError::NaoLocalizado) };
    let Some(vetor) = local.conhecimento else { return Err(SaveRepoError::NaoLocalizado) };
    let atual = VetorVivo::ler(&ProcessMemory, vetor.dono);
    match atual.filter(|a| a.inicio == vetor.inicio).and_then(|a| ler_conhecimento_de(&ProcessMemory, &a).map(|l| (a, l))) {
        Some((atual, lista)) => {
            local.conhecimento = Some(atual);
            Ok(lista)
        }
        None => {
            local.conhecimento = None;
            Err(SaveRepoError::NaoLocalizado)
        }
    }
}

// ---------------------------------------------------------------------
// Escrita na lista de escolhidos nativa (Story 7.2)
// ---------------------------------------------------------------------

/// Interruptor da sincronização com o jogo (a Story 7.4 o liga às
/// configurações e o persiste). Ligado por padrão (decisão de 2026-10-06).
static SINCRONIZAR: AtomicBool = AtomicBool::new(true);

/// A Central pode escrever no scout nativo agora?
pub fn sincronizacao_ligada() -> bool {
    SINCRONIZAR.load(Ordering::Relaxed)
}

#[allow(dead_code)] // ligado às configurações na Story 7.4
pub fn definir_sincronizacao(ligada: bool) {
    SINCRONIZAR.store(ligada, Ordering::Relaxed);
}

/// O que aconteceu com um pedido de adicionar/remover na lista nativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoLista {
    Adicionado,
    /// Já estava na lista do jogo (não é "da Central": ela não o remove depois).
    JaEstava,
    Removido,
    NaoEstava,
}

/// Lê os donos ainda coerentes com a lista em cache. Os que não batem mais
/// com o início e a capacidade guardados (estrutura liberada e reaproveitada)
/// ficam de fora; os que sobram precisam concordar no fim. Sem dono vivo, ou
/// com donos discordando, o cache é considerado velho.
fn donos_vivos(mem: &impl ByteSource, local: &LocalNativo) -> Result<(VetorVivo, Vec<usize>), SaveRepoError> {
    let esperado = local.escolhidos.ok_or(SaveRepoError::NaoLocalizado)?;
    let vivos: Vec<VetorVivo> = local
        .escolhidos_donos
        .iter()
        .filter_map(|dono| VetorVivo::ler(mem, *dono))
        .filter(|v| v.inicio == esperado.inicio && v.fim_capacidade == esperado.fim_capacidade)
        .collect();
    let Some(primeiro) = vivos.first().copied() else {
        return Err(SaveRepoError::NativoMudou);
    };
    if vivos.iter().any(|v| v.fim != primeiro.fim) {
        return Err(SaveRepoError::NativoMudou);
    }
    Ok((primeiro, vivos.iter().map(|v| v.dono).collect()))
}

fn escrever_fim(mem: &impl ByteSink, donos: &[usize], novo_fim: usize) -> Result<(), SaveRepoError> {
    for dono in donos {
        if !mem.write(dono + 8, &(novo_fim as u64).to_le_bytes()) {
            return Err(SaveRepoError::ProcessoInacessivel);
        }
    }
    Ok(())
}

/// Confere, depois de escrever, que todos os donos mostram `novo_fim` e que
/// a lista lida é exatamente `esperada`.
fn conferir_lista(
    mem: &impl ByteSource,
    donos: &[usize],
    base: &VetorVivo,
    novo_fim: usize,
    esperada: &[EntradaEscolhido],
) -> Result<VetorVivo, SaveRepoError> {
    let mut ultimo = None;
    for dono in donos {
        let v = VetorVivo::ler(mem, *dono).ok_or(SaveRepoError::ProcessoInacessivel)?;
        if v.inicio != base.inicio || v.fim != novo_fim || v.fim_capacidade != base.fim_capacidade {
            return Err(SaveRepoError::Interno(format!("releitura do dono 0x{dono:X} não confere")));
        }
        ultimo = Some(v);
    }
    let vetor = ultimo.ok_or(SaveRepoError::NativoMudou)?;
    match ler_escolhidos_de(mem, &vetor) {
        Some(lista) if lista == esperada => Ok(vetor),
        _ => Err(SaveRepoError::Interno("a lista nativa relida não é a escrita".into())),
    }
}

/// Acrescenta `jogador` (do time `time`) no fim da lista de escolhidos do
/// jogo. Compare-and-write: só escreve se a lista continua como foi achada
/// (mesmo início/capacidade nos donos vivos, entradas válidas), grava a
/// entrada ANTES de avançar o ponteiro de fim e relê tudo.
fn adicionar_em(
    mem: &(impl ByteSource + ByteSink),
    local: &mut LocalNativo,
    time: i32,
    jogador: i32,
) -> Result<ResultadoLista, SaveRepoError> {
    let (vetor, donos) = donos_vivos(mem, local)?;
    let atual = ler_escolhidos_de(mem, &vetor).ok_or(SaveRepoError::NativoMudou)?;
    if atual.iter().any(|e| e.jogador == jogador) {
        return Ok(ResultadoLista::JaEstava);
    }
    if atual.len() >= CAPACIDADE_ESCOLHIDOS || vetor.fim + ENTRADA_ESCOLHIDO > vetor.fim_capacidade {
        return Err(SaveRepoError::ListaNativaCheia);
    }
    let nova = EntradaEscolhido { time, jogador, revelado: [-1; 4], marca: 1 };
    if !mem.write(vetor.fim, &nova.para_bytes()) {
        return Err(SaveRepoError::ProcessoInacessivel);
    }
    let novo_fim = vetor.fim + ENTRADA_ESCOLHIDO;
    escrever_fim(mem, &donos, novo_fim)?;
    let mut esperada = atual;
    esperada.push(nova);
    let relido = conferir_lista(mem, &donos, &vetor, novo_fim, &esperada)?;
    local.escolhidos = Some(relido);
    local.escolhidos_donos = donos;
    Ok(ResultadoLista::Adicionado)
}

/// Tira `jogador` da lista de escolhidos do jogo (as entradas seguintes
/// sobem uma casa). O fim encolhe ANTES de os dados serem reescritos.
fn remover_em(mem: &(impl ByteSource + ByteSink), local: &mut LocalNativo, jogador: i32) -> Result<ResultadoLista, SaveRepoError> {
    let (vetor, donos) = donos_vivos(mem, local)?;
    let atual = ler_escolhidos_de(mem, &vetor).ok_or(SaveRepoError::NativoMudou)?;
    let Some(posicao) = atual.iter().position(|e| e.jogador == jogador) else {
        return Ok(ResultadoLista::NaoEstava);
    };
    let mut restante = atual;
    restante.remove(posicao);
    let novo_fim = vetor.inicio + restante.len() * ENTRADA_ESCOLHIDO;
    escrever_fim(mem, &donos, novo_fim)?;
    if posicao < restante.len() {
        let bytes: Vec<u8> = restante.iter().skip(posicao).flat_map(|e| e.para_bytes()).collect();
        if !mem.write(vetor.inicio + posicao * ENTRADA_ESCOLHIDO, &bytes) {
            return Err(SaveRepoError::ProcessoInacessivel);
        }
    }
    let relido = conferir_lista(mem, &donos, &vetor, novo_fim, &restante)?;
    local.escolhidos = Some(relido);
    local.escolhidos_donos = donos;
    Ok(ResultadoLista::Removido)
}

/// Acrescenta um jogador à lista de escolhidos do jogo (ver `adicionar_em`).
/// Se o cache estiver velho (`NativoMudou`), ele é descartado.
pub fn write_native_shortlist_add(time: i32, jogador: i32) -> Result<ResultadoLista, SaveRepoError> {
    com_cache(|local| adicionar_em(&ProcessMemory, local, time, jogador))
}

/// Tira um jogador da lista de escolhidos do jogo (ver `remover_em`).
pub fn write_native_shortlist_remove(jogador: i32) -> Result<ResultadoLista, SaveRepoError> {
    com_cache(|local| remover_em(&ProcessMemory, local, jogador))
}

fn com_cache<R>(f: impl FnOnce(&mut LocalNativo) -> Result<R, SaveRepoError>) -> Result<R, SaveRepoError> {
    let mut cache = lock_cache();
    let Some(local) = cache.as_mut() else { return Err(SaveRepoError::NaoLocalizado) };
    let resultado = f(local);
    if matches!(resultado, Err(SaveRepoError::NativoMudou)) {
        local.escolhidos = None;
        local.escolhidos_donos.clear();
    }
    resultado
}

/// Descarta o cache (outra carreira carregada).
pub(super) fn esquecer() {
    *lock_cache() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_repo::SliceSource;

    /// Registros reais do array de conhecimento da carreira de teste
    /// (Real Madrid, 10/07/2026; capturados da memória viva em 2026-10-06).
    const REAIS: [(i32, i32, i32, i32); 29] = [
        (40205, 1048576, 28, 20260630),
        (41964, 1048576, 28, 20260630),
        (42442, 1048576, 28, 20260630),
        (55837, 1048577, 56, 20260703),
        (70497, 1048577, 22, 20260703),
        (71178, 1048577, 28, 20260703),
        (71611, 1048577, 50, 20260703),
        (71651, 1048578, 28, 20260706),
        (73885, 1048578, 198, 20260706),
        (73990, 1048576, 24, 20260630),
        (74759, 1048578, 56, 20260706),
        (76087, 1048577, 56, 20260703),
        (76740, 1048576, 56, 20260630),
        (76803, 1048578, 56, 20260706),
        (77593, 1048578, 28, 20260706),
        (230621, 1048578, 78, 20260706),
        (231521, 1048578, 168, 20260706),
        (232643, 1048577, 28, 20260703),
        (235073, 1048577, 66, 20260703),
        (242879, 1048577, 17, 20260703),
        (246606, 65535, 198, 20260630),
        (251632, 1048577, 28, 20260703),
        (263798, 1048578, 11, 20260706),
        (264681, 1048578, 56, 20260706),
        (268428, 1048577, 16, 20260703),
        (268737, 393218, 198, 20260710),
        (271574, 1048578, 27, 20260706),
        (278394, 65535, 198, 20260630),
        (278399, 65535, 198, 20260630),
    ];

    fn registro(jogador: i32, a: i32, nivel: i32, data: i32) -> [u8; REGISTRO_CONHECIMENTO] {
        RegistroConhecimento { jogador, a, nivel, data: Date(data) }.para_bytes()
    }

    fn escreve_u64(memoria: &mut [u8], pos: usize, valor: u64) {
        memoria[pos..pos + 8].copy_from_slice(&valor.to_le_bytes());
    }

    /// Memória simulada: endereços = posições. Conhecimento (29 reais) em
    /// `DADOS`, com cabeçalho de lixo antes e zeros depois; dono em `DONO_C`.
    /// Lista de escolhidos (Nypan e Smit, bytes reais) em `LISTA`, dono em
    /// `DONO_L`.
    const DADOS: usize = 0x2_0000;
    const DONO_C: usize = 0x1_0000;
    const LISTA: usize = 0x3_0000;
    const DONO_L: usize = 0x1_1000;

    fn memoria_simulada() -> Vec<u8> {
        let mut memoria = vec![0u8; 0x4_0000];
        for (i, (jogador, a, nivel, data)) in REAIS.iter().enumerate() {
            let pos = DADOS + i * REGISTRO_CONHECIMENTO;
            memoria[pos..pos + REGISTRO_CONHECIMENTO].copy_from_slice(&registro(*jogador, *a, *nivel, *data));
        }
        // cabeçalho antes do primeiro registro (contador 29, ano, ...)
        for (k, v) in [1_946_293_452i32, 4209, 9, 7, 2026, 29].iter().enumerate() {
            let pos = DADOS - 24 + k * 4;
            memoria[pos..pos + 4].copy_from_slice(&v.to_le_bytes());
        }
        let fim = DADOS + REAIS.len() * REGISTRO_CONHECIMENTO;
        escreve_u64(&mut memoria, DONO_C, DADOS as u64);
        escreve_u64(&mut memoria, DONO_C + 8, fim as u64);
        escreve_u64(&mut memoria, DONO_C + 16, (DADOS + 1500 * REGISTRO_CONHECIMENTO) as u64);

        let nypan = EntradaEscolhido { time: 10, jogador: 268737, revelado: [-1; 4], marca: 1 }.para_bytes();
        let smit = EntradaEscolhido { time: 1906, jogador: 71532, revelado: [-1; 4], marca: 1 }.para_bytes();
        memoria[LISTA..LISTA + ENTRADA_ESCOLHIDO].copy_from_slice(&nypan);
        memoria[LISTA + ENTRADA_ESCOLHIDO..LISTA + 2 * ENTRADA_ESCOLHIDO].copy_from_slice(&smit);
        escreve_u64(&mut memoria, DONO_L, LISTA as u64);
        escreve_u64(&mut memoria, DONO_L + 8, (LISTA + 2 * ENTRADA_ESCOLHIDO) as u64);
        escreve_u64(&mut memoria, DONO_L + 16, (LISTA + CAPACIDADE_ESCOLHIDOS * ENTRADA_ESCOLHIDO) as u64);
        memoria
    }

    #[test]
    fn a_real_knowledge_record_round_trips_and_bad_ones_are_rejected() {
        let bytes = registro(268737, 393218, 198, 20260710);
        let r = RegistroConhecimento::de_bytes(&bytes).expect("registro real");
        assert_eq!((r.jogador, r.a, r.nivel, r.data), (268737, 393218, 198, Date(20260710)));
        assert_eq!(r.para_bytes(), bytes);
        assert!(RegistroConhecimento::de_bytes(&registro(268737, 2, 199, 20260710)).is_none(), "nível acima de 198");
        assert!(RegistroConhecimento::de_bytes(&registro(268737, 2, 140, 20269999)).is_none(), "data implausível");
        let mut sem_fim = registro(268737, 2, 140, 20260710);
        sem_fim[16..20].copy_from_slice(&0i32.to_le_bytes());
        assert!(RegistroConhecimento::de_bytes(&sem_fim).is_none(), "falta o -1 final");
    }

    #[test]
    fn a_real_shortlist_entry_parses_even_with_garbage_padding() {
        // Entrada do Nypan como estava na memória viva: os 3 bytes depois da
        // marca eram lixo (`-1366816767` = 0xAE9F4D01).
        let mut bytes = EntradaEscolhido { time: 10, jogador: 268737, revelado: [-1; 4], marca: 1 }.para_bytes();
        bytes[24..28].copy_from_slice(&(-1_366_816_767i32).to_le_bytes());
        let e = EntradaEscolhido::de_bytes(&bytes).expect("entrada real");
        assert_eq!((e.time, e.jogador, e.revelado, e.marca), (10, 268737, [-1; 4], 1));
        let mut marca_estranha = bytes;
        marca_estranha[24] = 7;
        assert!(EntradaEscolhido::de_bytes(&marca_estranha).is_none());
    }

    #[test]
    fn finds_the_real_knowledge_array_with_header_garbage_before_and_zeros_after() {
        let memoria = memoria_simulada();
        let achadas = sequencias_de_conhecimento(0, &memoria);
        assert_eq!(achadas, vec![Sequencia { inicio: DADOS, registros: 29 }]);
    }

    #[test]
    fn unordered_or_too_short_runs_are_not_knowledge_arrays() {
        let mut memoria = vec![0u8; 0x1000];
        // 2 registros válidos: curto demais
        memoria[0x100..0x114].copy_from_slice(&registro(100, 1, 10, 20260701));
        memoria[0x114..0x128].copy_from_slice(&registro(200, 1, 10, 20260701));
        // 4 registros com a ordem quebrada: só os 2 primeiros contam
        for (i, id) in [10, 20, 15, 30].iter().enumerate() {
            let pos = 0x400 + i * 20;
            memoria[pos..pos + 20].copy_from_slice(&registro(*id, 1, 10, 20260701));
        }
        assert!(sequencias_de_conhecimento(0, &memoria).is_empty());
    }

    #[test]
    fn the_knowledge_owner_must_match_start_end_and_capacity() {
        let mut memoria = memoria_simulada();
        let sequencias = sequencias_de_conhecimento(0, &memoria);
        let donos = donos_de_conhecimento(0, &memoria, &sequencias);
        assert_eq!(donos, vec![(sequencias[0], DONO_C)]);

        // dono velho: o início aponta pro array, mas o fim é de outro tamanho
        escreve_u64(&mut memoria, 0x1_2000, DADOS as u64);
        escreve_u64(&mut memoria, 0x1_2008, (DADOS + 3 * REGISTRO_CONHECIMENTO) as u64);
        escreve_u64(&mut memoria, 0x1_2010, (DADOS + 1500 * REGISTRO_CONHECIMENTO) as u64);
        let donos = donos_de_conhecimento(0, &memoria, &sequencias);
        assert_eq!(donos.len(), 1, "o dono incoerente é ignorado");
    }

    #[test]
    fn the_shortlist_owner_needs_capacity_for_100_entries_and_sane_pointers() {
        let mut memoria = memoria_simulada();
        assert_eq!(donos_de_escolhidos(0, &memoria), vec![DONO_L]);

        // cópia velha como a vista depois do recarregamento: início certo,
        // fim e capacidade de outra coisa
        escreve_u64(&mut memoria, 0x1_3000, LISTA as u64);
        escreve_u64(&mut memoria, 0x1_3008, 0x8CC8_E960);
        escreve_u64(&mut memoria, 0x1_3010, 0x8CC8_E990);
        assert_eq!(donos_de_escolhidos(0, &memoria), vec![DONO_L]);

        // lista VAZIA (início == fim) também é um dono válido
        escreve_u64(&mut memoria, 0x1_4000, LISTA as u64);
        escreve_u64(&mut memoria, 0x1_4008, LISTA as u64);
        escreve_u64(&mut memoria, 0x1_4010, (LISTA + 2800) as u64);
        assert_eq!(donos_de_escolhidos(0, &memoria), vec![DONO_L, 0x1_4000]);
    }

    #[test]
    fn locates_both_vectors_in_a_simulated_memory_and_reads_them() {
        let memoria = memoria_simulada();
        let origem = SliceSource(&memoria);
        let sequencias = sequencias_de_conhecimento(0, &memoria);
        let local = montar_local(&donos_de_conhecimento(0, &memoria, &sequencias), &donos_de_escolhidos(0, &memoria), &origem);

        let escolhidos = local.escolhidos.expect("lista de escolhidos");
        let lista = ler_escolhidos_de(&origem, &escolhidos).expect("lê a lista");
        assert_eq!(lista.iter().map(|e| (e.time, e.jogador)).collect::<Vec<_>>(), vec![(10, 268737), (1906, 71532)]);
        assert_eq!(escolhidos.fim_capacidade - escolhidos.inicio, 2800);

        let conhecimento = local.conhecimento.expect("conhecimento");
        let registros = ler_conhecimento_de(&origem, &conhecimento).expect("lê o conhecimento");
        assert_eq!(registros.len(), 29);
        let nypan = registros.iter().find(|r| r.jogador == 268737).expect("Nypan");
        assert_eq!((nypan.nivel, nypan.data), (198, Date(20260710)));
        assert!(registros.iter().find(|r| r.jogador == 71532).is_none(), "o Smit ainda não tem registro");
    }

    #[test]
    fn a_shortlist_with_duplicates_or_bad_entries_is_refused() {
        let mut memoria = memoria_simulada();
        // dois jogadores iguais
        let nypan = EntradaEscolhido { time: 10, jogador: 268737, revelado: [-1; 4], marca: 1 }.para_bytes();
        memoria[LISTA + ENTRADA_ESCOLHIDO..LISTA + 2 * ENTRADA_ESCOLHIDO].copy_from_slice(&nypan);
        let origem = SliceSource(&memoria);
        let vetor = VetorVivo::ler(&origem, DONO_L).expect("dono");
        assert!(ler_escolhidos_de(&origem, &vetor).is_none());

        // uso que não é múltiplo de uma entrada
        let torto = VetorVivo { fim: vetor.inicio + 30, ..vetor };
        assert!(ler_escolhidos_de(&origem, &torto).is_none());
    }

    #[test]
    fn two_coherent_shortlist_owners_prefer_the_one_with_more_entries() {
        let mut memoria = memoria_simulada();
        // segundo dono coerente mas VAZIO, apontando para outro bloco
        let outro = 0x3_8000usize;
        escreve_u64(&mut memoria, 0x1_5000, outro as u64);
        escreve_u64(&mut memoria, 0x1_5008, outro as u64);
        escreve_u64(&mut memoria, 0x1_5010, (outro + 2800) as u64);
        let origem = SliceSource(&memoria);
        let sequencias = sequencias_de_conhecimento(0, &memoria);
        let local = montar_local(&donos_de_conhecimento(0, &memoria, &sequencias), &donos_de_escolhidos(0, &memoria), &origem);
        assert_eq!(local.escolhidos.map(|v| v.dono), Some(DONO_L));
    }

    // -----------------------------------------------------------------
    // Escrita na lista nativa (Story 7.2)
    // -----------------------------------------------------------------

    /// Memória simulada que aceita escrita (os testes de leitura usam `SliceSource`).
    struct Mutavel(std::cell::RefCell<Vec<u8>>);

    impl ByteSource for Mutavel {
        fn read(&self, address: usize, len: usize) -> Option<Vec<u8>> {
            self.0.borrow().get(address..address.checked_add(len)?).map(<[u8]>::to_vec)
        }
    }

    impl ByteSink for Mutavel {
        fn write(&self, address: usize, bytes: &[u8]) -> bool {
            let mut memoria = self.0.borrow_mut();
            match memoria.get_mut(address..address + bytes.len()) {
                Some(destino) => {
                    destino.copy_from_slice(bytes);
                    true
                }
                None => false,
            }
        }
    }

    const DONO_L2: usize = 0x1_6000;

    /// Memória + local achado, com DOIS donos coerentes da mesma lista.
    fn com_dois_donos() -> (Mutavel, LocalNativo) {
        let mut memoria = memoria_simulada();
        escreve_u64(&mut memoria, DONO_L2, LISTA as u64);
        escreve_u64(&mut memoria, DONO_L2 + 8, (LISTA + 2 * ENTRADA_ESCOLHIDO) as u64);
        escreve_u64(&mut memoria, DONO_L2 + 16, (LISTA + CAPACIDADE_ESCOLHIDOS * ENTRADA_ESCOLHIDO) as u64);
        let sequencias = sequencias_de_conhecimento(0, &memoria);
        let local = montar_local(&donos_de_conhecimento(0, &memoria, &sequencias), &donos_de_escolhidos(0, &memoria), &SliceSource(&memoria));
        (Mutavel(std::cell::RefCell::new(memoria)), local)
    }

    fn lista_do_jogo(mem: &Mutavel, local: &LocalNativo) -> Vec<(i32, i32)> {
        let vetor = VetorVivo::ler(mem, local.escolhidos_donos[0]).expect("dono");
        ler_escolhidos_de(mem, &vetor).expect("lista").iter().map(|e| (e.time, e.jogador)).collect()
    }

    #[test]
    fn both_coherent_owners_of_the_same_list_are_remembered() {
        let (_, local) = com_dois_donos();
        let mut donos = local.escolhidos_donos.clone();
        donos.sort_unstable();
        assert_eq!(donos, vec![DONO_L, DONO_L2]);
    }

    #[test]
    fn adding_appends_the_entry_and_moves_the_end_of_every_owner() {
        let (mem, mut local) = com_dois_donos();
        let resultado = adicionar_em(&mem, &mut local, 1808, 73885).expect("adiciona");
        assert_eq!(resultado, ResultadoLista::Adicionado);
        assert_eq!(lista_do_jogo(&mem, &local), vec![(10, 268737), (1906, 71532), (1808, 73885)]);
        for dono in [DONO_L, DONO_L2] {
            let v = VetorVivo::ler(&mem, dono).expect("dono");
            assert_eq!(v.fim, LISTA + 3 * ENTRADA_ESCOLHIDO, "o fim de 0x{dono:X} avançou");
        }
        // o cache acompanha a lista nova
        assert_eq!(local.escolhidos.map(|v| v.fim), Some(LISTA + 3 * ENTRADA_ESCOLHIDO));
        // a entrada nova é a do jogo: -1 ×4 e marca 1, preenchimento zerado
        let bruto = mem.read(LISTA + 2 * ENTRADA_ESCOLHIDO, ENTRADA_ESCOLHIDO).expect("bytes");
        assert_eq!(bruto, EntradaEscolhido { time: 1808, jogador: 73885, revelado: [-1; 4], marca: 1 }.para_bytes());
        assert_eq!(&bruto[25..28], &[0, 0, 0]);
    }

    #[test]
    fn adding_a_player_who_is_already_there_writes_nothing() {
        let (mem, mut local) = com_dois_donos();
        let antes = mem.0.borrow().clone();
        assert_eq!(adicionar_em(&mem, &mut local, 1906, 71532), Ok(ResultadoLista::JaEstava));
        assert_eq!(*mem.0.borrow(), antes);
    }

    #[test]
    fn a_full_native_list_refuses_one_more() {
        let (mem, mut local) = com_dois_donos();
        for i in 2..CAPACIDADE_ESCOLHIDOS {
            let jogador = 100_000 + i as i32;
            adicionar_em(&mem, &mut local, 5, jogador).expect("cabe");
        }
        assert_eq!(lista_do_jogo(&mem, &local).len(), CAPACIDADE_ESCOLHIDOS);
        let antes = mem.0.borrow().clone();
        assert_eq!(adicionar_em(&mem, &mut local, 5, 399_999), Err(SaveRepoError::ListaNativaCheia));
        assert_eq!(*mem.0.borrow(), antes, "nada escrito");
    }

    #[test]
    fn owners_that_disagree_or_moved_make_the_cache_stale_without_writing() {
        // donos discordando no fim
        let (mem, mut local) = com_dois_donos();
        mem.write(DONO_L2 + 8, &((LISTA + ENTRADA_ESCOLHIDO) as u64).to_le_bytes());
        let antes = mem.0.borrow().clone();
        assert_eq!(adicionar_em(&mem, &mut local, 1808, 73885), Err(SaveRepoError::NativoMudou));
        assert_eq!(*mem.0.borrow(), antes);

        // todos os donos se mexeram (estrutura liberada e reaproveitada)
        let (mem, mut local) = com_dois_donos();
        mem.write(DONO_L, &[0u8; 24]);
        mem.write(DONO_L2, &[0u8; 24]);
        let antes = mem.0.borrow().clone();
        assert_eq!(adicionar_em(&mem, &mut local, 1808, 73885), Err(SaveRepoError::NativoMudou));
        assert_eq!(*mem.0.borrow(), antes);
    }

    #[test]
    fn an_owner_that_was_reused_is_dropped_and_the_live_one_still_works() {
        let (mem, mut local) = com_dois_donos();
        mem.write(DONO_L2, &[0u8; 24]); // virou outra coisa
        assert_eq!(adicionar_em(&mem, &mut local, 1808, 73885), Ok(ResultadoLista::Adicionado));
        assert_eq!(local.escolhidos_donos, vec![DONO_L], "o dono morto saiu do cache");
        assert_eq!(lista_do_jogo(&mem, &local).len(), 3);
    }

    #[test]
    fn removing_the_first_entry_compacts_the_list_and_shrinks_the_end() {
        let (mem, mut local) = com_dois_donos();
        adicionar_em(&mem, &mut local, 1808, 73885).expect("adiciona");
        assert_eq!(remover_em(&mem, &mut local, 268737), Ok(ResultadoLista::Removido));
        assert_eq!(lista_do_jogo(&mem, &local), vec![(1906, 71532), (1808, 73885)]);
        for dono in [DONO_L, DONO_L2] {
            assert_eq!(VetorVivo::ler(&mem, dono).map(|v| v.fim), Some(LISTA + 2 * ENTRADA_ESCOLHIDO));
        }
    }

    #[test]
    fn removing_the_last_entry_and_a_missing_one() {
        let (mem, mut local) = com_dois_donos();
        assert_eq!(remover_em(&mem, &mut local, 71532), Ok(ResultadoLista::Removido));
        assert_eq!(lista_do_jogo(&mem, &local), vec![(10, 268737)]);
        let antes = mem.0.borrow().clone();
        assert_eq!(remover_em(&mem, &mut local, 999), Ok(ResultadoLista::NaoEstava));
        assert_eq!(*mem.0.borrow(), antes);
        // esvazia: a lista vazia continua sendo um dono válido
        assert_eq!(remover_em(&mem, &mut local, 268737), Ok(ResultadoLista::Removido));
        assert!(lista_do_jogo(&mem, &local).is_empty());
        assert_eq!(adicionar_em(&mem, &mut local, 12, 268737), Ok(ResultadoLista::Adicionado));
    }
}
