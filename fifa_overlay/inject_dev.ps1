# Injeta a build MAIS RECENTE do overlay no FIFA 16 (ciclo de desenvolvimento).
#
# Uso (PowerShell como Administrador -- o FIFA roda elevado):
#   .\inject_dev.ps1
#
# Antes de rodar, se houver uma DLL carregada no jogo, clique em
# "Descarregar DLL (eject)" na janela do overlay.
#
# Por que copiar: o jogo trava o arquivo da DLL que carregou. Injetando
# uma CÓPIA (target\fifa_overlay_dev.dll), o `cargo build` continua
# podendo sobrescrever target\release\fifa_overlay.dll, e o caminho
# injetado é sempre o mesmo (evita injetar uma versão velha por engano).
# Confira a versão na janela ("Versão: ...", const BUILD_TAG em lib.rs).

$ErrorActionPreference = "Stop"

$overlayDir = $PSScriptRoot
$built = Join-Path $overlayDir "target\release\fifa_overlay.dll"
$dev = Join-Path $overlayDir "target\fifa_overlay_dev.dll"
$injector = Join-Path $overlayDir "..\fifa_injector\target\release\fifa_injector.exe"

if (-not (Test-Path $built)) {
    Write-Error "Build não encontrada: $built (rode 'cargo build --release' em fifa_overlay)."
}
if (-not (Test-Path $injector)) {
    Write-Error "Injetor não encontrado: $injector (rode 'cargo build --release' em fifa_injector)."
}

try {
    Copy-Item $built $dev -Force
} catch {
    Write-Error "A DLL de dev ainda está carregada no jogo. Clique em 'Descarregar DLL (eject)' na janela e rode de novo."
}

Write-Host "Injetando $dev (build de $((Get-Item $built).LastWriteTime))..."
& $injector $dev
