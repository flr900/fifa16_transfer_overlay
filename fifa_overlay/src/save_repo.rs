//! `save_repo` — ÚNICA porta para `memscan`/`fifa_db`/`pointer_scan` (AD-2).
//!
//! Expõe o estado da carreira no vocabulário de domínio (data, orçamento,
//! identidade do save) e esconde offsets, tabelas e bit-packing. Nada
//! fora deste módulo (telas, `scout::*`) pode chamar `memscan`/`fifa_db`
//! diretamente.
//!
//! ## Por que não lemos o blob de database do heap
//! Teste com o jogo aberto (2026-09-30, `PROJECT_MEMORY.md` "Sessão 6"):
//! o blob de carreira no heap é só o buffer do último load/save — não
//! acompanha o orçamento ao vivo e some depois de um save.
//!
//! ## Como funciona
//! 1. `start_locating` roda num `AsyncTask` (varredura de memória inteira,
//!    NUNCA no render thread — AD-4):
//!    - lê os saves recentes em `Documents\FIFA 16` (identidade, data e os
//!      três valores de início de temporada de `dqXv`);
//!    - procura na memória a struct viva de finanças do jogo: esses três
//!      valores aparecem contíguos 20 bytes depois do `transferbudget`
//!      vivo (achado pela sonda e confirmado no Cheat Engine). O save
//!      cuja assinatura está viva É a carreira carregada.
//! 2. Depois disso, orçamento/salário são leituras de poucos bytes
//!    (`ReadProcessMemory` protegido), sempre revalidando a assinatura no
//!    endereço em cache.
//!
//! ## Limitações conhecidas (Story 1.1, Task 1.3)
//! - A DATA viva ainda não foi localizada: `read_current_date` devolve a
//!   data do último save.
//! - Se a temporada virar e o jogo não tiver salvo depois, a assinatura
//!   do disco não existe mais na memória → `CarreiraNaoCarregada` até
//!   salvar.

pub mod jogadores;
pub mod nativo;

use std::fmt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use sha2::{Digest, Sha256};

use crate::async_task::AsyncTask;
use crate::fifa_db::{self, TableDescriptor};
use crate::memscan::{self, Region};

pub use jogadores::{
    funcao_da_posicao, ler_miniface, nome_posicao, read_all_players, read_nations, read_squad_players, Atributo, Confederacao,
    Funcao, Liga, Nacao, Pe, PlayerPool, PlayerRaw, RitmoTrabalho, read_leagues, read_club_profile, DadosDoClube,
};

/// Short names (4 chars) das tabelas/campos que o `save_repo` lê.
///
/// Extraídos de `D:\Program Files\FIFA 16\data\db\fifa_ng_db-meta.xml`
/// com `python tools/resolve_short_names.py` (Story 1.1, Task 1.1) e
/// conferidos contra os `DATA` reais em `save_backups/` (mesmo layout
/// em todos os saves: `GJUr`/`mPrV`/`dqXv` ficam na database de
/// carreira de 34 tabelas, separada da de jogadores com `CZUM`).
///
/// `firstname`/`surname` NÃO são Huffman: são strings fixas inline
/// (storage_type 0, 32 bytes, terminadas em `\0`) — AD-11 vale sem
/// desvio.
pub mod fields {
    #[derive(Debug, Clone, Copy)]
    pub struct FieldRef {
        pub table: [u8; 4],
        pub field: [u8; 4],
        /// `rangelow` do metadata XML, somado ao valor cru do campo
        /// (mesma regra de `fifa16_db_parser.decode_table`).
        pub range_low: i64,
    }

    // GJUr = career_calendar (data cru `YYYYMMDD - 20080101`, 19 bits)
    pub const GJUR_CURRDATE: FieldRef = FieldRef { table: *b"GJUr", field: *b"aLZZ", range_low: 20080101 };
    pub const GJUR_STARTDATE: FieldRef = FieldRef { table: *b"GJUr", field: *b"vHhZ", range_low: 20080101 };
    pub const GJUR_ENDDATE: FieldRef = FieldRef { table: *b"GJUr", field: *b"ZPUX", range_low: 20080101 };
    // mPrV = career_users (o manager)
    pub const MPRV_FIRSTNAME: FieldRef = FieldRef { table: *b"mPrV", field: *b"HdeP", range_low: 0 };
    pub const MPRV_SURNAME: FieldRef = FieldRef { table: *b"mPrV", field: *b"rREd", range_low: 0 };
    pub const MPRV_CLUBTEAMID: FieldRef = FieldRef { table: *b"mPrV", field: *b"NTyS", range_low: -1 };
    // dqXv = career_managerpref (31 bits)
    pub const DQXV_TRANSFERBUDGET: FieldRef = FieldRef { table: *b"dqXv", field: *b"SnDr", range_low: 0 };
    pub const DQXV_START_WAGE_BUDGET: FieldRef = FieldRef { table: *b"dqXv", field: *b"BuJs", range_low: 0 };
    pub const DQXV_START_TRANSFER_BUDGET: FieldRef = FieldRef { table: *b"dqXv", field: *b"NFbp", range_low: 0 };
    pub const DQXV_START_PLAYER_WAGES: FieldRef = FieldRef { table: *b"dqXv", field: *b"ppDw", range_low: 0 };
}

use fields::FieldRef;

/// `ProductVersion` (string) esperado do `fifa16.exe`. O `FileVersion`
/// fixo do exe é `1.0.0.0` e o `ProductVersion` fixo é `16.0.0.0` — só a
/// string do `StringFileInfo` carrega o número da build (2904053 nem
/// cabe nos campos de 16 bits do `VS_FIXEDFILEINFO`). Conferido no exe
/// instalado em `D:\Program Files\FIFA 16\fifa16.exe` (2026-09-30).
const EXPECTED_PRODUCT_VERSION: Option<&str> = Some("16.0.2904053");

/// `storage_type` dos campos inteiros bit-packed.
const INT_STORAGE_TYPE: u32 = 3;
/// `storage_type` das strings fixas inline (`depth` em bits, `\0` no fim).
const STRING_STORAGE_TYPE: u32 = 0;

#[derive(Debug, Clone, PartialEq)]
pub enum SaveRepoError {
    /// Uma tabela/campo esperado não existe (ou ainda não foi mapeado).
    TabelaNaoEncontrada,
    /// Não foi possível ler a memória do processo.
    ProcessoInacessivel,
    /// O processo está acessível, mas não há carreira carregada.
    CarreiraNaoCarregada,
    /// `start_locating` ainda não terminou (ou o cache foi invalidado).
    NaoLocalizado,
    /// O orçamento vivo não é mais o valor que o usuário viu ao confirmar
    /// (o jogo mexeu nele no meio): nada foi escrito. Traz o valor atual.
    OrcamentoMudou(i32),
    /// A lista de escolhidos nativa está cheia (100 jogadores).
    ListaNativaCheia,
    /// O array de conhecimento do jogo não tem espaço para mais registros.
    ConhecimentoCheio,
    /// O scout nativo não está mais como foi localizado (a estrutura se
    /// moveu ou o conteúdo mudou): nada foi escrito.
    NativoMudou,
    /// Falha inesperada, com descrição para o log.
    Interno(String),
}

impl fmt::Display for SaveRepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveRepoError::TabelaNaoEncontrada => {
                write!(f, "Tabela ou campo do save não encontrado.")
            }
            SaveRepoError::ProcessoInacessivel => write!(f, "Não foi possível ler o save ativo."),
            SaveRepoError::CarreiraNaoCarregada => write!(f, "Nenhuma carreira carregada."),
            SaveRepoError::NaoLocalizado => write!(f, "Carreira ainda não localizada."),
            SaveRepoError::OrcamentoMudou(atual) => write!(f, "O orçamento mudou para {atual}."),
            SaveRepoError::ListaNativaCheia => write!(f, "A lista de escolhidos do jogo está cheia."),
            SaveRepoError::ConhecimentoCheio => write!(f, "O conhecimento do jogo está cheio."),
            SaveRepoError::NativoMudou => write!(f, "O scout do jogo mudou; nada foi escrito."),
            SaveRepoError::Interno(msg) => write!(f, "Erro interno: {msg}"),
        }
    }
}

/// Data no formato cru `YYYYMMDD` (mesma forma de `GJUr.currdate`).
/// No JSON de estado do Scout vira o inteiro puro (`20261015`), nunca
/// struct nem string ISO (AD-12) — por isso o `transparent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Date(pub i32);

impl Date {
    pub fn year(self) -> i32 {
        self.0 / 10_000
    }
    pub fn month(self) -> i32 {
        (self.0 / 100) % 100
    }
    pub fn day(self) -> i32 {
        self.0 % 100
    }
    /// Sanidade básica — a carreira do save real está em 2035 (ver
    /// `PROJECT_MEMORY.md`), então aceitamos um intervalo generoso.
    pub fn is_plausible(self) -> bool {
        (1900..=2200).contains(&self.year())
            && (1..=12).contains(&self.month())
            && (1..=31).contains(&self.day())
    }

    /// Dias desde 1970-01-01 (algoritmo `days_from_civil`, calendário
    /// gregoriano) — para comparar datas atravessando mês/ano.
    pub fn day_number(self) -> i64 {
        let (y, m, d) = (i64::from(self.year()), i64::from(self.month()), i64::from(self.day()));
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Inverso de `day_number` (algoritmo `civil_from_days`).
    pub fn from_day_number(dias: i64) -> Date {
        let z = dias + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        let yyyymmdd = y * 10_000 + m * 100 + d;
        Date(i32::try_from(yyyymmdd).unwrap_or(i32::MAX))
    }

    /// Esta data mais `dias` dias de calendário (prazo de uma Missão).
    pub fn mais_dias(self, dias: u32) -> Date {
        Date::from_day_number(self.day_number() + i64::from(dias))
    }
}

// ---------------------------------------------------------------------
// Verificação da build do jogo (NFR4)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum BuildCheck {
    Verified(String),
    /// Versão não legível ou esperada não configurada: só logamos, não
    /// bloqueamos (o exe é protegido por packer; preferimos não travar o
    /// usuário por um recurso de versão ilegível).
    Unverified(Option<String>),
    Mismatch(String),
}

