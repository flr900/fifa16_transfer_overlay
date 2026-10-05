; Instalador da Central de Scout (Inno Setup 6).
; Gere com installer\build_installer.ps1 — ele compila o overlay e o injetor
; em release e depois roda o ISCC neste arquivo.
;
; Instala por usuário (sem Administrador) em %LOCALAPPDATA%\Programs. O
; Administrador só é pedido ao abrir a Central de Scout, porque o FIFA 16
; roda elevado (ver scripts\iniciar_scout.ps1).
;
; Instalação silenciosa:
;   CentralDeScout_Setup.exe /VERYSILENT /FIFAFRIENDS="D:\Program Files\FIFA 16\Server16Python.exe"

#ifndef AppVersao
  #define AppVersao "1.0.0"
#endif

#define AppNome "Central de Scout"
#define NomeAtalhoFifaFriends "FIFA FRIENDS PREMIUM!.lnk"
#define PowerShell "{sys}\WindowsPowerShell\v1.0\powershell.exe"

[Setup]
AppId={{06E27AB7-34A8-43FA-90F4-AB88D4D95B01}
AppName={#AppNome}
AppVersion={#AppVersao}
AppVerName={#AppNome} {#AppVersao}
DefaultDirName={autopf}\{#AppNome}
DefaultGroupName={#AppNome}
DisableProgramGroupPage=yes
UninstallDisplayName={#AppNome} (FIFA 16)
OutputDir=Output
OutputBaseFilename=CentralDeScout_Setup_{#AppVersao}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=no

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\fifa_overlay\target\release\fifa_overlay.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\fifa_injector\target\release\fifa_injector.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "scripts\iniciar_scout.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "scripts\configurar_scout.ps1"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
; Configuração lida por iniciar_scout.ps1 / configurar_scout.ps1.
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "FifaFriendsExe"; ValueData: "{code:GetFifaFriendsExe}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "FifaFriendsArgs"; ValueData: "{code:GetFifaFriendsArgs}"
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "PastaInstalacao"; ValueData: "{app}"

[Icons]
; O ícone vem do executável do FIFA Friends.
Name: "{autodesktop}\{#AppNome}"; Filename: "{#PowerShell}"; Parameters: "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{app}\iniciar_scout.ps1"""; WorkingDir: "{app}"; IconFilename: "{code:GetFifaFriendsExe}"; Comment: "Abre o FIFA Friends e liga a Central de Scout quando o FIFA 16 abrir"; Tasks: desktopicon
Name: "{group}\{#AppNome}"; Filename: "{#PowerShell}"; Parameters: "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{app}\iniciar_scout.ps1"""; WorkingDir: "{app}"; IconFilename: "{code:GetFifaFriendsExe}"; Comment: "Abre o FIFA Friends e liga a Central de Scout quando o FIFA 16 abrir"
Name: "{group}\Configurar {#AppNome}"; Filename: "{#PowerShell}"; Parameters: "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{app}\configurar_scout.ps1"""; WorkingDir: "{app}"; Comment: "Escolher de novo o executável do FIFA Friends"
Name: "{group}\Desinstalar {#AppNome}"; Filename: "{uninstallexe}"

[Run]
Filename: "{#PowerShell}"; Parameters: "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{app}\iniciar_scout.ps1"""; WorkingDir: "{app}"; Description: "Abrir a {#AppNome} agora"; Flags: postinstall nowait skipifsilent unchecked

[UninstallDelete]
Type: filesandordirs; Name: "{app}\runtime"

[Code]
var
  PaginaFifa: TInputFileWizardPage;
  ArgsFifaFriends: String;

// O overlay e o injetor são x64 e dependem do VC++ 2015-2022 (vcruntime140).
function VcRedistInstalado: Boolean;
var
  Instalado: Cardinal;
begin
  Result := RegQueryDWordValue(HKLM64, 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64', 'Installed', Instalado)
    and (Instalado = 1);
end;

function InitializeSetup: Boolean;
begin
  Result := True;
  if not VcRedistInstalado then
    Result := (SuppressibleMsgBox(
      'O Microsoft Visual C++ Redistributable 2015-2022 (x64) não foi encontrado, e a Central de Scout precisa dele.' + #13#10 + #13#10 +
      'Baixe em https://aka.ms/vs/17/release/vc_redist.x64.exe e instale depois deste instalador, se a Central de Scout não abrir no jogo.' + #13#10 + #13#10 +
      'Continuar a instalação mesmo assim?',
      mbConfirmation, MB_YESNO, IDYES) = IDYES);
end;

// Alvo (e argumentos) de um atalho .lnk.
function LerAtalho(const Caminho: String; var Alvo, Argumentos: String): Boolean;
var
  Shell, Atalho: Variant;
begin
  Result := False;
  try
    Shell := CreateOleObject('WScript.Shell');
    Atalho := Shell.CreateShortcut(Caminho);
    Alvo := Atalho.TargetPath;
    Argumentos := Atalho.Arguments;
    Result := (Alvo <> '') and FileExists(Alvo);
  except
  end;
end;

// Atalho do FIFA Friends na área de trabalho (do usuário ou pública).
function AcharPeloAtalho(var Alvo, Argumentos: String): Boolean;
var
  Pastas: array[0..1] of String;
  Busca: TFindRec;
  I: Integer;
begin
  Result := False;
  Pastas[0] := ExpandConstant('{userdesktop}');
  Pastas[1] := ExpandConstant('{commondesktop}');
  for I := 0 to 1 do
  begin
    if FileExists(Pastas[I] + '\{#NomeAtalhoFifaFriends}') then
      if LerAtalho(Pastas[I] + '\{#NomeAtalhoFifaFriends}', Alvo, Argumentos) then
      begin
        Result := True;
        Exit;
      end;
    // Outro nome de atalho: qualquer .lnk com FIFA e FRIENDS no nome.
    if FindFirst(Pastas[I] + '\*FIFA*FRIENDS*.lnk', Busca) then
    try
      repeat
        if LerAtalho(Pastas[I] + '\' + Busca.Name, Alvo, Argumentos) then
        begin
          Result := True;
          Exit;
        end;
      until not FindNext(Busca);
    finally
      FindClose(Busca);
    end;
  end;
end;

// Pastas comuns do FIFA 16, em qualquer unidade.
function AcharPelasPastas: String;
var
  Relativas: array[0..4] of String;
  Unidade, I: Integer;
  Candidato: String;
begin
  Result := '';
  Relativas[0] := 'Program Files\FIFA 16';
  Relativas[1] := 'Program Files (x86)\FIFA 16';
  Relativas[2] := 'Program Files (x86)\Origin Games\FIFA 16';
  Relativas[3] := 'Games\FIFA 16';
  Relativas[4] := 'FIFA 16';
  if RegQueryStringValue(HKLM32, 'SOFTWARE\EA Games\FIFA 16', 'Install Dir', Candidato) then
    if FileExists(AddBackslash(Candidato) + 'Server16Python.exe') then
    begin
      Result := AddBackslash(Candidato) + 'Server16Python.exe';
      Exit;
    end;
  for Unidade := Ord('C') to Ord('H') do
    for I := 0 to 4 do
    begin
      Candidato := Chr(Unidade) + ':\' + Relativas[I] + '\Server16Python.exe';
      if FileExists(Candidato) then
      begin
        Result := Candidato;
        Exit;
      end;
    end;
end;

// Ordem: parâmetro /FIFAFRIENDS, instalação anterior, atalho da área de
// trabalho, pastas comuns.
function DetectarFifaFriends: String;
var
  Alvo, Argumentos: String;
begin
  Result := ExpandConstant('{param:FifaFriends|}');
  if Result <> '' then Exit;
  if RegQueryStringValue(HKCU, 'Software\{#AppNome}', 'FifaFriendsExe', Result) and FileExists(Result) then
  begin
    RegQueryStringValue(HKCU, 'Software\{#AppNome}', 'FifaFriendsArgs', ArgsFifaFriends);
    Exit;
  end;
  if AcharPeloAtalho(Alvo, Argumentos) then
  begin
    ArgsFifaFriends := Argumentos;
    Result := Alvo;
    Exit;
  end;
  Result := AcharPelasPastas;
end;

procedure InitializeWizard;
begin
  PaginaFifa := CreateInputFilePage(wpSelectDir,
    'FIFA Friends',
    'Onde está o executável do FIFA Friends?',
    'A Central de Scout abre o FIFA Friends (o mesmo programa do atalho "FIFA FRIENDS PREMIUM!") e liga o overlay quando o FIFA 16 abrir. ' +
    'Confira o caminho abaixo ou escolha outro.');
  PaginaFifa.Add('Executável do FIFA Friends (ex.: Server16Python.exe):',
    'Executáveis (*.exe)|*.exe|Todos os arquivos (*.*)|*.*', '.exe');
  PaginaFifa.Values[0] := DetectarFifaFriends;
end;

function GetFifaFriendsExe(Param: String): String;
begin
  Result := Trim(PaginaFifa.Values[0]);
end;

function GetFifaFriendsArgs(Param: String): String;
begin
  Result := ArgsFifaFriends;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Exe: String;
begin
  Result := True;
  if CurPageID = PaginaFifa.ID then
  begin
    Exe := GetFifaFriendsExe('');
    if not FileExists(Exe) then
    begin
      MsgBox('Escolha o executável do FIFA Friends: o arquivo "' + Exe + '" não existe.', mbError, MB_OK);
      Result := False;
    end
    else if not FileExists(ExtractFilePath(Exe) + 'fifa16.exe') then
      Result := (MsgBox('Não achei o fifa16.exe na mesma pasta de "' + ExtractFileName(Exe) + '". ' +
        'Esse é mesmo o executável do FIFA Friends?', mbConfirmation, MB_YESNO) = IDYES);
  end;
end;

// Cobre também a instalação silenciosa, que pula as páginas.
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not FileExists(GetFifaFriendsExe('')) then
    Result := 'Executável do FIFA Friends não encontrado: "' + GetFifaFriendsExe('') + '".' + #13#10 +
      'Informe o caminho na instalação ou use /FIFAFRIENDS="caminho\Server16Python.exe".';
end;

// Missões, olheiros e relatórios ficam em %LOCALAPPDATA%\FifaCompanion\scout,
// fora da pasta do programa: por padrão sobrevivem à desinstalação.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Dados: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    Dados := ExpandConstant('{localappdata}\FifaCompanion\scout');
    if DirExists(Dados) and not UninstallSilent then
      if MsgBox('Remover também os dados salvos da Central de Scout (missões, olheiros e relatórios das suas carreiras)?' + #13#10 + #13#10 +
        'Se pretende reinstalar, responda Não.', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(Dados, True, True, True);
  end;
end;
