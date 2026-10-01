//! `save_repo` — ÚNICA porta para `memscan`/`fifa_db`/`pointer_scan` (AD-2).
//!
//! Expõe o estado da carreira no vocabulário de domínio (data atual,
//! orçamento de transferência, identidade do save) e esconde offsets,
//! tabelas e bit-packing. Nada fora deste módulo (telas, `scout::*`)
//! pode chamar `memscan`/`fifa_db` diretamente.
//!
//! ## Como funciona
//! 1. `start_locating` roda num `AsyncTask` (varredura de memória inteira,
//!    ~17s, NUNCA no render thread — AD-4) e acha o blob de database da
//!    carreira no heap. Só guardamos `região + offset + tabelas parseadas`,
//!    não os ~64MB de bytes.
//! 2. Depois de localizado, cada leitura de campo é uma leitura mínima
//!    (alguns bytes) via `ReadProcessMemory` protegido, sempre
//!    revalidando a assinatura `DB\0\x08...` no endereço em cache (o heap
//!    do jogo pode ser liberado/movido, p.ex. ao recarregar a carreira).
//!
//! ## ATENÇÃO — frescor dos dados (Story 1.1, Task 1)
//! O blob no heap é, pelo que sabemos (`PROJECT_MEMORY.md`), um snapshot
//! lido uma vez: pode NÃO acompanhar `currdate`/`transferbudget` ao vivo.
//! Campos estáticos (`startdate`, manager, `clubteamid`) não têm esse
//! problema. A estratégia definitiva para data/orçamento depende do
//! teste de frescor descrito na story e ainda NÃO foi validada.

use std::fmt;
use std::sync::{Mutex, MutexGuard, OnceLock};

use sha2::{Digest, Sha256};

use crate::async_task::AsyncTask;
use crate::fifa_db::{self, TableDescriptor};
use crate::memscan::{self, Region};

/// Short names (4 chars) das tabelas/campos que o `save_repo` lê.
///
/// Os short names de TABELA abaixo já aparecem documentados em
/// `PROJECT_MEMORY.md`. Os de CAMPO precisam ser extraídos do
/// `fifa_ng_db-meta.xml` rodando `python tools/resolve_short_names.py`
/// (Story 1.1, Task 1.1) — enquanto valerem `????`, leituras devolvem
/// `TabelaNaoEncontrada` (falha segura, nunca um valor errado).
pub mod fields {
    #[derive(Debug, Clone, Copy)]
    pub struct FieldRef {
        pub table: [u8; 4],
        pub field: [u8; 4],
        /// `rangelow` do metadata XML, somado ao valor cru do campo.
        pub range_low: i64,
    }

    pub const UNRESOLVED: [u8; 4] = *b"????";

    pub const GJUR_CURRDATE: FieldRef = FieldRef { table: *b"GJUr", field: UNRESOLVED, range_low: 0 };
    pub const GJUR_STARTDATE: FieldRef = FieldRef { table: *b"GJUr", field: UNRESOLVED, range_low: 0 };
    pub const MPRV_FIRSTNAME: FieldRef = FieldRef { table: *b"mPrV", field: UNRESOLVED, range_low: 0 };
    pub const MPRV_SURNAME: FieldRef = FieldRef { table: *b"mPrV", field: UNRESOLVED, range_low: 0 };
    pub const MPRV_CLUBTEAMID: FieldRef = FieldRef { table: *b"mPrV", field: UNRESOLVED, range_low: 0 };
    pub const DQXV_TRANSFERBUDGET: FieldRef = FieldRef { table: *b"dqXv", field: UNRESOLVED, range_low: 0 };

    pub const ALL: [FieldRef; 6] = [
        GJUR_CURRDATE,
        GJUR_STARTDATE,
        MPRV_FIRSTNAME,
        MPRV_SURNAME,
        MPRV_CLUBTEAMID,
        DQXV_TRANSFERBUDGET,
    ];
}

use fields::FieldRef;

/// `FileVersion` esperado do `fifa16.exe` (build `16.0.2904053`). `None`
/// até alguém ler o valor real numa máquina com o jogo (o log da
/// localização imprime a versão observada) e preencher aqui.
const EXPECTED_FILE_VERSION: Option<[u16; 4]> = None;

/// `storage_type` dos campos inteiros em `fifa_db.rs` (doc do módulo).
const INT_STORAGE_TYPE: u32 = 3;

/// Uma database de carreira completa tem ~32k jogadores em `CZUM`;
/// qualquer coisa muito abaixo disso é outro blob (assinatura avulsa).
const MIN_PLAYER_RECORDS: u16 = 10_000;

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
            SaveRepoError::Interno(msg) => write!(f, "Erro interno: {msg}"),
        }
    }
}

