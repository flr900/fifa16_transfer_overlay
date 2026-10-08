//! `scout::persistence` — o arquivo de estado do Scout, um por carreira.
//!
//! Cada carreira tem o próprio JSON em
//! `%LOCALAPPDATA%\FifaCompanion\scout\<hash>.json`, onde `<hash>` é o
//! SHA-256 da identidade do save (AD-11, Story 1.1). Arquivo por save
//! elimina por design o caso "Missões de outra carreira".
//!
//! Regras (AD-7):
//! - existe UM `Arc<Mutex<ScoutStateFile>>` por arquivo; quem precisar
//!   mutar de outra thread (ex. a busca de uma Missão no Épico 2) recebe
//!   um clone de `EstadoPersistido`, que compartilha o mesmo mutex;
//! - toda mutação — entidade de domínio ou `ui_prefs` — é write-through:
//!   trava → aplica numa cópia → serializa → grava o arquivo inteiro →
//!   só então troca o estado em memória → destrava. Se a gravação falhar
//!   a memória fica como estava, então memória e disco nunca divergem;
//! - só `scout::state` chama este módulo (AD-1).
//!
//! Por que gravar num `.tmp` e renomear: se o jogo crashar no meio da
//! escrita, o arquivo anterior continua inteiro (NFR3). Não chamamos
//! `sync_all`: o risco é o processo do jogo morrer, não a máquina
//! desligar, e o flush custaria dezenas de ms no thread de render.
//!
//! Arquivo vazio, corrompido ou ilegível nunca derruba o Scout: começa
//! vazio com `tracing::warn!`, e um arquivo com conteúdo só é substituído
//! depois de guardado como `<hash>.json.corrompido-<ms>`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use super::state::{Escolhido, Missao, Olheiro, Relatorio};
use super::Aba;

/// Versão do formato do arquivo. Um arquivo com versão MAIOR foi gravado
/// por uma DLL mais nova: lemos o que entendemos e não gravamos por cima
/// (gravar com esta versão apagaria os campos que não conhecemos).
/// v2 (2026-10-04): Olheiros com nome/nação/estrelas/mercados, Lista de
/// Escolhidos e ofertas contratadas — uma DLL v1 apagaria tudo isso.
pub const VERSAO_FORMATO: u32 = 2;

/// Conteúdo do JSON de estado de uma carreira.
///
/// Todos os campos têm `default`: um campo que faltar (arquivo de uma
/// versão anterior) vira vazio em vez de invalidar o arquivo inteiro.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoutStateFile {
    #[serde(default = "versao_atual")]
    pub versao: u32,
    #[serde(default)]
    pub olheiros: Vec<Olheiro>,
    #[serde(default)]
    pub missoes: Vec<Missao>,
    #[serde(default)]
    pub relatorios: Vec<Relatorio>,
    /// Lista de Escolhidos (Épico 6).
    #[serde(default)]
    pub escolhidos: Vec<Escolhido>,
    /// Ofertas do mercado de Olheiros já contratadas (não voltam à lista).
    #[serde(default)]
    pub ofertas_contratadas: Vec<uuid::Uuid>,
    /// Épico 7: o nível de conhecimento que o JOGO tinha de cada jogador
    /// antes de a Central mexer nele (0 = não tinha registro). É o piso: a
    /// Central só rebaixa o que ela mesma subiu, e nunca abaixo disto.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub nivel_original: std::collections::BTreeMap<u32, u8>,
    /// Épico 7: valor de transferência EXATO que o jogo calculou, colhido da
    /// tela do jogo quando o jogador passou por ela (player id → valor).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub valores_do_jogo: std::collections::BTreeMap<u32, crate::scout::state::ValorDoJogo>,
    /// Atratividade do clube fixada para o mês do mercado de Olheiros:
    /// `(período, atratividade)`.
    #[serde(default)]
    pub mercado_do_mes: Option<(u32, u8)>,
    /// Preferência de UI inválida (ex. aba que não existe mais) não pode
    /// custar os Olheiros: só esta seção volta ao padrão.
    #[serde(default, deserialize_with = "ou_padrao")]
    pub ui_prefs: UiPrefs,
}

