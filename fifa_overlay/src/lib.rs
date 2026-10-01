//! FIFA 16 Companion Overlay — DLL injetada no processo do fifa16.exe.
//!
//! MVP (fase 1): validar o pipeline inteiro (injeção + hook do
//! Present via DirectX 11 + renderização de uma janela ImGui simples
//! por cima do jogo), e leitura in-process da database de jogadores
//! como prova de conceito.
//!
//! IMPORTANTE: qualquer operação potencialmente lenta (como varrer
//! toda a memória do processo) NUNCA deve rodar diretamente dentro de
//! `ImguiRenderLoop::render` — isso bloqueia a thread de render (a
//! mesma que desenha o jogo inteiro), travando a tela por completo até
//! terminar. Rodamos esse tipo de trabalho numa thread separada e
//! compartilhamos o resultado via `Arc<Mutex<..>>`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use hudhook::ImguiRenderLoop;
use imgui::Condition;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

mod async_task;
mod fifa_db;
mod memscan;
mod pointer_scan;
mod save_repo;

use async_task::{AsyncTask, TaskState};

/// Playerid usado como alvo de teste nesta fase de prova de conceito
/// (Ibrahim Mbaye, já validado em sessões anteriores). No futuro isso
/// vira configurável/dinâmico (jogador selecionado na UI do jogo).
const TEST_PLAYER_ID: u32 = 74449;

fn setup_tracing() {
    let file_appender = tracing_appender::rolling::never(std::env::temp_dir(), "fifa_overlay.log");

    let _ = tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false)
                .with_level(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .with(EnvFilter::new("trace"))
        .try_init();
}

#[derive(Debug, Clone)]
struct PlayerScanResult {
    region_base: usize,
    strength: Option<u32>,
    overall: Option<u32>,
    potential: Option<u32>,
}

#[derive(Debug, Clone)]
enum ScanState {
    Idle,
    Running,
    Done {
        db_locations_found: usize,
        result: Option<PlayerScanResult>,
        error: Option<String>,
        elapsed_ms: u128,
    },
}

/// Executa o scan pesado (varredura de memória + parse da database)
/// numa thread separada, escrevendo o resultado em `state` quando
/// terminar. `state` é compartilhado com a thread de render via Arc.
fn spawn_scan_thread(state: Arc<Mutex<ScanState>>, scan_in_progress: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let start = Instant::now();
        tracing::info!("[scan-thread] Iniciando scan de memória por databases...");

        let locations = memscan::find_databases_in_memory();
        let db_locations_found = locations.len();
        tracing::info!(
            "[scan-thread] {} localização(ões) de database encontrada(s) em {:?}.",
            db_locations_found,
            start.elapsed()
        );

        let mut found_result = None;
        let mut found_error = None;

        for loc in &locations {
            let bytes = &loc.region_bytes;

            let Some(tables) = fifa_db::parse_database_tables(bytes, loc.offset_in_region) else {
                continue;
            };

            let Some(czum) = tables.iter().find(|t| t.short_name == *b"CZUM") else { continue };

            let Some(playerid_field) = fifa_db::field_by_shortname(czum, "ykFq") else { continue };

            let Some(record_index) =
                fifa_db::find_player_record_index(bytes, czum, playerid_field, TEST_PLAYER_ID)
            else {
                continue;
            };

            tracing::info!(
                "[scan-thread] Jogador {} encontrado no registro #{} da tabela CZUM (região 0x{:X}).",
                TEST_PLAYER_ID,
                record_index,
                loc.region_base
            );

            let strength = fifa_db::field_by_shortname(czum, "nmgT")
                .and_then(|f| fifa_db::read_field_for_record(bytes, czum, f, record_index))
                .map(|v| v + 1);
            let overall = fifa_db::field_by_shortname(czum, "UERs")
                .and_then(|f| fifa_db::read_field_for_record(bytes, czum, f, record_index))
                .map(|v| v + 1);
            let potential = fifa_db::field_by_shortname(czum, "mpuH")
                .and_then(|f| fifa_db::read_field_for_record(bytes, czum, f, record_index))
                .map(|v| v + 1);

            found_result = Some(PlayerScanResult {
                region_base: loc.region_base,
                strength,
                overall,
                potential,
            });
            break;
        }

        if found_result.is_none() {
            found_error = Some(format!(
                "Jogador {} não encontrado em nenhuma das {} database(s).",
                TEST_PLAYER_ID, db_locations_found
            ));
        }

        let elapsed_ms = start.elapsed().as_millis();
        tracing::info!("[scan-thread] Concluído em {}ms.", elapsed_ms);

        if let Ok(mut guard) = state.lock() {
            *guard = ScanState::Done {
                db_locations_found,
                result: found_result,
                error: found_error,
                elapsed_ms,
            };
        }
        scan_in_progress.store(false, Ordering::SeqCst);
    });
}

