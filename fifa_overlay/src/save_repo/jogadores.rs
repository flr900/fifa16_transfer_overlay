//! Todos os jogadores da carreira ativa (Story 2.4, AD-2/AD-3).
//!
//! **De onde vêm:** do `DATA` do save ativo no disco — o mesmo arquivo que
//! a localização escolheu pela memória (AD-11) — mais o banco estático do
//! jogo (`data\db\fifa_ng_db.db`) para os nomes (`BGwe`, Huffman) e as
//! nações (`Crbb`). Não da memória: o blob `CZUM` do heap é só o buffer do
//! último load/save (sessão 6), ou seja, o mesmo conteúdo do arquivo. Os
//! atributos mudam pouco dentro da temporada; o que o jogo ainda não
//! salvou aparece na próxima Missão depois de salvar.
//!
//! **Nomes** (conferido em 2026-10-01 contra o save do Felipe, 39.229
//! jogadores, nenhum sem nome):
//! 1. `editedplayernames` (save) por `playerid`, quando existe;
//! 2. senão, `commonnameid` (ex.: "Neymar") ou `firstnameid` + `lastnameid`,
//!    procurados primeiro em `dcplayernames` (save; regens, ids ≥ 29000) e
//!    depois em `playernames` (banco estático). Os ids que existem nas duas
//!    tabelas têm o mesmo texto.
//!
//! **Ligas** (2026-10-03, filtro geográfico por onde o jogador joga): da
//! tabela `leagues` (`onMQ`) do save, com o país (`countryid` = nationid) e
//! o continente da nação. Só entram ligas com clubes; ver `ler_ligas` para
//! as ligas especiais do banco do FIFA Friends.
//!
//! Devolve dados crus (AD-3): nenhum filtro de Missão passa por aqui.
//! Quem filtra é `scout::search`.

use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::{lock_cache, Date, SaveRepoError};
use crate::fifa_db::{self, FieldDescriptor, HuffmanStrings, TableDescriptor};
use crate::memscan;

// ---------------------------------------------------------------------
// Atributos
// ---------------------------------------------------------------------

/// Os 33 atributos de jogador do FIFA 16 (28 de linha + 5 de goleiro). No
/// JSON do Scout: `"aceleracao"`, `"gk_reflexos"` etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Atributo {
    Aceleracao,
    Velocidade,
    Agilidade,
    Equilibrio,
    Reacao,
    ControleDeBola,
    Drible,
    PosicionamentoOfensivo,
    Finalizacao,
    ForcaDoChute,
    ChuteDeLonge,
    Voleio,
    Penaltis,
    Visao,
    Cruzamento,
    Falta,
    PasseCurto,
    PasseLongo,
    Curva,
    Interceptacao,
    Cabeceio,
    Marcacao,
    DesarmeEmPe,
    Carrinho,
    Impulsao,
    Folego,
    Forca,
    Agressividade,
    GkMergulho,
    GkManejo,
    GkReposicao,
    GkColocacao,
    GkReflexos,
}

/// Quantos atributos existem (tamanho de `PlayerRaw::atributos`).
pub const TOTAL_ATRIBUTOS: usize = 33;

impl Atributo {
    pub const TODOS: [Atributo; TOTAL_ATRIBUTOS] = [
        Atributo::Aceleracao,
        Atributo::Velocidade,
        Atributo::Agilidade,
        Atributo::Equilibrio,
        Atributo::Reacao,
        Atributo::ControleDeBola,
        Atributo::Drible,
        Atributo::PosicionamentoOfensivo,
        Atributo::Finalizacao,
        Atributo::ForcaDoChute,
        Atributo::ChuteDeLonge,
        Atributo::Voleio,
        Atributo::Penaltis,
        Atributo::Visao,
        Atributo::Cruzamento,
        Atributo::Falta,
        Atributo::PasseCurto,
        Atributo::PasseLongo,
        Atributo::Curva,
        Atributo::Interceptacao,
        Atributo::Cabeceio,
        Atributo::Marcacao,
        Atributo::DesarmeEmPe,
        Atributo::Carrinho,
        Atributo::Impulsao,
        Atributo::Folego,
        Atributo::Forca,
        Atributo::Agressividade,
        Atributo::GkMergulho,
        Atributo::GkManejo,
        Atributo::GkReposicao,
        Atributo::GkColocacao,
        Atributo::GkReflexos,
    ];

    /// Posição em `TODOS` e em `PlayerRaw::atributos`.
    pub fn indice(self) -> usize {
        self as usize
    }

    pub fn goleiro(self) -> bool {
        self >= Atributo::GkMergulho
    }

