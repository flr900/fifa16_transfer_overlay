# Central de Scout — escolhe de novo o executável do FIFA Friends.
# Grava em HKCU\Software\Central de Scout (o mesmo lugar que o instalador usa).

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms

$chave = "HKCU:\Software\Central de Scout"
$atual = (Get-ItemProperty -Path $chave -ErrorAction SilentlyContinue).FifaFriendsExe

$dialogo = New-Object Windows.Forms.OpenFileDialog
$dialogo.Title = "Escolha o executável do FIFA Friends (ex.: Server16Python.exe)"
$dialogo.Filter = "Executáveis (*.exe)|*.exe|Todos os arquivos (*.*)|*.*"
if ($atual -and (Test-Path -LiteralPath (Split-Path -Parent $atual))) {
    $dialogo.InitialDirectory = Split-Path -Parent $atual
    $dialogo.FileName = Split-Path -Leaf $atual
}

if ($dialogo.ShowDialog() -eq [Windows.Forms.DialogResult]::OK) {
    New-Item -Path $chave -Force | Out-Null
    Set-ItemProperty -Path $chave -Name FifaFriendsExe -Value $dialogo.FileName
    [void][Windows.Forms.MessageBox]::Show("FIFA Friends: $($dialogo.FileName)", "Central de Scout", "OK", "Information")
}
