#define AppName "Hliðskjálf"
#define AppVersion "0.1.0"
#define AppPublisher "Hliðskjálf"
#define RepoRoot AddBackslash(SourcePath) + "..\.."

[Setup]
AppId={{6A0B5C31-AC7B-4B83-982B-5D18C95D03A8}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\Hlidskjalf
DefaultGroupName=Hliðskjálf
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
OutputDir={#RepoRoot}\dist\windows
OutputBaseFilename=Hlidskjalf-Setup-x64
SetupIconFile={#RepoRoot}\crates\hlidskjalf-desktop\assets\hlidskjalf.ico
UninstallDisplayIcon={app}\hlidskjalf.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
VersionInfoVersion=0.1.0.0
VersionInfoProductName=Hliðskjálf
VersionInfoDescription=Hliðskjálf desktop test build

[Tasks]
Name: "desktopicon"; Description: "Criar atalho na área de trabalho"; GroupDescription: "Atalhos:"; Flags: unchecked

[Files]
Source: "{#RepoRoot}\target\release\hlidskjalf-desktop.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\target\release\hlidskjalf.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\target\release\hlidskjalfd.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\crates\hlidskjalf-desktop\assets\hlidskjalf.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\installers\windows\README.md"; DestDir: "{app}"; DestName: "README-WINDOWS.md"; Flags: ignoreversion
Source: "{#RepoRoot}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Hliðskjálf"; Filename: "{app}\hlidskjalf-desktop.exe"; IconFilename: "{app}\hlidskjalf.ico"
Name: "{autodesktop}\Hliðskjálf"; Filename: "{app}\hlidskjalf-desktop.exe"; IconFilename: "{app}\hlidskjalf.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\hlidskjalf-desktop.exe"; Description: "Abrir Hliðskjálf"; Flags: postinstall nowait skipifsilent