    pub fn nome(self) -> &'static str {
        match self {
            Atributo::Aceleracao => "Aceleração",
            Atributo::Velocidade => "Velocidade",
            Atributo::Agilidade => "Agilidade",
            Atributo::Equilibrio => "Equilíbrio",
            Atributo::Reacao => "Reação",
            Atributo::ControleDeBola => "Controle de bola",
            Atributo::Drible => "Drible",
            Atributo::PosicionamentoOfensivo => "Posicionamento",
            Atributo::Finalizacao => "Finalização",
            Atributo::ForcaDoChute => "Força do chute",
            Atributo::ChuteDeLonge => "Chute de longe",
            Atributo::Voleio => "Voleio",
            Atributo::Penaltis => "Pênaltis",
            Atributo::Visao => "Visão",
            Atributo::Cruzamento => "Cruzamento",
            Atributo::Falta => "Cobrança de falta",
            Atributo::PasseCurto => "Passe curto",
            Atributo::PasseLongo => "Passe longo",
            Atributo::Curva => "Curva",
            Atributo::Interceptacao => "Interceptação",
            Atributo::Cabeceio => "Cabeceio",
            Atributo::Marcacao => "Marcação",
            Atributo::DesarmeEmPe => "Desarme em pé",
            Atributo::Carrinho => "Carrinho",
            Atributo::Impulsao => "Impulsão",
            Atributo::Folego => "Fôlego",
            Atributo::Forca => "Força",
            Atributo::Agressividade => "Agressividade",
            Atributo::GkMergulho => "GK Mergulho",
            Atributo::GkManejo => "GK Manejo",
            Atributo::GkReposicao => "GK Reposição",
            Atributo::GkColocacao => "GK Colocação",
            Atributo::GkReflexos => "GK Reflexos",
        }
    }

    /// Sigla de 3 letras para cabeçalho de coluna (o nome inteiro vai no
    /// tooltip).
    pub fn sigla(self) -> &'static str {
        match self {
            Atributo::Aceleracao => "ACE",
            Atributo::Velocidade => "VEL",
            Atributo::Agilidade => "AGI",
            Atributo::Equilibrio => "EQU",
            Atributo::Reacao => "REA",
            Atributo::ControleDeBola => "CTR",
            Atributo::Drible => "DRI",
            Atributo::PosicionamentoOfensivo => "POF",
            Atributo::Finalizacao => "FIN",
            Atributo::ForcaDoChute => "FCH",
            Atributo::ChuteDeLonge => "CLO",
            Atributo::Voleio => "VOL",
            Atributo::Penaltis => "PEN",
            Atributo::Visao => "VIS",
            Atributo::Cruzamento => "CRU",
            Atributo::Falta => "FAL",
            Atributo::PasseCurto => "PCU",
            Atributo::PasseLongo => "PLO",
            Atributo::Curva => "CUR",
            Atributo::Interceptacao => "INT",
            Atributo::Cabeceio => "CAB",
            Atributo::Marcacao => "MAR",
            Atributo::DesarmeEmPe => "DPE",
            Atributo::Carrinho => "CAR",
            Atributo::Impulsao => "IMP",
            Atributo::Folego => "FOL",
            Atributo::Forca => "FOR",
            Atributo::Agressividade => "AGR",
            Atributo::GkMergulho => "MER",
            Atributo::GkManejo => "MAN",
            Atributo::GkReposicao => "REP",
            Atributo::GkColocacao => "COL",
            Atributo::GkReflexos => "REF",
        }
    }

    /// Short name do campo em `CZUM` (`fifa_ng_db-meta.xml`; todos com
    /// `rangelow` 1).
    fn campo(self) -> &'static [u8; 4] {
        match self {
            Atributo::Aceleracao => b"SPge",
            Atributo::Velocidade => b"NrcP",
            Atributo::Agilidade => b"RRQB",
            Atributo::Equilibrio => b"onkY",
            Atributo::Reacao => b"YCnI",
            Atributo::ControleDeBola => b"MgwU",
            Atributo::Drible => b"nEbM",
            Atributo::PosicionamentoOfensivo => b"XsFD",
            Atributo::Finalizacao => b"xJZL",
            Atributo::ForcaDoChute => b"ohpV",
            Atributo::ChuteDeLonge => b"CsBG",
            Atributo::Voleio => b"Dydz",
            Atributo::Penaltis => b"AGsE",
            Atributo::Visao => b"ZoOK",
            Atributo::Cruzamento => b"wGOH",
            Atributo::Falta => b"VgKc",
            Atributo::PasseCurto => b"vObb",
            Atributo::PasseLongo => b"kerE",
            Atributo::Curva => b"YFaA",
            Atributo::Interceptacao => b"wWzG",
            Atributo::Cabeceio => b"aReg",
            Atributo::Marcacao => b"wxMq",
            Atributo::DesarmeEmPe => b"CsyD",
            Atributo::Carrinho => b"PhuM",
            Atributo::Impulsao => b"URGo",
            Atributo::Folego => b"XjDq",
            Atributo::Forca => b"nmgT",
            Atributo::Agressividade => b"iTce",
            Atributo::GkMergulho => b"xrSG",
            Atributo::GkManejo => b"GBGj",
            Atributo::GkReposicao => b"kqda",
            Atributo::GkColocacao => b"yfhq",
            Atributo::GkReflexos => b"eYFI",
        }
    }
}

// ---------------------------------------------------------------------
// Nações
// ---------------------------------------------------------------------

/// Confederação de uma nação (`Crbb.confederation`, conferido com Brasil 4,
/// Inglaterra 2, Nigéria 3, Japão 5, Nova Zelândia 6, EUA 7). É o
/// "continente" do filtro geográfico e do mapa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confederacao {
    Europa,
    AmericaDoSul,
    AmericaDoNorte,
    Africa,
    Asia,
    Oceania,
    /// "Rest of World" e afins (ids sem continente).
    Outras,
}

impl Confederacao {
    pub const TODAS: [Confederacao; 7] = [
        Confederacao::Europa,
        Confederacao::AmericaDoSul,
        Confederacao::AmericaDoNorte,
        Confederacao::Africa,
        Confederacao::Asia,
        Confederacao::Oceania,
        Confederacao::Outras,
    ];

    fn de_raw(valor: i64) -> Confederacao {
        match valor {
            2 => Confederacao::Europa,
            3 => Confederacao::Africa,
            4 => Confederacao::AmericaDoSul,
            5 => Confederacao::Asia,
            6 => Confederacao::Oceania,
            7 => Confederacao::AmericaDoNorte,
            _ => Confederacao::Outras,
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Confederacao::Europa => "Europa",
            Confederacao::AmericaDoSul => "América do Sul",
            Confederacao::AmericaDoNorte => "América do Norte e Central",
            Confederacao::Africa => "África",
            Confederacao::Asia => "Ásia",
            Confederacao::Oceania => "Oceania",
            Confederacao::Outras => "Outras",
        }
    }
}

/// Uma liga com clubes, para o filtro geográfico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Liga {
    /// `leagues.leagueid`.
    pub id: u32,
    pub nome: String,
    /// País (`Crbb.nationid`); `None` nas ligas que juntam clubes de um
    /// continente inteiro ("Clubes da UEFA") ou sem país (passes livres).
    pub pais: Option<u16>,
    /// Nome do país (vazio sem país).
    pub pais_nome: String,
    pub continente: Confederacao,
    /// Divisão (1 = primeira).
    pub nivel: u8,
    /// Quantos clubes a liga tem.
    pub clubes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nacao {
    /// `Crbb.nationid` — o mesmo valor de `CZUM.nationality`.
    pub id: u16,
    pub nome: String,
    /// Código ISO de 2 letras (vazio em algumas nações sem país real).
    pub iso: String,
    pub confederacao: Confederacao,
}

// ---------------------------------------------------------------------
// Jogadores
// ---------------------------------------------------------------------

/// Um jogador cru, como o save guarda (AD-3: sem filtro nem fórmula).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRaw {
    pub player_id: u32,
    /// Nome de exibição (apelido, ou nome + sobrenome).
    pub nome: String,
    pub nascimento: Date,
    /// `preferredposition1` (0 = GK … 27; ver `nome_posicao`).
    pub posicao: u8,
    /// `Crbb.nationid`.
    pub nacionalidade: u16,
    pub overall: u8,
    pub potencial: u8,
    /// Na ordem de `Atributo::TODOS`.
    pub atributos: [u8; TOTAL_ATRIBUTOS],
    /// Time de clube (`None` = sem vínculo a clube).
    pub clube_id: Option<u32>,
    pub clube: String,
    /// O clube é da liga "Rest of World" (times genéricos, fora do mercado).
    pub resto_do_mundo: bool,
    /// `preferredfoot` (Story 3.1).
    pub pe: Pe,
    /// Liga do clube (`leagueteamlinks`); `None` sem clube ou fora das
    /// ligas de `ler_ligas`.
    pub liga_id: Option<u32>,
    /// `contractvaliduntil`: ano em que o contrato termina (no fim da
    /// temporada europeia, 30/06).
    pub contrato_ate: u16,
    pub ritmo_ataque: RitmoTrabalho,
    pub ritmo_defesa: RitmoTrabalho,
    /// Estrelas de drible, 1–5 (`skillmoves` 0–4 + 1).
    pub estrelas_drible: u8,
    /// Estrelas do pé fraco, 1–5 (`weakfootabilitytypecode`).
    pub pe_fraco: u8,
    /// Altura em cm (`height`, 150–215).
    pub altura: u8,
}

