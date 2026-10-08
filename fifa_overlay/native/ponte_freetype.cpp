// Ponte C para o construtor de atlas FreeType do Dear ImGui.
//
// `ImGuiFreeType::GetBuilderForFreeType()` é C++ (nome decorado); o Rust
// só chama funções `extern "C"`. O imgui-sys é compilado SEM a feature
// `freetype` (ela exige pkg-config ou vcpkg); por isso o overlay compila o
// `imgui_freetype.cpp` por conta própria (build.rs) e liga o construtor no
// atlas em tempo de execução (`ImFontAtlas::FontBuilderIO`), o caminho que
// o próprio imgui_freetype.h documenta.
#include "imgui.h"
#include "imgui_freetype.h"

extern "C" const ImFontBuilderIO* fifa_overlay_construtor_freetype()
{
    return ImGuiFreeType::GetBuilderForFreeType();
}