/// Data no formato cru `YYYYMMDD` (mesma forma de `GJUr.currdate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
}

// ---------------------------------------------------------------------
// Verificação da build do jogo (NFR4)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum BuildCheck {
    Verified([u16; 4]),
    /// Versão não legível ou esperada ainda não configurada: só logamos.
    Unverified(Option<[u16; 4]>),
    Mismatch([u16; 4]),
}

pub fn evaluate_build(observed: Option<[u16; 4]>, expected: Option<[u16; 4]>) -> BuildCheck {
    match (observed, expected) {
        (Some(o), Some(e)) if o == e => BuildCheck::Verified(o),
        (Some(o), Some(_)) => BuildCheck::Mismatch(o),
        (o, _) => BuildCheck::Unverified(o),
    }
}

/// Lê a FileVersion do executável principal (o `fifa16.exe`, já que a
/// DLL roda dentro do processo do jogo). `None` se o recurso de versão
/// não existir/for ilegível (o exe é protegido por packer).
fn read_main_module_file_version() -> Option<[u16; 4]> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

    let mut path = [0u16; 520];
    let len = unsafe { GetModuleFileNameW(None, &mut path) } as usize;
    if len == 0 || len >= path.len() {
        return None;
    }
    let path_ptr = PCWSTR(path.as_ptr());

    let size = unsafe { GetFileVersionInfoSizeW(path_ptr, None) };
    if size == 0 {
        return None;
    }

    let mut buf = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(path_ptr, None, size, buf.as_mut_ptr() as *mut _) }.ok()?;

    let mut info_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut info_len: u32 = 0;
    let found = unsafe {
        VerQueryValueW(buf.as_ptr() as *const _, w!("\\"), &mut info_ptr, &mut info_len)
    };
    if !found.as_bool()
        || info_ptr.is_null()
        || (info_len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
    {
        return None;
    }

    let info = unsafe { &*(info_ptr as *const VS_FIXEDFILEINFO) };
    Some([
        (info.dwFileVersionMS >> 16) as u16,
        (info.dwFileVersionMS & 0xFFFF) as u16,
        (info.dwFileVersionLS >> 16) as u16,
        (info.dwFileVersionLS & 0xFFFF) as u16,
    ])
}

// ---------------------------------------------------------------------
// Localização da database da carreira (pesada — sempre via AsyncTask)
// ---------------------------------------------------------------------

struct CareerDb {
    region_base: usize,
    offset_in_region: usize,
    tables: Vec<TableDescriptor>,
}

static CAREER: OnceLock<Mutex<Option<CareerDb>>> = OnceLock::new();

fn lock_cache() -> MutexGuard<'static, Option<CareerDb>> {
    let mutex = CAREER.get_or_init(|| Mutex::new(None));
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// É o blob de uma carreira carregada? (`GJUr` com registro + `CZUM`
/// com a base completa de jogadores.)
fn is_career_db(tables: &[TableDescriptor]) -> bool {
    let has_career_row = tables
        .iter()
        .any(|t| t.short_name == *b"GJUr" && t.written_record_count >= 1);
    let has_full_players = tables
        .iter()
        .any(|t| t.short_name == *b"CZUM" && t.written_record_count >= MIN_PLAYER_RECORDS);
    has_career_row && has_full_players
}