/// Pé preferido (`CZUM.preferredfoot`: 1 = direito, 2 = esquerdo). No JSON
/// do Scout: `"direito"` / `"esquerdo"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pe {
    #[default]
    Direito,
    Esquerdo,
}

impl Pe {
    fn de_raw(valor: i64) -> Pe {
        if valor == 2 {
            Pe::Esquerdo
        } else {
            Pe::Direito
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Pe::Direito => "Direito",
            Pe::Esquerdo => "Esquerdo",
        }
    }
}

impl PlayerRaw {
    pub fn atributo(&self, atributo: Atributo) -> u8 {
        self.atributos.get(atributo.indice()).copied().unwrap_or(0)
    }

    /// Idade completa em `hoje` (data da carreira, não do PC).
    pub fn idade(&self, hoje: Date) -> u8 {
        let mut anos = hoje.year() - self.nascimento.year();
        if (hoje.month(), hoje.day()) < (self.nascimento.month(), self.nascimento.day()) {
            anos -= 1;
        }
        u8::try_from(anos.max(0)).unwrap_or(u8::MAX)
    }
}

/// Sigla da posição (`preferredposition1`). Enum do FIFA: 0 GK, 1 SW,
/// 2 RWB, 3 RB, 4 RCB, 5 CB, 6 LCB, 7 LB, 8 LWB, 9 RDM, 10 CDM, 11 LDM,
/// 12 RM, 13 RCM, 14 CM, 15 LCM, 16 LM, 17 RAM, 18 CAM, 19 LAM, 20 RF,
/// 21 CF, 22 LF, 23 RW, 24 RS, 25 ST, 26 LS, 27 LW. Conferido no save do
/// Felipe (2026-10-03): 5 (CB) e 25 (ST) são os mais comuns, Mbappé é 25
/// com 27 de segunda posição. Até essa data a tabela estava deslocada uma
/// casa (sem o SW): lateral direito aparecia como "ZAD".
pub fn nome_posicao(posicao: u8) -> &'static str {
    const NOMES: [&str; 28] = [
        "GOL", "LIB", "ALD", "LD", "ZAD", "ZAG", "ZAE", "LE", "ALE", "VOD", "VOL", "VOE", "MD", "MCD", "MC", "MCE", "ME",
        "MAD", "MEI", "MAE", "SAD", "SA", "SAE", "PD", "ATD", "ATA", "ATE", "PE",
    ];
    NOMES.get(posicao as usize).copied().unwrap_or("?")
}

/// Grupo de função da posição, usado para decidir quais atributos um
/// Olheiro observa primeiro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Funcao {
    Goleiro,
    Defensor,
    MeioCampo,
    Atacante,
}

pub fn funcao_da_posicao(posicao: u8) -> Funcao {
    match posicao {
        0 => Funcao::Goleiro,
        1..=8 => Funcao::Defensor,
        9..=19 => Funcao::MeioCampo,
        _ => Funcao::Atacante,
    }
}

/// Ritmo de trabalho (`attackingworkrate` / `defensiveworkrate`: 0 =
/// médio, 1 = baixo, 2 = alto; conferido com Mbappé, alto/baixo). No JSON:
/// `"baixo"`, `"medio"`, `"alto"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RitmoTrabalho {
    Baixo,
    #[default]
    Medio,
    Alto,
}

impl RitmoTrabalho {
    pub const TODOS: [RitmoTrabalho; 3] = [RitmoTrabalho::Baixo, RitmoTrabalho::Medio, RitmoTrabalho::Alto];

    fn de_raw(valor: i64) -> RitmoTrabalho {
        match valor {
            1 => RitmoTrabalho::Baixo,
            2 => RitmoTrabalho::Alto,
            _ => RitmoTrabalho::Medio,
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            RitmoTrabalho::Baixo => "Baixo",
            RitmoTrabalho::Medio => "Médio",
            RitmoTrabalho::Alto => "Alto",
        }
    }
}

/// O que a busca de uma Missão recebe.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPool {
    pub jogadores: Vec<PlayerRaw>,
    pub nacoes: Vec<Nacao>,
    /// Clube do técnico (`mPrV.clubteamid`): os jogadores dele não entram
    /// num Relatório.
    pub clube_usuario: i64,
    /// Ligas com clubes (filtro geográfico).
    pub ligas: Vec<Liga>,
}

impl PlayerPool {
    /// Só os jogadores do clube do técnico.
    pub fn elenco(self) -> PlayerPool {
        let clube = self.clube_usuario;
        PlayerPool {
            jogadores: self.jogadores.into_iter().filter(|j| j.clube_id.map(i64::from) == Some(clube)).collect(),
            ..self
        }
    }
}

// ---------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------

/// Pasta do jogo: a do `fifa16.exe` que carregou esta DLL. Fora do jogo
/// (testes), a instalação do Felipe.
pub fn pasta_do_jogo() -> PathBuf {
    let mut buffer = [0u16; 1024];
    let len = unsafe { windows::Win32::System::LibraryLoader::GetModuleFileNameW(None, &mut buffer) } as usize;
    let exe = PathBuf::from(String::from_utf16_lossy(buffer.get(..len).unwrap_or_default()));
    let eh_o_jogo = exe.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("fifa16.exe"));
    match exe.parent() {
        Some(pasta) if eh_o_jogo => pasta.to_path_buf(),
        _ => PathBuf::from(r"D:\Program Files\FIFA 16"),
    }
}

fn caminho_banco_estatico() -> PathBuf {
    pasta_do_jogo().join("data").join("db").join("fifa_ng_db.db")
}

/// Nomes e nações do banco estático (não muda na sessão: lido uma vez).
#[derive(Debug)]
struct Estatico {
    nomes: HashMap<u32, String>,
    nacoes: Vec<Nacao>,
}

static ESTATICO: OnceLock<Mutex<Option<Arc<Estatico>>>> = OnceLock::new();

fn estatico() -> Result<Arc<Estatico>, SaveRepoError> {
    let celula = ESTATICO.get_or_init(|| Mutex::new(None));
    let mut guarda = celula.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(pronto) = guarda.as_ref() {
        return Ok(Arc::clone(pronto));
    }
    let caminho = caminho_banco_estatico();
    let dados = std::fs::read(&caminho).map_err(|err| {
        tracing::warn!("[save_repo] Banco estático ilegível ({}): {err}", caminho.display());
        SaveRepoError::ProcessoInacessivel
    })?;
    let pronto = Arc::new(ler_estatico(&dados)?);
    tracing::info!(
        "[save_repo] Banco estático: {} nomes, {} nações ({}).",
        pronto.nomes.len(),
        pronto.nacoes.len(),
        caminho.display()
    );
    *guarda = Some(Arc::clone(&pronto));
    Ok(pronto)
}

