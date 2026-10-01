//! Tema da Central de Scout: tokens do `DESIGN.md` traduzidos para o
//! estilo do ImGui, mais as fontes.
//!
//! Registro "console de operações escuro": painel 93% opaco com tint
//! verde, um único acento roxo para marca/aba ativa, verde-campo para a
//! ação primária, bordas hairline e **sem sombra, glow ou gradiente**
//! (o ImGui 0.12 nem desenha sombra; aqui também não simulamos).

use imgui::{Context, FontConfig, FontGlyphRanges, FontId, FontSource, Style, StyleColor, Ui};

// ---------------------------------------------------------------------
// Cores (DESIGN.md → colors)
// ---------------------------------------------------------------------

const fn rgba(rgb: u32, alpha: f32) -> [f32; 4] {
    [
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
        alpha,
    ]
}

pub const BG_BASE: [f32; 4] = rgba(0x0b0e0c, 1.0);
pub const BG_PANEL: [f32; 4] = rgba(0x101412, 0.93);
pub const BG_PANEL_RAISED: [f32; 4] = rgba(0x181d1a, 0.96);
pub const BORDER_HAIRLINE: [f32; 4] = rgba(0xb45cff, 0.28);
pub const BORDER_HAIRLINE_SUBTLE: [f32; 4] = rgba(0xffffff, 0.08);
pub const TEXT_PRIMARY: [f32; 4] = rgba(0xe9f2ec, 1.0);
pub const TEXT_SECONDARY: [f32; 4] = rgba(0x869488, 1.0);
pub const TEXT_DISABLED: [f32; 4] = rgba(0x4d564f, 1.0);
pub const ACCENT_PRIMARY: [f32; 4] = rgba(0xb45cff, 1.0);
pub const ACCENT_PRIMARY_DIM: [f32; 4] = rgba(0xb45cff, 0.13);
pub const FIELD_GREEN: [f32; 4] = rgba(0x3ecf6e, 1.0);
#[allow(dead_code)]
pub const DANGER: [f32; 4] = rgba(0xe5484d, 1.0);
pub const TRANSPARENTE: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

// ---------------------------------------------------------------------
// Forma e espaço (DESIGN.md → rounded, spacing)
// ---------------------------------------------------------------------

pub const RAIO_SM: f32 = 4.0;
pub const RAIO_PADRAO: f32 = 6.0;
pub const RAIO_MD: f32 = 8.0;
pub const RAIO_LG: f32 = 12.0;

pub const ESPACO_1: f32 = 4.0;
pub const ESPACO_2: f32 = 8.0;
pub const ESPACO_3: f32 = 12.0;
pub const ESPACO_4: f32 = 16.0;
pub const ESPACO_5: f32 = 24.0;
#[allow(dead_code)]
pub const ESPACO_6: f32 = 32.0;

/// Alvo mínimo de clique (EXPERIENCE.md, Accessibility Floor).
pub const ALVO_MINIMO: f32 = 32.0;

// ---------------------------------------------------------------------
// Fontes (DESIGN.md → typography)
// ---------------------------------------------------------------------

// Oswald (estática, googlefonts/OswaldFont) e Inter (variável,
// google/fonts — o ImGui renderiza a instância padrão, Regular 400),
// ambas OFL; licenças ao lado dos .ttf. Consolas é do Windows e não pode
// ser redistribuída: lida em tempo de execução.
static OSWALD_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Oswald-SemiBold.ttf");
static OSWALD_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Oswald-Medium.ttf");
static INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Variable.ttf");
const CONSOLAS: &str = r"C:\Windows\Fonts\consola.ttf";

/// Latin-1 (acentos do português) + pontuação geral (travessões, aspas
/// curvas, reticências `…`) + `€`. Termina em 0, como o ImGui exige.
static FAIXAS_DE_GLIFOS: [u32; 7] = [0x0020, 0x00FF, 0x2010, 0x2027, 0x20AC, 0x20AC, 0];