impl Default for ScoutStateFile {
    fn default() -> Self {
        ScoutStateFile {
            versao: VERSAO_FORMATO,
            olheiros: Vec::new(),
            missoes: Vec::new(),
            relatorios: Vec::new(),
            escolhidos: Vec::new(),
            ofertas_contratadas: Vec::new(),
            nivel_original: std::collections::BTreeMap::new(),
            valores_do_jogo: std::collections::BTreeMap::new(),
            mercado_do_mes: None,
            ui_prefs: UiPrefs::default(),
        }
    }
}

fn versao_atual() -> u32 {
    VERSAO_FORMATO
}

/// Preferências de UI persistidas por carreira (AD-7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub aba_ativa: Aba,
    /// Visão dos Relatórios (Story 2.6). No JSON: `"tabular"` / `"cards"`.
    pub densidade: Densidade,
    /// Visão da aba Olheiros (2026-10-03); Cards por padrão.
    pub densidade_olheiros: Densidade,
    /// Visão de cada lista de jogadores (2026-10-08): Cards, ou Tabular para
    /// ordenar. Só a Base do Scout, que cresce sem parar, abre em Tabular.
    pub modo_escolhidos: Densidade,
    pub modo_relatorios: Densidade,
    pub modo_base: Densidade,
    pub modo_relatorio_aberto: Densidade,
    /// Épico 7: "Sincronizar com o FIFA". Ligado por padrão (decisão de
    /// 2026-10-06); só vai para o arquivo quando desligado.
    #[serde(skip_serializing_if = "e_verdadeiro")]
    pub sincronizar_com_o_jogo: bool,
}

fn e_verdadeiro(valor: &bool) -> bool {
    *valor
}

impl Default for UiPrefs {
    fn default() -> Self {
        UiPrefs {
            aba_ativa: Aba::Olheiros,
            densidade: Densidade::Tabular,
            densidade_olheiros: Densidade::Cards,
            modo_escolhidos: Densidade::Cards,
            modo_relatorios: Densidade::Cards,
            modo_base: Densidade::Tabular,
            modo_relatorio_aberto: Densidade::Cards,
            sincronizar_com_o_jogo: true,
        }
    }
}

/// Tabular (linhas densas, números alinhados) ou Cards (rostos).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Densidade {
    #[default]
    Tabular,
    Cards,
}