/// Nações do banco estático, em ordem de id (o mapa da Story 2.9).
pub fn read_nations() -> Result<Vec<Nacao>, SaveRepoError> {
    Ok(estatico()?.nacoes.clone())
}

/// Caminho do `DATA` do save que a localização escolheu.
fn caminho_save_ativo() -> Result<(PathBuf, i64), SaveRepoError> {
    let cache = lock_cache();
    let live = cache.as_ref().ok_or(SaveRepoError::NaoLocalizado)?;
    Ok((live.save.path.clone(), live.save.identity.club_team_id))
}

/// Todos os jogadores do save ativo. Pesado (~10 MB de arquivo + 39 mil
/// registros): chamar só de dentro de um `AsyncTask` (AD-4).
pub fn read_all_players() -> Result<PlayerPool, SaveRepoError> {
    let inicio = std::time::Instant::now();
    let (caminho, clube_usuario) = caminho_save_ativo()?;
    let estatico = estatico()?;
    let dados = std::fs::read(&caminho).map_err(|err| {
        tracing::warn!("[save_repo] Save ativo ilegível ({}): {err}", caminho.display());
        SaveRepoError::ProcessoInacessivel
    })?;
    let mut jogadores = ler_jogadores(&dados, &estatico.nomes)?;
    let ligas = ler_ligas(&dados, &estatico.nacoes)?;
    // liga fora do filtro geográfico (seleção, resto do mundo…) = sem liga
    let validas: std::collections::HashSet<u32> = ligas.iter().map(|l| l.id).collect();
    for j in &mut jogadores {
        j.liga_id = j.liga_id.filter(|l| validas.contains(l));
    }
    tracing::info!(
        "[save_repo] {} jogadores e {} ligas lidos de {} em {} ms.",
        jogadores.len(),
        ligas.len(),
        caminho.display(),
        inicio.elapsed().as_millis()
    );
    Ok(PlayerPool { jogadores, nacoes: estatico.nacoes.clone(), clube_usuario, ligas })
}

/// Ligas com clubes do save ativo (filtro geográfico). Lê o `DATA` do
/// disco: chamar fora do thread de render.
pub fn read_leagues() -> Result<Vec<Liga>, SaveRepoError> {
    let (caminho, _) = caminho_save_ativo()?;
    let estatico = estatico()?;
    let dados = std::fs::read(&caminho).map_err(|err| {
        tracing::warn!("[save_repo] Save ativo ilegível ({}): {err}", caminho.display());
        SaveRepoError::ProcessoInacessivel
    })?;
    ler_ligas(&dados, &estatico.nacoes)
}

/// O que a oferta de Olheiros precisa do clube do técnico (Épico 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDoClube {
    /// `teams.domesticprestige` / `internationalprestige` (`ppLE` / `edvw`), 0–20.
    pub prestigio_nacional: u8,
    pub prestigio_internacional: u8,
    /// Liga do clube (`qdZF`), se estiver entre as ligas com clubes.
    pub liga: Option<Liga>,
    /// Troféus da carreira: um bit por troféu em `career_trophies.flags`
    /// (`KNNX.glmx`), uma linha por temporada. Conferido no save do Felipe
    /// (Barcelona, 13 temporadas): o bit 1 aparece em quase todas (a liga).
    pub titulos: u16,
}

/// Prestígio, liga e títulos do clube do técnico. Lê o `DATA` do disco:
/// chamar fora do thread de render.
pub fn read_club_profile() -> Result<DadosDoClube, SaveRepoError> {
    let (caminho, clube) = caminho_save_ativo()?;
    let estatico = estatico()?;
    let dados = std::fs::read(&caminho).map_err(|err| {
        tracing::warn!("[save_repo] Save ativo ilegível ({}): {err}", caminho.display());
        SaveRepoError::ProcessoInacessivel
    })?;
    ler_dados_do_clube(&dados, clube, &estatico.nacoes)
}

fn ler_dados_do_clube(dados: &[u8], clube: i64, nacoes: &[Nacao]) -> Result<DadosDoClube, SaveRepoError> {
    let todas = tabelas(dados);
    let times = achar(dados, &todas, b"lyxL")?;
    let (id, nacional, internacional) = (times.campo(b"mCXg")?, times.campo(b"ppLE")?, times.campo(b"edvw")?);
    let registro = times.registros().find(|r| inteiro(r, id, 1) == clube).ok_or_else(|| {
        tracing::warn!("[save_repo] Clube {clube} do técnico não está em teams.");
        SaveRepoError::TabelaNaoEncontrada
    })?;
    let prestigio_nacional = como(inteiro(registro, nacional, 0));
    let prestigio_internacional = como(inteiro(registro, internacional, 0));
    let ligas_dos_times = achar(dados, &todas, b"qdZF")?;
    let (liga_time, liga_id) = (ligas_dos_times.campo(b"mCXg")?, ligas_dos_times.campo(b"aQrQ")?);
    let id_da_liga = ligas_dos_times.registros().find(|r| inteiro(r, liga_time, 1) == clube).map(|r| inteiro(r, liga_id, 1));
    let liga = match id_da_liga {
        Some(l) => ler_ligas(dados, nacoes)?.into_iter().find(|x| i64::from(x.id) == l),
        None => None,
    };
    // Sem a tabela de troféus (carreira nova) = nenhum título.
    let titulos = match achar(dados, &todas, b"KNNX") {
        Ok(t) => {
            let flags = t.campo(b"glmx")?;
            t.registros().map(|r| inteiro(r, flags, 0).count_ones()).sum::<u32>()
        }
        Err(_) => 0,
    };
    Ok(DadosDoClube { prestigio_nacional, prestigio_internacional, liga, titulos: u16::try_from(titulos).unwrap_or(u16::MAX) })
}

/// Elenco do técnico (`clube_id == mPrV.clubteamid`), da mesma fonte que
/// `read_all_players` — o `DATA` do save ativo. Não é uma leitura barata:
/// o arquivo inteiro é decodificado (~0,5 s), então também roda num
/// `AsyncTask` (AD-4, emenda da Story 3.2).
pub fn read_squad_players() -> Result<PlayerPool, SaveRepoError> {
    Ok(read_all_players()?.elenco())
}

/// Tabela + buffer, com busca de campo por short name.
struct Tabela<'a> {
    dados: &'a [u8],
    descritor: &'a TableDescriptor,
}

