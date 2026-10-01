#define MyAppName "REAL Video Enhancer"
#define MyAppVersion "2.4.1"
#define MyAppPublisher "TNTwise"
#define MyAppURL "https://github.com/TNTwise/REAL-Video-Enhancer"
#define MyAppExeName "REAL-Video-Enhancer.exe"
#ifexist "LICENSE"
  #define MyLicenseFile "LICENSE"
#elifexist "..\LICENSE"
  #define MyLicenseFile "..\LICENSE"
#endif

[Setup]
AppId={{8BEA9D2A-2D25-4D4B-9D96-12D6E149C2D6}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
#ifdef MyLicenseFile
LicenseFile={#MyLicenseFile}
#endif
OutputDir=dist
OutputBaseFilename=REAL-Video-Enhancer-{#MyAppVersion}-Windows-Installer
SetupIconFile=icons\logo-v3.ico
UninstallDisplayIcon={app}\logo-v3.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
UsePreviousTasks=yes
VersionInfoVersion=2.4.1.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} Setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "dist\REAL-Video-Enhancer\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion
Source: "icons\logo-v3.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon; WorkingDir: "{app}"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\REAL-Video-Enhancer"
