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
; Uma fifa_overlay.dll por versão do histórico (gerado pelo build_installer.ps1);
; só a escolhida na página "Versão" é instalada.
#include "versoes_arquivos.gen.inc"
Source: "..\fifa_injector\target\release\fifa_injector.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "scripts\iniciar_scout.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "scripts\configurar_scout.ps1"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
; Configuração lida por iniciar_scout.ps1 / configurar_scout.ps1.
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "FifaFriendsExe"; ValueData: "{code:GetFifaFriendsExe}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "FifaFriendsArgs"; ValueData: "{code:GetFifaFriendsArgs}"
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "PastaInstalacao"; ValueData: "{app}"
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "VersaoInstalada"; ValueData: "{code:GetVersaoEscolhida}"
Root: HKCU; Subkey: "Software\{#AppNome}"; ValueType: string; ValueName: "VersaoInstalador"; ValueData: "{#AppVersao}"

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
  PaginaVersao: TInputOptionWizardPage;
  NotasVersao: TNewMemo;
  // Histórico de versões (preenchido por CarregarVersoes, gerada pelo
  // build_installer.ps1): mais nova primeiro.
  VersaoId, VersaoTitulo, VersaoData, VersaoNotas, VersaoHash: array of String;
  // O que já está instalado: id da build ('' = nenhuma instalação;
  // 'desconhecida' = há uma DLL que não é de nenhuma versão do histórico).
  InstaladaId, InstaladaPasta, InstaladorInstalado: String;

#include "versoes_codigo.gen.inc"

// A pasta onde o Inno instalaria sem perguntar (sem o registro, a
// instalação anterior só é achada por ela).
function PastaPadrao: String;
begin
  Result := ExpandConstant('{autopf}\{#AppNome}');
end;

function IdsDisponiveis: String;
var
  I: Integer;
begin
  Result := '';
  for I := 0 to GetArrayLength(VersaoId) - 1 do
  begin
    if I > 0 then Result := Result + ', ';
    Result := Result + VersaoId[I];
  end;
end;

function IndiceDaVersao(const Id: String): Integer;
var
  I: Integer;
begin
  Result := -1;
  for I := 0 to GetArrayLength(VersaoId) - 1 do
    if SameText(VersaoId[I], Id) then
    begin
      Result := I;
      Exit;
    end;
end;

// Procura uma instalação anterior: registro da Central de Scout, entrada de
// desinstalação do Inno e, como o registro pode ter sumido, a pasta padrão.
// A versão sai do registro ou, sem ele, do hash da DLL instalada.
procedure DetectarInstalacao;
var
  Hash: String;
  I: Integer;
begin
  InstaladaId := '';
  InstaladaPasta := '';
  InstaladorInstalado := '';
  RegQueryStringValue(HKCU, 'Software\{#AppNome}', 'PastaInstalacao', InstaladaPasta);
  RegQueryStringValue(HKCU, 'Software\{#AppNome}', 'VersaoInstalada', InstaladaId);
  RegQueryStringValue(HKCU, 'Software\{#AppNome}', 'VersaoInstalador', InstaladorInstalado);
  if InstaladorInstalado = '' then
    RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{06E27AB7-34A8-43FA-90F4-AB88D4D95B01}_is1',
      'DisplayVersion', InstaladorInstalado);
  if (InstaladaPasta = '') or not FileExists(AddBackslash(InstaladaPasta) + 'fifa_overlay.dll') then
    InstaladaPasta := PastaPadrao;
  if not FileExists(AddBackslash(InstaladaPasta) + 'fifa_overlay.dll') then
  begin
    InstaladaPasta := '';
    InstaladaId := '';
    Exit;
  end;
  // O que o registro diz só vale se a DLL ainda for a dessa versão.
  Hash := GetSHA256OfFile(AddBackslash(InstaladaPasta) + 'fifa_overlay.dll');
  for I := 0 to GetArrayLength(VersaoId) - 1 do
    if SameText(VersaoHash[I], Hash) then
    begin
      InstaladaId := VersaoId[I];
      Exit;
    end;
  InstaladaId := 'desconhecida';
end;

function DescricaoInstalada: String;
begin
  if InstaladaPasta = '' then
    Result := 'Nenhuma instalação anterior foi encontrada.'
  else
  begin
    if InstaladaId = 'desconhecida' then
      Result := 'Já instalada: uma versão que não está no histórico'
    else
      Result := 'Já instalada: versão ' + InstaladaId;
    if InstaladorInstalado <> '' then
      Result := Result + ' (instalador ' + InstaladorInstalado + ')';
    Result := Result + ' em ' + InstaladaPasta + '.';
  end;
end;