impl<'a> Tabela<'a> {
    fn campo(&self, short: &[u8; 4]) -> Result<&'a FieldDescriptor, SaveRepoError> {
        self.descritor.fields.iter().find(|f| &f.short_name == short).ok_or_else(|| {
            tracing::warn!(
                "[save_repo] Campo {}.{} não existe.",
                fifa_db::shortname_str(&self.descritor.short_name),
                fifa_db::shortname_str(short)
            );
            SaveRepoError::TabelaNaoEncontrada
        })
    }

    /// Registros válidos (pula os apagados).
    fn registros(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        (0..self.descritor.written_record_count as usize)
            .filter_map(|i| self.descritor.record(self.dados, i))
            .filter(|r| !TableDescriptor::is_deleted(r))
    }
}

/// Todas as tabelas de todas as databases de um arquivo.
fn tabelas(dados: &[u8]) -> Vec<TableDescriptor> {
    memchr::memmem::find_iter(dados, memscan::DB_SIGNATURE)
        .filter_map(|inicio| fifa_db::parse_database_tables(dados, inicio))
        .flatten()
        .collect()
}

fn achar<'a>(dados: &'a [u8], todas: &'a [TableDescriptor], short: &[u8; 4]) -> Result<Tabela<'a>, SaveRepoError> {
    todas
        .iter()
        .find(|t| &t.short_name == short)
        .map(|descritor| Tabela { dados, descritor })
        .ok_or_else(|| {
            tracing::warn!("[save_repo] Tabela {} não encontrada.", fifa_db::shortname_str(short));
            SaveRepoError::TabelaNaoEncontrada
        })
}

fn inteiro(registro: &[u8], campo: &FieldDescriptor, range_low: i64) -> i64 {
    fifa_db::read_int_field(registro, campo, range_low).unwrap_or(0)
}

fn como<T: TryFrom<i64> + Default>(valor: i64) -> T {
    T::try_from(valor).unwrap_or_default()
}

fn ler_estatico(dados: &[u8]) -> Result<Estatico, SaveRepoError> {
    let todas = tabelas(dados);

    let bgwe = achar(dados, &todas, b"BGwe")?;
    let (texto, id) = (bgwe.campo(b"vIys")?, bgwe.campo(b"FuiB")?);
    let huffman = HuffmanStrings::for_table(dados, bgwe.descritor).ok_or(SaveRepoError::TabelaNaoEncontrada)?;
    let mut nomes = HashMap::with_capacity(bgwe.descritor.written_record_count as usize);
    for registro in bgwe.registros() {
        if let Some(nome) = huffman.read(registro, texto) {
            nomes.insert(como(inteiro(registro, id, 0)), nome);
        }
    }

    let crbb = achar(dados, &todas, b"Crbb")?;
    let (iso, nome, id, conf) = (crbb.campo(b"UItq")?, crbb.campo(b"zMVU")?, crbb.campo(b"LEtt")?, crbb.campo(b"BJHH")?);
    let mut nacoes: Vec<Nacao> = crbb
        .registros()
        .map(|r| Nacao {
            id: como(inteiro(r, id, 1)),
            nome: fifa_db::read_fixed_string(r, nome).unwrap_or_default(),
            iso: fifa_db::read_fixed_string(r, iso).unwrap_or_default(),
            confederacao: Confederacao::de_raw(inteiro(r, conf, 1)),
        })
        .filter(|n| !n.nome.is_empty())
        .collect();
    nacoes.sort_by_key(|n| n.id);
    Ok(Estatico { nomes, nacoes })
}

/// Liga "Rest of World" (`onMQ.leagueid` 76).
const LIGA_RESTO_DO_MUNDO: i64 = 76;

/// "Países" especiais do `countryid` das ligas (conferido no save do Felipe,
/// 2026-10-03, banco do FIFA Friends):
/// - 75 "International" — seleções (fora do mercado);
/// - 210 "Free Agents Country" — passes livres (sem país);
/// - 211 "Rest of World" — ligas que juntam clubes de um continente
///   ("Clubes da UEFA", "Clubes da Concacaf", "Clubes da AFC", "Clubes da
///   CAF/OFC"); "Clubes do Mundo" (liga 76) é o resto do mundo, fora;
/// - 216 "Creation Zone" e 0 — sem clubes;
/// - 156, 217, 220, 221, 223, 224 ("Brasil Sudeste", "Brasil Sul"…) —
///   regiões das federações estaduais brasileiras: valem como Brasil.
const PAIS_SELECOES: i64 = 75;
const PAIS_PASSES_LIVRES: i64 = 210;
const PAIS_RESTO_DO_MUNDO: i64 = 211;
const REGIOES_DO_BRASIL: [i64; 6] = [156, 217, 220, 221, 223, 224];
const BRASIL: u16 = 54;

/// Continente de uma liga "Clubes da …" pelo nome.
fn continente_da_liga_regional(nome: &str) -> Option<Confederacao> {
    let nome = nome.to_uppercase();
    if nome.contains("UEFA") {
        Some(Confederacao::Europa)
    } else if nome.contains("CONCACAF") {
        Some(Confederacao::AmericaDoNorte)
    } else if nome.contains("AFC") && !nome.contains("CAF") {
        Some(Confederacao::Asia)
    } else if nome.contains("CAF") || nome.contains("OFC") {
        Some(Confederacao::Africa)
    } else {
        None
    }
}

/// Ligas com pelo menos um clube, com país e continente (ver os "países"
/// especiais acima). O sufixo " [OFF]" das federações estaduais sai do nome.
fn ler_ligas(dados: &[u8], nacoes: &[Nacao]) -> Result<Vec<Liga>, SaveRepoError> {
    let todas = tabelas(dados);
    let onmq = achar(dados, &todas, b"onMQ")?;
    let (id, pais, nivel, nome) = (onmq.campo(b"aQrQ")?, onmq.campo(b"WDGJ")?, onmq.campo(b"paPI")?, onmq.campo(b"HEQX")?);
    let huffman = HuffmanStrings::for_table(dados, onmq.descritor);
    let ligas_dos_times = achar(dados, &todas, b"qdZF")?;
    let liga_do_time = ligas_dos_times.campo(b"aQrQ")?;
    let mut clubes: HashMap<u32, u16> = HashMap::new();
    for r in ligas_dos_times.registros() {
        *clubes.entry(como(inteiro(r, liga_do_time, 1))).or_default() += 1;
    }
    let nacao = |id: u16| nacoes.iter().find(|n| n.id == id);
    let mut ligas: Vec<Liga> = onmq
        .registros()
        .filter_map(|r| {
            let liga_id: u32 = como(inteiro(r, id, 1));
            let pais_raw = inteiro(r, pais, 0);
            let texto = fifa_db::read_fixed_string(r, nome).or_else(|| huffman.as_ref().and_then(|h| h.read(r, nome)))?;
            let texto = texto.trim().trim_end_matches("[OFF]").trim().to_string();
            let qtd = clubes.get(&liga_id).copied().unwrap_or(0);
            if qtd == 0 || texto.is_empty() || i64::from(liga_id) == LIGA_RESTO_DO_MUNDO {
                return None;
            }
            let (pais_id, continente) = match pais_raw {
                0 | PAIS_SELECOES => return None,
                PAIS_PASSES_LIVRES => (None, Confederacao::Outras),
                PAIS_RESTO_DO_MUNDO => (None, continente_da_liga_regional(&texto)?),
                p if REGIOES_DO_BRASIL.contains(&p) => (Some(BRASIL), Confederacao::AmericaDoSul),
                p => {
                    let n = nacao(como(p))?;
                    (Some(n.id), n.confederacao)
                }
            };
            Some(Liga {
                id: liga_id,
                nome: if pais_raw == PAIS_PASSES_LIVRES { "Passes livres".to_string() } else { texto },
                pais_nome: pais_id.and_then(nacao).map(|n| n.nome.clone()).unwrap_or_default(),
                pais: pais_id,
                continente,
                nivel: como(inteiro(r, nivel, 1)),
                clubes: qtd,
            })
        })
        .collect();
    ligas.sort_by(|a, b| (a.continente, &a.pais_nome, a.nivel, &a.nome).cmp(&(b.continente, &b.pais_nome, b.nivel, &b.nome)));
    Ok(ligas)
}

