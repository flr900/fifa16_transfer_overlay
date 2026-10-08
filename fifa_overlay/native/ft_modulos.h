/*
 * Módulos do FreeType compilados no overlay (FT_CONFIG_MODULES_H).
 *
 * Só o necessário para TrueType com hinting: o driver TrueType (com o
 * interpretador de bytecode), o auto-hinter (fontes sem instruções, como a
 * Inter variável), o leitor de tabelas SFNT, os nomes PostScript e o
 * rasterizador com antisserrilhado. A lista precisa casar com os .c que o
 * build.rs compila.
 */
FT_USE_MODULE( FT_Module_Class, autofit_module_class )
FT_USE_MODULE( FT_Driver_ClassRec, tt_driver_class )
FT_USE_MODULE( FT_Module_Class, psnames_module_class )
FT_USE_MODULE( FT_Module_Class, sfnt_module_class )
FT_USE_MODULE( FT_Renderer_Class, ft_smooth_renderer_class )
