//! Decodificador mínimo de DDS para os minifaces do FIFA 16 (Story 2.6).
//!
//! Os rostos em `data\ui\imgAssets\heads\p<PLAYERID>.dds` são 128×128 em
//! DXT5 (BC3), sem mipmaps (conferido em 2026-10-01). Decodificamos para
//! RGBA8, o formato que o hudhook sobe como textura. Também aceita DXT1
//! (BC1) e 32 bits sem compressão (BGRA/RGBA), por segurança; qualquer
//! outra coisa é `None` (a tela usa a silhueta).

/// Imagem RGBA8, linha a linha de cima para baixo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imagem {
    pub largura: u32,
    pub altura: u32,
    pub rgba: Vec<u8>,
}

const TAMANHO_CABECALHO: usize = 128;
/// Rostos maiores que isso não fazem sentido (e protege de arquivo estranho).
const LADO_MAXIMO: u32 = 1024;

fn u32_em(dados: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_le_bytes(dados.get(pos..pos + 4)?.try_into().ok()?))
}

pub fn decodificar(dados: &[u8]) -> Option<Imagem> {
    if dados.get(..4)? != b"DDS " || u32_em(dados, 4)? != 124 {
        return None;
    }
    let altura = u32_em(dados, 12)?;
    let largura = u32_em(dados, 16)?;
    if largura == 0 || altura == 0 || largura > LADO_MAXIMO || altura > LADO_MAXIMO {
        return None;
    }
    let flags_formato = u32_em(dados, 80)?;
    let four_cc = dados.get(84..88)?;
    let corpo = dados.get(TAMANHO_CABECALHO..)?;
    const DDPF_FOURCC: u32 = 0x4;
    const DDPF_RGB: u32 = 0x40;
    if flags_formato & DDPF_FOURCC != 0 {
        match four_cc {
            b"DXT5" => blocos(corpo, largura, altura, 16, bloco_dxt5),
            b"DXT1" => blocos(corpo, largura, altura, 8, bloco_dxt1),
            _ => None,
        }
    } else if flags_formato & DDPF_RGB != 0 && u32_em(dados, 88)? == 32 {
        let mascara_r = u32_em(dados, 92)?;
        let n = (largura * altura) as usize;
        let origem = corpo.get(..n * 4)?;
        let mut rgba = Vec::with_capacity(n * 4);
        for px in origem.as_chunks::<4>().0 {
            // 0x00ff0000 em R = BGRA na memória; senão RGBA
            let (r, g, b) = if mascara_r == 0x00ff_0000 { (px[2], px[1], px[0]) } else { (px[0], px[1], px[2]) };
            rgba.extend_from_slice(&[r, g, b, px[3]]);
        }
        Some(Imagem { largura, altura, rgba })
    } else {
        None
    }
}

/// Percorre os blocos 4×4 e escreve cada pixel no lugar.
fn blocos(
    corpo: &[u8],
    largura: u32,
    altura: u32,
    bytes_por_bloco: usize,
    decodificar_bloco: fn(&[u8]) -> Option<[[u8; 4]; 16]>,
) -> Option<Imagem> {
    let (bx, by) = (largura.div_ceil(4) as usize, altura.div_ceil(4) as usize);
    let (w, h) = (largura as usize, altura as usize);
    let mut rgba = vec![0u8; w * h * 4];
    for j in 0..by {
        for i in 0..bx {
            let inicio = (j * bx + i) * bytes_por_bloco;
            let pixels = decodificar_bloco(corpo.get(inicio..inicio + bytes_por_bloco)?)?;
            for (k, px) in pixels.iter().enumerate() {
                let (x, y) = (i * 4 + k % 4, j * 4 + k / 4);
                if x < w && y < h {
                    let at = (y * w + x) * 4;
                    rgba.get_mut(at..at + 4)?.copy_from_slice(px);
                }
            }
        }
    }
    Some(Imagem { largura, altura, rgba })
}

fn rgb565(valor: u16) -> [u8; 3] {
    let r = ((valor >> 11) & 0x1f) as u32;
    let g = ((valor >> 5) & 0x3f) as u32;
    let b = (valor & 0x1f) as u32;
    [((r * 255 + 15) / 31) as u8, ((g * 255 + 31) / 63) as u8, ((b * 255 + 15) / 31) as u8]
}

/// As 4 cores da paleta BC1. `quatro_cores`: em DXT5 sempre; em DXT1 só
/// quando c0 > c1 (senão a 4ª é transparente).
fn paleta(bloco: &[u8], quatro_cores: bool) -> Option<[[u8; 4]; 4]> {
    let c0 = u16::from_le_bytes([*bloco.first()?, *bloco.get(1)?]);
    let c1 = u16::from_le_bytes([*bloco.get(2)?, *bloco.get(3)?]);
    let (a, b) = (rgb565(c0), rgb565(c1));
    let mistura = |pa: u32, pb: u32, d: u32| -> [u8; 4] {
        let canal = |x: u8, y: u8| ((u32::from(x) * pa + u32::from(y) * pb) / d) as u8;
        [canal(a[0], b[0]), canal(a[1], b[1]), canal(a[2], b[2]), 255]
    };
    let cores = if quatro_cores || c0 > c1 {
        [[a[0], a[1], a[2], 255], [b[0], b[1], b[2], 255], mistura(2, 1, 3), mistura(1, 2, 3)]
    } else {
        [[a[0], a[1], a[2], 255], [b[0], b[1], b[2], 255], mistura(1, 1, 2), [0, 0, 0, 0]]
    };
    Some(cores)
}

