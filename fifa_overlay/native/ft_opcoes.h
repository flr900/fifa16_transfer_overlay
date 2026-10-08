/*
 * Opções de compilação do FreeType para o overlay (FT_CONFIG_OPTIONS_H).
 *
 * Parte das opções padrão do FreeType 2.13.2 e desliga o que o overlay não
 * usa: só rasterizamos TTFs embutidos (Inter, Oswald) e a Consolas do
 * Windows, nunca WOFF comprimido, fontes PCF/LZW nem glifos SVG. Assim o
 * build não precisa de zlib nem dos módulos gzip/lzw/svg.
 */
#ifndef FIFA_OVERLAY_FT_OPCOES_H
#define FIFA_OVERLAY_FT_OPCOES_H

#include <freetype/config/ftoption.h>

#undef FT_CONFIG_OPTION_USE_ZLIB
#undef FT_CONFIG_OPTION_USE_LZW
#undef FT_CONFIG_OPTION_SVG

#endif