/// Fontes já registradas no atlas do ImGui, uma por papel tipográfico.
#[derive(Debug, Clone, Copy)]
pub struct Fonts {
    /// Oswald 600 — título do painel, números de destaque.
    pub display: FontId,
    /// Oswald 500 — títulos de aba, nomes em cards.
    pub heading: FontId,
    /// Inter 400 — corpo (fonte padrão do atlas).
    pub body: FontId,
    /// Inter 400 menor — texto secundário.
    pub meta: FontId,
    /// Consolas — colunas numéricas; `None` se o arquivo não existir.
    #[allow(dead_code)]
    pub mono: Option<FontId>,
}

fn fonte(data: &'static [u8], tamanho: f32) -> FontSource<'static> {
    FontSource::TtfData {
        data,
        size_pixels: tamanho,
        config: Some(FontConfig {
            glyph_ranges: FontGlyphRanges::from_slice(&FAIXAS_DE_GLIFOS),
            oversample_h: 2,
            ..FontConfig::default()
        }),
    }
}

/// Posição de cada fonte no atlas. O `FontId` do ImGui é um ponteiro cru
/// (não é `Send`/`Sync`, e o hudhook exige isso do overlay); então o
/// overlay guarda só os índices e resolve os `FontId` a cada frame, no
/// thread de render (`resolver`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontSlots {
    display: usize,
    heading: usize,
    body: usize,
    meta: usize,
    mono: Option<usize>,
}

impl FontSlots {
    /// `FontId`s do frame atual; `None` se o atlas não tiver as fontes.
    pub fn resolver(&self, ui: &Ui) -> Option<Fonts> {
        let ids = ui.fonts().fonts();
        Some(Fonts {
            display: *ids.get(self.display)?,
            heading: *ids.get(self.heading)?,
            body: *ids.get(self.body)?,
            meta: *ids.get(self.meta)?,
            mono: self.mono.and_then(|i| ids.get(i).copied()),
        })
    }
}

/// Registra as fontes no atlas. Tem de rodar em `initialize`, antes de o
/// hudhook montar a textura de fontes. A PRIMEIRA fonte adicionada vira a
/// padrão do ImGui (por isso o corpo vem primeiro).
pub fn carregar_fontes(ctx: &mut Context) -> FontSlots {
    let atlas = ctx.fonts();
    let mut adicionar = |source: FontSource<'static>| {
        atlas.add_font(&[source]);
        atlas.fonts().len().saturating_sub(1)
    };
    let body = adicionar(fonte(INTER, 17.0));
    let meta = adicionar(fonte(INTER, 14.0));
    let heading = adicionar(fonte(OSWALD_MEDIUM, 21.0));
    let display = adicionar(fonte(OSWALD_SEMIBOLD, 30.0));

    // O ImGui guarda o ponteiro dos dados da fonte enquanto o atlas
    // existir; o vazamento (uma vez, ~400 KB) dá a eles vida `'static`.
    let mono = match std::fs::read(CONSOLAS) {
        Ok(bytes) => {
            let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            Some(adicionar(fonte(bytes, 16.0)))
        }
        Err(err) => {
            tracing::warn!("[scout::theme] Consolas indisponível ({err}); colunas numéricas usam Inter.");
            None
        }
    };

    tracing::info!("[scout::theme] Fontes carregadas (Inter, Oswald, Consolas: {}).", mono.is_some());
    FontSlots { display, heading, body, meta, mono }
}