fn locate() -> Result<CareerDb, SaveRepoError> {
    let observed = read_main_module_file_version();
    match evaluate_build(observed, EXPECTED_FILE_VERSION) {
        BuildCheck::Verified(v) => {
            tracing::info!("[save_repo] Build do jogo verificada: {:?}.", v);
        }
        BuildCheck::Unverified(v) => {
            tracing::warn!(
                "[save_repo] Build do jogo NÃO verificada (observada: {:?}; esperada ainda não configurada).",
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

    for loc in memscan::find_databases_in_memory() {
        let Some(tables) = fifa_db::parse_database_tables(&loc.region_bytes, loc.offset_in_region)
        else {
            continue;
        };
        if is_career_db(&tables) {
            tracing::info!(
                "[save_repo] Database da carreira localizada (região 0x{:X}, {} tabelas).",
                loc.region_base,
                tables.len()
            );
            return Ok(CareerDb {
                region_base: loc.region_base,
                offset_in_region: loc.offset_in_region,
                tables,
            });
        }
    }

    Err(SaveRepoError::CarreiraNaoCarregada)
}

/// Dispara a localização em background. `false` se já houver uma em
/// andamento. Ao terminar com sucesso, o cache fica preenchido e as
/// leituras síncronas passam a funcionar.
pub fn start_locating(task: &AsyncTask<()>) -> bool {
    task.start(|| {
        let db = locate()?;
        *lock_cache() = Some(db);
        Ok(())
    })
}

// ---------------------------------------------------------------------
// Leituras síncronas (poucos bytes cada) sobre o cache
// ---------------------------------------------------------------------

fn signature_status(db: &CareerDb) -> Result<(), SaveRepoError> {
    let region = Region {
        base: db.region_base.saturating_add(db.offset_in_region),
        size: memscan::DB_SIGNATURE.len(),
    };
    let bytes = memscan::read_region_bytes(&region).ok_or(SaveRepoError::ProcessoInacessivel)?;
    if bytes.get(..memscan::DB_SIGNATURE.len()) == Some(memscan::DB_SIGNATURE) {
        Ok(())
    } else {
        Err(SaveRepoError::NaoLocalizado)
    }
}

/// Executa `f` sobre o cache, revalidando antes a assinatura. Se o
/// endereço em cache ficou inválido, o cache é descartado e o chamador
/// deve chamar `start_locating` de novo.
fn with_db<R>(f: impl FnOnce(&CareerDb) -> Result<R, SaveRepoError>) -> Result<R, SaveRepoError> {
    let mut guard = lock_cache();

    let status = match guard.as_ref() {
        None => Err(SaveRepoError::NaoLocalizado),
        Some(db) => signature_status(db),
    };
    if let Err(err) = status {
        *guard = None;
        return Err(err);
    }

    match guard.as_ref() {
        Some(db) => f(db),
        None => Err(SaveRepoError::NaoLocalizado),
    }
}

/// Decodifica um campo inteiro bit-packed a partir dos poucos bytes já
/// lidos (`shift` = bits a descartar no primeiro byte).
fn decode_int_field(bytes: &[u8], shift: u32, depth: u32, range_low: i64) -> Option<i64> {
    let raw = fifa_db::read_packed_int(bytes, shift, depth)?;
    Some(i64::from(raw) + range_low)
}

fn read_raw(db: &CareerDb, field: FieldRef, record_index: usize) -> Result<i64, SaveRepoError> {
    if field.field == fields::UNRESOLVED {
        tracing::warn!(
            "[save_repo] Campo da tabela {} ainda sem short name (rode tools/resolve_short_names.py).",
            fifa_db::shortname_str(&field.table)
        );
        return Err(SaveRepoError::TabelaNaoEncontrada);
    }

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
    if descriptor.storage_type != INT_STORAGE_TYPE {
        return Err(SaveRepoError::Interno(format!(
            "campo {}.{} não é inteiro (storage_type={}); ver Story 1.1 Task 1.4",
            fifa_db::shortname_str(&field.table),
            field_name,
            descriptor.storage_type
        )));
    }

    let location = fifa_db::locate_packed_field(table, descriptor, record_index);
    let abs = db
        .region_base
        .checked_add(location.byte_offset_in_region)
        .ok_or(SaveRepoError::ProcessoInacessivel)?;
    let bytes = memscan::read_region_bytes(&Region { base: abs, size: location.byte_count })
        .ok_or(SaveRepoError::ProcessoInacessivel)?;

    decode_int_field(&bytes, location.shift, descriptor.depth, field.range_low)
        .ok_or(SaveRepoError::ProcessoInacessivel)
}

/// `GJUr.currdate` (registro único da tabela de carreira).
pub fn read_current_date() -> Result<Date, SaveRepoError> {
    with_db(|db| {
        let raw = read_raw(db, fields::GJUR_CURRDATE, 0)?;
        let value = i32::try_from(raw)
            .map_err(|_| SaveRepoError::Interno(format!("currdate fora de i32: {raw}")))?;
        let date = Date(value);
        if date.is_plausible() {
            Ok(date)
        } else {
            Err(SaveRepoError::Interno(format!("currdate implausível: {value}")))
        }
    })
}

/// `dqXv.transferbudget` (registro único).
pub fn read_transfer_budget() -> Result<i32, SaveRepoError> {
    with_db(|db| {
        let raw = read_raw(db, fields::DQXV_TRANSFERBUDGET, 0)?;
        i32::try_from(raw)
            .map_err(|_| SaveRepoError::Interno(format!("transferbudget fora de i32: {raw}")))
    })
}

/// SHA-256 (hex minúsculo) de `startdate|firstname|surname|clubteamid`
/// (AD-11). Usa o primeiro registro de `GJUr` e de `mPrV`.
///
/// Se `firstname`/`surname` forem strings (Huffman) em vez de inteiros,
/// a leitura falha com `Interno(..)` explicando — decisão pendente da
/// Story 1.1, Task 1.4.
pub fn identify_active_save() -> Result<String, SaveRepoError> {
    with_db(|db| {
        let parts = [
            read_raw(db, fields::GJUR_STARTDATE, 0)?.to_string(),
            read_raw(db, fields::MPRV_FIRSTNAME, 0)?.to_string(),
            read_raw(db, fields::MPRV_SURNAME, 0)?.to_string(),
            read_raw(db, fields::MPRV_CLUBTEAMID, 0)?.to_string(),
        ];
        Ok(hash_identity(&parts))
    })
}

/// Junta as partes com `|`, aplica SHA-256 e devolve hex minúsculo
/// (64 chars `[0-9a-f]`, sempre um nome de arquivo válido no Windows).
pub fn hash_identity(parts: &[String]) -> String {
    let joined = parts.join("|");
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
            written_record_count: records,
            field_count: 0,
            fields: Vec::new(),
        }
    }

    #[test]
    fn sha256_known_vector() {
        // SHA-256("abc") — vetor de teste padrão
        let parts = ["abc".to_string()];
        assert_eq!(
            hash_identity(&parts),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hash_is_lowercase_hex_even_with_accents_and_reserved_chars() {
        let parts = [
            "20350701".to_string(),
            "José:/\\*?\"<>|".to_string(),
            "Gonçalves ".to_string(),
            "241".to_string(),
        ];
        let hash = hash_identity(&parts);
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    }

    #[test]
    fn hash_changes_with_any_component_and_is_deterministic() {
        let a = ["1".to_string(), "2".to_string(), "3".to_string(), "4".to_string()];
        let b = ["1".to_string(), "2".to_string(), "3".to_string(), "5".to_string()];
        assert_eq!(hash_identity(&a), hash_identity(&a));
        assert_ne!(hash_identity(&a), hash_identity(&b));
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
    fn decode_int_field_byte_aligned() {
        // 0x1234 little-endian, 16 bits, sem shift
        assert_eq!(decode_int_field(&[0x34, 0x12], 0, 16, 0), Some(0x1234));
    }

    #[test]
    fn decode_int_field_unaligned_with_range_low() {
        // valor 5 (3 bits) deslocado 2 bits dentro do byte: 0b0001_0100
        assert_eq!(decode_int_field(&[0b0001_0100], 2, 3, 0), Some(5));
        assert_eq!(decode_int_field(&[0b0001_0100], 2, 3, 1), Some(6));
    }

    #[test]
    fn decode_int_field_returns_none_when_buffer_too_short() {
        assert_eq!(decode_int_field(&[0x01], 0, 16, 0), None);
    }

    #[test]
    fn career_db_requires_career_row_and_full_player_table() {
        assert!(is_career_db(&[fake_table(b"GJUr", 1), fake_table(b"CZUM", 32_602)]));
        assert!(!is_career_db(&[fake_table(b"GJUr", 0), fake_table(b"CZUM", 32_602)]));
        assert!(!is_career_db(&[fake_table(b"GJUr", 1), fake_table(b"CZUM", 50)]));
        assert!(!is_career_db(&[fake_table(b"CZUM", 32_602)]));
        assert!(!is_career_db(&[]));
    }

    #[test]
    fn build_check_cases() {
        let v = [16, 0, 2904, 53];
        assert_eq!(evaluate_build(Some(v), Some(v)), BuildCheck::Verified(v));
        assert_eq!(evaluate_build(Some(v), Some([1, 0, 0, 0])), BuildCheck::Mismatch(v));
        assert_eq!(evaluate_build(Some(v), None), BuildCheck::Unverified(Some(v)));
        assert_eq!(evaluate_build(None, Some(v)), BuildCheck::Unverified(None));
        assert_eq!(evaluate_build(None, None), BuildCheck::Unverified(None));
    }

    #[test]
    fn reads_before_locating_report_not_located() {
        // cache vazio => nenhum acesso à memória, erro tipado
        *lock_cache() = None;
        assert_eq!(read_current_date(), Err(SaveRepoError::NaoLocalizado));
        assert_eq!(read_transfer_budget(), Err(SaveRepoError::NaoLocalizado));
        assert_eq!(identify_active_save(), Err(SaveRepoError::NaoLocalizado));
    }

    /// Fica VERMELHO até a Task 1.1 da story: enquanto algum campo valer
    /// `????`, a leitura real do save não pode funcionar.
    #[test]
    fn all_field_short_names_are_resolved() {
        for field in fields::ALL {
            assert_ne!(
                field.field,
                fields::UNRESOLVED,
                "short name pendente na tabela {}: rode tools/resolve_short_names.py",
                fifa_db::shortname_str(&field.table)
            );
        }
    }
}
