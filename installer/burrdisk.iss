; Inno Setup script for Burrdisk's Windows installer.
;
; Built by script\bundle.ps1, which passes the version and paths:
;   iscc /DAppVersion=0.2.0 /DSourceExe=...\burrdisk.exe /DOutputDir=...\dist installer\burrdisk.iss
;
; Installs per user by default (no administrator prompt), with an option to
; install for all users. Burrdisk updates itself by downloading the next
; release's installer and running it silently over the same install, so the
; installer's file name and AppId must stay as they are.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\burrdisk.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
; Never change AppId: Windows uses it to find an existing install to upgrade.
AppId={{902DCAC4-ABC7-4260-BE62-06348DD42161}
AppName=Burrdisk
AppVersion={#AppVersion}
AppVerName=Burrdisk {#AppVersion}
AppPublisher=BUNK
AppPublisherURL=https://github.com/bunkhq/burrdisk
AppSupportURL=https://github.com/bunkhq/burrdisk/issues
AppUpdatesURL=https://github.com/bunkhq/burrdisk/releases
DefaultDirName={autopf}\Burrdisk
DefaultGroupName=Burrdisk
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=Burrdisk-{#AppVersion}-windows-x64-setup
SetupIconFile=..\assets\windows\burrdisk.ico
UninstallDisplayIcon={app}\Burrdisk.exe
UninstallDisplayName=Burrdisk
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "Burrdisk.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Burrdisk"; Filename: "{app}\Burrdisk.exe"
Name: "{autodesktop}\Burrdisk"; Filename: "{app}\Burrdisk.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Burrdisk.exe"; Description: "{cm:LaunchProgram,Burrdisk}"; Flags: nowait postinstall skipifsilent
; Burrdisk's own updater runs this installer silently with /relaunch=1 and quits;
; this opens the updated app again.
Filename: "{app}\Burrdisk.exe"; Flags: nowait runasoriginaluser skipifnotsilent; Check: Relaunching

[Code]
function Relaunching: Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;
