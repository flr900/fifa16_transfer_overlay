//! Teste de nitidez das fontes (build 2.2-v5).
//!
//! Sem o jogo não dá para ver o painel; então o teste monta o atlas do
//! ImGui com cada rasterização candidata e "desenha" frases do painel do
//! jeito que o ImGui + backend DX11 do hudhook desenham: um quad por glifo,
//! em posição inteira, texel 1:1 (o filtro linear não mistura nada), cor do
//! texto vezes o alfa do atlas, mistura alfa em espaço gama (RTV UNORM).
//!
//! `cargo test nitidez -- --ignored --nocapture` grava os PNGs em
//! `target/nitidez/`: um por variante (1:1), a comparação empilhada,
//! recortes ampliados 6× (pixel a pixel, sem suavizar) e o antes/depois.
//! Também imprime, por papel tipográfico, a fração de pixels de texto em
//! cinza (cobertura parcial: menos = contorno mais duro) e a largura.

use super::*;
use imgui::{sys, Font, FontGlyph, SharedFontAtlas};
use std::path::PathBuf;

const STB_V4: Rasterizacao = Rasterizacao { freetype: false, flags_freetype: 0, reforco_traco: 1.15 };

/// Candidatas comparadas (nome do arquivo, configuração).
fn variantes() -> Vec<(&'static str, Rasterizacao)> {
    let ft = |flags_freetype, reforco_traco| Rasterizacao { freetype: true, flags_freetype, reforco_traco };
    vec![
        ("1_stb_v4", STB_V4),
        ("2_ft_nativo", ft(0, 1.0)),
        ("3_ft_leve", ft(flags_freetype::HINTING_LEVE, 1.0)),
        ("4_ft_mono_hint", ft(flags_freetype::HINTING_MONO, 1.0)),
        ("5_ft_sem_hinting", ft(flags_freetype::SEM_HINTING, 1.0)),
        ("6_ft_autohint", ft(flags_freetype::FORCAR_AUTO_HINT, 1.0)),
        ("7_ft_autohint_reforco", ft(flags_freetype::FORCAR_AUTO_HINT, 1.15)),
        ("8_ft_autohint_reforco_forte", ft(flags_freetype::FORCAR_AUTO_HINT, 1.3)),
    ]
}

/// Atlas montado, com o canal alfa da textura RGBA32 (o que vai para a GPU).
struct Atlas {
    _dono: SharedFontAtlas,
    slots: FontSlots,
    fontes: Vec<*mut sys::ImFont>,
    largura: usize,
    alfa: Vec<u8>,
}

fn montar(r: &Rasterizacao) -> Atlas {
    let mut dono = SharedFontAtlas::create();
    // SAFETY: o ponteiro vem de `ImFontAtlas_ImFontAtlas` e vive com `dono`.
    let atlas = unsafe { FontAtlas::from_raw_mut(&mut *dono.as_ptr_mut()) };
    let slots = registrar_fontes(atlas, r);
    let tex = atlas.build_rgba32_texture();
    let largura = tex.width as usize;
    let alfa: Vec<u8> = tex.data.chunks_exact(4).map(|p| p[3]).collect();
    let fontes = atlas
        .fonts()
        .iter()
        .map(|id| atlas.get_font(*id).unwrap() as *const Font as *mut sys::ImFont)
        .collect();
    Atlas { _dono: dono, slots, fontes, largura, alfa }
}

fn glifo(fonte: *mut sys::ImFont, c: char) -> Option<FontGlyph> {
    // SAFETY: `fonte` pertence a um atlas vivo e montado.
    unsafe {
        let g = sys::ImFont_FindGlyphNoFallback(fonte, c as sys::ImWchar);
        (!g.is_null()).then(|| *FontGlyph::from_raw(&*g))
    }
}