fn ler_jogadores(dados: &[u8], nomes_estaticos: &HashMap<u32, String>) -> Result<Vec<PlayerRaw>, SaveRepoError> {
    let todas = tabelas(dados);

    // Nomes de regens (dcplayernames; nameid com rangelow 29000).
    let dc = achar(dados, &todas, b"bneD")?;
    let (texto, id) = (dc.campo(b"vIys")?, dc.campo(b"FuiB")?);
    let nomes_dc: HashMap<u32, String> = dc
        .registros()
        .filter_map(|r| Some((como(inteiro(r, id, 29_000)), fifa_db::read_fixed_string(r, texto)?)))
        .collect();
    let nome_por_id = |id: u32| -> &str {
        nomes_dc.get(&id).or_else(|| nomes_estaticos.get(&id)).map(String::as_str).unwrap_or_default()
    };

    // Nomes editados (editedplayernames), por playerid.
    let editados: HashMap<u32, String> = match achar(dados, &todas, b"nQVU") {
        Ok(t) => {
            let (primeiro, apelido, sobrenome, pid) = (t.campo(b"HdeP")?, t.campo(b"xnfZ")?, t.campo(b"rREd")?, t.campo(b"ykFq")?);
            t.registros()
                .map(|r| {
                    let ler = |c| fifa_db::read_fixed_string(r, c).unwrap_or_default();
                    let nome = juntar_nome(&ler(apelido), &ler(primeiro), &ler(sobrenome));
                    (como(inteiro(r, pid, 0)), nome)
                })
                .filter(|(_, nome)| !nome.is_empty())
                .collect()
        }
        Err(_) => HashMap::new(),
    };

    // Times: nomes, seleções e liga.
    let times = achar(dados, &todas, b"lyxL")?;
    let (time_id, time_nome) = (times.campo(b"mCXg")?, times.campo(b"AUsv")?);
    let nome_do_time: HashMap<u32, String> = times
        .registros()
        .map(|r| (como(inteiro(r, time_id, 1)), fifa_db::read_fixed_string(r, time_nome).unwrap_or_default()))
        .collect();
    let selecoes = achar(dados, &todas, b"CxJp")?;
    let selecao_id = selecoes.campo(b"mCXg")?;
    let eh_selecao: std::collections::HashSet<u32> =
        selecoes.registros().map(|r| como(inteiro(r, selecao_id, 1))).collect();
    let ligas = achar(dados, &todas, b"qdZF")?;
    let (liga_time, liga_id) = (ligas.campo(b"mCXg")?, ligas.campo(b"aQrQ")?);
    let liga_do_time: HashMap<u32, i64> =
        ligas.registros().map(|r| (como(inteiro(r, liga_time, 1)), inteiro(r, liga_id, 1))).collect();


    // Vínculo jogador → clube (ignora a seleção).
    let vinculos = achar(dados, &todas, b"RrqT")?;
    let (vinc_time, vinc_jogador) = (vinculos.campo(b"mCXg")?, vinculos.campo(b"ykFq")?);
    let mut clube_de: HashMap<u32, u32> = HashMap::new();
    for r in vinculos.registros() {
        let time: u32 = como(inteiro(r, vinc_time, 1));
        if !eh_selecao.contains(&time) {
            clube_de.entry(como(inteiro(r, vinc_jogador, 0))).or_insert(time);
        }
    }

    let czum = achar(dados, &todas, b"CZUM")?;
    let campos = CamposJogador::de(&czum)?;
    let epoca = Date(15821014).day_number();
    let jogadores = czum
        .registros()
        .filter(|r| inteiro(r, campos.genero, 0) == 0)
        .map(|r| {
            let player_id: u32 = como(inteiro(r, campos.player_id, 0));
            let nome = editados.get(&player_id).cloned().unwrap_or_else(|| {
                let id = |c| como::<u32>(inteiro(r, c, 0));
                juntar_nome(nome_por_id(id(campos.apelido)), nome_por_id(id(campos.primeiro)), nome_por_id(id(campos.sobrenome)))
            });
            let mut atributos = [0u8; TOTAL_ATRIBUTOS];
            for (valor, campo) in atributos.iter_mut().zip(&campos.atributos) {
                *valor = como(inteiro(r, campo, 1));
            }
            let clube_id = clube_de.get(&player_id).copied();
            PlayerRaw {
                player_id,
                nome,
                nascimento: Date::from_day_number(epoca + inteiro(r, campos.nascimento, 0)),
                posicao: como(inteiro(r, campos.posicao, 0)),
                nacionalidade: como(inteiro(r, campos.nacionalidade, 0)),
                overall: como(inteiro(r, campos.overall, 1)),
                potencial: como(inteiro(r, campos.potencial, 1)),
                atributos,
                clube: clube_id.and_then(|t| nome_do_time.get(&t).cloned()).unwrap_or_default(),
                resto_do_mundo: clube_id.and_then(|t| liga_do_time.get(&t)) == Some(&LIGA_RESTO_DO_MUNDO),
                clube_id,
                pe: Pe::de_raw(inteiro(r, campos.pe, 1)),
                liga_id: clube_id
                    .and_then(|t| liga_do_time.get(&t))
                    .and_then(|&l| u32::try_from(l).ok()),
                contrato_ate: como(inteiro(r, campos.contrato, 0)),
                ritmo_ataque: RitmoTrabalho::de_raw(inteiro(r, campos.ritmo_ataque, 0)),
                ritmo_defesa: RitmoTrabalho::de_raw(inteiro(r, campos.ritmo_defesa, 0)),
                estrelas_drible: como::<u8>(inteiro(r, campos.dribles, 0)).saturating_add(1),
                pe_fraco: como(inteiro(r, campos.pe_fraco, 1)),
                altura: como(inteiro(r, campos.altura, 150)),
            }
        })
        .collect();
    Ok(jogadores)
}

