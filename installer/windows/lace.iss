; Instalador do Lace para Windows (Inno Setup 6).
;
; O workflow de release compila dois assistentes deste script:
;
;   iscc /DStage=<estágio> /DLaceVersion=0.1.0 /DBundle=2026.09.29 /O<saída> installer\windows\lace.iss
;   iscc /DWeb /DStage=<estágio> /DLaceVersion=0.1.0 /DBundle=2026.09.29 /O<saída> installer\windows\lace.iss
;
; O estágio vem de `lace-pack inno`: bin\lace.exe, common\ e chunks\NN\ com
; os arquivos do bundle, e components.iss com as seções [Components] e
; [Files], geradas do índice do bundle (que arquivo cada componente usa; as
; bibliotecas que o Icarus, o Verilator, o Yosys e o Graphviz dividem vão
; com qualquer um deles).
;
; O primeiro assistente leva o bundle inteiro dentro dele. O segundo (/DWeb,
; com o web.iss que o `lace-pack inno --web` grava) leva só o lace.exe: ao
; clicar em Instalar, ele baixa da release o índice e os pedaços dos
; componentes marcados, cada um conferido pelo SHA-256, e o `lace setup` os
; extrai e troca o toolchain\ antes de o assistente copiar o lace.exe. A
; release vem do GitHub, ou de um espelho com /MIRROR=<url> (com uma pasta
; v<versão>\ por release, como o LACE_RELEASE_URL).
;
; Tipos de instalação: Recomendada (o padrão, sem lista de componentes) e
; Avançada (a lista, para escolher exatamente o que instalar). Reinstalar
; troca o bundle pelo da nova seleção; os componentes que a pasta já tem (do
; assistente ou do `lace install`) vêm marcados.

#ifndef Stage
  #error Pass /DStage=<lace-pack inno stage>
#endif
#ifndef LaceVersion
  #error Pass /DLaceVersion=<Lace version>
#endif
#ifndef Bundle
  #define Bundle "?"
#endif
; O AppId da lista de programas instalados. Um outro, só para testar o
; assistente sem mexer na instalação de verdade.
#ifndef AppGuid
  #define AppGuid "E34F1510-EC32-4839-8048-14CA03F9EFBB"
#endif
#ifndef ReleaseBase
  #define ReleaseBase "https://github.com/ART3121/lace/releases/download/v" + LaceVersion + "/"
#endif

; [Components], [Files], o #define Recommended (a lista da Recomendada) e
; SelectInstalledComponents.
#include AddBackslash(Stage) + "components.iss"
#ifdef Web
; AddDownloads e SelectedApps.
#include AddBackslash(Stage) + "web.iss"
#endif

