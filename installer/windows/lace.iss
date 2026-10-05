; Instalador do Lace para Windows (Inno Setup 6).
;
; O workflow de release compila assim:
;
;   iscc /DStage=<estágio> /DLaceVersion=0.1.0 /DBundle=2026.09.29 /O<saída> installer\windows\lace.iss
;
; O estágio vem de `lace-pack inno`: bin\lace.exe, common\ e chunks\NN\ com
; os arquivos do bundle, e components.iss com as seções [Components] e
; [Files], geradas do índice do bundle (que arquivo cada componente usa; as
; bibliotecas que o Icarus, o Verilator, o Yosys e o Graphviz dividem vão
; com qualquer um deles).
;
; Tipos de instalação: Recomendada (o padrão, sem lista de componentes) e
; Avançada (a lista, para escolher exatamente o que instalar). Reinstalar
; troca o bundle inteiro pelo da nova seleção.

#ifndef Stage
  #error Pass /DStage=<lace-pack inno stage>
#endif
#ifndef LaceVersion
  #error Pass /DLaceVersion=<Lace version>
#endif
#ifndef Bundle
  #define Bundle "?"
#endif

; [Components], [Files] e o #define Recommended (a lista da Recomendada).
#include AddBackslash(Stage) + "components.iss"

[Setup]
AppId={{E34F1510-EC32-4839-8048-14CA03F9EFBB}
AppName=Lace
AppVersion={#LaceVersion}
AppVerName=Lace {#LaceVersion}
AppPublisher=NIPS-CERN, Faculdade de Engenharia da UFJF
AppPublisherURL=https://nipscern.com
AppComments=SAPHO tools: Lace Studio, YANC, Icarus Verilog, Verilator, cocotb, Yosys, Graphviz and surfer-aurora (bundle {#Bundle})
VersionInfoVersion={#LaceVersion}
DefaultDirName={code:DefaultDir}
DisableProgramGroupPage=yes
DisableWelcomePage=no
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
AlwaysShowComponentsList=no
ChangesEnvironment=yes
Compression=lzma2/ultra64
SolidCompression=yes
LZMAUseSeparateProcess=yes
UninstallDisplayName=Lace {#LaceVersion}
OutputBaseFilename=lace-{#LaceVersion}-windows-x64-setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
WelcomeLabel2=This wizard installs Lace {#LaceVersion}, which orchestrates the SAPHO development tools, and Lace Studio, its graphical environment.%n%nLace only uses the tools of the bundle installed with it, at the exact versions of bundle {#Bundle}: YANC, Icarus Verilog, Verilator, cocotb, Yosys, Graphviz and surfer-aurora. Verilator comes with the g++, make and Perl it uses, so there is no need to install MSYS2.%n%nWhen choosing components, the Recommended installation is the default; Advanced lets you choose exactly what to install.
SelectComponentsLabel2=The Recommended installation installs Lace with {#Recommended}.%n%nTo choose the components yourself, switch to Advanced.
FinishedLabel=Lace has been installed. Open Lace Studio from the Start menu, or open a new terminal and run: lace tools --verify

[Types]
Name: "recomendada"; Description: "Recommended"
Name: "avancada"; Description: "Advanced (choose the components)"; Flags: iscustom

[Tasks]
Name: "path"; Description: "Add Lace to PATH, to use ""lace"" in any terminal"
Name: "studioicon"; Description: "Create a desktop shortcut for Lace Studio"; Components: studio; Flags: unchecked

; O Lace Studio fica no bundle (toolchain\studio, o componente studio) e acha
; o bundle por estar dentro dele.
[Icons]
Name: "{autoprograms}\Lace Studio"; Filename: "{app}\toolchain\studio\lace-studio.exe"; Components: studio
Name: "{autodesktop}\Lace Studio"; Filename: "{app}\toolchain\studio\lace-studio.exe"; Components: studio; Tasks: studioicon

[Run]
Filename: "{app}\toolchain\studio\lace-studio.exe"; Description: "Open Lace Studio"; Components: studio; Flags: postinstall nowait skipifsilent unchecked

[InstallDelete]
; Reinstalar troca o bundle inteiro: o que foi desmarcado sai.
Type: filesandordirs; Name: "{app}\toolchain"

[UninstallDelete]
Type: filesandordirs; Name: "{app}\toolchain"

[Code]
const
  UserEnvKey = 'Environment';
  SystemEnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  { O cliente do WebView2 no EdgeUpdate (ver HasWebView2). }
  WebView2Client = '\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

{ Pasta padrão sem espaço no caminho: o make do Verilator não aceita
  espaços, e "Program Files" tem um. }
function DefaultDir(Param: String): String;
begin
  if IsAdminInstallMode then
    Result := ExpandConstant('{sd}\Lace')
  else
    Result := ExpandConstant('{localappdata}\Programs\Lace');
end;

function EnvRoot: Integer;
begin
  if IsAdminInstallMode then
    Result := HKEY_LOCAL_MACHINE
  else
    Result := HKEY_CURRENT_USER;
end;

function EnvKey: String;
begin
  if IsAdminInstallMode then
    Result := SystemEnvKey
  else
    Result := UserEnvKey;
end;

function PathHas(Paths, Dir: String): Boolean;
begin
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') > 0;
end;

procedure AddToPath(Dir: String);
var
  Paths: String;
begin
  if not RegQueryStringValue(EnvRoot, EnvKey, 'Path', Paths) then
    Paths := '';
  if PathHas(Paths, Dir) then
    exit;
  if (Paths <> '') and (Paths[Length(Paths)] <> ';') then
    Paths := Paths + ';';
  RegWriteExpandStringValue(EnvRoot, EnvKey, 'Path', Paths + Dir);
end;

procedure RemoveFromPath(Dir: String);
var
  Paths: String;
  P: Integer;
begin
  if not RegQueryStringValue(EnvRoot, EnvKey, 'Path', Paths) then
    exit;
  Paths := ';' + Paths + ';';
  P := Pos(';' + Uppercase(Dir) + ';', Uppercase(Paths));
  if P = 0 then
    exit;
  Delete(Paths, P, Length(Dir) + 1);
  Paths := Copy(Paths, 2, Length(Paths) - 2);
  if Paths = '' then
    RegDeleteValue(EnvRoot, EnvKey, 'Path')
  else
    RegWriteExpandStringValue(EnvRoot, EnvKey, 'Path', Paths);
end;

{ O aviso só aparece na instalação com o assistente; na silenciosa
  (/VERYSILENT) um MsgBox esperaria um clique para sempre, então vai só
  para o log. }
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpSelectDir) and (Pos(' ', WizardDirValue) > 0) then begin
    Log('Warning: The folder path has a space, and the make used by Verilator does not accept spaces.');
    if not WizardSilent then
      Result := MsgBox('The selected folder has a space in its path, and the make used by Verilator does not accept spaces.' + #13#10#13#10 + 'Install there anyway?', mbConfirmation, MB_YESNO) = IDYES;
  end;
end;

{ O Lace Studio usa o WebView2, que vem com o Windows 11 e com o Windows 10
  atualizado. A Microsoft manda conferir a versao ("pv") nestas chaves. }
function HasWebView2: Boolean;
var
  Version: String;
begin
  Result :=
    (RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node' + WebView2Client, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKLM, 'SOFTWARE' + WebView2Client, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKCU, 'Software' + WebView2Client, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('path') then
    AddToPath(ExpandConstant('{app}\bin'));
  if (CurStep = ssPostInstall) and WizardIsComponentSelected('studio') and not HasWebView2 then begin
    Log('Warning: The WebView2 Runtime, which Lace Studio uses, is not installed.');
    if not WizardSilent then
      MsgBox('Lace Studio uses the Microsoft Edge WebView2 Runtime, which is not installed on this computer.' + #13#10#13#10 + 'Install it from https://developer.microsoft.com/microsoft-edge/webview2/ (Evergreen Runtime) before opening Lace Studio.', mbInformation, MB_OK);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}\bin'));
end;