fn cor(c: [f32; 4]) -> [u8; 3] {
    [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
}

struct Tela {
    largura: usize,
    altura: usize,
    px: Vec<[u8; 3]>,
    /// Alfa de cobertura de cada pixel de texto desenhado (métrica).
    coberturas: Vec<u8>,
}

impl Tela {
    fn new(largura: usize, altura: usize) -> Self {
        Tela { largura, altura, px: vec![cor(BG_PANEL); largura * altura], coberturas: Vec::new() }
    }

    /// `ImFont::RenderText` de 1.89.2: caneta começa em `floor(pos)`, soma
    /// `AdvanceX` (sem kerning) e o quad vai de `caneta + X0` a `caneta + X1`.
    fn texto(&mut self, atlas: &Atlas, fonte: usize, x: i32, y: i32, texto: &str, rgb: [u8; 3]) {
        let f = atlas.fontes[fonte];
        let mut caneta = x as f32;
        for c in texto.chars() {
            let g = glifo(f, c).unwrap_or_else(|| panic!("sem glifo para {c:?}"));
            if g.visible() {
                let x0 = caneta + g.x0;
                let y0 = y as f32 + g.y0;
                assert!(x0.fract() == 0.0 && y0.fract() == 0.0, "quad fora da grade: {x0} {y0}");
                let largura = (g.x1 - g.x0).round() as usize;
                let altura = (g.y1 - g.y0).round() as usize;
                let alt_tex = atlas.alfa.len() / atlas.largura;
                let u0 = (g.u0 * atlas.largura as f32).round() as usize;
                let v0 = (g.v0 * alt_tex as f32).round() as usize;
                for ty in 0..altura {
                    for tx in 0..largura {
                        let a = atlas.alfa[(v0 + ty) * atlas.largura + u0 + tx];
                        let (px, py) = (x0 as usize + tx, y0 as usize + ty);
                        if a > 0 {
                            self.coberturas.push(a);
                        }
                        if px < self.largura && py < self.altura {
                            let fundo = self.px[py * self.largura + px];
                            let a = a as f32 / 255.0;
                            let mistura = |t: u8, b: u8| (t as f32 * a + b as f32 * (1.0 - a)).round() as u8;
                            self.px[py * self.largura + px] =
                                [mistura(rgb[0], fundo[0]), mistura(rgb[1], fundo[1]), mistura(rgb[2], fundo[2])];
                        }
                    }
                }
            }
            caneta += g.advance_x;
        }
    }

    fn recorte(&self, x: usize, y: usize, largura: usize, altura: usize, zoom: usize) -> Tela {
        let mut out = Tela::new(largura * zoom, altura * zoom);
        for oy in 0..altura * zoom {
            for ox in 0..largura * zoom {
                out.px[oy * out.largura + ox] = self.px[(y + oy / zoom) * self.largura + x + ox / zoom];
            }
        }
        out
    }

    fn empilhar(telas: &[Tela]) -> Tela {
        let largura = telas.iter().map(|t| t.largura).max().unwrap_or(1);
        let altura = telas.iter().map(|t| t.altura).sum();
        let mut out = Tela::new(largura, altura);
        let mut y0 = 0;
        for t in telas {
            for y in 0..t.altura {
                for x in 0..t.largura {
                    out.px[(y0 + y) * largura + x] = t.px[y * t.largura + x];
                }
            }
            y0 += t.altura;
        }
        out
    }

    fn lado_a_lado(telas: &[Tela]) -> Tela {
        let largura = telas.iter().map(|t| t.largura).sum();
        let altura = telas.iter().map(|t| t.altura).max().unwrap_or(1);
        let mut out = Tela::new(largura, altura);
        let mut x0 = 0;
        for t in telas {
            for y in 0..t.altura {
                for x in 0..t.largura {
                    out.px[y * largura + x0 + x] = t.px[y * t.largura + x];
                }
            }
            x0 += t.largura;
        }
        out
    }

    fn gravar(&self, caminho: &PathBuf) {
        let arquivo = std::io::BufWriter::new(std::fs::File::create(caminho).unwrap());
        let mut enc = png::Encoder::new(arquivo, self.largura as u32, self.altura as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let dados: Vec<u8> = self.px.iter().flatten().copied().collect();
        enc.write_header().unwrap().write_image_data(&dados).unwrap();
    }

    /// Fração dos pixels de texto com cobertura parcial (nem fundo nem
    /// traço cheio). Quanto menor, mais "duro" o contorno.
    fn fracao_cinza(&self) -> f32 {
        let cinza = self.coberturas.iter().filter(|&&a| (40..=215).contains(&a)).count();
        cinza as f32 / self.coberturas.len().max(1) as f32
    }
}

const LARGURA: usize = 470;
const ALTURA: usize = 238;

/// Painel de amostra com cada papel tipográfico (tamanhos de `registrar_fontes`).
fn painel(atlas: &Atlas, nome: &str) -> Tela {
    let s = atlas.slots;
    let (primario, secundario) = (cor(TEXT_PRIMARY), cor(TEXT_SECONDARY));
    let mut t = Tela::new(LARGURA, ALTURA);
    t.texto(atlas, s.meta, 12, 6, nome, cor(ACCENT_PRIMARY));
    t.texto(atlas, s.display, 12, 24, "Nova Missão", primario);
    t.texto(atlas, s.heading, 12, 62, "Caçador de Medalhões", primario);
    t.texto(atlas, s.body, 12, 92, "Orçamento insuficiente: faltam 2.100.000.", primario);
    t.texto(atlas, s.body, 12, 116, "Orçamento: 63.999.988 — Júnior, Experiente", primario);
    t.texto(atlas, s.meta, 12, 142, "Orçamento insuficiente: faltam 2.100.000.", secundario);
    t.texto(atlas, s.badge, 12, 164, "ELITE · EXPERIENTE · JÚNIOR · QUALIDADE", cor(TIER_ELITE));
    let mono = s.mono.unwrap_or(s.body);
    t.texto(atlas, mono, 12, 186, "63.999.988   2.100.000", primario);
    t.texto(atlas, s.body, 12, 210, "63.999.988   Caçador", secundario);
    t
}

fn pasta_de_saida() -> PathBuf {
    let pasta = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("nitidez");
    std::fs::create_dir_all(&pasta).unwrap();
    pasta
}


#[test]
fn freetype_monta_o_atlas_com_glifos_do_portugues_na_grade_de_pixels() {
    let ft = montar(&RASTERIZACAO);
    let stb = montar(&STB_V4);
    // Mesmos glifos, mas rasterizados por outro construtor (prova de que o
    // FreeType ligado pelo build.rs é quem montou o atlas).
    assert_ne!(ft.alfa, stb.alfa);
    for fonte in [ft.slots.body, ft.slots.meta, ft.slots.heading, ft.slots.display, ft.slots.badge] {
        for c in "Caçador de Medalhões ÇÃÕÉÚ áéíóúâêôãõç …—€ 0123456789".chars() {
            assert!(glifo(ft.fontes[fonte], c).is_some(), "fonte {fonte} sem {c:?}");
        }
    }
    // Desenhar falha se algum quad cair fora da grade de pixels.
    let tela = painel(&ft, "FreeType");
    assert!(!tela.coberturas.is_empty());
}

#[test]
fn autohint_mantem_os_digitos_da_consolas_tabulares() {
    let ft = montar(&RASTERIZACAO);
    let Some(mono) = ft.slots.mono else { return };
    let avancos: Vec<f32> = "0123456789.,".chars().filter_map(|c| glifo(ft.fontes[mono], c)).map(|g| g.advance_x).collect();
    assert_eq!(avancos.len(), 12);
    assert!(avancos.iter().all(|&a| a == avancos[0]), "{avancos:?}");
}

#[test]
#[ignore = "gera PNGs em target/nitidez para inspeção visual"]
fn nitidez_gera_pngs_de_comparacao() {
    let pasta = pasta_de_saida();
    // Recortes ampliados 6×: (arquivo, x, y, largura, altura) no painel.
    let recortes = [
        ("corpo", 10, 93, 130, 19),
        ("meta", 10, 142, 130, 17),
        ("titulo", 10, 64, 130, 22),
        ("badge", 10, 166, 130, 14),
        ("numeros", 10, 188, 130, 16),
    ];
    let mut paineis = Vec::new();
    let mut detalhes: Vec<Vec<Tela>> = recortes.iter().map(|_| Vec::new()).collect();
    for (nome, r) in variantes() {
        let atlas = montar(&r);
        let tela = painel(&atlas, nome);
        println!("{nome:>28}: {:.1}% cinza | {}", tela.fracao_cinza() * 100.0, medidas(&atlas).join(" | "));
        tela.gravar(&pasta.join(format!("{nome}.png")));
        for ((_, x, y, l, a), lista) in recortes.iter().zip(detalhes.iter_mut()) {
            lista.push(tela.recorte(*x, *y, *l, *a, 6));
        }
        paineis.push(tela);
    }
    Tela::empilhar(&paineis).gravar(&pasta.join("comparacao.png"));
    for ((arquivo, ..), lista) in recortes.iter().zip(&detalhes) {
        Tela::empilhar(lista).gravar(&pasta.join(format!("detalhe6x_{arquivo}.png")));
    }

    // Antes (2.2-v4) × depois (a rasterização do jogo), 1:1 e 3×.
    let antes = painel(&montar(&STB_V4), "ANTES: 2.2-v4 (stb_truetype)");
    let depois = painel(&montar(&RASTERIZACAO), "DEPOIS: 2.2-v5 (FreeType + autohint)");
    let par = Tela::lado_a_lado(&[antes, depois]);
    par.gravar(&pasta.join("antes_depois.png"));
    let antes_3x = par.recorte(8, 0, 340, 170, 3);
    let depois_3x = par.recorte(LARGURA + 8, 0, 340, 170, 3);
    Tela::empilhar(&[antes_3x, depois_3x]).gravar(&pasta.join("antes_depois_3x.png"));
}

/// Fração de cinza e largura (px) de cada papel tipográfico.
fn medidas(atlas: &Atlas) -> Vec<String> {
    let s = atlas.slots;
    let papeis = [
        ("corpo", s.body, "Orçamento insuficiente: faltam 2.100.000."),
        ("meta", s.meta, "Orçamento insuficiente: faltam 2.100.000."),
        ("titulo", s.heading, "Caçador de Medalhões"),
        ("display", s.display, "Nova Missão"),
        ("badge", s.badge, "ELITE · EXPERIENTE · JÚNIOR"),
        ("mono", s.mono.unwrap_or(s.body), "63.999.988"),
    ];
    papeis
        .iter()
        .map(|(papel, fonte, texto)| {
            let mut t = Tela::new(600, 60);
            t.texto(atlas, *fonte, 0, 10, texto, [255; 3]);
            let largura: f32 = texto.chars().filter_map(|c| glifo(atlas.fontes[*fonte], c)).map(|g| g.advance_x).sum();
            format!("{papel} {:.0}% {largura}px", t.fracao_cinza() * 100.0)
        })
        .collect()
}