pub fn evaluate_build(observed: Option<&str>, expected: Option<&str>) -> BuildCheck {
    match (observed, expected) {
        (Some(o), Some(e)) if o == e => BuildCheck::Verified(o.to_string()),
        (Some(o), Some(_)) => BuildCheck::Mismatch(o.to_string()),
        (o, _) => BuildCheck::Unverified(o.map(str::to_string)),
    }
}

/// Valor de um `VerQueryValueW` de string: UTF-16 terminado em `\0`
/// (às vezes com espaços em volta).
fn utf16_value_to_string(units: &[u16]) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(units.get(..end).unwrap_or_default())
        .trim()
        .to_string()
}

/// Lê a string `ProductVersion` do executável principal (o
/// `fifa16.exe`, já que a DLL roda dentro do processo do jogo). `None`
/// se o recurso de versão não existir/for ilegível.
fn read_main_module_product_version() -> Option<String> {
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

    let mut path = [0u16; 520];
    let len = unsafe { GetModuleFileNameW(None, &mut path) } as usize;
    if len == 0 || len >= path.len() {
        return None;
    }
    // `GetModuleFileNameW` já deixa o `\0` em `path[len]`
    read_product_version(path.get(..=len)?)
}

/// `ProductVersion` (string) de um executável; `path` é UTF-16
/// terminado em `\0`.
fn read_product_version(path: &[u16]) -> Option<String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    if path.last() != Some(&0) {
        return None;
    }
    let path_ptr = PCWSTR(path.as_ptr());

    let size = unsafe { GetFileVersionInfoSizeW(path_ptr, None) };
    if size == 0 {
        return None;
    }

    let mut buf = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(path_ptr, None, size, buf.as_mut_ptr() as *mut _) }.ok()?;

    // Os ponteiros devolvidos por VerQueryValueW apontam para dentro de
    // `buf`; conferimos os limites antes de montar qualquer slice.
    let buf_range = buf.as_ptr_range();
    let query = |sub_block: PCWSTR| -> Option<(*const u8, usize)> {
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        let found =
            unsafe { VerQueryValueW(buf.as_ptr() as *const _, sub_block, &mut ptr, &mut len) };
        if !found.as_bool() || ptr.is_null() {
            return None;
        }
        Some((ptr as *const u8, len as usize))
    };
    let within_buf = |ptr: *const u8, bytes: usize| -> bool {
        let start = ptr as usize;
        start >= buf_range.start as usize
            && start
                .checked_add(bytes)
                .is_some_and(|end| end <= buf_range.end as usize)
    };

    // Idioma/codepage do primeiro bloco de strings (fallback: en-US Unicode).
    let translation = match query(w!("\\VarFileInfo\\Translation")) {
        Some((ptr, bytes)) if bytes >= 4 && within_buf(ptr, 4) => {
            match *unsafe { std::slice::from_raw_parts(ptr, 4) } {
                [l0, l1, c0, c1] => Some((u16::from_le_bytes([l0, l1]), u16::from_le_bytes([c0, c1]))),
                _ => None,
            }
        }
        _ => None,
    };
    let (lang, codepage) = translation.unwrap_or((0x0409, 0x04B0));

    let key: Vec<u16> = format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\ProductVersion")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let (ptr, chars) = query(PCWSTR(key.as_ptr()))?;
    let bytes = chars.checked_mul(2)?;
    if chars == 0 || !within_buf(ptr, bytes) || (ptr as usize) % 2 != 0 {
        return None;
    }
    let units = unsafe { std::slice::from_raw_parts(ptr as *const u16, chars) };
    Some(utf16_value_to_string(units)).filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------------
// Leitura de saves no disco (`DATA`) — fonte de identidade/assinatura
// ---------------------------------------------------------------------

/// Uma database t3db dentro de um buffer (um `DATA` lido do disco).
struct CareerDb {
    region_base: usize,
    /// Início da assinatura `DB\0\x08...` no buffer (diagnóstico/testes).
    #[cfg_attr(not(test), allow(dead_code))]
    offset_in_region: usize,
    tables: Vec<TableDescriptor>,
}

/// De onde vêm os bytes de um campo. Em produção é a memória do próprio
/// processo (`ReadProcessMemory` protegido — nunca deref de ponteiro
/// cru); para os saves é o `DATA` lido do disco.
trait ByteSource {
    fn read(&self, address: usize, len: usize) -> Option<Vec<u8>>;
}

struct ProcessMemory;

impl ByteSource for ProcessMemory {
    fn read(&self, address: usize, len: usize) -> Option<Vec<u8>> {
        memscan::read_region_bytes(&Region { base: address, size: len })
    }
}

/// Escrita de bytes na memória do processo (só o `transferbudget` usa).
trait ByteSink {
    fn write(&self, address: usize, bytes: &[u8]) -> bool;
}

impl ByteSink for ProcessMemory {
    fn write(&self, address: usize, bytes: &[u8]) -> bool {
        memscan::write_bytes_at(address, bytes)
    }
}

/// Buffer já em memória (um arquivo `DATA`), endereçado a partir de 0.
struct SliceSource<'a>(&'a [u8]);

impl ByteSource for SliceSource<'_> {
    fn read(&self, address: usize, len: usize) -> Option<Vec<u8>> {
        self.0.get(address..address.checked_add(len)?).map(<[u8]>::to_vec)
    }
}

/// É a database de carreira? No save, `GJUr`/`mPrV`/`dqXv` ficam numa
/// database própria (34 tabelas), SEPARADA da de jogadores (`CZUM`, 38
/// tabelas) — exigir as duas no mesmo blob nunca casaria.
fn is_career_db(tables: &[TableDescriptor]) -> bool {
    let has_row = |short: &[u8; 4]| {
        tables
            .iter()
            .any(|t| t.short_name == *short && t.written_record_count >= 1)
    };
    has_row(b"GJUr") && has_row(b"mPrV") && has_row(b"dqXv")
}

/// Primeira database de carreira dentro de um `DATA`.
fn find_career_db(data: &[u8]) -> Option<CareerDb> {
    memchr::memmem::find_iter(data, memscan::DB_SIGNATURE).find_map(|start| {
        let tables = fifa_db::parse_database_tables(data, start)?;
        is_career_db(&tables).then_some(CareerDb { region_base: 0, offset_in_region: start, tables })
    })
}

/// Os três valores de início de temporada de `dqXv`. Ficam constantes a
/// temporada inteira e, na struct viva do jogo, aparecem contíguos 20
/// bytes depois do `transferbudget` (sonda de 2026-09-30, confirmada no
/// Cheat Engine) — é por eles que achamos o orçamento vivo sem precisar
/// saber o valor atual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeasonSignature {
    pub start_wage_budget: i32,
    pub start_transfer_budget: i32,
    pub start_player_wages: i32,
}

impl SeasonSignature {
    fn to_bytes(self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&self.start_wage_budget.to_le_bytes());
        out[4..8].copy_from_slice(&self.start_transfer_budget.to_le_bytes());
        out[8..12].copy_from_slice(&self.start_player_wages.to_le_bytes());
        out
    }
}

/// Layout da struct viva de finanças (o `dqXv` decodificado em i32),
/// relativo ao endereço do `transferbudget`.
const LIVE_WAGE_BUDGET_OFFSET: usize = 4; // fica ANTES: addr - 4
const LIVE_SIGNATURE_OFFSET: usize = 20; // fica DEPOIS: addr + 20

/// Uma carreira lida de um `DATA` no disco.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedCareer {
    pub path: PathBuf,
    pub identity: CareerIdentity,
    pub saved_date: Date,
    pub saved_transfer_budget: i32,
    /// `GJUr.enddate` — fim da temporada (limite da validação da data viva).
    pub season_end: Date,
    pub signature: SeasonSignature,
}

fn read_i32_field(db: &CareerDb, src: &impl ByteSource, field: FieldRef) -> Result<i32, SaveRepoError> {
    let raw = read_int(db, src, field, 0)?;
    i32::try_from(raw).map_err(|_| {
        SaveRepoError::Interno(format!("{} fora de i32: {raw}", fifa_db::shortname_str(&field.field)))
    })
}

fn read_saved_career(data: &[u8], path: PathBuf) -> Result<SavedCareer, SaveRepoError> {
    let db = find_career_db(data).ok_or(SaveRepoError::CarreiraNaoCarregada)?;
    let src = SliceSource(data);
    Ok(SavedCareer {
        path,
        identity: identity_from(&db, &src)?,
        saved_date: current_date_from(&db, &src)?,
        saved_transfer_budget: transfer_budget_from(&db, &src)?,
        season_end: read_date(&db, &src, fields::GJUR_ENDDATE)?,
        signature: SeasonSignature {
            start_wage_budget: read_i32_field(&db, &src, fields::DQXV_START_WAGE_BUDGET)?,
            start_transfer_budget: read_i32_field(&db, &src, fields::DQXV_START_TRANSFER_BUDGET)?,
            start_player_wages: read_i32_field(&db, &src, fields::DQXV_START_PLAYER_WAGES)?,
        },
    })
}

/// `%USERPROFILE%\Documents\FIFA 16\<perfil>\FIFA16` (o `<perfil>` é `0`
/// na máquina do Felipe; aceitamos qualquer um). O processo do jogo roda
/// elevado, mas com o mesmo usuário — o `USERPROFILE` é o mesmo.
fn save_parent_dirs() -> Vec<PathBuf> {
    let Some(profile) = std::env::var_os("USERPROFILE") else {
        return Vec::new();
    };
    let root = PathBuf::from(profile).join("Documents").join("FIFA 16");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path().join("FIFA16"))
        .filter(|p| p.is_dir())
        .collect()
}

/// Saves abaixo de `parents` (`<pai>/<id>/DATA`), do mais recente para o
/// mais antigo. Ignora arquivos < 1 MB (saves parciais/de configuração,
/// mesmo critério de `fifa16_search.py`).
fn list_save_files(parents: &[PathBuf], limit: usize) -> Vec<PathBuf> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = parents
        .iter()
        .filter_map(|parent| std::fs::read_dir(parent).ok())
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            let data = entry.path().join("DATA");
            let meta = std::fs::metadata(&data).ok()?;
            (meta.len() >= 1_000_000).then_some((meta.modified().ok()?, data))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(limit).map(|(_, p)| p).collect()
}

