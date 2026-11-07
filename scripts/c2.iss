; Instalador do C2 Desktop para Windows (Inno Setup 6).
;
; Uso:
;   iscc /DMyAppVersion=0.1.0 scripts\c2.iss
;
; O instalador:
;   - deixa escolher a pasta (DefaultDirName {commonpf}\C2);
;   - copia desktop (C2.exe), CLI (cli\c2.exe), MCP (cli\c2-mcp.cmd) e runtime;
;   - cria atalhos no Menu Iniciar e (opcional) na Area de Trabalho;
;   - adiciona <app>\cli ao PATH do SISTEMA (HKLM), com opcao de desmarcar;
;   - grava C2_HOME/C2_CLI no ambiente e remove no desinstalar;
;   - desinstalador padrao do Inno.

#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

#define MyAppName "C2"
#define MyAppPublisher "everctba"
#define MyAppURL "https://everctba.github.io/C2-Desktop/"
#define MyAppExeName "C2.exe"
#define SourceDir "..\dist\desktop"
#define MyIcon "..\apps\desktop\src-tauri\icons\icon.ico"

[Setup]
AppId={{B3E7C2A1-6F2E-4B7A-9D9C-3E1F4A5B6C7D}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={commonpf}\C2
DefaultGroupName={#MyAppName}
DisableDirPage=no
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist\installer
OutputBaseFilename=C2-Setup-{#MyAppVersion}
SetupIconFile={#MyIcon}
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
SetupLogging=yes
ChangesEnvironment=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[CustomMessages]
Integration=Integration:
AddToPath=Add C2 (CLI and MCP) to the system PATH
brazilianportuguese.Integration=Integração:
brazilianportuguese.AddToPath=Adicionar C2 (CLI e MCP) ao PATH do sistema

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "addpath"; Description: "{cm:AddToPath}"; GroupDescription: "{cm:Integration}"; Flags: checkedonce

[Files]
Source: "{#SourceDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\cli\c2.exe"; DestDir: "{app}\cli"; Flags: ignoreversion
Source: "{#SourceDir}\cli\c2-mcp.cmd"; DestDir: "{app}\cli"; Flags: ignoreversion
Source: "{#SourceDir}\runtime\*"; DestDir: "{app}\runtime"; Flags: recursesubdirs createallsubdirs ignoreversion
Source: "{#SourceDir}\LEIA-ME.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\CHECKSUMS.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\C2"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Desinstalar C2"; Filename: "{uninstallexe}"
Name: "{autodesktop}\C2"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Abrir o C2"; Flags: nowait postinstall skipifsilent

[Registry]
; Guarda os caminhos e dispara a notificacao de ambiente do Windows.
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "C2_HOME"; ValueData: "{app}"; Flags: uninsdeletevalue
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; ValueType: string; ValueName: "C2_CLI"; ValueData: "{app}\cli"; Flags: uninsdeletevalue

[Code]
const
  EnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  CliDir = '{app}\cli';

function HasSwitch(const Nome: string): Boolean;
var
  i: Integer;
begin
  Result := False;
  for i := 1 to ParamCount do
  begin
    if CompareText(ParamStr(i), Nome) = 0 then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

function PathHasEntry(const Path, Entrada: string): Boolean;
var
  P: Integer;
  Resto, Item: string;
begin
  Result := False;
  Resto := Path;
  while Resto <> '' do
  begin
    P := Pos(';', Resto);
    if P = 0 then
    begin
      Item := Resto;
      Resto := '';
    end
    else
    begin
      Item := Copy(Resto, 1, P - 1);
      Resto := Copy(Resto, P + 1, Length(Resto));
    end;
    Item := Trim(Item);
    if CompareText(Item, Entrada) = 0 then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

function RemovePathEntry(const Path, Entrada: string): string;
var
  P: Integer;
  Resto, Item, Resultado: string;
begin
  Resultado := '';
  Resto := Path;
  while Resto <> '' do
  begin
    P := Pos(';', Resto);
    if P = 0 then
    begin
      Item := Resto;
      Resto := '';
    end
    else
    begin
      Item := Copy(Resto, 1, P - 1);
      Resto := Copy(Resto, P + 1, Length(Resto));
    end;
    Item := Trim(Item);
    if (Item <> '') and (CompareText(Item, Entrada) <> 0) then
    begin
      if Resultado = '' then
        Resultado := Item
      else
        Resultado := Resultado + ';' + Item;
    end;
  end;
  Result := Resultado;
end;

procedure AddToSystemPath;
var
  Path, Entrada: string;
begin
  if HasSwitch('/NOPATH') then
    Exit;
  if not WizardIsTaskSelected('addpath') then
    Exit;

  Entrada := ExpandConstant(CliDir);
  if RegQueryStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', Path) then
  begin
    if PathHasEntry(Path, Entrada) then
      Exit;
    while (Length(Path) > 0) and (Path[Length(Path)] = ';') do
      Delete(Path, Length(Path), 1);
    if Path = '' then
      Path := Entrada
    else
      Path := Path + ';' + Entrada;
    RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', Path);
  end;
end;

procedure RemoveFromSystemPath;
var
  Path, NovoPath, Entrada: string;
begin
  Entrada := ExpandConstant(CliDir);
  if RegQueryStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', Path) then
  begin
    NovoPath := RemovePathEntry(Path, Entrada);
    if NovoPath <> Path then
      RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', NovoPath);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    AddToSystemPath;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromSystemPath;
end;