/// Desserializa `T`; se o valor não servir, usa `T::default()` e avisa.
fn ou_padrao<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let valor = serde_json::Value::deserialize(deserializer)?;
    Ok(T::deserialize(valor).unwrap_or_else(|err| {
        tracing::warn!("[scout::persistence] Seção inválida no arquivo de estado, usando o padrão: {err}");
        T::default()
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErroPersistencia {
    /// O estado está só em memória e não aceita mudanças: sem pasta de
    /// dados, arquivo ilegível que não deu para guardar, ou formato mais
    /// novo que esta DLL.
    SomenteLeitura,
    /// Falha ao gravar o arquivo; o estado em memória NÃO mudou.
    Gravacao(String),
}

/// `%LOCALAPPDATA%\FifaCompanion\scout`.
pub fn diretorio_padrao() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("FifaCompanion").join("scout"))
}

/// Estado de UMA carreira, com o caminho do arquivo dela. Clonar
/// compartilha o mesmo mutex (AD-7).
#[derive(Debug, Clone)]
pub struct EstadoPersistido {
    dados: Arc<Mutex<ScoutStateFile>>,
    /// `None` = somente leitura (ver `ErroPersistencia::SomenteLeitura`).
    caminho: Option<PathBuf>,
}

impl EstadoPersistido {
    /// Carrega (ou cria) o arquivo de `id_save` em `diretorio`. Nunca
    /// falha: no pior caso devolve um estado vazio somente leitura.
    pub fn carregar(diretorio: Option<&Path>, id_save: &str) -> Self {
        let Some(diretorio) = diretorio else {
            tracing::warn!(
                "[scout::persistence] Sem pasta de dados local (%LOCALAPPDATA%): o estado do Scout não será salvo."
            );
            return Self::somente_leitura(ScoutStateFile::default());
        };
        if !id_valido(id_save) {
            tracing::warn!("[scout::persistence] Id de save inválido para nome de arquivo: {id_save:?}.");
            return Self::somente_leitura(ScoutStateFile::default());
        }
        let caminho = diretorio.join(format!("{id_save}.json"));

        let bytes = match fs::read(&caminho) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                tracing::warn!(
                    "[scout::persistence] Arquivo de estado não existe, criando vazio: {}",
                    caminho.display()
                );
                return Self::novo_gravado(caminho);
            }
            Err(err) => {
                // Não dá para guardar o que não dá para ler: não gravamos
                // por cima nesta sessão.
                tracing::warn!(
                    "[scout::persistence] Não foi possível ler {} ({err}); estado vazio, sem gravar.",
                    caminho.display()
                );
                return Self::somente_leitura(ScoutStateFile::default());
            }
        };

        if bytes.iter().all(u8::is_ascii_whitespace) {
            tracing::warn!("[scout::persistence] Arquivo de estado vazio, recriando: {}", caminho.display());
            return Self::novo_gravado(caminho);
        }

        match serde_json::from_slice::<ScoutStateFile>(&bytes) {
            Ok(mut dados) if dados.versao < VERSAO_FORMATO => {
                // Formato anterior: tudo o que ele tinha continua igual
                // (campos novos com `default`); a próxima gravação já sai
                // na versão atual.
                tracing::info!(
                    "[scout::persistence] {} estava no formato v{}; atualizado para v{}.",
                    caminho.display(),
                    dados.versao,
                    VERSAO_FORMATO
                );
                dados.versao = VERSAO_FORMATO;
                EstadoPersistido { dados: Arc::new(Mutex::new(dados)), caminho: Some(caminho) }
            }
            Ok(dados) if dados.versao > VERSAO_FORMATO => {
                tracing::warn!(
                    "[scout::persistence] {} tem formato v{} (esta DLL entende até v{}): lido sem gravar.",
                    caminho.display(),
                    dados.versao,
                    VERSAO_FORMATO
                );
                Self::somente_leitura(dados)
            }
            Ok(dados) => {
                tracing::info!(
                    "[scout::persistence] Estado carregado: {} ({} olheiros, {} missões, {} relatórios).",
                    caminho.display(),
                    dados.olheiros.len(),
                    dados.missoes.len(),
                    dados.relatorios.len()
                );
                EstadoPersistido { dados: Arc::new(Mutex::new(dados)), caminho: Some(caminho) }
            }
            Err(err) => {
                tracing::warn!("[scout::persistence] Arquivo de estado corrompido ({err}): {}", caminho.display());
                match guardar_copia(&caminho) {
                    Ok(copia) => {
                        tracing::warn!("[scout::persistence] Cópia do arquivo corrompido: {}", copia.display());
                        Self::novo_gravado(caminho)
                    }
                    Err(err) => {
                        tracing::warn!(
                            "[scout::persistence] Não foi possível guardar a cópia ({err}); estado vazio, sem gravar."
                        );
                        Self::somente_leitura(ScoutStateFile::default())
                    }
                }
            }
        }
    }

    fn somente_leitura(dados: ScoutStateFile) -> Self {
        EstadoPersistido { dados: Arc::new(Mutex::new(dados)), caminho: None }
    }

    /// Estado vazio já gravado em disco (o arquivo existe a partir do
    /// primeiro uso). Se a gravação falhar, a próxima mutação tenta de novo.
    fn novo_gravado(caminho: PathBuf) -> Self {
        let dados = ScoutStateFile::default();
        if let Err(err) = gravar(&caminho, &dados) {
            tracing::warn!("[scout::persistence] Falha ao criar {}: {err:?}", caminho.display());
        }
        EstadoPersistido { dados: Arc::new(Mutex::new(dados)), caminho: Some(caminho) }
    }

    /// Aceita mutações (há um arquivo em disco por trás).
    pub fn gravavel(&self) -> bool {
        self.caminho.is_some()
    }

    #[allow(dead_code)] // diagnóstico/log
    pub fn caminho(&self) -> Option<&Path> {
        self.caminho.as_deref()
    }

    /// Leitura sob o mesmo lock das mutações.
    pub fn ler<R>(&self, f: impl FnOnce(&ScoutStateFile) -> R) -> R {
        f(&travar(&self.dados))
    }

    /// Mutação write-through (AD-7). `f` roda numa cópia; o estado em
    /// memória só muda se o arquivo inteiro foi gravado.
    pub fn mutar<R>(&self, f: impl FnOnce(&mut ScoutStateFile) -> R) -> Result<R, ErroPersistencia> {
        let Some(caminho) = &self.caminho else {
            return Err(ErroPersistencia::SomenteLeitura);
        };
        let mut guarda = travar(&self.dados);
        let mut novo = guarda.clone();
        let resultado = f(&mut novo);
        gravar(caminho, &novo)?;
        *guarda = novo;
        Ok(resultado)
    }
}

