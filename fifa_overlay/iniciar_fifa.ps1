# Abre o FIFA Friends e injeta a Central de Scout quando o FIFA 16 abrir
# (Story 1.7). Substitui abrir o atalho do FIFA Friends + rodar o
# injetor à mão.
#
# Uso: clique com o botão direito > "Executar com o PowerShell", ou
#   powershell -ExecutionPolicy Bypass -File .\iniciar_fifa.ps1
# Pede Administrador uma vez (o FIFA roda elevado, o injetor também
# precisa).
#
# O que faz:
#   1. copia target\release\fifa_overlay.dll para target\fifa_overlay_dev.dll
#      (o jogo trava a DLL que carregou; injetando a cópia, o
#      `cargo build` continua livre para sobrescrever a original);
#   2. abre o servidor do FIFA Friends (o mesmo alvo do atalho da área de
#      trabalho) — se já estiver aberto, reaproveita;
#   3. deixa o injetor esperando o fifa16.exe: quando o jogo carregar o
#      DirectX e mostrar a janela, injeta. O jogo mostra "Central de
#      Scout ativa" no canto superior direito por 3 segundos.
#   Quando o jogo fecha, o injetor termina e esta janela fecha junto (só
#   fica aberta se algo der errado, para dar tempo de ler a mensagem).

param(
    [string]$Atalho = (Join-Path ([Environment]::GetFolderPath("Desktop")) "FIFA FRIENDS PREMIUM!.lnk")
)

$ErrorActionPreference = "Stop"

# Janela elevada abre sem -NoExit: num erro, segura a mensagem na tela.
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
    $argumentos = "-ExecutionPolicy Bypass -File `"$PSCommandPath`" -Atalho `"$Atalho`""
    Start-Process powershell.exe -Verb RunAs -ArgumentList $argumentos
    exit
}

$overlayDir = $PSScriptRoot
$built = Join-Path $overlayDir "target\release\fifa_overlay.dll"
$copia = Join-Path $overlayDir "target\fifa_overlay_dev.dll"
$injector = Join-Path $overlayDir "..\fifa_injector\target\release\fifa_injector.exe"

if (-not (Test-Path $built)) {
    Write-Error "Build não encontrada: $built (rode 'cargo build --release' em fifa_overlay)."
}
if (-not (Test-Path $injector)) {
    Write-Error "Injetor não encontrado: $injector (rode 'cargo build --release' em fifa_injector)."
}
if (-not (Test-Path $Atalho)) {
    Write-Error "Atalho do FIFA Friends não encontrado: $Atalho (passe outro com -Atalho)."
}

try {
    Copy-Item $built $copia -Force
    Write-Host "Overlay: $copia (build de $((Get-Item $built).LastWriteTime))"
} catch {
    Write-Warning "A cópia da DLL está em uso (o jogo já tem a Central de Scout carregada). Usando a cópia existente."
}

$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut($Atalho)
$nomeServidor = [IO.Path]::GetFileNameWithoutExtension($lnk.TargetPath)
$servidor = Get-Process -Name $nomeServidor -ErrorAction SilentlyContinue | Sort-Object StartTime | Select-Object -First 1
if ($servidor) {
    Write-Host "FIFA Friends já está aberto (pid $($servidor.Id))."
} else {
    $abrir = @{ FilePath = $lnk.TargetPath; WorkingDirectory = $lnk.WorkingDirectory; PassThru = $true }
    if ($lnk.Arguments) { $abrir.ArgumentList = $lnk.Arguments }
    $servidor = Start-Process @abrir
    Write-Host "FIFA Friends aberto (pid $($servidor.Id)). Abra o FIFA 16 como de costume."
}

& $injector --aguardar --enquanto-pid $servidor.Id $copia
if ($LASTEXITCODE -ne 0) {
    Read-Host "O injetor terminou com erro. Pressione Enter para fechar"
}