/// Lê os saves e mantém UM por assinatura (o mais recente — a lista já
/// vem ordenada por data de modificação).
fn load_saved_careers(paths: &[PathBuf]) -> Vec<SavedCareer> {
    let mut careers: Vec<SavedCareer> = Vec::new();
    for path in paths {
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(err) => {
                tracing::warn!("[save_repo] Não li {}: {err}", path.display());
                continue;
            }
        };
        match read_saved_career(&data, path.clone()) {
            Ok(career) => {
                if !careers.iter().any(|c| c.signature == career.signature) {
                    careers.push(career);
                }
            }
            Err(err) => tracing::warn!("[save_repo] Save {} ignorado: {err}", path.display()),
        }
    }
    careers
}

// ---------------------------------------------------------------------
// Localização da struct viva (pesada — sempre via AsyncTask)
// ---------------------------------------------------------------------

/// Offsets (dentro de `bytes`) de cada `transferbudget` vivo cuja
/// assinatura bate. Só casamentos alinhados a 4 e com espaço para o
/// salário antes.
fn find_signature_in(bytes: &[u8], signature: SeasonSignature) -> Vec<usize> {
    let pattern = signature.to_bytes();
    memchr::memmem::find_iter(bytes, &pattern)
        .filter(|&pos| pos % 4 == 0)
        .filter_map(|pos| pos.checked_sub(LIVE_SIGNATURE_OFFSET))
        .filter(|&budget| budget >= LIVE_WAGE_BUDGET_OFFSET)
        .collect()
}

/// Uma struct de finanças achada na memória.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiveMatch {
    /// Índice em `careers`.
    career: usize,
    transfer_budget_addr: usize,
    /// Base da região (`VirtualQuery`) onde a struct está — a data viva
    /// fica num offset fixo a partir dela (`LIVE_DATE_REGION_OFFSETS`).
    region_base: usize,
}

/// Para cada região `(base, bytes)`, procura a assinatura de cada
/// carreira.
fn match_live_careers<'a>(
    regions: impl IntoIterator<Item = (usize, &'a [u8])>,
    careers: &[SavedCareer],
) -> Vec<LiveMatch> {
    let mut matches = Vec::new();
    for (base, bytes) in regions {
        for (career, saved) in careers.iter().enumerate() {
            for offset in find_signature_in(bytes, saved.signature) {
                matches.push(LiveMatch {
                    career,
                    transfer_budget_addr: base.saturating_add(offset),
                    region_base: base,
                });
            }
        }
    }
    matches
}

/// Carreira ativa: o save cuja assinatura está viva na memória, mais o
/// endereço do `transferbudget` vivo.
struct LiveCareer {
    transfer_budget_addr: usize,
    region_base: usize,
    save: SavedCareer,
    /// Última data viva vista; se a data VOLTAR, outro save foi carregado.
    last_date: Option<Date>,
}

static LIVE: OnceLock<Mutex<Option<LiveCareer>>> = OnceLock::new();

fn lock_cache() -> MutexGuard<'static, Option<LiveCareer>> {
    let mutex = LIVE.get_or_init(|| Mutex::new(None));
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Quantos saves recentes consideramos (cada `DATA` tem ~10 MB).
const MAX_SAVES_SCANNED: usize = 20;

// A DLL roda DENTRO do processo do jogo: a varredura também enxerga a
// memória da própria DLL. No primeiro teste (2026-09-30) ela "achou" a
// assinatura na própria lista de saves (`0x5529D4`, entradas a cada 120
// bytes) e escolheu essa cópia em vez da struct do jogo (`0x8CD9E74C`).
// Daí: (1) excluir as faixas de memória que são nossas; (2) zerar as
// cópias das assinaturas ao terminar, para a próxima varredura não
// achar restos de heap liberado.

/// Faixas de endereço que pertencem à própria varredura.
struct OwnMemory {
    buffer: std::ops::Range<usize>,
    others: Vec<std::ops::Range<usize>>,
}

fn range_of<T>(slice: &[T]) -> std::ops::Range<usize> {
    let start = slice.as_ptr() as usize;
    start..start.saturating_add(std::mem::size_of_val(slice))
}

impl OwnMemory {
    /// Lista de saves + buffer de leitura + pilha da thread atual.
    fn current(careers: &[SavedCareer], buffer: &[u8]) -> Self {
        let (mut low, mut high) = (0usize, 0usize);
        unsafe { windows::Win32::System::Threading::GetCurrentThreadStackLimits(&mut low, &mut high) };
        let mut others = vec![range_of(careers)];
        if low < high {
            others.push(low..high);
        }
        OwnMemory { buffer: range_of(buffer), others }
    }

    fn contains(&self, addr: usize) -> bool {
        self.buffer.contains(&addr) || self.others.iter().any(|r| r.contains(&addr))
    }

    /// A região `[base, base+size)` encosta no buffer de leitura? (Não
    /// lemos o buffer para dentro dele mesmo.)
    fn overlaps_buffer(&self, base: usize, size: usize) -> bool {
        base < self.buffer.end && base.saturating_add(size) > self.buffer.start
    }
}

/// Zera uma assinatura com escrita volátil (o compilador não pode
/// eliminar a escrita mesmo a memória sendo liberada logo depois).
fn scrub_signature(signature: &mut SeasonSignature) {
    unsafe { std::ptr::write_volatile(signature, SeasonSignature { start_wage_budget: 0, start_transfer_budget: 0, start_player_wages: 0 }) };
}

/// Lista de saves que zera as assinaturas ao sair de escopo (qualquer
/// caminho de retorno de `locate`).
struct ScrubbedCareers(Vec<SavedCareer>);

impl ScrubbedCareers {
    fn as_slice(&self) -> &[SavedCareer] {
        &self.0
    }
}

impl Drop for ScrubbedCareers {
    fn drop(&mut self) {
        for career in &mut self.0 {
            scrub_signature(&mut career.signature);
        }
    }
}

/// Buffer de leitura que é zerado antes de ser liberado.
struct ScrubbedBuffer(Vec<u8>);

impl Drop for ScrubbedBuffer {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&self.0);
    }
}

fn locate() -> Result<LiveCareer, SaveRepoError> {
    let observed = read_main_module_product_version();
    match evaluate_build(observed.as_deref(), EXPECTED_PRODUCT_VERSION) {
        BuildCheck::Verified(v) => {
            tracing::info!("[save_repo] Build do jogo verificada: {:?}.", v);
        }
        BuildCheck::Unverified(v) => {
            tracing::warn!(
                "[save_repo] Build do jogo NÃO verificada (ProductVersion observada: {:?}); seguindo sem bloquear.",
                v
            );
        }
        BuildCheck::Mismatch(v) => {
            tracing::warn!(
                "[save_repo] Build diferente da esperada (observada: {:?}); leitura recusada.",
                v
            );
            return Err(SaveRepoError::TabelaNaoEncontrada);
        }
    }

    let careers = ScrubbedCareers(load_saved_careers(&list_save_files(&save_parent_dirs(), MAX_SAVES_SCANNED)));
    let careers = careers.as_slice();
    for career in careers {
        tracing::info!(
            "[save_repo] Save candidato: {} | {} | assinatura {:?}",
            career.path.display(),
            career.identity.joined(),
            career.signature
        );
    }
    if careers.is_empty() {
        tracing::warn!("[save_repo] Nenhum save de carreira encontrado em Documents\\FIFA 16.");
        return Err(SaveRepoError::CarreiraNaoCarregada);
    }

    // Um único buffer de leitura, reaproveitado e zerado no fim: assim a
    // varredura não espalha cópias da memória do jogo pelo heap.
    let mut buffer = ScrubbedBuffer(vec![0u8; memscan::MAX_REGION_SIZE]);
    let own = OwnMemory::current(careers, &buffer.0);
    let regions = memscan::enumerate_private_committed_regions();

    // Caminho rápido: a struct de finanças mora na MESMA região (base do
    // `VirtualQuery`) que guarda a data viva em +0x373E08 (sessão 6).
    // Varrer só as regiões com data viva confirmada leva uma fração de
    // segundo em vez de ~15 s. Se não achar, cai na varredura completa.
    let quick = regions_with_live_date(&regions, &ProcessMemory);
    let mut found_any = false;
    if !quick.is_empty() {
        let inicio = std::time::Instant::now();
        let (chosen, found) = scan_and_choose(&quick, careers, &own, &mut buffer.0);
        found_any |= found;
        if let Some(live) = chosen {
            tracing::info!(
                "[save_repo] Localização rápida: {} região(ões) com data viva, {} ms.",
                quick.len(),
                inicio.elapsed().as_millis()
            );
            return Ok(live);
        }
        tracing::info!(
            "[save_repo] Localização rápida não achou a carreira em {} região(ões); varrendo a memória inteira.",
            quick.len()
        );
    }

    let (chosen, found) = scan_and_choose(&regions, careers, &own, &mut buffer.0);
    found_any |= found;
    drop(buffer);
    match chosen {
        Some(live) => Ok(live),
        None if found_any => {
            tracing::warn!(
                "[save_repo] Há structs de carreiras na memória, mas nenhuma com data viva confirmada \
                 (menu principal? são restos de carreiras carregadas antes)."
            );
            Err(SaveRepoError::CarreiraNaoCarregada)
        }
        None => {
            tracing::warn!(
                "[save_repo] Nenhuma assinatura de temporada dos saves está viva na memória \
                 (carreira não carregada, ou temporada virou depois do último save)."
            );
            Err(SaveRepoError::CarreiraNaoCarregada)
        }
    }
}

