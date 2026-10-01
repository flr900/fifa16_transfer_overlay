//! FIFA 16 Companion — overlay injetado no `fifa16.exe` (DLL + hudhook).
//!
//! Desenha a Central de Scout por cima do jogo em tela cheia exclusiva,
//! reaproveitando o hook de `Present()` do DirectX 11 (sem trocar modo de
//! vídeo). F10 abre/fecha o painel.
//!
//! A janela de diagnóstico usada na investigação de memória (sonda,
//! scans, teste de escrita) foi arquivada em `fifa_overlay_debug/`
//! (Story 1.2). Aqui fica só o produto.
//!
//! IMPORTANTE: `render()` roda no thread de render do jogo, a cada frame.
//! Nada pesado aqui: varreduras de memória vão para `AsyncTask` (AD-4).

mod async_task;
// Infraestrutura de memória (AD-2): usada só através do `save_repo`;
// partes dela (escrita, CZUM, pointer scan) servem a stories futuras.
#[allow(dead_code)]
mod fifa_db;
#[allow(dead_code)]
mod memscan;
#[allow(dead_code)]
mod pointer_scan;
mod save_repo;
mod scout;

use hudhook::hooks::dx11::ImguiDx11Hooks;
use hudhook::{ImguiRenderLoop, MessageFilter, RenderContext};
use imgui::{Context, Io, Ui};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use scout::screens::theme::{self, FontSlots};
use scout::Scout;

/// Mostrado no log ao injetar, para saber QUAL build está no jogo (já
/// houve confusão entre cópias injetadas).
const BUILD_TAG: &str = "1.3-v1 — estado do Scout por save";

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
        // "info": o TRACE do hudhook grava uma linha por frame.
        .with(EnvFilter::new("info"))
        .try_init();
}

struct FifaOverlay {
    scout: Scout,
    fonts: Option<FontSlots>,
}

impl FifaOverlay {
    fn new() -> Self {
        setup_tracing();
        tracing::info!("FifaOverlay::new() — DLL injetada ({BUILD_TAG}).");
        FifaOverlay { scout: Scout::new(), fonts: None }
    }
}

impl ImguiRenderLoop for FifaOverlay {
    fn initialize<'a>(&'a mut self, ctx: &mut Context, _render_context: &'a mut dyn RenderContext) {
        // Sem imgui.ini no diretório do jogo: o painel se posiciona sozinho.
        ctx.set_ini_filename(None);
        self.fonts = Some(theme::carregar_fontes(ctx));
        theme::aplicar_estilo(ctx.style_mut());
    }

    fn before_render<'a>(&'a mut self, ctx: &mut Context, _render_context: &'a mut dyn RenderContext) {
        // Em tela cheia o jogo esconde o cursor do Windows: com o painel
        // aberto o ImGui desenha o próprio.
        ctx.io_mut().mouse_draw_cursor = self.scout.painel_aberto();
    }

    fn render(&mut self, ui: &mut Ui) {
        let fonts = self.fonts.and_then(|slots| slots.resolver(ui));
        self.scout.frame(ui, fonts.as_ref());
    }

    /// Com o painel aberto o overlay "assume" mouse e teclado
    /// (EXPERIENCE.md): o jogo não recebe os cliques de trás do painel.
    /// O F10 continua funcionando porque é lido por `GetAsyncKeyState`.
    fn message_filter(&self, _io: &Io) -> MessageFilter {
        if self.scout.painel_aberto() {
            MessageFilter::InputAll
        } else {
            MessageFilter::empty()
        }
    }
}

hudhook::hudhook!(ImguiDx11Hooks, FifaOverlay::new());