procedure MostrarNotasDaVersao(Indice: Integer);
var
  Texto: String;
begin
  if (Indice < 0) or (Indice >= GetArrayLength(VersaoId)) then Exit;
  Texto := VersaoTitulo[Indice] + ' · ' + VersaoData[Indice];
  if VersaoNotas[Indice] <> '' then
    Texto := Texto + #13#10 + #13#10 + VersaoNotas[Indice];
  if Indice > 0 then
    Texto := Texto + #13#10 + #13#10 + 'Atenção: há versões mais novas; esta pode não ter as últimas correções.';
  if SameText(VersaoId[Indice], InstaladaId) then
    Texto := Texto + #13#10 + #13#10 + 'É a versão já instalada: será reinstalada.';
  NotasVersao.Text := Texto;
end;

procedure CliqueNaVersao(Sender: TObject);
begin
  MostrarNotasDaVersao(PaginaVersao.CheckListBox.ItemIndex);
end;

function GetVersaoEscolhida(Param: String): String;
begin
  Result := VersaoId[PaginaVersao.SelectedValueIndex];
end;

// Check dos arquivos de cada versão: só a escolhida é copiada.
function EscolhidaEh(Id: String): Boolean;
begin
  Result := SameText(GetVersaoEscolhida(''), Id);
end;

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
var
  I, Escolhida: Integer;
  Legenda: String;
begin
  CarregarVersoes;
  DetectarInstalacao;
  Log('Detecção: ' + DescricaoInstalada + ' [id=' + InstaladaId + '] Histórico: ' + IdsDisponiveis);

  // Página da versão: lista o histórico, marca o que já está instalado e
  // mostra as notas da versão em foco. Padrão: a mais nova (ou /VERSAO=).
  PaginaVersao := CreateInputOptionPage(wpWelcome,
    'Versão',
    'Qual versão da {#AppNome} instalar?',
    DescricaoInstalada + #13#10 + 'Para atualizar, mantenha a mais recente; escolha uma anterior para voltar a ela.',
    True, False);
  for I := 0 to GetArrayLength(VersaoId) - 1 do
  begin
    Legenda := VersaoId[I] + ' — ' + VersaoTitulo[I];
    if I = 0 then Legenda := Legenda + '  (mais recente)';
    if SameText(VersaoId[I], InstaladaId) then Legenda := Legenda + '  (instalada)';
    PaginaVersao.Add(Legenda);
  end;
  PaginaVersao.CheckListBox.Height := ScaleY(96);
  NotasVersao := TNewMemo.Create(PaginaVersao);
  NotasVersao.Parent := PaginaVersao.Surface;
  NotasVersao.Left := PaginaVersao.CheckListBox.Left;
  NotasVersao.Width := PaginaVersao.CheckListBox.Width;
  NotasVersao.Top := PaginaVersao.CheckListBox.Top + PaginaVersao.CheckListBox.Height + ScaleY(8);
  NotasVersao.Height := PaginaVersao.SurfaceHeight - NotasVersao.Top;
  NotasVersao.ReadOnly := True;
  NotasVersao.WordWrap := True;
  NotasVersao.ScrollBars := ssVertical;
  PaginaVersao.CheckListBox.OnClickCheck := @CliqueNaVersao;

  Escolhida := IndiceDaVersao(ExpandConstant('{param:Versao|}'));
  if Escolhida < 0 then Escolhida := 0;
  PaginaVersao.SelectedValueIndex := Escolhida;
  MostrarNotasDaVersao(Escolhida);

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

// Resumo na página "Pronto para instalar": versão e FIFA Friends.
function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
  Acao: String;
begin
  if InstaladaPasta = '' then
    Acao := 'instalar'
  else if SameText(GetVersaoEscolhida(''), InstaladaId) then
    Acao := 'reinstalar'
  else
    Acao := 'trocar a versão instalada (' + InstaladaId + ') por';
  Result := 'Versão da {#AppNome}:' + NewLine + Space + Acao + ' ' + GetVersaoEscolhida('') + NewLine + NewLine +
    MemoDirInfo + NewLine + NewLine +
    'FIFA Friends:' + NewLine + Space + GetFifaFriendsExe('') + NewLine + NewLine +
    MemoTasksInfo;
end;

// Cobre também a instalação silenciosa, que pula as páginas.
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if (ExpandConstant('{param:Versao|}') <> '') and (IndiceDaVersao(ExpandConstant('{param:Versao|}')) < 0) then
    Result := 'Versão "' + ExpandConstant('{param:Versao|}') + '" não existe neste instalador.' + #13#10 +
      'Use /VERSAO= com um destes ids: ' + IdsDisponiveis + '.'
  else if not FileExists(GetFifaFriendsExe('')) then
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