/// Campos de `CZUM` usados (short names do `fifa_ng_db-meta.xml`).
struct CamposJogador<'a> {
    player_id: &'a FieldDescriptor,
    primeiro: &'a FieldDescriptor,
    sobrenome: &'a FieldDescriptor,
    apelido: &'a FieldDescriptor,
    overall: &'a FieldDescriptor,
    potencial: &'a FieldDescriptor,
    posicao: &'a FieldDescriptor,
    nacionalidade: &'a FieldDescriptor,
    nascimento: &'a FieldDescriptor,
    genero: &'a FieldDescriptor,
    pe: &'a FieldDescriptor,
    contrato: &'a FieldDescriptor,
    ritmo_ataque: &'a FieldDescriptor,
    ritmo_defesa: &'a FieldDescriptor,
    dribles: &'a FieldDescriptor,
    pe_fraco: &'a FieldDescriptor,
    altura: &'a FieldDescriptor,
    atributos: Vec<&'a FieldDescriptor>,
}

impl<'a> CamposJogador<'a> {
    fn de(czum: &Tabela<'a>) -> Result<Self, SaveRepoError> {
        Ok(CamposJogador {
            player_id: czum.campo(b"ykFq")?,
            primeiro: czum.campo(b"tHlO")?,
            sobrenome: czum.campo(b"QCfa")?,
            apelido: czum.campo(b"HDYx")?,
            overall: czum.campo(b"UERs")?,
            potencial: czum.campo(b"mpuH")?,
            posicao: czum.campo(b"wZQU")?,
            nacionalidade: czum.campo(b"enmm")?,
            nascimento: czum.campo(b"WVIU")?,
            genero: czum.campo(b"EveZ")?,
            pe: czum.campo(b"MDvm")?,
            contrato: czum.campo(b"qvmK")?,
            ritmo_ataque: czum.campo(b"BqFe")?,
            ritmo_defesa: czum.campo(b"boFm")?,
            dribles: czum.campo(b"BAPc")?,
            pe_fraco: czum.campo(b"aOBn")?,
            altura: czum.campo(b"ypBQ")?,
            atributos: Atributo::TODOS.iter().map(|a| czum.campo(a.campo())).collect::<Result<_, _>>()?,
        })
    }
}

/// Apelido, se houver; senão "primeiro sobrenome".
fn juntar_nome(apelido: &str, primeiro: &str, sobrenome: &str) -> String {
    if !apelido.trim().is_empty() {
        return apelido.trim().to_string();
    }
    format!("{} {}", primeiro.trim(), sobrenome.trim()).trim().to_string()
}

/// Rosto do jogador (`data\ui\imgAssets\heads\p<id>.dds`), já em
/// RGBA. `None` se o arquivo não existe (regens) ou não é um DDS que
/// sabemos ler. Lê disco: chamar fora do thread de render (Story 2.6).
pub fn ler_miniface(player_id: u32) -> Option<crate::dds::Imagem> {
    let caminho = pasta_do_jogo()
        .join("data")
        .join("ui")
        .join("imgAssets")
        .join("heads")
        .join(format!("p{player_id}.dds"));
    let dados = std::fs::read(caminho).ok()?;
    crate::dds::decodificar(&dados)
}

/// Bandeira de uma nação (`data/ui/artassets/countryflags/f_<id>.big`),
/// como ícone RGBA. `None` se o arquivo não existe ou tem outro formato.
/// Lê disco: chamar fora do thread de render.
pub fn ler_bandeira(nacao_id: u32) -> Option<crate::dds::Imagem> {
    let caminho = pasta_do_jogo().join("data").join("ui").join("artassets").join("countryflags").join(format!("f_{nacao_id}.big"));
    let dados = std::fs::read(caminho).ok()?;
    crate::dds::bandeira_do_big(&dados)
}