/// Mutex envenenado (panic em outra thread) não derruba o Scout: o dado
/// lá dentro é sempre um estado completo, porque `mutar` só o troca
/// inteiro depois de gravar.
fn travar(dados: &Mutex<ScoutStateFile>) -> MutexGuard<'_, ScoutStateFile> {
    dados.lock().unwrap_or_else(|envenenado| envenenado.into_inner())
}

/// O id vira nome de arquivo: só aceitamos o hex do SHA-256 (AD-11), o
/// que também impede `..`/barras no caminho.
fn id_valido(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Grava o arquivo inteiro: `<arquivo>.tmp` e depois renomeia por cima
/// (no Windows o `rename` do std substitui o destino).
fn gravar(caminho: &Path, dados: &ScoutStateFile) -> Result<(), ErroPersistencia> {
    let falha = |etapa: &str, err: &dyn std::fmt::Display| {
        let msg = format!("{etapa} {}: {err}", caminho.display());
        tracing::warn!("[scout::persistence] Falha ao gravar: {msg}");
        ErroPersistencia::Gravacao(msg)
    };
    let json = serde_json::to_vec_pretty(dados).map_err(|err| falha("serializar", &err))?;
    if let Some(pasta) = caminho.parent() {
        fs::create_dir_all(pasta).map_err(|err| falha("criar pasta de", &err))?;
    }
    let mut temporario = caminho.as_os_str().to_owned();
    temporario.push(".tmp");
    let temporario = PathBuf::from(temporario);
    fs::write(&temporario, &json).map_err(|err| falha("escrever", &err))?;
    fs::rename(&temporario, caminho).map_err(|err| {
        let _ = fs::remove_file(&temporario);
        falha("renomear para", &err)
    })
}

/// Move o arquivo ilegível para `<arquivo>.corrompido-<ms desde 1970>`.
fn guardar_copia(caminho: &Path) -> io::Result<PathBuf> {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut nome = caminho.as_os_str().to_owned();
    nome.push(format!(".corrompido-{ms}"));
    let copia = PathBuf::from(nome);
    fs::rename(caminho, &copia)?;
    Ok(copia)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::save_repo::Date;
    use crate::scout::state::{Especializacao, StatusMissao, Tier};
    use uuid::Uuid;

    /// Pasta temporária única por teste, apagada no `drop`.
    pub(crate) struct PastaTemporaria(pub PathBuf);

    impl PastaTemporaria {
        pub(crate) fn nova() -> Self {
            let pasta = std::env::temp_dir().join("fifa_overlay_testes").join(Uuid::new_v4().to_string());
            fs::create_dir_all(&pasta).unwrap_or_else(|err| panic!("criar {}: {err}", pasta.display()));
            PastaTemporaria(pasta)
        }

        fn arquivos(&self) -> Vec<String> {
            let mut nomes: Vec<String> = fs::read_dir(&self.0)
                .map(|it| it.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            nomes.sort();
            nomes
        }
    }

    impl Drop for PastaTemporaria {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn json_do_arquivo(pasta: &PastaTemporaria, id: &str) -> serde_json::Value {
        let bytes = fs::read(pasta.0.join(format!("{id}.json"))).unwrap_or_default();
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    #[test]
    fn first_use_creates_the_file_with_empty_collections_and_ui_prefs() {
        let pasta = PastaTemporaria::nova();
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        assert!(estado.gravavel());
        assert_eq!(pasta.arquivos(), [format!("{ID_A}.json")]);
        assert_eq!(
            json_do_arquivo(&pasta, ID_A),
            serde_json::json!({
                "versao": 2,
                "olheiros": [],
                "missoes": [],
                "relatorios": [],
                "escolhidos": [],
                "ofertas_contratadas": [],
                "mercado_do_mes": null,
                "ui_prefs": {
                    "aba_ativa": "olheiros",
                    "densidade": "tabular",
                    "densidade_olheiros": "cards",
                    "modo_escolhidos": "cards",
                    "modo_relatorios": "cards",
                    "modo_base": "tabular",
                    "modo_relatorio_aberto": "cards"
                }
            })
        );
    }

    #[test]
    fn mutation_writes_the_whole_file_and_survives_a_reload() {
        let pasta = PastaTemporaria::nova();
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        estado.mutar(|d| d.ui_prefs.aba_ativa = Aba::Base).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(json_do_arquivo(&pasta, ID_A)["ui_prefs"]["aba_ativa"], "base");
        assert_eq!(pasta.arquivos(), [format!("{ID_A}.json")], "sem .tmp sobrando");

        // "reiniciar o jogo": novo carregamento do mesmo arquivo
        let recarregado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        assert_eq!(recarregado.ler(|d| d.ui_prefs.aba_ativa), Aba::Base);
    }

    #[test]
    fn clones_share_one_mutex() {
        let pasta = PastaTemporaria::nova();
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        let outro = estado.clone();
        let thread = std::thread::spawn(move || outro.mutar(|d| d.ui_prefs.aba_ativa = Aba::Missoes));
        assert!(matches!(thread.join(), Ok(Ok(()))));
        assert_eq!(estado.ler(|d| d.ui_prefs.aba_ativa), Aba::Missoes);
    }

    #[test]
    fn ids_and_dates_follow_ad12() {
        let olheiro = Olheiro { id: Uuid::new_v4(), especializacao: Especializacao::Tatico, tier: Tier::Experiente, ..Default::default() };
        let missao = Missao {
            criada_em: Date(20260703),
            prazo_estimado: Date(20261015),
            ..Missao::de_teste(olheiro.id, StatusMissao::Pendente)
        };
        let relatorio = Relatorio::de_teste(missao.id);

        let pasta = PastaTemporaria::nova();
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        let (o, m, r) = (olheiro.clone(), missao.clone(), relatorio.clone());
        estado
            .mutar(move |d| {
                d.olheiros.push(o);
                d.missoes.push(m);
                d.relatorios.push(r);
            })
            .unwrap_or_else(|e| panic!("{e:?}"));

        let json = json_do_arquivo(&pasta, ID_A);
        assert_eq!(json["olheiros"][0]["id"], olheiro.id.to_string());
        assert_eq!(json["olheiros"][0]["especializacao"], "tatico");
        assert_eq!(json["olheiros"][0]["tier"], "experiente");
        assert_eq!(json["missoes"][0]["olheiro_id"], olheiro.id.to_string());
        assert_eq!(json["missoes"][0]["criada_em"], 20260703);
        assert_eq!(json["missoes"][0]["prazo_estimado"], 20261015);
        assert_eq!(json["missoes"][0]["status"], "Pendente");
        assert_eq!(json["missoes"][0]["modo_busca"], "rapida");
        assert_eq!(json["missoes"][0]["filtros"]["overall"]["min"], 50);
        assert_eq!(json["missoes"][0]["tipo"], "geral");
        assert_eq!(json["missoes"][0]["amplitude"], "mundo");
        assert!(json["missoes"][0]["estimativa"]["custo"].is_number());
        assert_eq!(json["relatorios"][0]["missao_id"], missao.id.to_string());
        let canonico = json["olheiros"][0]["id"].as_str().unwrap_or_default();
        assert_eq!(canonico.len(), 36);
        assert_eq!(Uuid::parse_str(canonico).map(|u| u.get_version_num()), Ok(4));

        let recarregado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        assert_eq!(
            recarregado.ler(|d| (d.olheiros.clone(), d.missoes.clone(), d.relatorios.clone())),
            (vec![olheiro], vec![missao], vec![relatorio])
        );
    }

    #[test]
    fn corrupt_file_is_kept_as_a_copy_before_starting_empty() {
        let pasta = PastaTemporaria::nova();
        let original = br#"{"versao": 1, "olheiros": [ {"id": "#;
        let _ = fs::write(pasta.0.join(format!("{ID_A}.json")), original);

        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        assert!(estado.gravavel());
        assert_eq!(estado.ler(Clone::clone), ScoutStateFile::default());

        let arquivos = pasta.arquivos();
        assert_eq!(arquivos.len(), 2, "{arquivos:?}");
        let copia = arquivos.iter().find(|n| n.contains(".corrompido-")).cloned().unwrap_or_default();
        assert_eq!(fs::read(pasta.0.join(copia)).unwrap_or_default(), original);
        assert_eq!(json_do_arquivo(&pasta, ID_A)["versao"], 2, "arquivo novo é válido");
    }

    #[test]
    fn empty_file_starts_empty() {
        let pasta = PastaTemporaria::nova();
        let _ = fs::write(pasta.0.join(format!("{ID_A}.json")), b"  \r\n");
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        assert!(estado.gravavel());
        assert_eq!(estado.ler(Clone::clone), ScoutStateFile::default());
        assert_eq!(json_do_arquivo(&pasta, ID_A)["versao"], 2);
    }

    #[test]
    fn a_v1_file_keeps_everything_and_is_written_back_as_v2() {
        let pasta = PastaTemporaria::nova();
        let id = Uuid::new_v4();
        let conteudo = format!(
            r#"{{"versao": 1, "olheiros": [{{"id": "{id}", "especializacao": "tatico", "tier": "elite", "contratado_em": 20260701}}],
                "missoes": [], "relatorios": [], "ui_prefs": {{"aba_ativa": "missoes"}}}}"#
        );
        let _ = fs::write(pasta.0.join(format!("{ID_A}.json")), conteudo);
        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        assert!(estado.gravavel(), "v1 é mais antigo: pode gravar");
        let olheiro = estado.ler(|d| d.olheiros[0].clone());
        assert_eq!((olheiro.id, olheiro.tier, olheiro.contratado_em), (id, Tier::Elite, Some(Date(20260701))));
        assert_eq!(olheiro.perfil, None, "sem estrelas gravadas: vale o perfil do v1");
        assert!(olheiro.nome.is_empty() && olheiro.mercados.is_empty());
        assert_eq!(estado.ler(|d| d.ui_prefs.aba_ativa), Aba::Missoes);
        estado.mutar(|d| d.ofertas_contratadas.clear()).unwrap_or_else(|e| panic!("{e:?}"));
        let json = json_do_arquivo(&pasta, ID_A);
        assert_eq!(json["versao"], 2);
        assert_eq!(json["olheiros"][0]["tier"], "elite");
    }

    #[test]
    fn invalid_ui_prefs_do_not_cost_the_domain_data() {
        let pasta = PastaTemporaria::nova();
        let id = Uuid::new_v4();
        let conteudo = format!(
            r#"{{"olheiros": [{{"id": "{id}", "especializacao": "cacador_de_jovens", "tier": "elite"}}],
                "ui_prefs": {{"aba_ativa": "Mercado"}}}}"#
        );
        let _ = fs::write(pasta.0.join(format!("{ID_A}.json")), conteudo);

        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        let esperado = Olheiro { id, especializacao: Especializacao::CacadorDeJovens, tier: Tier::Elite, ..Default::default() };
        assert_eq!(estado.ler(|d| d.olheiros.clone()), vec![esperado]);
        assert_eq!(estado.ler(|d| d.ui_prefs.clone()), UiPrefs::default());
        assert_eq!(estado.ler(|d| d.versao), VERSAO_FORMATO, "versão ausente = atual");
    }

    #[test]
    fn newer_format_is_read_but_never_overwritten() {
        let pasta = PastaTemporaria::nova();
        let conteudo = r#"{"versao": 3, "ui_prefs": {"aba_ativa": "base"}, "campo_novo": 1}"#;
        let _ = fs::write(pasta.0.join(format!("{ID_A}.json")), conteudo);

        let estado = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        assert_eq!(estado.ler(|d| d.ui_prefs.aba_ativa), Aba::Base);
        assert_eq!(estado.mutar(|d| d.ui_prefs.aba_ativa = Aba::Missoes), Err(ErroPersistencia::SomenteLeitura));
        assert_eq!(estado.ler(|d| d.ui_prefs.aba_ativa), Aba::Base, "memória não mudou");
        assert_eq!(fs::read_to_string(pasta.0.join(format!("{ID_A}.json"))).unwrap_or_default(), conteudo);
    }

    #[test]
    fn each_career_reads_and_writes_only_its_own_file() {
        let pasta = PastaTemporaria::nova();
        let a = EstadoPersistido::carregar(Some(&pasta.0), ID_A);
        let b = EstadoPersistido::carregar(Some(&pasta.0), ID_B);
        a.mutar(|d| d.ui_prefs.aba_ativa = Aba::Base).unwrap_or_else(|e| panic!("{e:?}"));
        b.mutar(|d| d.ui_prefs.aba_ativa = Aba::Relatorios).unwrap_or_else(|e| panic!("{e:?}"));

        assert_eq!(json_do_arquivo(&pasta, ID_A)["ui_prefs"]["aba_ativa"], "base");
        assert_eq!(json_do_arquivo(&pasta, ID_B)["ui_prefs"]["aba_ativa"], "relatorios");
    }

    #[test]
    fn no_data_dir_or_bad_id_means_read_only_and_no_files() {
        let semdir = EstadoPersistido::carregar(None, ID_A);
        assert!(!semdir.gravavel());
        assert_eq!(semdir.mutar(|_| ()), Err(ErroPersistencia::SomenteLeitura));

        let pasta = PastaTemporaria::nova();
        for id in ["", "..\\..\\x", "ABC", &"a".repeat(65)] {
            assert!(!EstadoPersistido::carregar(Some(&pasta.0), id).gravavel(), "{id:?}");
        }
        assert!(pasta.arquivos().is_empty());
    }

    #[test]
    fn missing_data_dir_is_created_on_first_write() {
        let pasta = PastaTemporaria::nova();
        let funda = pasta.0.join("FifaCompanion").join("scout");
        let estado = EstadoPersistido::carregar(Some(&funda), ID_A);
        assert!(estado.gravavel());
        assert!(funda.join(format!("{ID_A}.json")).is_file());
    }

    #[test]
    fn the_sync_switch_defaults_to_on_is_saved_only_when_off_and_old_files_load() {
        let padrao = UiPrefs::default();
        assert!(padrao.sincronizar_com_o_jogo);
        assert!(!serde_json::to_string(&padrao).expect("serializa").contains("sincronizar"), "ligado não vai para o arquivo");
        let desligado = UiPrefs { sincronizar_com_o_jogo: false, ..UiPrefs::default() };
        let json = serde_json::to_string(&desligado).expect("serializa");
        assert!(json.contains("\"sincronizar_com_o_jogo\":false"));
        assert!(!serde_json::from_str::<UiPrefs>(&json).expect("recarrega").sincronizar_com_o_jogo);
        // arquivo de antes do Épico 7 (sem o campo): ligado
        let antigo: UiPrefs = serde_json::from_str(r#"{"aba_ativa":"olheiros","densidade":"tabular","densidade_olheiros":"cards"}"#).expect("carrega");
        assert!(antigo.sincronizar_com_o_jogo);
    }
}