fn bloco_dxt1(bloco: &[u8]) -> Option<[[u8; 4]; 16]> {
    let cores = paleta(bloco, false)?;
    let indices = u32::from_le_bytes(bloco.get(4..8)?.try_into().ok()?);
    let mut out = [[0u8; 4]; 16];
    for (k, px) in out.iter_mut().enumerate() {
        *px = cores[((indices >> (2 * k)) & 0x3) as usize];
    }
    Some(out)
}

fn bloco_dxt5(bloco: &[u8]) -> Option<[[u8; 4]; 16]> {
    let (a0, a1) = (*bloco.first()?, *bloco.get(1)?);
    let mut alfas = [a0, a1, 0, 0, 0, 0, 0, 0];
    for i in 2..8u32 {
        alfas[i as usize] = if a0 > a1 {
            ((u32::from(a0) * (8 - i) + u32::from(a1) * (i - 1)) / 7) as u8
        } else if i < 6 {
            ((u32::from(a0) * (6 - i) + u32::from(a1) * (i - 1)) / 5) as u8
        } else if i == 6 {
            0
        } else {
            255
        };
    }
    let mut bits_alfa = 0u64;
    for (n, byte) in bloco.get(2..8)?.iter().enumerate() {
        bits_alfa |= u64::from(*byte) << (8 * n);
    }
    let cores = paleta(bloco.get(8..16)?, true)?;
    let indices = u32::from_le_bytes(bloco.get(12..16)?.try_into().ok()?);
    let mut out = [[0u8; 4]; 16];
    for (k, px) in out.iter_mut().enumerate() {
        let mut cor = cores[((indices >> (2 * k)) & 0x3) as usize];
        cor[3] = alfas[((bits_alfa >> (3 * k)) & 0x7) as usize];
        *px = cor;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cabeçalho DDS mínimo.
    fn cabecalho(largura: u32, altura: u32, four_cc: &[u8; 4]) -> Vec<u8> {
        let mut d = vec![0u8; TAMANHO_CABECALHO];
        d[..4].copy_from_slice(b"DDS ");
        d[4..8].copy_from_slice(&124u32.to_le_bytes());
        d[12..16].copy_from_slice(&altura.to_le_bytes());
        d[16..20].copy_from_slice(&largura.to_le_bytes());
        d[80..84].copy_from_slice(&4u32.to_le_bytes());
        d[84..88].copy_from_slice(four_cc);
        d
    }

    #[test]
    fn dxt5_solid_block_decodes_to_one_colour_with_alpha() {
        let mut d = cabecalho(4, 4, b"DXT5");
        // alfa: a0 = 200, a1 = 0, todos os índices 0 → 200
        d.extend_from_slice(&[200, 0, 0, 0, 0, 0, 0, 0]);
        // cor: c0 = vermelho puro (0xF800), c1 = 0, índices 0
        d.extend_from_slice(&[0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0]);
        let img = decodificar(&d).expect("decodifica");
        assert_eq!((img.largura, img.altura), (4, 4));
        assert_eq!(img.rgba.len(), 64);
        assert!(img.rgba.as_chunks::<4>().0.iter().all(|px| px == &[255, 0, 0, 200]));
    }

    #[test]
    fn dxt1_interpolates_the_palette() {
        let mut d = cabecalho(4, 4, b"DXT1");
        // c0 = branco, c1 = preto; pixel 0 índice 2 (2/3 branco), resto 0
        d.extend_from_slice(&[0xFF, 0xFF, 0x00, 0x00, 0b10, 0, 0, 0]);
        let img = decodificar(&d).expect("decodifica");
        assert_eq!(&img.rgba[..4], &[170, 170, 170, 255]);
        assert_eq!(&img.rgba[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn garbage_and_unknown_formats_are_none_not_a_panic() {
        assert_eq!(decodificar(b"nada"), None);
        assert_eq!(decodificar(&cabecalho(4, 4, b"ATI2")), None);
        assert_eq!(decodificar(&cabecalho(4, 4, b"DXT5")), None, "sem dados");
        assert_eq!(decodificar(&cabecalho(0, 4, b"DXT5")), None);
        assert_eq!(decodificar(&cabecalho(100_000, 4, b"DXT5")), None);
    }

    #[test]
    fn a_real_miniface_from_the_game_decodes() {
        let caminho = crate::save_repo::jogadores::pasta_do_jogo().join("data").join("ui").join("imgAssets").join("heads").join("p231747.dds");
        let Ok(dados) = std::fs::read(&caminho) else {
            eprintln!("minifaces do FIFA 16 não instalados; teste pulado");
            return;
        };
        let img = decodificar(&dados).expect("rosto do Mbappé");
        assert_eq!((img.largura, img.altura), (128, 128));
        assert!(img.rgba.as_chunks::<4>().0.iter().any(|px| px[3] > 0), "não é tudo transparente");
    }
}
