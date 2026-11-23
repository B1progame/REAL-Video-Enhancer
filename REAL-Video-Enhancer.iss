#define MyAppName "REAL Video Enhancer"
#ifndef MyAppVersion
#define MyAppVersion "2.4.48"
#endif
#define MyAppPublisher "B1progame"
#define MyAppURL "https://github.com/B1progame/REAL-Video-Enhancer"
#define MyAppExeName "REAL-Video-Enhancer.exe"
#ifndef MyReleaseFolder
  #define MyReleaseFolder "release-stage"
#endif
#ifexist "LICENSE"
  #define MyLicenseFile "LICENSE"
#else
  #ifexist "..\LICENSE"
    #define MyLicenseFile "..\LICENSE"
  #endif
#endif

[Setup]
AppId={{F16B8DD5-5F26-47BD-88CE-43DA8D9BCE24}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
#ifdef MyLicenseFile
LicenseFile={#MyLicenseFile}
#endif
OutputDir={#MyReleaseFolder}\installer
OutputBaseFilename=REAL-Video-Enhancer-{#MyAppVersion}-Windows-Installer
SetupIconFile=icons\logo-v3.ico
UninstallDisplayIcon={app}\logo-v3.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
UsePreviousTasks=yes
VersionInfoVersion={#MyAppVersion}.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} Setup
CloseApplications=yes
RestartApplications=no
CloseApplicationsFilter=REAL-Video-Enhancer.exe,RVE-Engine.exe

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#MyReleaseFolder}\REAL-Video-Enhancer\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion
Source: "icons\logo-v3.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\logo-v3.ico"; IconIndex: 0; AppUserModelID: "B1progame.REALVideoEnhancer"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon; WorkingDir: "{app}"; IconFilename: "{app}\logo-v3.ico"; IconIndex: 0; AppUserModelID: "B1progame.REALVideoEnhancer"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch REAL Video Enhancer"; Flags: nowait

[Code]
var
  RemoveAppDataOnUninstall: Boolean;

function InitializeSetup(): Boolean;
var
  SetupPath, UpdateFolder, Params: String;
  ResultCode: Integer;
begin
  Result := True;
  SetupPath := ExpandConstant('{srcexe}');
  UpdateFolder := AddBackslash(ExpandConstant('{localappdata}\REAL-Video-Enhancer\updates'));

  { Older app versions start the verified download without silent flags. Detect
    that updater cache path, then relaunch after this setup process exits. }
  if (not WizardSilent) and
     (CompareText(AddBackslash(ExtractFileDir(SetupPath)), UpdateFolder) = 0) then
  begin
    Params := '/c ping 127.0.0.1 -n 2 >nul & start "" "' + SetupPath +
      '" /VERYSILENT /NORESTART /CLOSEAPPLICATIONS /SP-';
    if Exec(ExpandConstant('{sys}\cmd.exe'), Params, UpdateFolder,
      SW_HIDE, ewNoWait, ResultCode) then
      Result := False;
  end;
end;

function InitializeUninstall(): Boolean;
begin
  Result := True;
  RemoveAppDataOnUninstall := MsgBox(
    'Also remove downloaded models, the managed Python runtime, settings, and logs? ' +
    'This removes REAL Video Enhancer data stored on this PC and in WSL Ubuntu.',
    mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if (CurUninstallStep = usPostUninstall) and RemoveAppDataOnUninstall then
  begin
    DelTree(ExpandConstant('{localappdata}\REAL-Video-Enhancer'), True, True, True);
    Exec(ExpandConstant('{sys}\wsl.exe'),
      '--distribution Ubuntai --exec bash -lc "rm -rf -- $HOME/.local/share/REAL-Video-Enhancer"',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  end;
end;