/// Procura as structs de finanças de `careers` em `regions` e escolhe a
/// carreira ativa (`choose_career`). Devolve também se ACHOU alguma
/// struct (mesmo sem data viva), para a mensagem de erro.
fn scan_and_choose(
    regions: &[Region],
    careers: &[SavedCareer],
    own: &OwnMemory,
    buffer: &mut [u8],
) -> (Option<LiveCareer>, bool) {
    let mut matches = Vec::new();
    for region in regions {
        if own.overlaps_buffer(region.base, region.size) {
            continue;
        }
        let Some(read) = memscan::read_region_into(region, buffer) else {
            continue;
        };
        let bytes = buffer.get(..read).unwrap_or_default();
        for found in match_live_careers([(region.base, bytes)], careers) {
            if own.contains(found.transfer_budget_addr) {
                tracing::info!(
                    "[save_repo] Ignorado 0x{:X}: memória da própria DLL.",
                    found.transfer_budget_addr
                );
            } else {
                matches.push(found);
            }
        }
    }
    for found in &matches {
        tracing::info!(
            "[save_repo] Struct viva candidata: 0x{:X} (região 0x{:X}) → {}",
            found.transfer_budget_addr,
            found.region_base,
            careers.get(found.career).map(|c| c.identity.joined()).unwrap_or_default()
        );
    }

    // `matches` em ordem de save (mais recente primeiro).
    matches.sort_by_key(|m| m.career);
    let candidates: Vec<LiveCareer> = matches
        .iter()
        .filter_map(|m| {
            Some(LiveCareer {
                transfer_budget_addr: m.transfer_budget_addr,
                region_base: m.region_base,
                last_date: None,
                save: careers.get(m.career)?.clone(),
            })
        })
        .collect();
    for live in &candidates {
        for offset in LIVE_DATE_REGION_OFFSETS {
            let value = live
                .region_base
                .checked_add(offset)
                .and_then(|a| ProcessMemory.read(a, 4))
                .and_then(|b| i32_at(&b, 0));
            tracing::info!(
                "[save_repo] Data viva? região 0x{:X}+0x{:X} = {:?} (candidata {})",
                live.region_base,
                offset,
                value,
                live.save.identity.joined()
            );
        }
    }

    let found_any = !candidates.is_empty();
    let chosen = choose_career(candidates, &ProcessMemory).map(|mut live| {
        live.last_date = live_date(&live, &ProcessMemory);
        tracing::info!(
            "[save_repo] Carreira ativa: {} (struct viva em 0x{:X}; data viva {:?}).",
            live.save.identity.joined(),
            live.transfer_budget_addr,
            live.last_date.map(|d| d.0)
        );
        live
    });
    (chosen, found_any)
}

/// Escolhe a carreira ativa entre as structs achadas (já em ordem de
/// save, mais recente primeiro): a primeira struct válida cuja temporada
/// contém a data viva confirmada (`live_date`).
///
/// Restos de carreiras carregadas ANTES continuam na memória (teste de
/// 2026-09-30: a struct de "teste" seguiu intacta em `0x8CD9E74C` depois
/// de carregar a carreira Careca). Sem nenhuma struct confirmada pela
/// data não escolhemos nada: mostrar o orçamento de OUTRA carreira é
/// pior do que dizer que não há carreira (o Scout gasta esse dinheiro).
fn choose_career(candidates: Vec<LiveCareer>, src: &impl ByteSource) -> Option<LiveCareer> {
    candidates
        .into_iter()
        .find(|live| live_finances(live, src).is_ok() && live_date(live, src).is_some())
}

/// Dispara a localização em background. `false` se já houver uma em
/// andamento. Ao terminar com sucesso, o cache fica preenchido e as
/// leituras síncronas passam a funcionar.
pub fn start_locating(task: &AsyncTask<()>) -> bool {
    task.start(|| {
        let live = locate()?;
        *lock_cache() = Some(live);
        // Scout nativo (Épico 7): só registra no log; falhar aqui nunca
        // atrapalha a localização da carreira.
        nativo::localizar_e_guardar();
        Ok(())
    })
}

// ---------------------------------------------------------------------
// Leituras síncronas (poucos bytes cada) sobre o cache
// ---------------------------------------------------------------------

/// `(transferbudget, wagebudget)` vivos, depois de conferir que a
/// assinatura de temporada continua no lugar (se o jogo liberou/moveu a
/// struct, ou a temporada virou, a leitura falha em vez de mentir).
fn live_finances(live: &LiveCareer, src: &impl ByteSource) -> Result<(i32, i32), SaveRepoError> {
    let start = live
        .transfer_budget_addr
        .checked_sub(LIVE_WAGE_BUDGET_OFFSET)
        .ok_or(SaveRepoError::NaoLocalizado)?;
    let len = LIVE_WAGE_BUDGET_OFFSET + LIVE_SIGNATURE_OFFSET + 12;
    let bytes = src.read(start, len).ok_or(SaveRepoError::ProcessoInacessivel)?;

    let sig_at = LIVE_WAGE_BUDGET_OFFSET + LIVE_SIGNATURE_OFFSET;
    if bytes.get(sig_at..sig_at + 12) != Some(&live.save.signature.to_bytes()[..]) {
        return Err(SaveRepoError::NaoLocalizado);
    }
    let wage = i32_at(&bytes, 0).ok_or(SaveRepoError::ProcessoInacessivel)?;
    let budget = i32_at(&bytes, LIVE_WAGE_BUDGET_OFFSET).ok_or(SaveRepoError::ProcessoInacessivel)?;
    if budget < 0 || wage < 0 {
        return Err(SaveRepoError::NaoLocalizado);
    }
    Ok((budget, wage))
}

/// Por que a struct em cache deixou de valer.
#[derive(Debug, PartialEq)]
enum LiveCheck {
    /// Struct sumiu/foi sobrescrita, ou outro save foi carregado: o cache
    /// tem de ser descartado e a carreira localizada de novo.
    Gone(SaveRepoError),
    /// Sem data viva confirmada (ex.: voltou ao menu principal). O cache
    /// fica: ao voltar para a mesma carreira as leituras voltam a valer.
    NoCareer,
}

fn check_live(live: &mut LiveCareer, src: &impl ByteSource) -> Result<(), LiveCheck> {
    live_finances(live, src).map_err(LiveCheck::Gone)?;
    let date = live_date(live, src).ok_or(LiveCheck::NoCareer)?;
    if let Some(previous) = live.last_date {
        if date < previous {
            tracing::warn!(
                "[save_repo] A data viva voltou ({} -> {}): outro save foi carregado; localize de novo.",
                previous.0,
                date.0
            );
            return Err(LiveCheck::Gone(SaveRepoError::NaoLocalizado));
        }
    }
    live.last_date = Some(date);
    Ok(())
}

/// Executa `f` sobre o cache depois de `check_live`: a struct de finanças
/// continua no lugar E há uma carreira carregada com data viva. Se a
/// struct sumiu ou a data voltou, o cache é descartado e o chamador deve
/// chamar `start_locating` de novo.
fn with_live<R>(f: impl FnOnce(&LiveCareer) -> Result<R, SaveRepoError>) -> Result<R, SaveRepoError> {
    let mut guard = lock_cache();
    let check = match guard.as_mut() {
        None => return Err(SaveRepoError::NaoLocalizado),
        Some(live) => check_live(live, &ProcessMemory),
    };
    match check {
        Ok(()) => match guard.as_ref() {
            Some(live) => f(live),
            None => Err(SaveRepoError::NaoLocalizado),
        },
        Err(LiveCheck::Gone(err)) => {
            *guard = None;
            nativo::esquecer();
            Err(err)
        }
        Err(LiveCheck::NoCareer) => Err(SaveRepoError::CarreiraNaoCarregada),
    }
}

/// Decodifica um campo inteiro bit-packed a partir dos poucos bytes já
/// lidos (`shift` = bits a descartar no primeiro byte).
fn decode_int_field(bytes: &[u8], shift: u32, depth: u32, range_low: i64) -> Option<i64> {
    if depth == 0 || depth > 32 {
        return None;
    }
    let raw = fifa_db::read_packed_int(bytes, shift, depth)?;
    Some(i64::from(raw) + range_low)
}

/// String fixa inline: corta no primeiro `\0`; bytes inválidos viram `�`
/// (mesma regra do `fifa16_db_parser`, `errors="replace"`).
fn decode_fixed_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(bytes.get(..end).unwrap_or_default()).into_owned()
}

/// Acha tabela + campo, confere o `storage_type` esperado e lê os bytes
/// do campo no registro `record_index`. Devolve `(bytes, shift, depth)`.
fn read_field_bytes(
    db: &CareerDb,
    src: &impl ByteSource,
    field: FieldRef,
    record_index: usize,
    expected_storage: u32,
) -> Result<(Vec<u8>, u32, u32), SaveRepoError> {
    let table = db
        .tables
        .iter()
        .find(|t| t.short_name == field.table)
        .ok_or(SaveRepoError::TabelaNaoEncontrada)?;
    let field_name =
        std::str::from_utf8(&field.field).map_err(|_| SaveRepoError::TabelaNaoEncontrada)?;
    let descriptor =
        fifa_db::field_by_shortname(table, field_name).ok_or(SaveRepoError::TabelaNaoEncontrada)?;

    if record_index >= table.written_record_count as usize {
        return Err(SaveRepoError::CarreiraNaoCarregada);
    }
    if descriptor.storage_type != expected_storage {
        return Err(SaveRepoError::Interno(format!(
            "campo {}.{} tem storage_type={} (esperado {})",
            fifa_db::shortname_str(&field.table),
            field_name,
            descriptor.storage_type,
            expected_storage
        )));
    }

    let location = fifa_db::locate_packed_field(table, descriptor, record_index);
    let abs = db
        .region_base
        .checked_add(location.byte_offset_in_region)
        .ok_or(SaveRepoError::ProcessoInacessivel)?;
    let bytes = src
        .read(abs, location.byte_count)
        .ok_or(SaveRepoError::ProcessoInacessivel)?;
    Ok((bytes, location.shift, descriptor.depth))
}

fn read_int(
    db: &CareerDb,
    src: &impl ByteSource,
    field: FieldRef,
    record_index: usize,
) -> Result<i64, SaveRepoError> {
    let (bytes, shift, depth) = read_field_bytes(db, src, field, record_index, INT_STORAGE_TYPE)?;
    decode_int_field(&bytes, shift, depth, field.range_low).ok_or(SaveRepoError::ProcessoInacessivel)
}

fn read_string(
    db: &CareerDb,
    src: &impl ByteSource,
    field: FieldRef,
    record_index: usize,
) -> Result<String, SaveRepoError> {
    let (bytes, shift, _) = read_field_bytes(db, src, field, record_index, STRING_STORAGE_TYPE)?;
    if shift != 0 {
        return Err(SaveRepoError::Interno(format!(
            "string {} não alinhada a byte",
            fifa_db::shortname_str(&field.field)
        )));
    }
    Ok(decode_fixed_string(&bytes))
}

