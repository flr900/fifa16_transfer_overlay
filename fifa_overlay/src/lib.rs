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
mod dds;
mod gamepad;
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
use imgui::{BackendFlags, ConfigFlags, Context, Io, Ui};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use scout::screens::theme::{self, FontSlots};
use scout::Scout;

/// Mostrado no log ao injetar, para saber QUAL build está no jogo (já
/// houve confusão entre cópias injetadas).
const BUILD_TAG: &str = "2.10-v1 — Relatório parcial e Missão contínua";

/// Arquivo que pede para a DLL se descarregar sem fechar o jogo
/// (script `recarregar_dev.ps1` da pasta `fifa_overlay`, só para
/// desenvolvimento). Fica no
/// `%TEMP%` do usuário, o mesmo do log.
const PEDIDO_DESCARGA: &str = "fifa_overlay_eject.pedido";
const INTERVALO_PEDIDO: std::time::Duration = std::time::Duration::from_secs(1);

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
    controle: gamepad::Controle,
    /// Leitura do controle feita em `before_render`, usada no `render`.
    ultimo_controle: Option<gamepad::EstadoControle>,
    proxima_verificacao_pedido: std::time::Instant,
    descarregando: bool,
}

impl FifaOverlay {
    fn new() -> Self {
        setup_tracing();
        tracing::info!("FifaOverlay::new() — DLL injetada ({BUILD_TAG}).");
        FifaOverlay {
            scout: Scout::new(),
            fonts: None,
            controle: gamepad::Controle::new(),
            ultimo_controle: None,
            proxima_verificacao_pedido: std::time::Instant::now(),
            descarregando: false,
        }
    }

    /// Recarga de desenvolvimento: se o script pediu, desliga o gancho do
    /// XInput e pede ao hudhook para descarregar a DLL (ele termina de
    /// desfazer os próprios ganchos e libera o arquivo). Uma checagem de
    /// arquivo por segundo.
    fn atender_pedido_de_descarga(&mut self) {
        let agora = std::time::Instant::now();
        if self.descarregando || agora < self.proxima_verificacao_pedido {
            return;
        }
        self.proxima_verificacao_pedido = agora + INTERVALO_PEDIDO;
        let pedido = std::env::temp_dir().join(PEDIDO_DESCARGA);
        if !pedido.exists() {
            return;
        }
        let _ = std::fs::remove_file(&pedido);
        tracing::info!("Descarregando a DLL a pedido do recarregar_dev.ps1 ({BUILD_TAG}).");
        self.descarregando = true;
        gamepad::remover_ganchos();
        hudhook::eject();
    }
}

impl ImguiRenderLoop for FifaOverlay {
    fn initialize<'a>(&'a mut self, ctx: &mut Context, _render_context: &'a mut dyn RenderContext) {
        // Sem imgui.ini no diretório do jogo: o painel se posiciona sozinho.
        ctx.set_ini_filename(None);
        self.fonts = Some(theme::carregar_fontes(ctx));
        theme::aplicar_estilo(ctx.style_mut());
    }

    fn before_render<'a>(&'a mut self, ctx: &mut Context, render_context: &'a mut dyn RenderContext) {
        // Rostos da visão Cards (Story 2.6): só aqui há acesso ao
        // renderizador para criar texturas; poucas por frame.
        self.scout.enviar_minifaces(&mut |imagem, reuso| {
            let resultado = match reuso {
                Some(textura) => render_context
                    .replace_texture(textura, &imagem.rgba, imagem.largura, imagem.altura)
                    .map(|()| textura),
                None => render_context.load_texture(&imagem.rgba, imagem.largura, imagem.altura),
            };
            resultado.ok()
        });

        // Em tela cheia o jogo esconde o cursor do Windows: com o painel
        // aberto o ImGui desenha o próprio.
        let aberto = self.scout.painel_aberto();
        let io = ctx.io_mut();
        io.mouse_draw_cursor = aberto;

        // Navegação por controle e teclado (Story 1.6). O controle só vai
        // para o ImGui com o painel aberto; fechado, tudo "solto".
        io.config_flags.insert(ConfigFlags::NAV_ENABLE_GAMEPAD | ConfigFlags::NAV_ENABLE_KEYBOARD);
        io.backend_flags.insert(BackendFlags::HAS_GAMEPAD);
        self.ultimo_controle = self.controle.ler();
        let na_raiz = self.scout.navegacao_na_raiz();
        let navegacao = self.ultimo_controle.filter(|_| aberto).map(|e| gamepad::para_navegacao(e, na_raiz));
        gamepad::alimentar_imgui(io, navegacao);
    }

    fn render(&mut self, ui: &mut Ui) {
        self.atender_pedido_de_descarga();
        if self.descarregando {
            return;
        }
        let fonts = self.fonts.and_then(|slots| slots.resolver(ui));
        self.scout.frame(ui, fonts.as_ref(), self.ultimo_controle);
        gamepad::bloquear_jogo(self.scout.bloqueia_controle());
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