#[derive(Debug, Clone)]
enum PointerScanState {
    Idle,
    Running,
    Done {
        anchors: Vec<pointer_scan::AnchorFound>,
        levels_explored: usize,
        last_level_hit_count: usize,
        elapsed_ms: u128,
        level1_hits: Vec<pointer_scan::PointerHit>,
    },
}

fn spawn_pointer_scan_thread(
    target_address: usize,
    state: Arc<Mutex<PointerScanState>>,
    scan_in_progress: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let start = Instant::now();
        tracing::info!(
            "[pointer-scan-thread] Iniciando busca de âncora para 0x{:X}...",
            target_address
        );

        let outcome = pointer_scan::find_pointer_chain(target_address, 4, 0);
        let elapsed_ms = start.elapsed().as_millis();

        tracing::info!(
            "[pointer-scan-thread] Concluído em {}ms. {} âncora(s) encontrada(s) em {} nível(is).",
            elapsed_ms,
            outcome.anchors.len(),
            outcome.levels_explored
        );

        for a in &outcome.anchors {
            tracing::info!(
                "[pointer-scan-thread] ÂNCORA: {}+0x{:X} -> 0x{:X}",
                a.module_name,
                a.module_offset,
                a.pointer_value
            );
        }

        if let Ok(mut guard) = state.lock() {
            *guard = PointerScanState::Done {
                anchors: outcome.anchors,
                levels_explored: outcome.levels_explored,
                last_level_hit_count: outcome.last_level_hit_count,
                elapsed_ms,
                level1_hits: outcome.level1_hits,
            };
        }
        scan_in_progress.store(false, Ordering::SeqCst);
    });
}

#[derive(Debug, Clone)]
enum ValueScanState {
    Idle,
    Running,
    Done { addresses: Vec<usize>, elapsed_ms: u128 },
}

fn spawn_value_scan_thread(
    target_value: i32,
    previous_addresses: Option<Vec<usize>>,
    state: Arc<Mutex<ValueScanState>>,
    scan_in_progress: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let start = Instant::now();

        let addresses = match previous_addresses {
            Some(prev) => {
                tracing::info!(
                    "[value-scan-thread] Filtrando {} endereço(s) anteriores por valor {}...",
                    prev.len(),
                    target_value
                );
                memscan::filter_addresses_by_i32_value(&prev, target_value)
            }
            None => {
                tracing::info!("[value-scan-thread] Procurando valor {} (scan novo)...", target_value);
                memscan::scan_for_i32_value(target_value)
            }
        };

        let elapsed_ms = start.elapsed().as_millis();

        tracing::info!(
            "[value-scan-thread] {} ocorrência(s) encontrada(s) em {}ms.",
            addresses.len(),
            elapsed_ms
        );

        if let Ok(mut guard) = state.lock() {
            *guard = ValueScanState::Done { addresses, elapsed_ms };
        }
        scan_in_progress.store(false, Ordering::SeqCst);
    });
}

pub struct FifaOverlay {
    frame_count: u64,
    scan_state: Arc<Mutex<ScanState>>,
    scan_in_progress: Arc<AtomicBool>,
    pointer_scan_state: Arc<Mutex<PointerScanState>>,
    pointer_scan_in_progress: Arc<AtomicBool>,
    target_address_input: String,
    value_scan_state: Arc<Mutex<ValueScanState>>,
    value_scan_in_progress: Arc<AtomicBool>,
    value_scan_input: String,
    write_test_result: Option<String>,
    /// Localização da database da carreira (Story 1.1) — andaime de
    /// verificação manual; a Story 1.2 troca isto pelo painel real.
    career_task: AsyncTask<()>,
    career_lines: Vec<String>,
}

impl FifaOverlay {
    fn new() -> Self {
        setup_tracing();
        tracing::info!("FifaOverlay::new() — DLL injetada com sucesso, overlay inicializando.");
        Self {
            frame_count: 0,
            scan_state: Arc::new(Mutex::new(ScanState::Idle)),
            scan_in_progress: Arc::new(AtomicBool::new(false)),
            pointer_scan_state: Arc::new(Mutex::new(PointerScanState::Idle)),
            pointer_scan_in_progress: Arc::new(AtomicBool::new(false)),
            target_address_input: String::from("8E8CF4B0"),
            value_scan_state: Arc::new(Mutex::new(ValueScanState::Idle)),
            value_scan_in_progress: Arc::new(AtomicBool::new(false)),
            value_scan_input: String::new(),
            write_test_result: None,
            career_task: AsyncTask::new(),
            career_lines: Vec::new(),
        }
    }

