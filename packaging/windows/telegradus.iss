; Inno Setup 6 script for the Telegradus Windows installer.
;
; Build (from the repository root, after `cargo build --release -p telegradus`
; and copying telegradus.exe plus the TDLib DLLs into dist\windows):
;   iscc /DAppVersion=0.1.0 packaging\windows\telegradus.iss
; Relative paths below are resolved against this script's directory.

#define AppName "Telegradus"
#define AppExe "telegradus.exe"
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\windows"
#endif

[Setup]
; Never change AppId: it identifies the installation for upgrades and uninstall.
AppId={{FF4B7144-90A2-4307-9838-4DCABC87639C}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=ilovevodkaa
AppPublisherURL=https://github.com/ilovevodkaa/Telegradus
AppSupportURL=https://github.com/ilovevodkaa/Telegradus/issues
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
; Per-user install by default (no admin prompt); all-users is offered in a dialog.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\..\crates\telegradus\assets\icon\telegradus.ico
UninstallDisplayIcon={app}\{#AppExe}
OutputDir=..\..\dist
OutputBaseFilename=telegradus-{#AppVersion}-windows-x64-setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