/// Aplica os tokens ao estilo global do ImGui.
pub fn aplicar_estilo(style: &mut Style) {
    style.window_padding = [ESPACO_5, ESPACO_4 + ESPACO_1];
    style.window_rounding = RAIO_LG;
    style.window_border_size = 1.0;
    style.child_rounding = RAIO_MD;
    style.child_border_size = 1.0;
    style.popup_rounding = RAIO_MD;
    style.popup_border_size = 1.0;
    style.frame_padding = [ESPACO_3, ESPACO_2 - 2.0];
    style.frame_rounding = RAIO_PADRAO;
    style.frame_border_size = 0.0;
    style.item_spacing = [ESPACO_2, ESPACO_2];
    style.item_inner_spacing = [ESPACO_2, ESPACO_1];
    style.scrollbar_size = 10.0;
    style.scrollbar_rounding = RAIO_SM;
    style.grab_rounding = RAIO_SM;
    style.tab_rounding = RAIO_MD;

    style[StyleColor::Text] = TEXT_PRIMARY;
    style[StyleColor::TextDisabled] = TEXT_DISABLED;
    style[StyleColor::WindowBg] = BG_PANEL;
    style[StyleColor::ChildBg] = TRANSPARENTE;
    style[StyleColor::PopupBg] = BG_PANEL_RAISED;
    style[StyleColor::Border] = BORDER_HAIRLINE;
    style[StyleColor::BorderShadow] = TRANSPARENTE;
    style[StyleColor::FrameBg] = BG_PANEL_RAISED;
    style[StyleColor::FrameBgHovered] = ACCENT_PRIMARY_DIM;
    style[StyleColor::FrameBgActive] = ACCENT_PRIMARY_DIM;
    style[StyleColor::Button] = TRANSPARENTE;
    style[StyleColor::ButtonHovered] = ACCENT_PRIMARY_DIM;
    style[StyleColor::ButtonActive] = ACCENT_PRIMARY_DIM;
    style[StyleColor::Header] = ACCENT_PRIMARY_DIM;
    style[StyleColor::HeaderHovered] = ACCENT_PRIMARY_DIM;
    style[StyleColor::HeaderActive] = ACCENT_PRIMARY_DIM;
    style[StyleColor::Separator] = BORDER_HAIRLINE_SUBTLE;
    style[StyleColor::SeparatorHovered] = BORDER_HAIRLINE;
    style[StyleColor::SeparatorActive] = BORDER_HAIRLINE;
    style[StyleColor::ScrollbarBg] = TRANSPARENTE;
    style[StyleColor::ScrollbarGrab] = BORDER_HAIRLINE_SUBTLE;
    style[StyleColor::ScrollbarGrabHovered] = BORDER_HAIRLINE;
    style[StyleColor::ScrollbarGrabActive] = ACCENT_PRIMARY;
    // Foco de teclado/gamepad = borda roxa sólida (EXPERIENCE.md).
    style[StyleColor::NavHighlight] = ACCENT_PRIMARY;
    style[StyleColor::ModalWindowDimBg] = rgba(0x0b0e0c, 0.6);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_tokens_convert_to_imgui_colors() {
        assert_eq!(rgba(0xff0000, 1.0), [1.0, 0.0, 0.0, 1.0]);
        let accent = ACCENT_PRIMARY;
        assert!((accent[0] - 180.0 / 255.0).abs() < 1e-6);
        assert!((accent[1] - 92.0 / 255.0).abs() < 1e-6);
        assert!((accent[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn panels_with_text_are_at_least_90_percent_opaque() {
        assert!(BG_PANEL[3] >= 0.90);
        assert!(BG_PANEL_RAISED[3] >= 0.90);
    }

    #[test]
    fn embedded_fonts_are_truetype_files() {
        for data in [OSWALD_SEMIBOLD, OSWALD_MEDIUM, INTER] {
            assert_eq!(data.get(..4), Some(&[0x00, 0x01, 0x00, 0x00][..]));
        }
    }

    #[test]
    fn glyph_ranges_cover_portuguese_and_ellipsis_and_end_with_zero() {
        let cobre = |c: char| {
            FAIXAS_DE_GLIFOS
                .chunks_exact(2)
                .any(|par| (par[0]..=par[1]).contains(&(c as u32)))
        };
        for c in "ãçéêíóõúÁÇÉÕ…—".chars() {
            assert!(cobre(c), "{c}");
        }
        assert_eq!(FAIXAS_DE_GLIFOS.last(), Some(&0));
    }
}
