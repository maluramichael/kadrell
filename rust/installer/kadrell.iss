; Installer für die Rust-App unter Windows (Inno Setup 6). Bauen: rust/tools/installer-windows.sh
; Installiert pro Nutzer ohne Adminrechte nach %LOCALAPPDATA%\Programs\Kadrell, Startmenü- und Desktop-Verknüpfung.
; Deinstallieren lässt die Profile (%APPDATA%\Kadrell) liegen.

#ifndef AppVersion
  #error AppVersion fehlt: iscc /DAppVersion=x.y.z /DBinary=<kadrell.exe> kadrell.iss
#endif
#ifndef Binary
  #define Binary "..\target\release\kadrell.exe"
#endif

[Setup]
; AppId nie ändern: daran erkennt Windows Updates und die Deinstallation.
AppId={{F448F660-2D09-4B07-936F-C67434898EB8}
AppName=Kadrell
AppVersion={#AppVersion}
AppPublisher=Michael Malura
AppPublisherURL=https://kadrell.malura.de
DefaultDirName={autopf}\Kadrell
DefaultGroupName=Kadrell
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Läuft Kadrell beim Update, fragt Setup nach dem Schließen (Datei gesperrt). Nie still beenden: das nähme alle Claude-Sessions mit.
CloseApplications=yes
RestartApplications=no
SetupIconFile=..\crates\kadrell\windows\kadrell.ico
UninstallDisplayIcon={app}\kadrell.exe
UninstallDisplayName=Kadrell
WizardStyle=modern
OutputDir=..\target\installer
OutputBaseFilename=Kadrell-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes

[Languages]
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Binary}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; AppUserModelID wie kadrell-tray (TrayConfig::app_id), sonst zeigt die Taskleiste Verknüpfung und Fenster getrennt.
Name: "{autoprograms}\Kadrell"; Filename: "{app}\kadrell.exe"; AppUserModelID: "de.malura.kadrell.rust"
Name: "{autodesktop}\Kadrell"; Filename: "{app}\kadrell.exe"; AppUserModelID: "de.malura.kadrell.rust"; Tasks: desktopicon

[Run]
Filename: "{app}\kadrell.exe"; Description: "{cm:LaunchProgram,Kadrell}"; Flags: nowait postinstall skipifsilent