fn read_date(db: &CareerDb, src: &impl ByteSource, field: FieldRef) -> Result<Date, SaveRepoError> {
    let raw = read_int(db, src, field, 0)?;
    let value =
        i32::try_from(raw).map_err(|_| SaveRepoError::Interno(format!("data fora de i32: {raw}")))?;
    let date = Date(value);
    if date.is_plausible() {
        Ok(date)
    } else {
        Err(SaveRepoError::Interno(format!("data implausível: {value}")))
    }
}

fn current_date_from(db: &CareerDb, src: &impl ByteSource) -> Result<Date, SaveRepoError> {
    read_date(db, src, fields::GJUR_CURRDATE)
}

fn transfer_budget_from(db: &CareerDb, src: &impl ByteSource) -> Result<i32, SaveRepoError> {
    read_i32_field(db, src, fields::DQXV_TRANSFERBUDGET)
}

/// Componentes da identidade do save (AD-11), lidos do primeiro registro
/// de `GJUr` e de `mPrV`.
#[derive(Debug, Clone, PartialEq)]
pub struct CareerIdentity {
    pub start_date: Date,
    pub first_name: String,
    pub surname: String,
    pub club_team_id: i64,
}

impl CareerIdentity {
    /// `startdate|firstname|surname|clubteamid`
    pub fn joined(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.start_date.0, self.first_name, self.surname, self.club_team_id
        )
    }

    /// Nome do arquivo de estado do Scout desta carreira (Story 1.3).
    pub fn hash(&self) -> String {
        hash_identity(&self.joined())
    }
}

fn identity_from(db: &CareerDb, src: &impl ByteSource) -> Result<CareerIdentity, SaveRepoError> {
    Ok(CareerIdentity {
        start_date: read_date(db, src, fields::GJUR_STARTDATE)?,
        first_name: read_string(db, src, fields::MPRV_FIRSTNAME, 0)?,
        surname: read_string(db, src, fields::MPRV_SURNAME, 0)?,
        club_team_id: read_int(db, src, fields::MPRV_CLUBTEAMID, 0)?,
    })
}

/// `dqXv.transferbudget` VIVO (lido da struct do jogo, não do save).
pub fn read_transfer_budget() -> Result<i32, SaveRepoError> {
    with_live(|live| live_finances(live, &ProcessMemory).map(|(budget, _)| budget))
}

/// Escreve o `dqXv.transferbudget` VIVO (Story 1.5), só quando o usuário
/// confirma. Desde 2026-10-06 (NFR1 emendado) o Scout também escreve a
/// lista de escolhidos e o conhecimento nativos (`save_repo::nativo`,
/// Épico 7), mas essa sincronização tem interruptor próprio e nunca
/// cobra nada.
///
/// Compare-and-write: só escreve se o valor vivo ainda for `anterior` (o
/// que o usuário viu no modal); senão devolve `OrcamentoMudou(atual)` sem
/// escrever. Depois de escrever, relê a struct (com a assinatura de
/// temporada) e devolve o valor relido — que tem de ser `novo`.
///
/// Sessão 4: o valor escrito nesse campo aparece no jogo ao trocar de
/// tela e vai para o arquivo no próximo save (o próprio jogo recalcula o
/// checksum).
pub fn write_transfer_budget(anterior: i32, novo: i32) -> Result<i32, SaveRepoError> {
    with_live(|live| write_budget_at(live, &ProcessMemory, anterior, novo))
}

fn write_budget_at(
    live: &LiveCareer,
    mem: &(impl ByteSource + ByteSink),
    anterior: i32,
    novo: i32,
) -> Result<i32, SaveRepoError> {
    if novo < 0 {
        return Err(SaveRepoError::Interno(format!("orçamento negativo recusado: {novo}")));
    }
    let (atual, _) = live_finances(live, mem)?;
    if atual != anterior {
        tracing::warn!("[save_repo] Orçamento mudou antes da escrita ({anterior} -> {atual}); nada escrito.");
        return Err(SaveRepoError::OrcamentoMudou(atual));
    }
    if !mem.write(live.transfer_budget_addr, &novo.to_le_bytes()) {
        tracing::warn!("[save_repo] Falha ao escrever o orçamento em 0x{:X}.", live.transfer_budget_addr);
        return Err(SaveRepoError::ProcessoInacessivel);
    }
    let (relido, _) = live_finances(live, mem)?;
    if relido != novo {
        tracing::warn!("[save_repo] Releitura do orçamento não confere: escrito {novo}, lido {relido}.");
        return Err(SaveRepoError::Interno(format!("releitura {relido} diferente do escrito {novo}")));
    }
    tracing::info!("[save_repo] Orçamento de transferências: {anterior} -> {relido} (0x{:X}).", live.transfer_budget_addr);
    Ok(relido)
}

/// `dqXv.wagebudget` VIVO (vizinho do orçamento na mesma struct): a folha
/// salarial semanal disponível — o limite de salário "do clube" da busca.
pub fn read_wage_budget() -> Result<i32, SaveRepoError> {
    with_live(|live| live_finances(live, &ProcessMemory).map(|(_, wage)| wage))
}

/// Offsets (bytes) a partir da BASE DA REGIÃO onde está a struct de
/// finanças até cópias vivas da data atual, em ordem de preferência.
///
/// Testes de 2026-09-30: o jogo aloca um bloco grande com layout interno
/// repetido entre sessões (a struct de "teste" sempre em região
/// +`0xEE74C`). A data viva estava em região +`0x373E08` (campo isolado
/// num objeto cheio de ponteiros — "data de hoje" do calendário), com
/// cópias em listas de eventos (+`0x1D2BD4`, +`0x1D3114`). Ao trocar para
/// a carreira Careca, a struct dela caiu em outro offset (+`0xEAD3C`),
/// mas a data continuou em região +`0x373E08` — a data NÃO acompanha a
/// struct, fica num ponto fixo do bloco.
///
/// Por ser empírico, toda leitura é validada (`live_date`) e há queda
/// para a data do último save.
const LIVE_DATE_REGION_OFFSETS: [usize; 3] = [0x373E08, 0x1D2BD4, 0x1D3114];

/// Janela de uma temporada: até 366 dias antes do `enddate`.
fn in_season(date: Date, season_end: Date) -> bool {
    date <= season_end && date.day_number() >= season_end.day_number() - 366
}

/// Data viva confirmada: vale o valor (válido e dentro da temporada do
/// save, `in_season`) em que pelo menos DUAS das três posições
/// concordam. Teste de 2026-09-30 no menu principal: a posição principal
/// guarda a data da última carreira (resto) e as duas listas de eventos
/// ficam zeradas; dentro de uma carreira duas ou três concordam (logo
/// após carregar: `20260630` em duas, lixo `20240731` na terceira).
///
/// Não exigimos "depois do último save" porque o jogador pode ter
/// carregado um save mais antigo da mesma temporada (teste: save Careca
/// de 23/set carregado, o mais recente era de 27/set).
///
/// Logo depois de carregar, o campo vivo fica no dia ANTERIOR ao do save
/// (`20260630` com a tela em 1/jul) até o jogo processar o primeiro dia;
/// se ele estiver exatamente um dia antes do save mais recente,
/// devolvemos a data do save, que é a que o jogador vê.
fn live_date(live: &LiveCareer, src: &impl ByteSource) -> Option<Date> {
    let agreed = agreed_date_at(live.region_base, src, |date| in_season(date, live.save.season_end))?;
    let saved = live.save.saved_date;
    if agreed.day_number() + 1 == saved.day_number() {
        Some(saved)
    } else {
        Some(agreed)
    }
}

/// Data em que pelo menos DUAS das três posições `LIVE_DATE_REGION_OFFSETS`
/// (a partir de `region_base`) concordam, entre os valores plausíveis
/// aceitos por `accept`.
fn agreed_date_at(region_base: usize, src: &impl ByteSource, accept: impl Fn(Date) -> bool) -> Option<Date> {
    let values: Vec<Date> = LIVE_DATE_REGION_OFFSETS
        .iter()
        .filter_map(|&offset| {
            let addr = region_base.checked_add(offset)?;
            let date = Date(i32_at(&src.read(addr, 4)?, 0)?);
            (date.is_plausible() && accept(date)).then_some(date)
        })
        .collect();
    values
        .iter()
        .copied()
        .find(|date| values.iter().filter(|other| *other == date).count() >= 2)
}

/// Regiões cuja base tem uma data viva confirmada (2 de 3 posições).
/// Sem olhar saves nem temporada: é o SINAL barato de que há uma carreira
/// carregada (no menu as duas listas de eventos ficam zeradas — sessão 6).
fn regions_with_live_date(regions: &[Region], src: &impl ByteSource) -> Vec<Region> {
    regions
        .iter()
        .copied()
        .filter(|region| agreed_date_at(region.base, src, |_| true).is_some())
        .collect()
}

/// Sinal barato (Story 1.7): "parece haver uma carreira carregada?".
/// Lista as regiões (`VirtualQuery`) e lê 3 × 4 bytes em cada uma —
/// dezenas de ms, mas ainda assim fora do render, num `AsyncTask` (AD-4).
/// `true` não garante a carreira (quem confirma é `start_locating`).
pub fn start_career_probe(task: &AsyncTask<bool>) -> bool {
    task.start(|| {
        let regions = memscan::enumerate_private_committed_regions();
        Ok(!regions_with_live_date(&regions, &ProcessMemory).is_empty())
    })
}

/// `GJUr.currdate` VIVO (ver `live_date`).
pub fn read_current_date() -> Result<Date, SaveRepoError> {
    with_live(|live| live_date(live, &ProcessMemory).ok_or(SaveRepoError::CarreiraNaoCarregada))
}

/// Componentes legíveis da identidade (para a tela de debug/log). Vêm do
/// save cuja assinatura de temporada está viva na memória — o save é
/// ESCOLHIDO pela memória do processo, não pelo `mtime` (AD-11).
pub fn read_career_identity() -> Result<CareerIdentity, SaveRepoError> {
    with_live(|live| Ok(live.save.identity.clone()))
}

