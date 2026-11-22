; MINK Setup.exe: a thin wrapper, built the same way as the Kinoz setup file.
;
; It never shows an Inno wizard. It unpacks the payload (MINK's own files and a manifest of SHA-256
; digests) to a temporary folder and starts `mink.exe installer` from there. That program draws the
; setup window, installs for the current user without administrator rights, and removes MINK later
; from Apps & features.
;
;   Build:  tools\build_installer.ps1   (creates dist\payload.zip first)
;
; Silent use: mink-Setup-<version>.exe /SILENT  (or --silent). Every other switch is handed on to
; `mink installer`, so  mink-Setup-1.0.5.exe --silent --dir C:\Tools\MINK --no-path  works.
; The setup file is not signed yet, so Windows SmartScreen may ask you to confirm it.

#ifndef AppVer
  #define AppVer "1.0.5"
#endif
#define AppName "MINK"
#ifndef PayloadZip
  #define PayloadZip "..\dist\payload.zip"
#endif

[Setup]
AppId={{6D1E0F7C-2B54-4C0E-9C58-3A7B1F9D6E21}
AppName={#AppName}
AppVersion={#AppVer}
AppPublisher=P4inz Interactive Labs
AppPublisherURL=https://github.com/p4inz-code/mink
AppCopyright=Copyright (C) 2026 Atharva Patil. Apache License 2.0.
VersionInfoVersion={#AppVer}
VersionInfoDescription=MINK Setup
CreateAppDir=no
Uninstallable=no
DisableStartupPrompt=yes
DisableProgramGroupPage=yes
DisableDirPage=yes
DisableReadyPage=yes
DisableWelcomePage=yes
DisableFinishedPage=yes
OutputDir=..\dist\installer
OutputBaseFilename=mink-Setup-{#AppVer}
SetupIconFile=..\npm\mink\windows\mink-app.ico
Compression=none
SolidCompression=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Files]
Source: "{#PayloadZip}"; DestDir: "{tmp}"; Flags: dontcopy

[Code]
// Switches that belong to Inno itself and must not be passed on.
function IsInnoSwitch(const Param: String): Boolean;
var
  Upper: String;
begin
  Upper := Uppercase(Param);
  Result := (Pos('/SILENT', Upper) = 1) or (Pos('/VERYSILENT', Upper) = 1) or
            (Pos('/SUPPRESSMSGBOXES', Upper) = 1) or (Pos('/SP-', Upper) = 1) or
            (Pos('/NOCANCEL', Upper) = 1) or (Pos('/LOG', Upper) = 1) or (Pos('/NORESTART', Upper) = 1);
end;

function InstallerArguments(var Silent: Boolean): String;
var
  I: Integer;
  Param: String;
begin
  Result := 'installer';
  Silent := WizardSilent;
  for I := 1 to ParamCount do
  begin
    Param := ParamStr(I);
    if CompareText(Param, '--silent') = 0 then
      Silent := True
    else if not IsInnoSwitch(Param) then
      Result := Result + ' "' + Param + '"';
  end;
  if Silent then
    Result := Result + ' --silent';
end;

function InitializeSetup(): Boolean;
var
  Rc: Integer;
  Dir, Zip, Args: String;
  Silent: Boolean;
begin
  Result := False;                         // False ends Setup without ever showing Inno's own screens
  ExtractTemporaryFile('payload.zip');
  Zip := ExpandConstant('{tmp}\payload.zip');
  Dir := ExpandConstant('{tmp}\mink-setup');
  ForceDirectories(Dir);
  if (not Exec(ExpandConstant('{sys}\tar.exe'), '-xf "' + Zip + '" -C "' + Dir + '"', '', SW_HIDE, ewWaitUntilTerminated, Rc)) or (Rc <> 0) then
  begin
    if not WizardSilent then
      MsgBox('MINK Setup could not unpack its files. Your PC was not changed.', mbError, MB_OK);
    Exit;
  end;
  Args := InstallerArguments(Silent);
  if not Exec(Dir + '\bin\mink.exe', Args, Dir, SW_SHOW, ewWaitUntilTerminated, Rc) then
  begin
    if not WizardSilent then
      MsgBox('MINK Setup could not start. Your PC was not changed.', mbError, MB_OK);
    Exit;
  end;
  // A silent run that succeeded lets Setup finish with exit code 0 (it has nothing left to do);
  // anything else ends with Setup's own failure code, which is also fine for a person who closed the window.
  Result := Silent and (Rc = 0);
end;
