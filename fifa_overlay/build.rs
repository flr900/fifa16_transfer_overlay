//! Compila o rasterizador FreeType do Dear ImGui junto com o overlay
//! (build 2.2-v5, texto mais nítido).
//!
//! Por que aqui e não pela feature `freetype` do imgui-sys: ela acha o
//! FreeType via `pkg-config` ou `vcpkg`, que não existem nesta máquina, e
//! linkaria uma `freetype.dll` que teria de ir junto com a DLL injetada.
//! Em vez disso:
//!
//! 1. o FreeType 2.13.2 (só os módulos de TrueType) vem no repositório, em
//!    `third_party/freetype` (licença FTL, ver `LICENSE.TXT`), e é
//!    compilado como biblioteca ESTÁTICA — nada para instalar e nenhuma DLL
//!    extra ao lado da `fifa_overlay.dll`;
//! 2. o `imgui_freetype.cpp` que já vem no imgui-sys é compilado com os
//!    mesmos `#define`s do imgui-sys (`DEP_IMGUI_DEFINE_*`), mais uma ponte
//!    `extern "C"` (`native/ponte_freetype.cpp`);
//! 3. em tempo de execução, `theme::carregar_fontes` aponta o
//!    `ImFontAtlas::FontBuilderIO` para esse construtor.
//!
//! Só precisa do compilador C/C++ do Visual Studio, que o imgui-sys já
//! exige.

use std::path::{Path, PathBuf};

/// Arquivos do FreeType compilados (relativos a `third_party/freetype/src`).
/// `ftbase.c`, `autofit.c`, `truetype.c`, `sfnt.c`, `smooth.c` e
/// `psnames.c` são "unity builds" que incluem o resto do módulo. A lista
/// de módulos casa com `native/ft_modulos.h`.
const FONTES_FREETYPE: &[&str] = &[
    "base/ftbase.c",
    "base/ftbbox.c",
    "base/ftbitmap.c",
    "base/ftdebug.c",
    "base/ftglyph.c",
    "base/ftinit.c",
    "base/ftmm.c",
    "base/ftsynth.c",
    "base/ftsystem.c",
    "autofit/autofit.c",
    "truetype/truetype.c",
    "sfnt/sfnt.c",
    "smooth/smooth.c",
    "psnames/psnames.c",
];

fn main() -> Result<(), String> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let freetype = manifest.join("third_party").join("freetype");
    let nativo = manifest.join("native");
    let imgui = pasta_do_imgui()?;

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", freetype.display());
    println!("cargo:rerun-if-changed={}", nativo.display());

    compilar_freetype(&freetype, &nativo);
    compilar_ponte(&freetype, &nativo, &imgui);
    Ok(())
}

/// Pasta com `imgui.h` e `misc/freetype/imgui_freetype.cpp` do MESMO
/// Dear ImGui (1.89.2) que o imgui-sys compilou. O imgui-sys publica o
/// caminho em `DEP_IMGUI_THIRD_PARTY` (por isso o `imgui-sys` é
/// dependência direta no Cargo.toml).
fn pasta_do_imgui() -> Result<PathBuf, String> {
    let third_party = std::env::var("DEP_IMGUI_THIRD_PARTY")
        .map_err(|_| "DEP_IMGUI_THIRD_PARTY ausente: o imgui-sys precisa ser dependência direta".to_string())?;
    let base = PathBuf::from(third_party);
    // O imgui-sys anuncia a pasta duas vezes (raiz `third-party` e depois
    // `third-party/imgui-master`); aceita as duas.
    let candidatas = [base.join("imgui"), base.join("imgui-master").join("imgui")];
    candidatas
        .into_iter()
        .find(|p| p.join("misc").join("freetype").join("imgui_freetype.cpp").is_file())
        .ok_or_else(|| format!("imgui_freetype.cpp não encontrado a partir de {}", base.display()))
}

/// Opções e módulos próprios (ver os comentários dos .h em `native/`).
fn configurar_freetype(build: &mut cc::Build, freetype: &Path, nativo: &Path) {
    build
        .include(nativo)
        .include(freetype.join("include"))
        .define("FT_CONFIG_OPTIONS_H", Some("<ft_opcoes.h>"))
        .define("FT_CONFIG_MODULES_H", Some("<ft_modulos.h>"));
}

fn compilar_freetype(freetype: &Path, nativo: &Path) {
    let src = freetype.join("src");
    let mut build = cc::Build::new();
    configurar_freetype(&mut build, freetype, nativo);
    build
        .define("FT2_BUILD_LIBRARY", None)
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .warnings(false)
        .files(FONTES_FREETYPE.iter().map(|f| src.join(f)))
        .compile("fifa_freetype");
}

fn compilar_ponte(freetype: &Path, nativo: &Path, imgui: &Path) {
    let mut build = cc::Build::new();
    build.cpp(true).flag_if_supported("-std=c++11");
    configurar_freetype(&mut build, freetype, nativo);
    build.include(imgui).include(imgui.join("misc").join("freetype"));

    // Os mesmos #defines do imgui-sys (IMGUI_USE_WCHAR32 etc.): sem eles o
    // `ImWchar` e as structs do atlas teriam outro layout aqui.
    for (chave, valor) in std::env::vars() {
        if let Some(nome) = chave.strip_prefix("DEP_IMGUI_DEFINE_") {
            let valor = Some(valor.as_str()).filter(|v| !v.is_empty());
            build.define(nome, valor);
        }
    }

    build
        .warnings(false)
        .file(imgui.join("misc").join("freetype").join("imgui_freetype.cpp"))
        .file(nativo.join("ponte_freetype.cpp"))
        .compile("fifa_imgui_freetype");
}
