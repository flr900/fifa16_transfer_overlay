# Tira UMA captura da memoria do FIFA 16 para o experimento das telas de
# negociacao (so leitura). Uso, com o jogo parado na tela que se quer fotografar:
#
#   powershell -ExecutionPolicy Bypass -File .\capturar_tela.ps1 t1_hub
#
# O script se eleva sozinho (o FIFA roda como Administrador), usa o Python de
# verdade (nao o atalho da Microsoft Store), mostra o resultado e espera Enter
# antes de fechar. Tudo que aparece tambem fica em
# %TEMP%\scout_probe_saida.txt, para outra pessoa poder ler.

param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Rotulo,
    [string]$Python = ""
)

$ErrorActionPreference = "Stop"
$saida = Join-Path $env:TEMP "scout_probe_saida.txt"

trap {
    Write-Host ""
    Write-Host "Erro: $_" -ForegroundColor Red
    Add-Content -Path $saida -Value ("ERRO: {0}" -f $_) -ErrorAction SilentlyContinue
    Read-Host "Pressione Enter para fechar"
    exit 1
}

# Acha o Python ANTES de elevar (a janela de Administrador pode ter outro PATH).
if (-not $Python) {
    $candidatos = @("C:\Python314\python.exe") + @(
        Get-Command python -All -ErrorAction SilentlyContinue |
            Where-Object { $_.Source -and $_.Source -notlike "*WindowsApps*" } |
            ForEach-Object { $_.Source }
    )
    $Python = $candidatos | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $Python) { throw "Python nao encontrado. Instale o Python ou passe -Python <caminho>." }
}

$identidade = [Security.Principal.WindowsIdentity]::GetCurrent()
$admin = (New-Object Security.Principal.WindowsPrincipal($identidade)).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    Start-Process powershell.exe -Verb RunAs -ArgumentList "-ExecutionPolicy Bypass -File `"$PSCommandPath`" `"$Rotulo`" -Python `"$Python`""
    exit
}

if (-not (Get-Process fifa16 -ErrorAction SilentlyContinue)) {
    throw "O FIFA 16 nao esta aberto."
}

Set-Location $PSScriptRoot
Set-Content -Path $saida -Value ("--- capturar_tela.ps1 {0} ({1}) ---" -f $Rotulo, (Get-Date -Format "HH:mm:ss")) -Encoding utf8
Write-Host "Fotografando '$Rotulo' com $Python ..."
Write-Host "NAO mexa no jogo ate aparecer o resumo (uns 20 a 25 s)." -ForegroundColor Yellow

$env:PYTHONIOENCODING = "utf-8"
$textos = @("assinar contrato", "sobre compra", "sobre empr", "Perguntar sobre", "Escolhidos")
$argumentos = @("scout_probe.py", "capture", $Rotulo)
foreach ($t in $textos) { $argumentos += @("--str", $t) }

& $Python @argumentos 2>&1 | Tee-Object -FilePath $saida -Append
if ($LASTEXITCODE -ne 0) { throw "A captura falhou (codigo $LASTEXITCODE). Veja a mensagem acima." }

Write-Host ""
Write-Host "Pronto: $Rotulo gravada. Pode mexer no jogo." -ForegroundColor Green
Add-Content -Path $saida -Value "PRONTO" -ErrorAction SilentlyContinue
Read-Host "Pressione Enter para fechar"