    /// Relê data, orçamento e hash do save (leituras de poucos bytes,
    /// seguras para chamar do render — o scan pesado já foi feito pelo
    /// `AsyncTask` de localização).
    fn refresh_career_lines(&mut self) {
        let mut lines = Vec::new();
        lines.push(match save_repo::read_current_date() {
            Ok(date) => format!("Data da carreira: {}", date.0),
            Err(err) => format!("Data: {err}"),
        });
        lines.push(match save_repo::read_transfer_budget() {
            Ok(budget) => format!("Orçamento de transferência: {budget}"),
            Err(err) => format!("Orçamento: {err}"),
        });
        lines.push(match save_repo::identify_active_save() {
            Ok(hash) => format!("Hash do save: {hash}"),
            Err(err) => format!("Hash do save: {err}"),
        });
        self.career_lines = lines;
    }

    /// Testa escrita no blob CZUM: acha o registro do jogador de
    /// teste e escreve um novo valor de `strength`, usando
    /// `WriteProcessMemory` protegido. Roda de forma síncrona (rápida
    /// o suficiente por não precisar re-escanear toda a memória — só
    /// achar a database de novo, que é bem mais barato que o scan
    /// completo original).
    fn test_write_strength(&mut self, new_value: u32) {
        let locations = memscan::find_databases_in_memory();

        for loc in &locations {
            let bytes = &loc.region_bytes;
            let Some(tables) = fifa_db::parse_database_tables(bytes, loc.offset_in_region) else {
                continue;
            };
            let Some(czum) = tables.iter().find(|t| t.short_name == *b"CZUM") else { continue };
            let Some(playerid_field) = fifa_db::field_by_shortname(czum, "ykFq") else { continue };
            let Some(record_index) =
                fifa_db::find_player_record_index(bytes, czum, playerid_field, TEST_PLAYER_ID)
            else {
                continue;
            };
            let Some(strength_field) = fifa_db::field_by_shortname(czum, "nmgT") else { continue };

            let loc_info = fifa_db::locate_packed_field(czum, strength_field, record_index);
            let abs_addr = loc.region_base + loc_info.byte_offset_in_region;

            let end = loc_info.byte_offset_in_region + loc_info.byte_count;
            let Some(current_bytes) = bytes.get(loc_info.byte_offset_in_region..end) else {
                tracing::warn!(
                    "[write-test] Offset fora dos limites do buffer (offset={}, end={}, len={}).",
                    loc_info.byte_offset_in_region,
                    end,
                    bytes.len()
                );
                continue;
            };

            // range_low=1 para strength, então o valor "cru" a gravar é new_value - 1
            let raw_value = new_value.saturating_sub(1);
            let new_bytes = fifa_db::build_packed_bytes(
                current_bytes,
                loc_info.shift,
                strength_field.depth,
                raw_value,
            );

            let ok = memscan::write_bytes_at(abs_addr, &new_bytes);

            tracing::info!(
                "[write-test] Escrita em 0x{:X} ({} bytes): ok={}",
                abs_addr,
                new_bytes.len(),
                ok
            );

            self.write_test_result = Some(format!(
                "Escrita em 0x{:X}: {}",
                abs_addr,
                if ok { "OK (verifique a tela de edição)" } else { "FALHOU" }
            ));
            return;
        }

        self.write_test_result = Some("Jogador de teste não encontrado.".to_string());
    }
}

