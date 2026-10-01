# Recarrega a Central de Scout SEM fechar o jogo — só para desenvolvimento
# (ajustes pequenos sem esperar o FIFA reabrir).
#
# Uso, com o jogo aberto e a build nova já compilada
# (`cargo build --release` em fifa_overlay):
#   powershell -ExecutionPolicy Bypass -File .\recarregar_dev.ps1
# Pede Administrador uma vez (o FIFA roda elevado).
#
# O que faz:
#   1. cria %TEMP%\fifa_overlay_eject.pedido: a DLL carregada (build 1.6-v2
#      ou mais nova) vê o arquivo em até 1 s, desliga o gancho do controle e
#      se descarrega;
#   2. espera a cópia carregada (target\fifa_overlay_dev.dll) ser liberada
#      e copia a build nova por cima;
#   3. injeta de novo.
#
# Atenção: descarregar a DLL com o jogo rodando já derrubou o FIFA uma vez
# em ~5 tentativas (sessão 6). Se o jogo fechar, não é bug do código novo:
# reabra com iniciar_fifa.ps1.

$ErrorActionPreference = "Stop"

trap {
    Write-Host ""
    Write-Host "Erro: $_" -ForegroundColor Red
    Read-Host "Pressione Enter para fechar"
    exit 1
}

$identidade = [Security.Principal.WindowsIdentity]::GetCurrent()
$admin = (New-Object Security.Principal.WindowsPrincipal($identidade)).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    Start-Process powershell.exe -Verb RunAs -ArgumentList "-ExecutionPolicy Bypass -File `"$PSCommandPath`""
    exit
}

$overlayDir = $PSScriptRoot
$built = Join-Path $overlayDir "target\release\fifa_overlay.dll"
$copia = Join-Path $overlayDir "target\fifa_overlay_dev.dll"
$injector = Join-Path $overlayDir "..\fifa_injector\target\release\fifa_injector.exe"
$pedido = Join-Path $env:TEMP "fifa_overlay_eject.pedido"

if (-not (Get-Process fifa16 -ErrorAction SilentlyContinue)) {
    Write-Error "O FIFA 16 não está aberto. Para começar uma sessão, use iniciar_fifa.ps1."
}
if (-not (Test-Path $built)) {
    Write-Error "Build não encontrada: $built (rode 'cargo build --release' em fifa_overlay)."
}
if (-not (Test-Path $injector)) {
    Write-Error "Injetor não encontrado: $injector (rode 'cargo build --release' em fifa_injector)."
}

Write-Host "Pedindo para a Central de Scout carregada se descarregar..."
New-Item -Path $pedido -ItemType File -Force | Out-Null

# A cópia só pode ser sobrescrita depois que o jogo soltar a DLL.
$copiou = $false
for ($i = 0; $i -lt 30; $i++) {
    try {
        Copy-Item $built $copia -Force -ErrorAction Stop
        $copiou = $true
        break
    } catch {
        Start-Sleep -Milliseconds 500
    }
}
if (-not $copiou) {
    Remove-Item $pedido -Force -ErrorAction SilentlyContinue
    Write-Error ("A DLL não descarregou em 15 s. A build carregada pode ser anterior à 1.6-v2 " +
        "(sem suporte a recarregar): feche o jogo e use iniciar_fifa.ps1.")
}
Remove-Item $pedido -Force -ErrorAction SilentlyContinue
Write-Host "Descarregada. Build nova: $((Get-Item $built).LastWriteTime)"

# Um instante para o FreeLibrary terminar antes de injetar de novo.
Start-Sleep -Seconds 1
& $injector $copia
if ($LASTEXITCODE -ne 0) {
    Read-Host "A injeção falhou. Pressione Enter para fechar"
    exit 1
}
Write-Host "Central de Scout recarregada. Confira a versão no log (%TEMP%\fifa_overlay.log)."
Start-Sleep -Seconds 2