/// Data (`GJUr.currdate`) gravada no save que o jogo carregou — o ponto
/// para onde o Scout "volta no tempo" se o jogador sair sem salvar e
/// carregar de novo. Não é a data viva: logo depois do load a data viva
/// ainda mostra o dia anterior (sessão 6). Trazido da branch
/// `claude/relatorio-ficha` (2026-10-01).
pub fn read_saved_date() -> Result<Date, SaveRepoError> {
    with_live(|live| Ok(live.save.saved_date))
}

/// SHA-256 (hex minúsculo) de `startdate|firstname|surname|clubteamid`
/// (AD-11) — nome do arquivo de estado do Scout para esta carreira.
#[allow(dead_code)] // o Scout usa `read_career_identity().hash()` (uma leitura só)
pub fn identify_active_save() -> Result<String, SaveRepoError> {
    read_career_identity().map(|identity| identity.hash())
}

/// i32 little-endian em `bytes[pos..pos+4]`, sem panic.
fn i32_at(bytes: &[u8], pos: usize) -> Option<i32> {
    let chunk: [u8; 4] = bytes.get(pos..pos.checked_add(4)?)?.try_into().ok()?;
    Some(i32::from_le_bytes(chunk))
}

/// SHA-256 em hex minúsculo (64 chars `[0-9a-f]`, sempre um nome de
/// arquivo válido no Windows, mesmo com acentos/reservados na entrada).
pub fn hash_identity(joined: &str) -> String {
    let digest = Sha256::digest(joined.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_table(short: &[u8; 4], records: u16) -> TableDescriptor {
        TableDescriptor {
            short_name: *short,
            offset_abs: 0,
            record_size: 0,
            compressed_string_length: 0,
            written_record_count: records,
            field_count: 0,
            fields: Vec::new(),
        }
    }

    #[test]
    fn sha256_known_vector() {
        // SHA-256("abc") — vetor de teste padrão
        assert_eq!(
            hash_identity("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hash_is_lowercase_hex_even_with_accents_and_reserved_chars() {
        let identity = CareerIdentity {
            start_date: Date(20350701),
            first_name: "José:/\\*?\"<>|".to_string(),
            surname: "Gonçalves ".to_string(),
            club_team_id: 241,
        };
        let hash = identity.hash();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    }

    #[test]
    fn hash_changes_with_any_component_and_is_deterministic() {
        let a = CareerIdentity {
            start_date: Date(20350717),
            first_name: "Felipe".to_string(),
            surname: "Careca".to_string(),
            club_team_id: 241,
        };
        let b = CareerIdentity { club_team_id: 73, ..a.clone() };
        assert_eq!(a.joined(), "20350717|Felipe|Careca|241");
        assert_eq!(a.hash(), a.clone().hash());
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn date_ordering_and_plausibility() {
        assert!(Date(20351102) > Date(20350930));
        assert!(Date(20351102).is_plausible());
        assert!(!Date(0).is_plausible());
        assert!(!Date(20351302).is_plausible());
        assert!(!Date(20350032).is_plausible());
        assert_eq!((Date(20351102).year(), Date(20351102).month(), Date(20351102).day()), (2035, 11, 2));
    }

    #[test]
    fn adding_days_crosses_months_years_and_leap_days() {
        assert_eq!(Date(20280924).mais_dias(18), Date(20281012));
        assert_eq!(Date(20281220).mais_dias(20), Date(20290109));
        assert_eq!(Date(20280220).mais_dias(10), Date(20280301), "2028 é bissexto");
        assert_eq!(Date(20270220).mais_dias(10), Date(20270302));
        assert_eq!(Date(20260701).mais_dias(0), Date(20260701));
        for data in [Date(20080101), Date(20351231), Date(20280229)] {
            assert_eq!(Date::from_day_number(data.day_number()), data);
        }
    }

    #[test]
    fn day_number_crosses_months_years_and_leap_days() {
        assert_eq!(Date(19700101).day_number(), 0);
        assert_eq!(Date(20260701).day_number() - Date(20260630).day_number(), 1);
        assert_eq!(Date(20270101).day_number() - Date(20261231).day_number(), 1);
        assert_eq!(Date(20280301).day_number() - Date(20280228).day_number(), 2); // 2028 é bissexto
        assert_eq!(Date(20270301).day_number() - Date(20270228).day_number(), 1);
        assert_eq!(Date(20270630).day_number() - Date(20260701).day_number(), 364);
    }

    #[test]
    fn decode_int_field_byte_aligned() {
        // 0x1234 little-endian, 16 bits, sem shift
        assert_eq!(decode_int_field(&[0x34, 0x12], 0, 16, 0), Some(0x1234));
    }

    #[test]
    fn decode_int_field_unaligned_with_range_low() {
        // valor 5 (3 bits) deslocado 2 bits dentro do byte: 0b0001_0100
        assert_eq!(decode_int_field(&[0b0001_0100], 2, 3, 0), Some(5));
        assert_eq!(decode_int_field(&[0b0001_0100], 2, 3, 1), Some(6));
        assert_eq!(decode_int_field(&[0b0001_0100], 2, 3, -1), Some(4));
    }

    #[test]
    fn decode_int_field_rejects_short_buffer_and_bad_depth() {
        assert_eq!(decode_int_field(&[0x01], 0, 16, 0), None);
        assert_eq!(decode_int_field(&[0u8; 32], 0, 256, 0), None);
        assert_eq!(decode_int_field(&[0x01], 0, 0, 0), None);
    }

    #[test]
    fn decode_fixed_string_cuts_at_nul_and_keeps_utf8() {
        let mut buf = [0u8; 32];
        buf[..6].copy_from_slice(b"Felipe");
        assert_eq!(decode_fixed_string(&buf), "Felipe");
        assert_eq!(decode_fixed_string("José".as_bytes()), "José");
        assert_eq!(decode_fixed_string(&[0u8; 4]), "");
        assert_eq!(decode_fixed_string(&[0xFF, b'a', 0]), "\u{FFFD}a");
    }

    #[test]
    fn career_db_requires_the_three_career_tables() {
        let career = [fake_table(b"GJUr", 1), fake_table(b"mPrV", 1), fake_table(b"dqXv", 1)];
        assert!(is_career_db(&career));
        assert!(!is_career_db(&[fake_table(b"GJUr", 0), fake_table(b"mPrV", 1), fake_table(b"dqXv", 1)]));
        assert!(!is_career_db(&[fake_table(b"GJUr", 1), fake_table(b"mPrV", 1)]));
        assert!(!is_career_db(&[fake_table(b"CZUM", 32_602)]));
        assert!(!is_career_db(&[]));
    }

    #[test]
    fn build_check_cases() {
        let v = "16.0.2904053";
        assert_eq!(evaluate_build(Some(v), Some(v)), BuildCheck::Verified(v.to_string()));
        assert_eq!(evaluate_build(Some("16.0.0"), Some(v)), BuildCheck::Mismatch("16.0.0".to_string()));
        assert_eq!(evaluate_build(Some(v), None), BuildCheck::Unverified(Some(v.to_string())));
        assert_eq!(evaluate_build(None, Some(v)), BuildCheck::Unverified(None));
        assert_eq!(evaluate_build(None, None), BuildCheck::Unverified(None));
        assert_eq!(EXPECTED_PRODUCT_VERSION, Some(v));
    }

    #[test]
    fn utf16_version_string_is_trimmed_at_nul() {
        let units: Vec<u16> = " 16.0.2904053 \0lixo".encode_utf16().collect();
        assert_eq!(utf16_value_to_string(&units), "16.0.2904053");
        assert_eq!(utf16_value_to_string(&[]), "");
    }

    /// Exercita o código `VerQueryValueW` contra o exe instalado (o
    /// mesmo arquivo que `GetModuleFileNameW` devolve dentro do jogo).
    /// Pula se o FIFA não estiver instalado nessa máquina.
    #[test]
    fn installed_fifa16_exe_reports_expected_product_version() {
        let exe = r"D:\Program Files\FIFA 16\fifa16.exe";
        if !std::path::Path::new(exe).exists() {
            eprintln!("FIFA 16 não instalado em {exe}; teste pulado");
            return;
        }
        let wide: Vec<u16> = exe.encode_utf16().chain(std::iter::once(0)).collect();
        assert_eq!(read_product_version(&wide).as_deref(), EXPECTED_PRODUCT_VERSION);
        // sem `\0` final => recusa, sem chamar a API
        assert_eq!(read_product_version(&wide[..wide.len() - 1]), None);
    }

    #[test]
    fn reads_before_locating_report_not_located() {
        // cache vazio => nenhum acesso à memória, erro tipado
        *lock_cache() = None;
        assert_eq!(read_current_date(), Err(SaveRepoError::NaoLocalizado));
        assert_eq!(read_transfer_budget(), Err(SaveRepoError::NaoLocalizado));
        assert_eq!(read_wage_budget(), Err(SaveRepoError::NaoLocalizado));
        assert_eq!(identify_active_save(), Err(SaveRepoError::NaoLocalizado));
    }

    // -----------------------------------------------------------------
    // Struct viva: assinatura de temporada (memória sintética)
    // -----------------------------------------------------------------

    /// Mesmo layout observado no jogo (carreira "teste", 2026-09-30):
    /// `[salário][orçamento][+4][+8][+12][+16][startwage][starttransfer][startplayerwages]`.
    fn live_struct(wage: i32, budget: i32, sig: SeasonSignature) -> Vec<u8> {
        let mut out = Vec::new();
        for v in [
            wage,
            budget,
            0,
            0,
            932,
            1,
            sig.start_wage_budget,
            sig.start_transfer_budget,
            sig.start_player_wages,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    const TESTE_SIG: SeasonSignature = SeasonSignature {
        start_wage_budget: 4_650_000,
        start_transfer_budget: 74_000_000,
        start_player_wages: 4_150_000,
    };

    fn saved(sig: SeasonSignature, club: i64) -> SavedCareer {
        SavedCareer {
            path: PathBuf::from("DATA"),
            identity: CareerIdentity {
                start_date: Date(20260717),
                first_name: "Senhor".to_string(),
                surname: "Manager".to_string(),
                club_team_id: club,
            },
            saved_date: Date(20260701),
            saved_transfer_budget: 67_000_000,
            season_end: Date(20270630),
            signature: sig,
        }
    }

    #[test]
    fn signature_locates_budget_20_bytes_before() {
        let mut mem = vec![0u8; 16];
        mem.extend(live_struct(692_307, 63_999_988, TESTE_SIG));
        // orçamento começa no byte 16 + 4 (depois do salário)
        assert_eq!(find_signature_in(&mem, TESTE_SIG), vec![20]);
    }

    #[test]
    fn signature_ignores_unaligned_and_too_close_to_start() {
        let mut unaligned = vec![0u8; 2];
        unaligned.extend(live_struct(1, 2, TESTE_SIG));
        assert!(find_signature_in(&unaligned, TESTE_SIG).is_empty());
        // assinatura no começo do buffer: não há espaço para orçamento/salário
        assert!(find_signature_in(&TESTE_SIG.to_bytes(), TESTE_SIG).is_empty());
    }

    #[test]
    fn match_live_careers_reports_career_index_and_absolute_address() {
        let other = SeasonSignature { start_wage_budget: 1, start_transfer_budget: 2, start_player_wages: 3 };
        let careers = [saved(other, 73), saved(TESTE_SIG, 243)];
        let region = live_struct(692_307, 63_999_988, TESTE_SIG);
        let matches = match_live_careers([(0x8CD9_E748usize, region.as_slice())], &careers);
        assert_eq!(
            matches,
            vec![LiveMatch { career: 1, transfer_budget_addr: 0x8CD9_E74C, region_base: 0x8CD9_E748 }]
        );
    }

    /// Memória fake (região na base 0) com um valor em cada posição de
    /// data, na ordem de `LIVE_DATE_REGION_OFFSETS`.
    fn memory_with_dates(values: [i32; 3]) -> Vec<u8> {
        let size = LIVE_DATE_REGION_OFFSETS.iter().max().copied().unwrap_or(0) + 4;
        let mut mem = vec![0u8; size];
        for (offset, value) in LIVE_DATE_REGION_OFFSETS.iter().zip(values) {
            mem[*offset..*offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        mem
    }

    fn teste_live() -> LiveCareer {
        // saved_date 20260701, season_end 20270630
        LiveCareer { transfer_budget_addr: 16, region_base: 0, last_date: None, save: saved(TESTE_SIG, 243) }
    }

    #[test]
    fn in_season_window_is_the_year_before_enddate() {
        let end = Date(20270630);
        assert!(in_season(Date(20270630), end));
        assert!(in_season(Date(20260701), end));
        assert!(in_season(Date(20260630), end));
        assert!(!in_season(Date(20270701), end));
        assert!(!in_season(Date(20280923), end));
        assert!(!in_season(Date(20250101), end));
    }

    #[test]
    fn career_signal_finds_only_regions_with_two_agreeing_dates() {
        // duas "regiões" lado a lado na mesma memória falsa
        let carreira = memory_with_dates([20_260_630, 20_240_731, 20_260_630]);
        let menu = memory_with_dates([20_280_923, 0, 0]);
        let mut mem = menu.clone();
        mem.extend_from_slice(&carreira);
        let regioes = [
            Region { base: 0, size: menu.len() },
            Region { base: menu.len(), size: carreira.len() },
            // região cuja base + offsets cai fora da memória: leitura falha
            Region { base: mem.len(), size: 16 },
        ];
        let achadas = regions_with_live_date(&regioes, &SliceSource(&mem));
        assert_eq!(achadas.iter().map(|r| r.base).collect::<Vec<_>>(), [menu.len()]);

        // menu principal sozinho: sem sinal
        assert!(regions_with_live_date(&regioes[..1], &SliceSource(&mem)).is_empty());
    }

    #[test]
    fn live_date_needs_two_positions_to_agree() {
        let live = teste_live();
        let read = |values| live_date(&live, &SliceSource(&memory_with_dates(values)));

        // dentro da carreira, dia já processado: as três concordam
        assert_eq!(read([20_260_708, 20_260_708, 20_260_708]), Some(Date(20_260_708)));
        // quaisquer duas bastam
        assert_eq!(read([0, 20_261_115, 20_261_115]), Some(Date(20_261_115)));
        // logo após carregar (log real): dia anterior em duas, lixo na outra
        assert_eq!(read([20_260_630, 20_240_731, 20_260_630]), Some(Date(20_260_701)));

        // menu principal (log real): só a principal, com resto de outra carreira
        assert_eq!(read([20_280_923, 0, 0]), None);
        // só uma posição com data válida da temporada
        assert_eq!(read([20_260_708, 0, -1]), None);
        // duas concordam, mas fora da temporada desta carreira
        assert_eq!(read([20_280_923, 20_280_923, 20_280_923]), None);
        // discordam entre si
        assert_eq!(read([20_260_708, 20_260_709, 20_260_710]), None);
        assert_eq!(live_date(&live, &SliceSource(&[0u8; 8])), None);
    }

    /// Regressões dos testes de 2026-09-30.
    #[test]
    fn choose_career_requires_a_live_date_in_the_season() {
        let careca_sig = SeasonSignature {
            start_wage_budget: 2_342_000,
            start_transfer_budget: 49_000_000,
            start_player_wages: 2_102_000,
        };
        let careca = SavedCareer {
            saved_date: Date(20280927),
            season_end: Date(20290630),
            ..saved(careca_sig, 234)
        };
        let (careca_at, teste_at) = (0xEAD3C, 0xEE74C);
        let memory = |dates: [i32; 3]| {
            let mut mem = memory_with_dates(dates);
            for (at, wage, budget, sig) in [
                (careca_at, 310_000, 45_955_973, careca_sig),
                (teste_at, 692_307, 63_999_988, TESTE_SIG),
            ] {
                let bytes = live_struct(wage, budget, sig);
                mem[at - 4..at - 4 + bytes.len()].copy_from_slice(&bytes);
            }
            mem
        };
        // "teste" vem primeiro (save mais recente)
        let candidates = || {
            vec![
                LiveCareer { transfer_budget_addr: teste_at, region_base: 0, last_date: None, save: saved(TESTE_SIG, 243) },
                LiveCareer { transfer_budget_addr: careca_at, region_base: 0, last_date: None, save: careca.clone() },
            ]
        };

        // Careca carregada: a data viva (23/set/2028) decide, mesmo com o
        // resto de "teste" na frente e o save mais recente da Careca em 27/set
        let mem = memory([20_280_923, 20_280_923, 20_280_923]);
        let chosen = choose_career(candidates(), &SliceSource(&mem)).expect("escolha");
        assert_eq!(chosen.transfer_budget_addr, careca_at);
        assert_eq!(live_finances(&chosen, &SliceSource(&mem)), Ok((45_955_973, 310_000)));
        assert_eq!(live_date(&chosen, &SliceSource(&mem)), Some(Date(20_280_923)));

        // menu principal: só a principal com resto => nenhuma carreira
        let mem = memory([20_280_923, 0, 0]);
        assert!(choose_career(candidates(), &SliceSource(&mem)).is_none());
        assert!(choose_career(Vec::new(), &SliceSource(&mem)).is_none());
    }

    #[test]
    fn check_live_detects_menu_and_reloaded_saves() {
        let mut mem = memory_with_dates([20_260_708; 3]);
        let bytes = live_struct(692_307, 63_999_988, TESTE_SIG);
        mem[12..12 + bytes.len()].copy_from_slice(&bytes);
        let mut live = teste_live();

        assert_eq!(check_live(&mut live, &SliceSource(&mem)), Ok(()));
        assert_eq!(live.last_date, Some(Date(20_260_708)));

        // avançou um dia: ok
        let mut later = mem.clone();
        for offset in LIVE_DATE_REGION_OFFSETS {
            later[offset..offset + 4].copy_from_slice(&20_260_709i32.to_le_bytes());
        }
        assert_eq!(check_live(&mut live, &SliceSource(&later)), Ok(()));

        // voltou ao menu: sem carreira, cache mantido (last_date intacto)
        let mut menu = later.clone();
        for offset in &LIVE_DATE_REGION_OFFSETS[1..] {
            menu[*offset..*offset + 4].copy_from_slice(&0i32.to_le_bytes());
        }
        assert_eq!(check_live(&mut live, &SliceSource(&menu)), Err(LiveCheck::NoCareer));
        assert_eq!(live.last_date, Some(Date(20_260_709)));

        // data voltou (save mais antigo carregado): cache tem de cair
        assert_eq!(
            check_live(&mut live, &SliceSource(&mem)),
            Err(LiveCheck::Gone(SaveRepoError::NaoLocalizado))
        );

        // struct de finanças sumiu
        let mut gone = later.clone();
        gone[12..12 + bytes.len()].fill(0);
        assert!(matches!(check_live(&mut teste_live(), &SliceSource(&gone)), Err(LiveCheck::Gone(_))));
    }

    #[test]
    fn own_memory_ranges_and_buffer_overlap() {
        let own = OwnMemory { buffer: 100..200, others: vec![300..400] };
        assert!(own.contains(150));
        assert!(own.contains(350));
        assert!(!own.contains(250));
        assert!(!own.contains(200));
        assert!(own.overlaps_buffer(50, 60));
        assert!(own.overlaps_buffer(150, 1000));
        assert!(!own.overlaps_buffer(200, 10));
        assert!(!own.overlaps_buffer(0, 100));
    }

    /// Regressão do teste de 2026-09-30: a assinatura guardada na lista
    /// de saves da DLL (e na pilha) tem de cair em `OwnMemory`.
    #[test]
    fn own_memory_covers_the_saves_list_the_stack_and_the_buffer() {
        let careers = vec![saved(TESTE_SIG, 243), saved(TESTE_SIG, 73)];
        let buffer = vec![0u8; 4096];
        let own = OwnMemory::current(&careers, &buffer);

        let sig_addr = |c: &SavedCareer| std::ptr::addr_of!(c.signature) as usize;
        assert!(careers.iter().all(|c| own.contains(sig_addr(c))));
        assert!(own.contains(buffer.as_ptr() as usize + 100));
        let on_stack = TESTE_SIG;
        assert!(own.contains(std::ptr::addr_of!(on_stack) as usize));

        // uma "struct do jogo" alocada à parte não é nossa
        let game = Box::new(live_struct(692_307, 63_999_988, TESTE_SIG));
        assert!(!own.contains(game.as_ptr() as usize));
    }

    #[test]
    fn scrub_signature_zeroes_all_fields() {
        let mut sig = TESTE_SIG;
        scrub_signature(&mut sig);
        assert_eq!(sig, SeasonSignature { start_wage_budget: 0, start_transfer_budget: 0, start_player_wages: 0 });
    }

    /// Memória falsa que aceita escrita (ou recusa, para testar a falha).
    struct MemoriaGravavel {
        bytes: std::cell::RefCell<Vec<u8>>,
        aceita: bool,
        /// Simula o jogo sobrescrevendo o campo logo depois da escrita.
        sobrescreve_com: Option<i32>,
    }

    impl ByteSource for MemoriaGravavel {
        fn read(&self, address: usize, len: usize) -> Option<Vec<u8>> {
            self.bytes.borrow().get(address..address.checked_add(len)?).map(<[u8]>::to_vec)
        }
    }

    impl ByteSink for MemoriaGravavel {
        fn write(&self, address: usize, bytes: &[u8]) -> bool {
            if !self.aceita {
                return false;
            }
            let mut mem = self.bytes.borrow_mut();
            let Some(alvo) = mem.get_mut(address..address + bytes.len()) else {
                return false;
            };
            alvo.copy_from_slice(bytes);
            if let Some(valor) = self.sobrescreve_com {
                alvo.copy_from_slice(&valor.to_le_bytes());
            }
            true
        }
    }

    fn memoria_com_orcamento(orcamento: i32) -> (MemoriaGravavel, LiveCareer) {
        let mut mem = vec![0u8; 8];
        mem.extend(live_struct(692_307, orcamento, TESTE_SIG));
        let live = LiveCareer { transfer_budget_addr: 12, region_base: 0, last_date: None, save: saved(TESTE_SIG, 243) };
        (MemoriaGravavel { bytes: std::cell::RefCell::new(mem), aceita: true, sobrescreve_com: None }, live)
    }

    #[test]
    fn budget_write_changes_only_the_transfer_budget_and_reads_it_back() {
        let (mem, live) = memoria_com_orcamento(63_999_988);
        let antes = mem.bytes.borrow().clone();
        assert_eq!(write_budget_at(&live, &mem, 63_999_988, 58_199_988), Ok(58_199_988));
        assert_eq!(live_finances(&live, &mem), Ok((58_199_988, 692_307)), "salário intacto");
        // só os 4 bytes do transferbudget mudaram (NFR1)
        let depois = mem.bytes.borrow().clone();
        let mudados: Vec<usize> = (0..antes.len()).filter(|&i| antes[i] != depois[i]).collect();
        assert!(mudados.iter().all(|&i| (12..16).contains(&i)), "{mudados:?}");
    }

    #[test]
    fn budget_write_refuses_when_the_live_value_moved_and_writes_nothing() {
        let (mem, live) = memoria_com_orcamento(60_000_000);
        let antes = mem.bytes.borrow().clone();
        assert_eq!(write_budget_at(&live, &mem, 63_999_988, 58_199_988), Err(SaveRepoError::OrcamentoMudou(60_000_000)));
        assert_eq!(*mem.bytes.borrow(), antes);
    }

    #[test]
    fn budget_write_failures_are_errors_never_success() {
        // escrita recusada pelo sistema
        let (mut mem, live) = memoria_com_orcamento(63_999_988);
        mem.aceita = false;
        assert_eq!(write_budget_at(&live, &mem, 63_999_988, 1), Err(SaveRepoError::ProcessoInacessivel));

        // o jogo sobrescreveu logo depois: a releitura não confere
        let (mut mem, live) = memoria_com_orcamento(63_999_988);
        mem.sobrescreve_com = Some(63_999_988);
        assert!(matches!(write_budget_at(&live, &mem, 63_999_988, 1), Err(SaveRepoError::Interno(_))));

        // valor negativo nunca é escrito
        let (mem, live) = memoria_com_orcamento(63_999_988);
        assert!(matches!(write_budget_at(&live, &mem, 63_999_988, -1), Err(SaveRepoError::Interno(_))));
        assert_eq!(live_finances(&live, &mem), Ok((63_999_988, 692_307)));
    }

    #[test]
    fn live_finances_reads_budget_and_wage_and_validates_signature() {
        let mut mem = vec![0u8; 8];
        mem.extend(live_struct(692_307, 63_999_988, TESTE_SIG));
        let live = LiveCareer { transfer_budget_addr: 12, region_base: 0, last_date: None, save: saved(TESTE_SIG, 243) };
        assert_eq!(live_finances(&live, &SliceSource(&mem)), Ok((63_999_988, 692_307)));

        // struct liberada/sobrescrita: a assinatura não confere mais
        let other = LiveCareer {
            transfer_budget_addr: 12,
            region_base: 0,
            last_date: None,
            save: saved(SeasonSignature { start_player_wages: 1, ..TESTE_SIG }, 243),
        };
        assert_eq!(live_finances(&other, &SliceSource(&mem)), Err(SaveRepoError::NaoLocalizado));

        // orçamento negativo => não é a struct certa
        let mut negative = vec![0u8; 8];
        negative.extend(live_struct(10, -5, TESTE_SIG));
        assert_eq!(live_finances(&live, &SliceSource(&negative)), Err(SaveRepoError::NaoLocalizado));

        // memória curta => ProcessoInacessivel, sem panic
        assert_eq!(
            live_finances(&live, &SliceSource(mem.get(..20).unwrap_or_default())),
            Err(SaveRepoError::ProcessoInacessivel)
        );
        let at_zero = LiveCareer { transfer_budget_addr: 0, region_base: 0, last_date: None, save: saved(TESTE_SIG, 243) };
        assert_eq!(live_finances(&at_zero, &SliceSource(&mem)), Err(SaveRepoError::NaoLocalizado));
    }

    // -----------------------------------------------------------------
    // Oráculo com saves reais (`save_backups/`, versionados no repo).
    // Valores esperados conferidos com `fifa16_db_parser.py`.
    // -----------------------------------------------------------------

    fn backups_dir() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("save_backups")
    }

    fn load_backup(name: &str) -> (Vec<u8>, PathBuf) {
        let path = backups_dir().join(name).join("DATA");
        let data = std::fs::read(&path).unwrap_or_else(|e| panic!("não abriu {}: {e}", path.display()));
        (data, path)
    }

    struct Expected {
        backup: &'static str,
        currdate: i32,
        budget: i32,
        identity: &'static str,
        signature: SeasonSignature,
    }

    const SIG_241: SeasonSignature = SeasonSignature {
        start_wage_budget: 5_007_001,
        start_transfer_budget: 85_500_000,
        start_player_wages: 4_607_001,
    };

    const BACKUPS: [Expected; 3] = [
        Expected {
            backup: "717036e3_20260913_172921",
            currdate: 20351102,
            budget: 125_725_739,
            identity: "20350717|Felipe|Careca|241",
            signature: SIG_241,
        },
        Expected {
            backup: "705c22c4_backup_20260913_202407",
            currdate: 20351206,
            budget: 128_832_820,
            identity: "20350717|Felipe|Careca|241",
            signature: SIG_241,
        },
        Expected {
            backup: "7a096416_20260913_175525",
            currdate: 20350716,
            budget: 166_870_000,
            identity: "20350721|Felipe|Careca|73",
            signature: SeasonSignature {
                start_wage_budget: 6_554_000,
                start_transfer_budget: 160_000_000,
                start_player_wages: 5_779_000,
            },
        },
    ];

    #[test]
    fn real_saves_read_date_budget_identity_and_signature() {
        for exp in &BACKUPS {
            let (data, path) = load_backup(exp.backup);
            let career = read_saved_career(&data, path).expect("save legível");
            assert_eq!(career.saved_date, Date(exp.currdate), "{}", exp.backup);
            assert_eq!(career.saved_transfer_budget, exp.budget, "{}", exp.backup);
            assert_eq!(career.identity.joined(), exp.identity, "{}", exp.backup);
            assert_eq!(career.signature, exp.signature, "{}", exp.backup);
            assert_eq!(career.season_end, Date(20360630), "{}", exp.backup);
        }
    }

    /// AC #3 offline: dois saves da MESMA carreira (dias diferentes) dão
    /// o mesmo hash; uma carreira diferente dá outro.
    #[test]
    fn real_saves_same_career_same_hash_other_career_differs() {
        let hash_of = |backup: &str| {
            let (data, path) = load_backup(backup);
            read_saved_career(&data, path).expect("save legível").identity.hash()
        };
        let a1 = hash_of(BACKUPS[0].backup);
        let a2 = hash_of(BACKUPS[1].backup);
        let b = hash_of(BACKUPS[2].backup);
        assert_eq!(a1, a2);
        assert_ne!(a1, b);
    }

    #[test]
    fn listing_and_loading_saves_dedupes_by_signature() {
        let files = list_save_files(&[backups_dir()], 20);
        assert_eq!(files.len(), 3);
        assert!(files.iter().all(|f| f.ends_with("DATA")));
        // 717036e3 e 705c22c4 são a mesma carreira/temporada
        let careers = load_saved_careers(&files);
        assert_eq!(careers.len(), 2);
        assert!(list_save_files(&[backups_dir()], 1).len() == 1);
        assert!(list_save_files(&[backups_dir().join("nao_existe")], 20).is_empty());
    }

    #[test]
    fn wrong_storage_type_is_a_typed_error_not_a_panic() {
        let (data, _) = load_backup(BACKUPS[0].backup);
        let db = find_career_db(&data).expect("database de carreira");
        let src = SliceSource(&data);
        // firstname é string: lê-lo como inteiro tem de falhar com erro
        assert!(matches!(
            read_int(&db, &src, fields::MPRV_FIRSTNAME, 0),
            Err(SaveRepoError::Interno(_))
        ));
        // registro inexistente
        assert_eq!(
            read_int(&db, &src, fields::GJUR_CURRDATE, 5),
            Err(SaveRepoError::CarreiraNaoCarregada)
        );
        // buffer truncado => ProcessoInacessivel, sem panic
        let short = SliceSource(data.get(..db.offset_in_region + 64).unwrap_or_default());
        assert_eq!(current_date_from(&db, &short), Err(SaveRepoError::ProcessoInacessivel));
        // arquivo sem database de carreira
        assert_eq!(
            read_saved_career(&[0u8; 64], PathBuf::from("x")),
            Err(SaveRepoError::CarreiraNaoCarregada)
        );
    }
}