impl ImguiRenderLoop for FifaOverlay {
    fn render(&mut self, ui: &mut imgui::Ui) {
        self.frame_count += 1;

        let is_running = self.scan_in_progress.load(Ordering::SeqCst);

        ui.window("FIFA 16 Companion")
            .size([500.0, 700.0], Condition::FirstUseEver)
            .position([32.0, 32.0], Condition::FirstUseEver)
            .resizable(true)
            .always_vertical_scrollbar(true)
            .build(|| {
                ui.text("Overlay funcionando!");
                ui.separator();
                ui.text(format!("Frames renderizados: {}", self.frame_count));
                ui.spacing();

                if ui.button("Descarregar DLL (eject)") {
                    tracing::info!("Eject solicitado pelo usuário.");
                    hudhook::eject();
                }

                ui.spacing();
                ui.separator();
                ui.text("Carreira (save_repo)");

                let career_state = self.career_task.poll();
                let career_locating = matches!(career_state, TaskState::Running);
                let career_label = if career_locating {
                    "Localizando carreira... (background)"
                } else {
                    "Localizar carreira"
                };
                if ui.button(career_label) && !career_locating {
                    save_repo::start_locating(&self.career_task);
                }

                match &career_state {
                    TaskState::Idle => ui.text("(carreira ainda não localizada)"),
                    TaskState::Running => ui.text("Localizando carreira, aguarde (~17s)..."),
                    TaskState::Failed(err) => {
                        ui.text_colored([1.0, 0.4, 0.4, 1.0], format!("{err}"));
                    }
                    TaskState::Done(()) => {
                        ui.text("Carreira localizada.");
                        if ui.button("Reler valores") {
                            self.refresh_career_lines();
                        }
                        for line in &self.career_lines {
                            ui.text(line);
                        }
                    }
                }

                ui.spacing();
                ui.separator();

                let button_label = if is_running {
                    "Escaneando... (rodando em background)"
                } else {
                    "Escanear memória (achar jogador de teste)"
                };

                if ui.button(button_label) && !is_running {
                    self.scan_in_progress.store(true, Ordering::SeqCst);
                    if let Ok(mut guard) = self.scan_state.lock() {
                        *guard = ScanState::Running;
                    }
                    spawn_scan_thread(self.scan_state.clone(), self.scan_in_progress.clone());
                }

                ui.spacing();
                ui.separator();

                {
                    let state = self.scan_state.lock().ok();
                    match state.as_deref() {
                        Some(ScanState::Idle) | None => {
                            ui.text("(nenhum scan executado ainda)");
                        }
                        Some(ScanState::Running) => {
                            ui.text("Escaneando memória, aguarde...");
                        }
                        Some(ScanState::Done { db_locations_found, result, error, elapsed_ms }) => {
                            ui.text(format!(
                                "Databases encontradas: {} (em {}ms)",
                                db_locations_found, elapsed_ms
                            ));
                            if let Some(result) = result {
                                ui.text(format!("Jogador de teste (ID {})", TEST_PLAYER_ID));
                                ui.text(format!("Região da database: 0x{:X}", result.region_base));
                                ui.text(format!("Strength:   {:?}", result.strength));
                                ui.text(format!("Overall:    {:?}", result.overall));
                                ui.text(format!("Potential:  {:?}", result.potential));
                            }
                            if let Some(err) = error {
                                ui.text_colored([1.0, 0.4, 0.4, 1.0], err);
                            }
                        }
                    }
                }

                ui.spacing();
                ui.separator();
                ui.text("Teste de escrita no blob CZUM (jogador de teste)");
                if ui.button("Escrever Strength=99 no jogador de teste") {
                    self.test_write_strength(99);
                }
                ui.same_line();
                if ui.button("Reverter para 43") {
                    self.test_write_strength(43);
                }
                if let Some(result) = &self.write_test_result {
                    ui.text(result);
                }

                ui.spacing();
                ui.separator();
                ui.text("Scan por valor exato (i32)");
                ui.input_text("Valor", &mut self.value_scan_input).build();

                let val_scan_running = self.value_scan_in_progress.load(Ordering::SeqCst);

                let previous_addresses: Option<Vec<usize>> = match self.value_scan_state.lock().ok().as_deref() {
                    Some(ValueScanState::Done { addresses, .. }) => Some(addresses.clone()),
                    _ => None,
                };

                let new_scan_label = if val_scan_running { "Escaneando..." } else { "Novo scan" };
                if ui.button(new_scan_label) && !val_scan_running {
                    if let Ok(value) = self.value_scan_input.trim().parse::<i32>() {
                        self.value_scan_in_progress.store(true, Ordering::SeqCst);
                        if let Ok(mut guard) = self.value_scan_state.lock() {
                            *guard = ValueScanState::Running;
                        }
                        spawn_value_scan_thread(
                            value,
                            None,
                            self.value_scan_state.clone(),
                            self.value_scan_in_progress.clone(),
                        );
                    } else {
                        tracing::warn!("Valor inválido: '{}'", self.value_scan_input);
                    }
                }

                if previous_addresses.is_some() {
                    ui.same_line();
                    let next_scan_label =
                        if val_scan_running { "Escaneando..." } else { "Next scan (filtrar)" };
                    if ui.button(next_scan_label) && !val_scan_running {
                        if let Ok(value) = self.value_scan_input.trim().parse::<i32>() {
                            self.value_scan_in_progress.store(true, Ordering::SeqCst);
                            if let Ok(mut guard) = self.value_scan_state.lock() {
                                *guard = ValueScanState::Running;
                            }
                            spawn_value_scan_thread(
                                value,
                                previous_addresses,
                                self.value_scan_state.clone(),
                                self.value_scan_in_progress.clone(),
                            );
                        } else {
                            tracing::warn!("Valor inválido: '{}'", self.value_scan_input);
                        }
                    }
                }

                ui.spacing();
                let val_state = self.value_scan_state.lock().ok();
                match val_state.as_deref() {
                    Some(ValueScanState::Idle) | None => {
                        ui.text("(nenhum scan de valor executado ainda)");
                    }
                    Some(ValueScanState::Running) => {
                        ui.text("Escaneando...");
                    }
                    Some(ValueScanState::Done { addresses, elapsed_ms }) => {
                        ui.text(format!(
                            "{} endereço(s) encontrado(s) em {}ms:",
                            addresses.len(),
                            elapsed_ms
                        ));
                        for addr in addresses.iter().take(20) {
                            if ui.small_button(&format!("0x{:X}##valscan", addr)) {
                                self.target_address_input = format!("{:X}", addr);
                            }
                        }
                        if addresses.len() > 20 {
                            ui.text(format!("... e mais {} endereço(s).", addresses.len() - 20));
                        }
                    }
                }

                ui.spacing();
                ui.separator();
                ui.text("Pointer scan reverso (achar âncora estável)");
                ui.input_text("Endereço alvo (hex)", &mut self.target_address_input).build();

                let ptr_scan_running = self.pointer_scan_in_progress.load(Ordering::SeqCst);
                let ptr_button_label = if ptr_scan_running {
                    "Buscando âncora... (background)"
                } else {
                    "Buscar âncora para este endereço"
                };

                if ui.button(ptr_button_label) && !ptr_scan_running {
                    let cleaned = self.target_address_input.trim().trim_start_matches("0x");
                    if let Ok(target) = usize::from_str_radix(cleaned, 16) {
                        self.pointer_scan_in_progress.store(true, Ordering::SeqCst);
                        if let Ok(mut guard) = self.pointer_scan_state.lock() {
                            *guard = PointerScanState::Running;
                        }
                        spawn_pointer_scan_thread(
                            target,
                            self.pointer_scan_state.clone(),
                            self.pointer_scan_in_progress.clone(),
                        );
                    } else {
                        tracing::warn!("Endereço hex inválido: '{}'", self.target_address_input);
                    }
                }

                ui.spacing();
                let ptr_state = self.pointer_scan_state.lock().ok();
                match ptr_state.as_deref() {
                    Some(PointerScanState::Idle) | None => {
                        ui.text("(nenhuma busca de âncora executada ainda)");
                    }
                    Some(PointerScanState::Running) => {
                        ui.text("Buscando âncora, isso pode levar alguns segundos...");
                    }
                    Some(PointerScanState::Done {
                        anchors,
                        levels_explored,
                        last_level_hit_count,
                        elapsed_ms,
                        level1_hits,
                    }) => {
                        ui.text(format!(
                            "Concluído em {}ms — {} nível(is) explorado(s), {} hit(s) no último nível.",
                            elapsed_ms, levels_explored, last_level_hit_count
                        ));
                        if anchors.is_empty() {
                            ui.text_colored([1.0, 0.8, 0.3, 1.0], "Nenhuma âncora em módulo encontrada.");
                        } else {
                            for a in anchors.iter().take(10) {
                                ui.text(format!(
                                    "{}+0x{:X} -> 0x{:X}",
                                    a.module_name, a.module_offset, a.pointer_value
                                ));
                            }
                            if anchors.len() > 10 {
                                ui.text(format!("... e mais {} âncora(s).", anchors.len() - 10));
                            }
                        }

                        ui.spacing();
                        ui.text(format!(
                            "Hits de NÍVEL 1 (ponteiros diretos, mais confiáveis): {}",
                            level1_hits.len()
                        ));
                        for hit in level1_hits.iter().take(40) {
                            if ui.small_button(&format!("0x{:X}##lvl1", hit.pointer_addr)) {
                                self.target_address_input = format!("{:X}", hit.pointer_addr);
                            }
                        }
                        if level1_hits.len() > 40 {
                            ui.text(format!("... e mais {} hit(s).", level1_hits.len() - 40));
                        }
                    }
                }
            });
    }
}

hudhook::hudhook!(hudhook::hooks::dx11::ImguiDx11Hooks, FifaOverlay::new());