[Setup]
AppId={{{#AppGuid}}
AppName=Lace
AppVersion={#LaceVersion}
AppVerName=Lace {#LaceVersion}
AppPublisher=NIPS-CERN, Faculdade de Engenharia da UFJF
AppPublisherURL=https://nipscern.com
AppComments=SAPHO tools: Lace Studio, YANC, Icarus Verilog, Verilator, cocotb, Yosys, Graphviz, surfer-aurora and openFPGALoader (bundle {#Bundle})
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
#ifdef Web
OutputBaseFilename=lace-{#LaceVersion}-windows-x64-web-setup
#else
OutputBaseFilename=lace-{#LaceVersion}-windows-x64-setup
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
#ifdef Web
WelcomeLabel2=This wizard installs Lace {#LaceVersion}, which orchestrates the SAPHO development tools, and Lace Studio, its graphical environment.%n%nIt downloads from the Lace release only the apps you choose, at the exact versions of bundle {#Bundle}: YANC, Icarus Verilog, Verilator, cocotb, Yosys, Graphviz, surfer-aurora and openFPGALoader. Each download is checked against its SHA-256. Verilator comes with the g++, make and Perl it uses, so there is no need to install MSYS2.%n%nWhen choosing components, the Recommended installation is the default; Advanced lets you choose exactly what to install.
#else
WelcomeLabel2=This wizard installs Lace {#LaceVersion}, which orchestrates the SAPHO development tools, and Lace Studio, its graphical environment.%n%nLace only uses the tools of the bundle installed with it, at the exact versions of bundle {#Bundle}: YANC, Icarus Verilog, Verilator, cocotb, Yosys, Graphviz, surfer-aurora and openFPGALoader. Verilator comes with the g++, make and Perl it uses, so there is no need to install MSYS2.%n%nWhen choosing components, the Recommended installation is the default; Advanced lets you choose exactly what to install.
#endif
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
#ifndef Web
; Reinstalar troca o bundle inteiro: o que foi desmarcado sai. O assistente
; que baixa os aplicativos troca o toolchain\ ele mesmo (o `lace setup`).
Type: filesandordirs; Name: "{app}\toolchain"
#endif
; As pastas .antigo-* e .instalando-* são sobras do lace update por
; componentes (o lace.exe que rodava a atualização fica numa .antigo-*).
Type: filesandordirs; Name: "{app}\.antigo-*"
Type: filesandordirs; Name: "{app}\.instalando-*"

[UninstallDelete]
Type: filesandordirs; Name: "{app}\toolchain"
Type: filesandordirs; Name: "{app}\.antigo-*"
Type: filesandordirs; Name: "{app}\.instalando-*"

[Code]
const
  UserEnvKey = 'Environment';
  SystemEnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  { O cliente do WebView2 no EdgeUpdate (ver HasWebView2). }
  WebView2Client = '\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  { A entrada do Lace na lista de programas instalados. }
  UninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{{#AppGuid}}_is1';

var
  Preselected: Boolean;
#ifdef Web
  DownloadPage: TDownloadWizardPage;
  ExtractPage: TOutputProgressWizardPage;
  { A última linha do `lace setup`, para a mensagem de erro. }
  SetupLine: String;
#endif

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

{ O Lace Studio usa o WebView2, que vem com o Windows 11 e com o Windows 10
  atualizado. A Microsoft manda conferir a versão ("pv") nestas chaves. }
function HasWebView2: Boolean;
var
  Version: String;
begin
  Result :=
    (RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node' + WebView2Client, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKLM, 'SOFTWARE' + WebView2Client, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKCU, 'Software' + WebView2Client, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'));
end;

{ Os componentes que a pasta escolhida já tem vêm marcados, uma vez: na
  página de componentes, ou antes de instalar numa instalação silenciosa
  sem /COMPONENTS. Um componente a mais que a Recomendada põe o tipo em
  Avançada, com a lista. }
procedure PreselectOnce;
begin
  if not Preselected then begin
    Preselected := True;
    SelectInstalledComponents(WizardDirValue);
  end;
end;

#ifdef Web
{ De onde vêm os arquivos da release: o GitHub, ou o espelho de /MIRROR. }
function ReleaseBase: String;
var
  Mirror: String;
begin
  Mirror := ExpandConstant('{param:MIRROR|}');
  if Mirror = '' then
    Result := '{#ReleaseBase}'
  else begin
    if Copy(Mirror, Length(Mirror), 1) <> '/' then
      Mirror := Mirror + '/';
    Result := Mirror + 'v{#LaceVersion}/';
  end;
end;

{ Baixa para a pasta temporária o índice e os pedaços dos componentes marcados,
  cada um conferido pelo SHA-256 que o lace-pack gravou no web.iss. }
function DownloadApps: Boolean;
begin
  DownloadPage.Clear;
  AddDownloads(DownloadPage, ReleaseBase);
  DownloadPage.Show;
  try
    try
      DownloadPage.Download;
      Result := True;
    except
      if DownloadPage.AbortedByUser then
        Log('The download was cancelled.')
      else
        SuppressibleMsgBox(AddPeriod(Format('Could not download %s: %s', [DownloadPage.LastBaseNameOrUrl, GetExceptionMessage])), mbCriticalError, MB_OK, IDOK);
      Result := False;
    end;
  finally
    DownloadPage.Hide;
  end;
end;

{ Cada linha do `lace setup`: o pedaço que ele está extraindo, "[2/5] Yosys",
  vira o texto e a barra da página de progresso. }
procedure OnSetupOutput(const S: String; const Error, FirstLine: Boolean);
var
  Line, Counter: String;
  Close, Slash, Done, Total: Integer;
begin
  Line := Trim(S);
  if Line = '' then
    exit;
  Log('lace setup: ' + Line);
  SetupLine := Line;
  Close := Pos(']', Line);
  if (Copy(Line, 1, 1) = '[') and (Close > 0) then begin
    Counter := Copy(Line, 2, Close - 2);
    Slash := Pos('/', Counter);
    if Slash > 0 then begin
      Done := StrToIntDef(Copy(Counter, 1, Slash - 1), 0);
      Total := StrToIntDef(Copy(Counter, Slash + 1, Length(Counter)), 0);
      if (Total > 0) and (Done > 0) then
        ExtractPage.SetProgress(Done - 1, Total);
    end;
  end;
  ExtractPage.SetText('Installing the apps', Line);
end;

{ Antes de copiar o lace.exe: o `lace setup` extrai os pedaços baixados,
  confere os executáveis e troca o toolchain\ da pasta. Se ele falha, o
  assistente para sem ter copiado nada. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Params, Apps: String;
  ResultCode: Integer;
begin
  Result := '';
  ExtractTemporaryFile('lace.exe');
  Params := 'setup --payload "' + ExpandConstant('{tmp}') + '" --prefix "' + WizardDirValue + '"';
  Apps := SelectedApps;
  if Apps <> '' then
    Params := Params + ' --components ' + Apps;
  ExtractPage.SetText('Installing the apps', '');
  ExtractPage.SetProgress(0, 1);
  ExtractPage.Show;
  try
    SetupLine := '';
    if not ExecAndLogOutput(ExpandConstant('{tmp}\lace.exe'), Params, ExpandConstant('{tmp}'), SW_HIDE, ewWaitUntilTerminated, ResultCode, @OnSetupOutput) then
      Result := 'Could not run the Lace that installs the apps: ' + SysErrorMessage(ResultCode)
    else if ResultCode <> 0 then
      Result := 'The apps could not be installed. ' + SetupLine;
  finally
    ExtractPage.Hide;
  end;
end;
#endif

procedure InitializeWizard;
var
  Previous: String;
begin
  if RegQueryStringValue(HKA, UninstallKey, 'DisplayVersion', Previous) and (Previous <> '') then
    WizardForm.WelcomeLabel2.Caption := WizardForm.WelcomeLabel2.Caption + #13#10#13#10 +
      Format('Lace %s is installed on this computer: this wizard replaces it with Lace %s, and the apps it has stay selected.', [Previous, '{#LaceVersion}']);
#ifdef Web
  DownloadPage := CreateDownloadPage('Downloading the apps', 'Lace {#LaceVersion}: the chosen apps, each checked against its SHA-256.', nil);
  DownloadPage.ShowBaseNameInsteadOfUrl := True;
  ExtractPage := CreateOutputProgressPage('Installing the apps', 'Extracting the downloaded apps into the installation folder.');
#endif
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpSelectComponents then
    PreselectOnce;
end;

{ Os avisos só aparecem na instalação com o assistente; na silenciosa
  (/VERYSILENT) um MsgBox esperaria um clique para sempre, então vão só para
  o log. }
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpSelectDir) and (Pos(' ', WizardDirValue) > 0) then begin
    Log('Warning: The folder path has a space, and the make used by Verilator does not accept spaces.');
    if not WizardSilent then
      Result := MsgBox('The selected folder has a space in its path, and the make used by Verilator does not accept spaces.' + #13#10#13#10 + 'Install there anyway?', mbConfirmation, MB_YESNO) = IDYES;
  end;
  if CurPageID = wpReady then begin
    if WizardSilent and (ExpandConstant('{param:COMPONENTS|}') = '') then
      PreselectOnce;
    { Antes de instalar, e não depois: sem o WebView2, o Studio não abre. }
    if WizardIsComponentSelected('studio') and not HasWebView2 then begin
      Log('Warning: The WebView2 Runtime, which Lace Studio uses, is not installed.');
      if not WizardSilent then
        Result := MsgBox('Lace Studio uses the Microsoft Edge WebView2 Runtime, which is not installed on this computer. Lace Studio will open only after you install it from https://developer.microsoft.com/microsoft-edge/webview2/ (Evergreen Runtime); the rest of Lace works without it.' + #13#10#13#10 + 'Install Lace now anyway?', mbConfirmation, MB_YESNO) = IDYES;
    end;
#ifdef Web
    if Result then
      Result := DownloadApps;
#endif
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('path') then
    AddToPath(ExpandConstant('{app}\bin'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}\bin'));
end;
