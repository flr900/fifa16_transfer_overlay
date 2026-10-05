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

# --- Histórico de versões ---------------------------------------------------
# O instalador leva uma DLL por versão do histórico e deixa o usuário escolher.
# 1) arquiva a build atual em installer\versoes\<id>\;
# 2) lê historico_versoes.ini e gera os dois .inc que o .iss inclui.

$pastaVersoes = Join-Path $PSScriptRoot "versoes"
$dllAtual = Join-Path $raiz "fifa_overlay\target\release\fifa_overlay.dll"
$tagAtual = [regex]::Match(
    (Get-Content (Join-Path $raiz "fifa_overlay\src\lib.rs") -Raw -Encoding UTF8),
    'const BUILD_TAG: &str = "([^"]+)"').Groups[1].Value
if (-not $tagAtual) { throw "BUILD_TAG não encontrado em fifa_overlay\src\lib.rs." }
$idAtual = ($tagAtual -split ' — ', 2)[0]
$tituloAtual = if ($tagAtual -match ' — ') { ($tagAtual -split ' — ', 2)[1] } else { $idAtual }

$destinoAtual = Join-Path $pastaVersoes "$idAtual\fifa_overlay.dll"
New-Item -ItemType Directory -Path (Split-Path $destinoAtual) -Force | Out-Null
if ((Test-Path $destinoAtual) -and ((Get-FileHash $destinoAtual).Hash -ne (Get-FileHash $dllAtual).Hash)) {
    Write-Warning "A build $idAtual já estava arquivada com outro conteúdo; sobrescrevendo. Suba o BUILD_TAG a cada build de teste."
}
Copy-Item $dllAtual $destinoAtual -Force

# historico_versoes.ini -> lista ordenada de @{ Id; Titulo; Data; Notas }
$historico = @()
$secao = $null
foreach ($linha in (Get-Content (Join-Path $PSScriptRoot "historico_versoes.ini") -Encoding UTF8)) {
    if ($linha -match '^\s*\[(.+)\]\s*$') {
        $secao = @{ Id = $Matches[1]; Titulo = ""; Data = ""; Notas = "" }
        $historico += $secao
    } elseif ($secao -and $linha -match '^\s*(Titulo|Data|Notas)\s*=\s*(.*)$') {
        $secao[$Matches[1]] = $Matches[2].Trim()
    }
}
if (-not ($historico | Where-Object { $_.Id -eq $idAtual })) {
    Write-Warning "A build $idAtual não está em historico_versoes.ini; entrando como a mais nova, sem notas."
    $historico = @(@{ Id = $idAtual; Titulo = $tituloAtual; Data = (Get-Date -Format "yyyy-MM-dd"); Notas = "" }) + $historico
}

$incArquivos = @("; Gerado por build_installer.ps1 — não editar.")
$linhasVersoes = @()
$incluidas = @()
foreach ($v in $historico) {
    $dll = Join-Path $pastaVersoes "$($v.Id)\fifa_overlay.dll"
    if (-not (Test-Path $dll)) {
        Write-Warning "Versão $($v.Id) está no histórico mas sem DLL arquivada em $dll; fora do instalador."
        continue
    }
    $i = $incluidas.Count
    $incluidas += $v.Id
    $hash = (Get-FileHash $dll -Algorithm SHA256).Hash.ToLower()
    $pascal = { param($t) "'" + ($t -replace "'", "''" -replace '\\n', "' + #13#10 + '") + "'" }
    $incArquivos += "Source: `"versoes\$($v.Id)\fifa_overlay.dll`"; DestDir: `"{app}`"; DestName: `"fifa_overlay.dll`"; Flags: ignoreversion; Check: EscolhidaEh('$($v.Id -replace "'", "''")')"
    $linhasVersoes += "  VersaoId[$i] := $(& $pascal $v.Id);"
    $linhasVersoes += "  VersaoTitulo[$i] := $(& $pascal $v.Titulo);"
    $linhasVersoes += "  VersaoData[$i] := $(& $pascal $v.Data);"
    $linhasVersoes += "  VersaoNotas[$i] := $(& $pascal $v.Notas);"
    $linhasVersoes += "  VersaoHash[$i] := '$hash';"
}
if ($incluidas.Count -eq 0) { throw "Nenhuma versão com DLL arquivada." }
$n = $incluidas.Count
$incCodigo = @("// Gerado por build_installer.ps1 — não editar.", "procedure CarregarVersoes;", "begin") +
    @("VersaoId", "VersaoTitulo", "VersaoData", "VersaoNotas", "VersaoHash" | ForEach-Object { "  SetArrayLength($_, $n);" }) +
    $linhasVersoes + @("end;")
# UTF-8 com BOM: o ISPP lê os acentos.
Set-Content -Path (Join-Path $PSScriptRoot "versoes_arquivos.gen.inc") -Value $incArquivos -Encoding UTF8
Set-Content -Path (Join-Path $PSScriptRoot "versoes_codigo.gen.inc") -Value $incCodigo -Encoding UTF8
Write-Host "Versões no instalador: $($incluidas -join ', ')" -ForegroundColor Cyan

$argumentos = @("/Qp")
if ($Versao) { $argumentos += "/DAppVersao=$Versao" }
$argumentos += (Join-Path $PSScriptRoot "CentralDeScout.iss")

Write-Host "== ISCC ==" -ForegroundColor Cyan
& $iscc @argumentos
if ($LASTEXITCODE -ne 0) { throw "ISCC falhou (código $LASTEXITCODE)." }

Get-ChildItem (Join-Path $PSScriptRoot "Output") -Filter *.exe |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1 |
    ForEach-Object { Write-Host "Instalador: $($_.FullName) ($([math]::Round($_.Length / 1MB, 1)) MB)" -ForegroundColor Green }