/// Lê jogadores de um `DATA` + banco estático em disco (testes e
/// diagnóstico, sem a carreira localizada).
#[cfg(test)]
pub fn ler_de_arquivos(save: &Path, banco_estatico: &Path) -> Result<(Vec<PlayerRaw>, Vec<Nacao>), SaveRepoError> {
    let estatico = ler_estatico(&std::fs::read(banco_estatico).map_err(|_| SaveRepoError::ProcessoInacessivel)?)?;
    let dados = std::fs::read(save).map_err(|_| SaveRepoError::ProcessoInacessivel)?;
    Ok((ler_jogadores(&dados, &estatico.nomes)?, estatico.nacoes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banco_estatico() -> Option<PathBuf> {
        let caminho = caminho_banco_estatico();
        caminho.exists().then_some(caminho)
    }

    fn backup() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("save_backups").join("717036e3_20260913_172921").join("DATA")
    }

    #[test]
    fn attribute_order_matches_indices_and_names_are_unique() {
        for (i, a) in Atributo::TODOS.iter().enumerate() {
            assert_eq!(a.indice(), i);
        }
        let mut siglas: Vec<&str> = Atributo::TODOS.iter().map(|a| a.sigla()).collect();
        siglas.sort_unstable();
        siglas.dedup();
        assert_eq!(siglas.len(), TOTAL_ATRIBUTOS);
        assert_eq!(Atributo::TODOS.iter().filter(|a| a.goleiro()).count(), 5);
    }

    #[test]
    fn regional_club_leagues_map_to_their_continent() {
        assert_eq!(continente_da_liga_regional("Clubes da UEFA"), Some(Confederacao::Europa));
        assert_eq!(continente_da_liga_regional("Clubes da Concacaf"), Some(Confederacao::AmericaDoNorte));
        assert_eq!(continente_da_liga_regional("Clubes da AFC"), Some(Confederacao::Asia));
        assert_eq!(continente_da_liga_regional("Clubes da CAF/OFC"), Some(Confederacao::Africa));
        assert_eq!(continente_da_liga_regional("Clubes do Mundo"), None);
    }

    /// Ligas do save versionado: só ligas com clubes, com país e continente.
    #[test]
    fn real_save_lists_only_leagues_with_clubs() {
        let Some(estatico) = banco_estatico() else {
            eprintln!("banco estático do FIFA 16 não instalado; teste pulado");
            return;
        };
        let nacoes = ler_estatico(&std::fs::read(estatico).expect("banco")).expect("estático").nacoes;
        let ligas = ler_ligas(&std::fs::read(backup()).expect("save"), &nacoes).expect("ligas");
        assert!(ligas.iter().all(|l| l.clubes > 0));
        let premier = ligas.iter().find(|l| l.id == 13).expect("Premier League");
        assert_eq!((premier.pais, premier.continente, premier.nivel), (Some(14), Confederacao::Europa, 1));
        let brasileirao = ligas.iter().find(|l| l.id == 7).expect("Brasileirão");
        assert_eq!(brasileirao.nome, "Brasileirão", "acento lido em Latin-1");
        // federações estaduais valem como Brasil, sem o [OFF]
        let paulista = ligas.iter().find(|l| l.id == 101).expect("Federação Paulista");
        assert_eq!((paulista.pais, paulista.nome.as_str()), (Some(54), "Federação Paulista"));
        // seleções e resto do mundo ficam de fora; clubes da UEFA sem país
        assert!(!ligas.iter().any(|l| l.id == 78 || l.id == 76));
        let uefa = ligas.iter().find(|l| l.id == 77).expect("Clubes da UEFA");
        assert_eq!((uefa.pais, uefa.continente), (None, Confederacao::Europa));
    }

    /// Clube do save versionado (Barcelona, carreira de 13 temporadas).
    #[test]
    fn real_save_reads_the_club_prestige_league_and_titles() {
        let Some(estatico) = banco_estatico() else {
            eprintln!("banco estático do FIFA 16 não instalado; teste pulado");
            return;
        };
        let nacoes = ler_estatico(&std::fs::read(estatico).expect("banco")).expect("estático").nacoes;
        let clube = ler_dados_do_clube(&std::fs::read(backup()).expect("save"), 241, &nacoes).expect("clube");
        assert_eq!((clube.prestigio_nacional, clube.prestigio_internacional), (20, 20));
        let liga = clube.liga.expect("La Liga");
        assert_eq!((liga.id, liga.nivel, liga.pais, liga.continente), (53, 1, Some(45), Confederacao::Europa));
        assert!((15..=30).contains(&clube.titulos), "{}", clube.titulos);
    }

    #[test]
    fn names_prefer_the_nickname() {
        assert_eq!(juntar_nome("Neymar", "Neymar", "da Silva Santos"), "Neymar");
        assert_eq!(juntar_nome("", "Kylian", "Mbappé"), "Kylian Mbappé");
        assert_eq!(juntar_nome(" ", "", "Pelé"), "Pelé");
    }

    #[test]
    fn positions_and_roles() {
        assert_eq!(nome_posicao(0), "GOL");
        assert_eq!(nome_posicao(3), "LD", "lateral direito não é ZAD");
        assert_eq!(nome_posicao(5), "ZAG");
        assert_eq!(nome_posicao(7), "LE");
        assert_eq!(nome_posicao(10), "VOL");
        assert_eq!(nome_posicao(18), "MEI");
        assert_eq!(nome_posicao(25), "ATA");
        assert_eq!(nome_posicao(27), "PE");
        assert_eq!(nome_posicao(99), "?");
        assert_eq!(funcao_da_posicao(0), Funcao::Goleiro);
        assert_eq!(funcao_da_posicao(8), Funcao::Defensor, "ala esquerdo");
        assert_eq!(funcao_da_posicao(14), Funcao::MeioCampo);
        assert_eq!(funcao_da_posicao(25), Funcao::Atacante);
    }

    #[test]
    fn age_counts_full_years_on_the_career_date() {
        let p = PlayerRaw {
            player_id: 1,
            nome: String::new(),
            nascimento: Date(20000721),
            posicao: 0,
            nacionalidade: 0,
            overall: 0,
            potencial: 0,
            atributos: [0; TOTAL_ATRIBUTOS],
            clube_id: None,
            clube: String::new(),
            resto_do_mundo: false,
            pe: Pe::Direito,
            liga_id: None,
            contrato_ate: 2030,
            ritmo_ataque: RitmoTrabalho::Medio,
            ritmo_defesa: RitmoTrabalho::Medio,
            estrelas_drible: 3,
            pe_fraco: 3,
            altura: 180,
        };
        assert_eq!(p.idade(Date(20350720)), 34);
        assert_eq!(p.idade(Date(20350721)), 35);
    }

    /// Oráculo com o save versionado e o banco estático instalado
    /// (conferido com `fifa16_db_parser.py`). Pulado sem o jogo.
    #[test]
    fn real_save_reads_every_player_with_name_club_and_nation() {
        let Some(estatico) = banco_estatico() else {
            eprintln!("banco estático do FIFA 16 não instalado; teste pulado");
            return;
        };
        let (jogadores, nacoes) = ler_de_arquivos(&backup(), &estatico).expect("save legível");
        assert!(jogadores.len() > 30_000, "{}", jogadores.len());
        assert!(nacoes.len() > 200);
        let sem_nome = jogadores.iter().filter(|j| j.nome.is_empty()).count();
        assert!(sem_nome < 10, "{sem_nome} jogadores sem nome");
        let mbappe = jogadores.iter().find(|j| j.player_id == 231747).expect("Mbappé no save");
        assert_eq!(mbappe.nome, "Kylian Mbappé");
        assert_eq!(mbappe.nascimento, Date(19981220));
        assert_eq!(nome_posicao(mbappe.posicao), "ATA");
        assert_eq!((mbappe.ritmo_ataque, mbappe.ritmo_defesa), (RitmoTrabalho::Alto, RitmoTrabalho::Baixo));
        assert_eq!((mbappe.estrelas_drible, mbappe.pe_fraco), (5, 4));
        assert!((80..=99).contains(&mbappe.overall));
        // em 2035 ele tem 36 anos: a velocidade caiu, a finalização não
        assert!(mbappe.atributo(Atributo::Finalizacao) >= 75);
        assert!(!mbappe.clube.is_empty());
        let franca = nacoes.iter().find(|n| n.id == mbappe.nacionalidade).expect("nação do Mbappé");
        assert_eq!(franca.nome, "France");
        assert_eq!(franca.confederacao, Confederacao::Europa);
        assert!(jogadores.iter().all(|j| j.atributos.iter().all(|&v| (1..=99).contains(&v))));
        // contratos: anos de verdade, e quase todo mundo de clube tem liga
        assert!(jogadores.iter().filter(|j| j.clube_id.is_some()).all(|j| (2000..=2100).contains(&j.contrato_ate)));
        let com_liga = jogadores.iter().filter(|j| j.liga_id.is_some()).count(); // inclui ligas fora do filtro
        assert!(com_liga > jogadores.len() / 2, "{com_liga}");
        // altura: centímetros de verdade (150–215), média perto de 1,80 m
        assert!(jogadores.iter().all(|j| (150..=215).contains(&j.altura)), "altura fora da faixa");
        let media = jogadores.iter().map(|j| u64::from(j.altura)).sum::<u64>() / jogadores.len() as u64;
        assert!((175..=185).contains(&media), "altura média {media}");
        assert!((170..=186).contains(&mbappe.altura), "Mbappé: {}", mbappe.altura);
        // canhotos existem, mas são minoria
        let canhotos = jogadores.iter().filter(|j| j.pe == Pe::Esquerdo).count();
        assert!(canhotos > jogadores.len() / 10 && canhotos < jogadores.len() / 2, "{canhotos}");
    }
}
