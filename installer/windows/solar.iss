; Instalador do Solar para Windows (Inno Setup 6).
;
; O workflow de release compila assim:
;
;   iscc /DStage=<estágio> /DSolarVersion=0.1.0 /DBundle=2026.09.29 /O<saída> installer\windows\solar.iss
;
; O estágio vem de `solar-pack inno`: bin\solar.exe, common\ e chunks\NN\ com
; os arquivos do bundle, e components.iss com as seções [Components] e
; [Files], geradas do índice do bundle (que arquivo cada componente usa; as
; bibliotecas que o Icarus, o Verilator, o Yosys e o Graphviz dividem vão
; com qualquer um deles).
;
; Tipos de instalação: Recomendada (o padrão, sem lista de componentes) e
; Avançada (a lista, para escolher exatamente o que instalar). Reinstalar
; troca o bundle inteiro pelo da nova seleção.

#ifndef Stage
  #error Passe /DStage=<estágio do solar-pack inno>
#endif
#ifndef SolarVersion
  #error Passe /DSolarVersion=<versão do Solar>
#endif
#ifndef Bundle
  #define Bundle "?"
#endif

; [Components], [Files] e o #define Recommended (a lista da Recomendada).
#include AddBackslash(Stage) + "components.iss"

[Setup]
AppId={{012C5865-1BB4-4F20-AC8D-420240425E25}
AppName=Solar
AppVersion={#SolarVersion}
AppVerName=Solar {#SolarVersion}
AppPublisher=NIPS-CERN, Faculdade de Engenharia da UFJF
AppPublisherURL=https://nipscern.com
AppComments=Ferramentas do SAPHO: YANC, Icarus Verilog, Verilator, Yosys, Graphviz e surfer-aurora (bundle {#Bundle})
VersionInfoVersion={#SolarVersion}
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
UninstallDisplayName=Solar {#SolarVersion}
OutputBaseFilename=solar-{#SolarVersion}-windows-x64-setup

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Messages]
WelcomeLabel2=Este assistente instala o Solar {#SolarVersion}, que orquestra as ferramentas de desenvolvimento do SAPHO.%n%nO Solar só usa as ferramentas do bundle instalado com ele, nas versões exatas do bundle {#Bundle}: YANC, Icarus Verilog, Verilator, Yosys, Graphviz e surfer-aurora. A exceção é o Verilator, que compila com o g++, o make e o Perl do MSYS2.%n%nNa escolha dos componentes, a instalação Recomendada é o padrão; a Avançada deixa escolher exatamente o que instalar.
SelectComponentsLabel2=A instalação Recomendada instala o Solar com {#Recommended}.%n%nPara escolher exatamente os componentes, troque para Avançada.
FinishedLabel=O Solar foi instalado. Abra um terminal novo e rode: solar tools --verify

[Types]
Name: "recomendada"; Description: "Recomendada"
Name: "avancada"; Description: "Avançada (escolher os componentes)"; Flags: iscustom

[Tasks]
Name: "path"; Description: "Adicionar o Solar ao PATH, para usar ""solar"" em qualquer terminal"

[InstallDelete]
; Reinstalar troca o bundle inteiro: o que foi desmarcado sai.
Type: filesandordirs; Name: "{app}\toolchain"

[UninstallDelete]
Type: filesandordirs; Name: "{app}\toolchain"

[Code]
const
  UserEnvKey = 'Environment';
  SystemEnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';

{ Pasta padrão sem espaço no caminho: o make do Verilator não aceita
  espaços, e "Program Files" tem um. }
function DefaultDir(Param: String): String;
begin
  if IsAdminInstallMode then
    Result := ExpandConstant('{sd}\Solar')
  else
    Result := ExpandConstant('{localappdata}\Programs\Solar');
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

function MsysFound: Boolean;
begin
  Result := FileExists('C:\msys64\usr\bin\perl.exe') or FileExists('C:\tools\msys64\usr\bin\perl.exe');
end;

{ Os avisos só aparecem na instalação com o assistente; na silenciosa
  (/VERYSILENT) um MsgBox esperaria um clique para sempre, então vão só
  para o log. }
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpSelectDir) and (Pos(' ', WizardDirValue) > 0) then begin
    Log('Aviso: a pasta tem espaço no caminho; o make do Verilator não aceita espaços.');
    if not WizardSilent then
      Result := MsgBox('A pasta escolhida tem espaço no caminho, e o make usado pelo Verilator não aceita espaços.' + #13#10#13#10 + 'Instalar nela assim mesmo?', mbConfirmation, MB_YESNO) = IDYES;
  end;
  if (CurPageID = wpSelectComponents) and WizardIsComponentSelected('verilator') and not MsysFound then begin
    Log('Aviso: Verilator escolhido e MSYS2 não encontrado em C:\msys64.');
    if not WizardSilent then
      MsgBox('O Verilator compila a simulação com o g++, o make e o Perl do MSYS2, e o MSYS2 não foi encontrado em C:\msys64.' + #13#10#13#10 + 'O Verilator será instalado, mas só simula depois que o MSYS2 for instalado (ou declarado com "solar config set-compiler").', mbInformation, MB_OK);
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
