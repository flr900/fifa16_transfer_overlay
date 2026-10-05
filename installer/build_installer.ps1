# Gera o instalador da Central de Scout: installer\Output\CentralDeScout_Setup_<versão>.exe
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File .\installer\build_installer.ps1
#   ... -Versao 1.2.0        versão que aparece no instalador (padrão: a do .iss)
#   ... -SemCompilar         só empacota o que já está em target\release
#
# Pré-requisito: Inno Setup 6 (winget install JRSoftware.InnoSetup).

param(
    [string]$Versao,
    [switch]$SemCompilar
)

$ErrorActionPreference = "Stop"

$raiz = Split-Path -Parent $PSScriptRoot

# O instalador leva o código desta checkout: avisa se ela está atrás do remoto
# (já saiu um instalador sem os Olheiros novos por causa disso).
Push-Location $raiz
try {
    git fetch origin --quiet 2>$null
    $ramo = (git rev-parse --abbrev-ref HEAD).Trim()
    $atras = (git rev-list --count "HEAD..origin/$ramo" 2>$null)
    if ($atras -and [int]$atras -gt 0) {
        Write-Warning "A branch '$ramo' está $atras commit(s) atrás de origin/$ramo. Rode 'git pull --ff-only' antes de gerar o instalador."
    }
} catch {
    Write-Warning "Não deu para comparar com o remoto: $_"
} finally {
    Pop-Location
}

if (-not $SemCompilar) {
    foreach ($crate in "fifa_injector", "fifa_overlay") {
        Write-Host "== cargo build --release ($crate) ==" -ForegroundColor Cyan
        Push-Location (Join-Path $raiz $crate)
        try {
            cargo build --release
            if ($LASTEXITCODE -ne 0) { throw "cargo build falhou em $crate." }
        } finally {
            Pop-Location
        }
    }
}

$iscc = @(
    (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source,
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) {
    throw "Inno Setup 6 não encontrado. Instale com: winget install JRSoftware.InnoSetup"
}

$argumentos = @("/Qp")
if ($Versao) { $argumentos += "/DAppVersao=$Versao" }
$argumentos += (Join-Path $PSScriptRoot "CentralDeScout.iss")

Write-Host "== ISCC ==" -ForegroundColor Cyan
& $iscc @argumentos
if ($LASTEXITCODE -ne 0) { throw "ISCC falhou (código $LASTEXITCODE)." }

Get-ChildItem (Join-Path $PSScriptRoot "Output") -Filter *.exe |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1 |
    ForEach-Object { Write-Host "Instalador: $($_.FullName) ($([math]::Round($_.Length / 1MB, 1)) MB)" -ForegroundColor Green }
