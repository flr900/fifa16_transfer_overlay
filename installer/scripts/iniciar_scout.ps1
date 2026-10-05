# Central de Scout — abre o FIFA Friends e injeta a Central de Scout quando o
# FIFA 16 abrir. É o que o atalho instalado executa.
#
# Versão instalada de fifa_overlay\iniciar_fifa.ps1: em vez de procurar a
# build e o atalho do FIFA Friends no repositório, lê o caminho do
# executável do FIFA Friends gravado pelo instalador em
# HKCU\Software\Central de Scout (mude em "Configurar Central de Scout").
#
# O FIFA roda elevado, então o injetor também precisa: o script pede
# Administrador uma vez (o atalho abre esta primeira etapa sem janela; a
# segunda, elevada, mostra o andamento e fecha junto com o jogo).

param(
    [string]$FifaFriendsExe,
    [string]$FifaFriendsArgs
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms

# Sem janela de console na primeira etapa: erros aparecem numa caixa de mensagem.
function Mostrar-Erro([string]$mensagem) {
    [void][Windows.Forms.MessageBox]::Show($mensagem, "Central de Scout", "OK", "Error")
}

trap {
    Mostrar-Erro "$_"
    exit 1
}

$chave = "HKCU:\Software\Central de Scout"
if (-not $FifaFriendsExe) {
    $config = Get-ItemProperty -Path $chave -ErrorAction SilentlyContinue
    if ($config) {
        $FifaFriendsExe = $config.FifaFriendsExe
        $FifaFriendsArgs = $config.FifaFriendsArgs
    }
}
if (-not $FifaFriendsExe -or -not (Test-Path -LiteralPath $FifaFriendsExe)) {
    Write-Error ("Não encontrei o executável do FIFA Friends ($FifaFriendsExe).`n`n" +
        "Escolha-o de novo em Menu Iniciar > Central de Scout > Configurar Central de Scout.")
}

$identidade = [Security.Principal.WindowsIdentity]::GetCurrent()
$admin = (New-Object Security.Principal.WindowsPrincipal($identidade)).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    # A configuração segue como parâmetro: se quem aprova o UAC for outra
    # conta, o HKCU dela não tem o caminho.
    $lista = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"",
        "-FifaFriendsExe", "`"$FifaFriendsExe`"")
    if ($FifaFriendsArgs) { $lista += @("-FifaFriendsArgs", "`"$($FifaFriendsArgs -replace '"', '\"')`"") }
    try {
        Start-Process powershell.exe -Verb RunAs -ArgumentList $lista
    } catch {
        Write-Error "A Central de Scout precisa de permissão de Administrador (o FIFA 16 roda elevado)."
    }
    exit
}

# --- Etapa elevada ---------------------------------------------------------

$pasta = $PSScriptRoot
$dll = Join-Path $pasta "fifa_overlay.dll"
$injetor = Join-Path $pasta "fifa_injector.exe"
$pastaCopia = Join-Path $pasta "runtime"
$copia = Join-Path $pastaCopia "fifa_overlay_carregada.dll"

if (-not (Test-Path $dll) -or -not (Test-Path $injetor)) {
    Write-Error "A instalação está incompleta (faltam arquivos em $pasta). Rode o instalador de novo."
}

# O jogo trava a DLL que carregou. Injetando uma cópia, o instalador pode
# atualizar a original com o FIFA aberto.
New-Item -ItemType Directory -Path $pastaCopia -Force | Out-Null
try {
    Copy-Item $dll $copia -Force
} catch {
    if (-not (Test-Path $copia)) { throw }
    Write-Warning "A cópia da DLL está em uso (o jogo já tem a Central de Scout carregada). Usando a cópia existente."
}

$nomeServidor = [IO.Path]::GetFileNameWithoutExtension($FifaFriendsExe)
$servidor = Get-Process -Name $nomeServidor -ErrorAction SilentlyContinue | Sort-Object StartTime | Select-Object -First 1
if ($servidor) {
    Write-Host "FIFA Friends já está aberto (pid $($servidor.Id))."
} else {
    $abrir = @{
        FilePath         = $FifaFriendsExe
        WorkingDirectory = (Split-Path -Parent $FifaFriendsExe)
        PassThru         = $true
    }
    if ($FifaFriendsArgs) { $abrir.ArgumentList = $FifaFriendsArgs }
    $servidor = Start-Process @abrir
    Write-Host "FIFA Friends aberto (pid $($servidor.Id)). Abra o FIFA 16 como de costume."
}

Write-Host "Esperando o FIFA 16 abrir para ligar a Central de Scout..."
& $injetor --aguardar --enquanto-pid $servidor.Id $copia
if ($LASTEXITCODE -ne 0) {
    Write-Error "O injetor terminou com erro (código $LASTEXITCODE). O log da Central de Scout fica em %TEMP%\fifa_overlay.log."
}
