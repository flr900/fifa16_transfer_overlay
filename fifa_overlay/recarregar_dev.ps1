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

# O que o script viu e fez, para dar para conferir depois (a janela fecha).
$logArquivo = Join-Path $env:TEMP "recarregar_dev.log"
function Registrar([string]$texto) {
    Write-Host $texto
    Add-Content -Path $logArquivo -Value ("{0} {1}" -f (Get-Date -Format "HH:mm:ss"), $texto) -ErrorAction SilentlyContinue
}

trap {
    Registrar "Erro: $_"
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

# Cópias do overlay carregadas no jogo agora (a de desenvolvimento, a
# instalada, a de outro worktree...). Todas respondem ao mesmo pedido.
function Get-OverlayCarregadas {
    try {
        @(Get-Process fifa16 -ErrorAction Stop | ForEach-Object { $_.Modules } |
            Where-Object { $_.ModuleName -like "fifa_overlay*" })
    } catch {
        @()
    }
}

$carregadas = Get-OverlayCarregadas
Registrar "--- recarregar_dev.ps1 ---"
Registrar ("Cópias do overlay carregadas: {0}" -f $carregadas.Count)
foreach ($m in $carregadas) { Registrar "  - $($m.FileName)" }

Write-Host "Pedindo para a Central de Scout carregada se descarregar..."
New-Item -Path $pedido -ItemType File -Force | Out-Null

# Espera TODAS saírem do jogo. Só esperar a cópia de desenvolvimento ficar
# livre não basta: uma cópia carregada de outro caminho (instalada, outro
# worktree) não trava o arquivo, e apagar o pedido cedo demais a deixava no
# jogo, com a nova injetada por cima (duas Centrais ao mesmo tempo).
#
# E cada cópia APAGA o pedido ao vê-lo (lib.rs, atender_pedido_de_descarga):
# com duas carregadas, a primeira a olhar consome o arquivo e a segunda nunca
# o vê. Por isso o pedido é recriado enquanto ainda sobrar alguma cópia.
$restantes = $carregadas
for ($i = 0; $i -lt 80; $i++) {
    $restantes = Get-OverlayCarregadas
    if ($restantes.Count -eq 0) { break }
    if (-not (Test-Path $pedido)) {
        New-Item -Path $pedido -ItemType File -Force | Out-Null
    }
    Start-Sleep -Milliseconds 500
}
Remove-Item $pedido -Force -ErrorAction SilentlyContinue
Registrar ("Depois da espera: {0} cópia(s) ainda carregada(s)." -f @($restantes).Count)
if ($restantes.Count -gt 0) {
    # Uma cópia que não atende o pedido por 40 s está parada (o desenho dela
    # morreu: por exemplo, os ganchos caíram junto com os de outra cópia que
    # se descarregou) ou é anterior à 1.6-v2. Ela não desenha mais, mas o
    # arquivo dela continua travado: a build nova vai para uma cópia com nome
    # novo, e a antiga só sai do jogo quando o jogo fechar.
    $quais = ($restantes | ForEach-Object { $_.FileName }) -join ", "
    Registrar "AVISO: ainda carregada(s) e sem responder: $quais"
    Registrar "Injetando a build nova numa cópia com outro nome; reinicie o jogo quando puder (iniciar_fifa.ps1)."
    Write-Host "AVISO: a cópia antiga continua no jogo (parada) até ele fechar." -ForegroundColor Yellow
    $copia = Join-Path $overlayDir ("target\fifa_overlay_dev_{0}.dll" -f (Get-Date -Format "yyyyMMdd_HHmmss"))
}
# cópias de nome novo de recargas anteriores que já saíram do jogo
Get-ChildItem (Join-Path $overlayDir "target") -Filter "fifa_overlay_dev_*.dll" -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -ne $copia } |
    ForEach-Object { Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue }

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
    Write-Error "Não foi possível copiar a build nova para $copia (arquivo em uso)."
}
Registrar "Descarregada. Build nova: $((Get-Item $built).LastWriteTime)"

# Um instante para o FreeLibrary terminar antes de injetar de novo.
Start-Sleep -Seconds 1
& $injector $copia
if ($LASTEXITCODE -ne 0) {
    Read-Host "A injeção falhou. Pressione Enter para fechar"
    exit 1
}
Start-Sleep -Seconds 3
$depois = Get-OverlayCarregadas
Registrar ("Depois de injetar: {0} cópia(s) carregada(s)." -f @($depois).Count)
foreach ($m in $depois) { Registrar "  - $($m.FileName)" }
if ($depois.Count -ne 1) {
    Write-Host ("ATENÇÃO: {0} cópias do overlay carregadas (era para ser 1):" -f $depois.Count) -ForegroundColor Yellow
    foreach ($m in $depois) { Write-Host "  - $($m.FileName)" -ForegroundColor Yellow }
    Read-Host "Pressione Enter para fechar"
} else {
    Write-Host "Central de Scout recarregada (1 cópia carregada). Confira a versão no log (%TEMP%\fifa_overlay.log)."
    Start-Sleep -Seconds 2
}
