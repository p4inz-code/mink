#Requires -Version 5.1
<#
.SYNOPSIS
    Clean-room validation of the `.mink` Windows file association.

.DESCRIPTION
    Builds a temporary "installed package" tree, registers the file type with
    npm\mink\windows\register-mink-filetype.ps1, and checks what Windows itself reports
    rather than what the registry files claim. Every association entry this test writes
    is restored to its previous state in a finally block, so running it on a developer
    machine does not leave the developer's own `.mink` association changed.

    Steps:
      1. snapshot the HKCU keys this test touches (recursively)
      2. build a temporary install tree whose path contains a space
      3. take-over safety: a `.mink` association owned by another application is not
         replaced without -Force, and -Force preserves that application's own entries
      4. uninstall after a takeover restores the previous owner
      5. from a clean extension, register through the package layout alone and check the
         file type, source-file icon and open behaviour as the shell reports them
      6. repeated registration is idempotent (same values, same subkeys, one session)
      7. with an editor bound, opening the file through the association reaches the
         editor and never compiles or runs the source
      8. re-registering without -Editor removes the stale open command
      9. the icon path round-trips from a Unicode installation directory
     10. uninstall from a clean extension removes every MINK-owned entry
     11. the machine is restored to its starting state

    Run with Windows PowerShell 5.1:

        powershell -ExecutionPolicy Bypass -File validation\windows_filetype\verify_filetype_association.ps1

    Exit code 0 when every check passes, 1 otherwise.
#>
[CmdletBinding()]
param(
    [string]$KitDir,
    [switch]$KeepWorkDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Extension = '.mink'
$ProgId = 'MINK.SourceFile'
$TypeName = 'MINK Source File'
$ContentType = 'text/x-mink'

$ExtensionSubKey = 'Software\Classes\.mink'
$ProgIdSubKey = 'Software\Classes\MINK.SourceFile'
$DefaultIconSubKey = 'Software\Classes\MINK.SourceFile\DefaultIcon'
$OpenCommandSubKey = 'Software\Classes\MINK.SourceFile\shell\open\command'
$FileExtsSubKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.mink'
$OpenWithSubKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.mink\OpenWithProgids'

if (-not $KitDir) {
    $KitDir = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'npm\mink\windows'
}
$KitDir = (Resolve-Path -LiteralPath $KitDir).Path
$registerScript = Join-Path $KitDir 'register-mink-filetype.ps1'
if (-not (Test-Path -LiteralPath $registerScript)) {
    Write-Host "ERROR: registration script not found: $registerScript" -ForegroundColor Red
    exit 1
}

# --- tiny assertion framework ----------------------------------------------
# Script-scoped so the counters survive the function boundaries below.
$script:passed = 0
$script:failed = 0

function Write-Step {
    param([string]$Title)
    Write-Host ''
    Write-Host "[$Title]"
}

function Assert-True {
    param([string]$What, [bool]$Condition, [string]$Detail = '')
    if ($Condition) {
        $script:passed++
        Write-Host ("  PASS  {0}" -f $What) -ForegroundColor Green
    }
    else {
        $script:failed++
        $suffix = if ($Detail) { " ($Detail)" } else { '' }
        Write-Host ("  FAIL  {0}{1}" -f $What, $suffix) -ForegroundColor Red
    }
}

function Assert-Equal {
    param([string]$What, $Expected, $Actual)
    $expectedText = if ($null -eq $Expected) { '<none>' } else { "$Expected" }
    $actualText = if ($null -eq $Actual) { '<none>' } else { "$Actual" }
    Assert-True -What $What -Condition ($expectedText -eq $actualText) `
        -Detail "expected '$expectedText', got '$actualText'"
}

function Write-Note {
    param([string]$Message)
    Write-Host ("  note  {0}" -f $Message) -ForegroundColor DarkGray
}

# --- registry helpers ------------------------------------------------------
function Get-CurrentUserKey {
    param([string]$SubKey, [switch]$Writable)
    return [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey, [bool]$Writable)
}

function Get-KeyValue {
    param([string]$SubKey, [string]$Name = '')
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return $null }
    try { return $key.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $key.Dispose() }
}

function Get-KeyValueNames {
    param([string]$SubKey)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return @() }
    try { return @($key.GetValueNames()) }
    finally { $key.Dispose() }
}

function Get-KeySubNames {
    param([string]$SubKey)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return @() }
    try { return @($key.GetSubKeyNames()) }
    finally { $key.Dispose() }
}

function Test-KeyExists {
    param([string]$SubKey)
    return ($null -ne (Get-CurrentUserKey -SubKey $SubKey))
}

function Remove-KeyIfPresent {
    param([string]$SubKey)
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($SubKey, $false)
}

function Set-KeyValue {
    param([string]$SubKey, [string]$Name, [string]$Value)
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($SubKey)
    try { $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String) }
    finally { $key.Dispose() }
}

# Snapshots are recursive (values and subkeys), so restoring a key puts the machine back
# exactly as it was even if the developer already had their own .mink association.
function Get-Snapshot {
    param([string]$SubKey)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return $null }
    try {
        $entries = New-Object System.Collections.Generic.List[object]
        foreach ($name in $key.GetValueNames()) {
            $entries.Add([pscustomobject]@{
                    Name  = $name
                    Value = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                    Kind  = $key.GetValueKind($name)
                })
        }
        $children = @{}
        foreach ($name in $key.GetSubKeyNames()) {
            $children[$name] = Get-Snapshot -SubKey (Join-Path $SubKey $name)
        }
        return [pscustomobject]@{ Values = $entries; Children = $children }
    }
    finally { $key.Dispose() }
}

function Restore-Snapshot {
    param([string]$SubKey, $Snapshot)
    $currentUser = [Microsoft.Win32.Registry]::CurrentUser
    $currentUser.DeleteSubKeyTree($SubKey, $false)
    if ($null -eq $Snapshot) { return }
    $key = $currentUser.CreateSubKey($SubKey)
    try {
        foreach ($entry in $Snapshot.Values) {
            $key.SetValue($entry.Name, $entry.Value, $entry.Kind)
        }
    }
    finally { $key.Dispose() }
    foreach ($name in $Snapshot.Children.Keys) {
        Restore-Snapshot -SubKey (Join-Path $SubKey $name) -Snapshot $Snapshot.Children[$name]
    }
}

function Test-Snapshot {
    param([string]$SubKey, $Snapshot)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return ($null -eq $Snapshot) }
    try {
        if ($null -eq $Snapshot) { return $false }
        foreach ($name in $key.GetValueNames()) {
            $expected = $Snapshot.Values | Where-Object { $_.Name -eq $name }
            if (-not $expected) { return $false }
            $current = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            if ("$current" -ne "$($expected.Value)") { return $false }
        }
        foreach ($name in $Snapshot.Values | ForEach-Object { $_.Name }) {
            if (-not ($key.GetValueNames() -contains $name)) { return $false }
        }
        if (@($key.GetSubKeyNames()).Count -ne @($Snapshot.Children.Keys).Count) { return $false }
        foreach ($name in $key.GetSubKeyNames()) {
            if (-not $Snapshot.Children.ContainsKey($name)) { return $false }
            if (-not (Test-Snapshot -SubKey (Join-Path $SubKey $name) -Snapshot $Snapshot.Children[$name])) {
                return $false
            }
        }
        return $true
    }
    finally { $key.Dispose() }
}

# --- shell helpers ---------------------------------------------------------
if (-not ('MinkFiletypeTest.ShellApi' -as [type])) {
    Add-Type -Namespace MinkFiletypeTest -Name ShellApi -MemberDefinition @'
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct SHFILEINFO {
    public System.IntPtr hIcon;
    public int iIcon;
    public uint dwAttributes;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szDisplayName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)] public string szTypeName;
}

[DllImport("shell32.dll", CharSet = CharSet.Unicode)]
public static extern System.IntPtr SHGetFileInfo(string pszPath, uint dwFileAttributes,
    ref SHFILEINFO psfi, uint cbFileInfo, uint uFlags);

[DllImport("user32.dll")]
public static extern bool DestroyIcon(System.IntPtr hIcon);

[DllImport("shlwapi.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
public static extern void AssocQueryString(int flags, int str, string assoc, string extra,
    System.Text.StringBuilder outBuffer, ref uint outBufferSize);
'@
}

$SHGFI_ICON = 0x000000100
$SHGFI_TYPENAME = 0x000000400
$ASSOCF_NONE = 0
$ASSOCSTR_COMMAND = 1
$ASSOCSTR_FRIENDLYDOCNAME = 3
$ASSOCSTR_DEFAULTICON = 15
$ASSOCSTR_PROGID = 20

function Get-ShellFileInfo {
    param([string]$Path, [uint32]$Flags)
    $info = New-Object MinkFiletypeTest.ShellApi+SHFILEINFO
    $size = [System.Runtime.InteropServices.Marshal]::SizeOf($info)
    [void][MinkFiletypeTest.ShellApi]::SHGetFileInfo($Path, 0, [ref]$info, $size, $Flags)
    return $info
}

function Get-AssocString {
    param([string]$Association, [int]$StringType)
    $bufferSize = [uint32]1024
    $buffer = New-Object System.Text.StringBuilder 1024
    try {
        [MinkFiletypeTest.ShellApi]::AssocQueryString($ASSOCF_NONE, $StringType, $Association,
            $null, $buffer, [ref]$bufferSize)
        return $buffer.ToString()
    }
    catch {
        return $null
    }
}

# --- invocation helpers ----------------------------------------------------
function Invoke-RegisterScript {
    param([string[]]$Arguments, [string]$ScriptPath)
    if (-not $ScriptPath) { $ScriptPath = $script:registerScript }
    $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @Arguments 2>&1
    return [pscustomobject]@{
        ExitCode = $LASTEXITCODE
        Output   = (@($output) -join "`n")
    }
}

function Show-Output {
    param([string]$Output)
    foreach ($line in ($Output -split "`n")) {
        if ($line.Trim()) { Write-Note $line.TrimEnd() }
    }
}

$work = Join-Path $env:TEMP ("mink-filetype-" + [guid]::NewGuid().ToString('N'))
$package = Join-Path $work 'package with space'
$packageBin = Join-Path $package 'bin'
$probeLog = Join-Path $work 'probe.log'
$snapshots = @{}
$result = 0

function Restore-MachineState {
    Restore-Snapshot -SubKey $ExtensionSubKey $snapshots[0]
    Restore-Snapshot -SubKey $ProgIdSubKey $snapshots[1]
    Restore-Snapshot -SubKey $FileExtsSubKey $snapshots[2]
}

try {
    Write-Host 'MINK .mink file association - clean-room validation'
    Write-Host '  registry : HKCU (per-user)'
    Write-Host "  work dir : $work"

    # ---------------------------------------------------------------- step 1
    Write-Step '1. snapshot'
    $snapshots[0] = Get-Snapshot -SubKey $ExtensionSubKey
    $snapshots[1] = Get-Snapshot -SubKey $ProgIdSubKey
    $snapshots[2] = Get-Snapshot -SubKey $FileExtsSubKey
    for ($index = 0; $index -lt 3; $index++) {
        $state = if ($null -eq $snapshots[$index]) { 'absent' } else { "$($snapshots[$index].Values.Count) value(s)" }
        Write-Note "snapshot ${index}: $state"
    }
    Assert-True -What 'previous association state captured' -Condition $true

    # ---------------------------------------------------------------- step 2
    Write-Step '2. temporary install tree (path contains a space)'
    New-Item -ItemType Directory -Path (Join-Path $package 'windows') -Force | Out-Null
    New-Item -ItemType Directory -Path $packageBin -Force | Out-Null
    Copy-Item -Path (Join-Path $KitDir '*') -Destination (Join-Path $package 'windows') -Force
    Copy-Item -Path (Join-Path $KitDir '*.ico') -Destination $packageBin
    # A stand-in for mink.exe: registration only notices that an installation exists, it
    # never runs the compiler.
    Set-Content -Path (Join-Path $packageBin 'mink.exe') -Value 'stand-in' -Encoding Ascii
    $installIcon = (Resolve-Path -LiteralPath (Join-Path $packageBin 'mink-file.ico')).Path
    $helloMink = Join-Path $package 'hello.mink'
    Set-Content -Path $helloMink -Value 'fn main() -> Int { return 0; }' -Encoding Ascii
    Assert-True -What 'temporary package tree created' -Condition (Test-Path -LiteralPath $helloMink)

    # ---------------------------------------------------------------- step 3
    Write-Step '3. an association owned by another application is not taken over'
    Remove-KeyIfPresent -SubKey $ExtensionSubKey
    Remove-KeyIfPresent -SubKey $ProgIdSubKey
    Set-KeyValue -SubKey $ExtensionSubKey -Name '' -Value 'Contoso.SourceFile'
    Set-KeyValue -SubKey $ExtensionSubKey -Name 'DecoyOwned' -Value 'keep'
    Set-KeyValue -SubKey "$ExtensionSubKey\OpenWithProgids" -Name 'Contoso.SourceFile' -Value ''

    $run = Invoke-RegisterScript -Arguments @('-InstallDir', $package)
    Assert-Equal -What 'refuses without -Force' -Expected 1 -Actual $run.ExitCode
    Assert-True -What 'refusal names the current owner' -Condition ($run.Output -match 'Contoso\.SourceFile')
    Assert-Equal -What 'refusal changes nothing' -Expected 'Contoso.SourceFile' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-True -What 'no MINK ProgID was created' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))

    $run = Invoke-RegisterScript -Arguments @('-InstallDir', $package, '-Force')
    Assert-Equal -What 'registration with -Force exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-Equal -What 'extension now points at the MINK ProgID' -Expected $ProgId `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-Equal -What "the other application's value survives" -Expected 'keep' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'DecoyOwned')
    Assert-True -What "the other application's subkey survives" `
        -Condition ((Get-KeySubNames -SubKey $ExtensionSubKey) -contains 'OpenWithProgids')
    Assert-Equal -What 'previous owner recorded for undo' -Expected 'Contoso.SourceFile' `
        -Actual (Get-KeyValue -SubKey $ProgIdSubKey -Name 'PreviousDotMinkOwner')
    Write-Note ('while another application''s class-level OpenWithProgids list exists, Windows treats')
    Write-Note ('.mink as "open with" and asks the user to confirm the application once.')

    # ---------------------------------------------------------------- step 4
    Write-Step '4. uninstall after a takeover'
    $run = Invoke-RegisterScript -Arguments @('-Uninstall')
    Assert-Equal -What 'uninstall exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-True -What 'MINK ProgID removed' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))
    Assert-Equal -What 'previous owner restored' -Expected 'Contoso.SourceFile' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-Equal -What 'MINK content type removed' -Expected $null `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'Content Type')
    Assert-Equal -What "the other application's value still present" -Expected 'keep' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'DecoyOwned')
    Assert-True -What "the other application's ProgID still present" `
        -Condition ((Get-KeyValueNames -SubKey "$ExtensionSubKey\OpenWithProgids") -contains 'Contoso.SourceFile')
    Assert-True -What 'extension key kept for the other application' `
        -Condition (Test-KeyExists -SubKey $ExtensionSubKey)

    # ---------------------------------------------------------------- step 5
    Write-Step '5. clean extension: file type, icon and open behaviour'
    Remove-KeyIfPresent -SubKey $ExtensionSubKey
    Remove-KeyIfPresent -SubKey $ProgIdSubKey
    # The copy inside the temporary package, run with no arguments: it must find
    # bin\mink.exe and bin\mink-file.ico relative to itself.
    $packageScript = Join-Path $package 'windows\register-mink-filetype.ps1'
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 'registration through the package layout alone exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }

    Assert-Equal -What 'extension points at the MINK ProgID' -Expected $ProgId `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-Equal -What 'content type' -Expected $ContentType `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'Content Type')
    Assert-Equal -What 'perceived type' -Expected 'text' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'PerceivedType')
    Assert-Equal -What 'display name' -Expected $TypeName `
        -Actual (Get-KeyValue -SubKey $ProgIdSubKey)
    Assert-Equal -What 'default icon is the designed .mink icon, quoted, index 0' `
        -Expected ('"{0}",0' -f $installIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-True -What 'the registered icon file exists' -Condition (Test-Path -LiteralPath $installIcon)
    Assert-True -What 'no open command: a .mink file is never executed' `
        -Condition (-not (Test-KeyExists -SubKey $OpenCommandSubKey))
    Assert-True -What 'MINK is offered in "Open with"' `
        -Condition ((Get-KeyValueNames -SubKey $OpenWithSubKey) -contains $ProgId)

    $typeInfo = Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_TYPENAME
    Assert-Equal -What 'Explorer type name' -Expected $TypeName -Actual $typeInfo.szTypeName
    $friendly = Get-AssocString -Association $Extension -StringType $ASSOCSTR_FRIENDLYDOCNAME
    Assert-Equal -What 'association friendly name' -Expected $TypeName -Actual $friendly
    $iconViaShell = Get-AssocString -Association $Extension -StringType $ASSOCSTR_DEFAULTICON
    Assert-Equal -What 'the shell icon comes from mink-file.ico' -Expected ('"{0}",0' -f $installIcon) `
        -Actual $iconViaShell
    $progIdViaShell = Get-AssocString -Association $Extension -StringType $ASSOCSTR_PROGID
    Assert-Equal -What 'association ProgID' -Expected $ProgId -Actual $progIdViaShell
    $iconInfo = Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_ICON
    Assert-True -What 'the shell resolves an icon for a .mink file' -Condition ($iconInfo.hIcon -ne [IntPtr]::Zero)
    if ($iconInfo.hIcon -ne [IntPtr]::Zero) {
        [void][MinkFiletypeTest.ShellApi]::DestroyIcon($iconInfo.hIcon)
    }
    $command = Get-AssocString -Association $Extension -StringType $ASSOCSTR_COMMAND
    Assert-True -What 'the resolved open command never runs mink.exe' `
        -Condition (-not $command -or $command -notmatch 'mink\.exe')
    Write-Note ("open command resolved by the shell: " + $(if ($command) { $command } else { '<none>' }))
    Write-Note ("file name as reported for the fixture: " + (Get-Item -LiteralPath $helloMink).Name)
    Write-Note 'cmd.exe assoc/ftype read machine-wide (HKLM) classes only, so they do not report a'
    Write-Note 'per-user registration; the shell APIs above are what Explorer actually uses.'

    # ---------------------------------------------------------------- step 6
    Write-Step '6. repeated registration is idempotent'
    $before = @{
        extension  = (Get-KeyValueNames -SubKey $ExtensionSubKey) -join ','
        progId     = (Get-KeyValueNames -SubKey $ProgIdSubKey) -join ','
        subkeys    = (Get-KeySubNames -SubKey $ProgIdSubKey) -join ','
        icon       = Get-KeyValue -SubKey $DefaultIconSubKey
        typeName   = (Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_TYPENAME).szTypeName
    }
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 'second registration exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-Equal -What 'no duplicated extension values' -Expected $before.extension `
        -Actual ((Get-KeyValueNames -SubKey $ExtensionSubKey) -join ',')
    Assert-Equal -What 'no duplicated ProgID values' -Expected $before.progId `
        -Actual ((Get-KeyValueNames -SubKey $ProgIdSubKey) -join ',')
    Assert-Equal -What 'no leftover ProgID subkeys' -Expected $before.subkeys `
        -Actual ((Get-KeySubNames -SubKey $ProgIdSubKey) -join ',')
    Assert-Equal -What 'icon unchanged' -Expected $before.icon -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-Equal -What 'Explorer type name unchanged' -Expected $before.typeName `
        -Actual (Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_TYPENAME).szTypeName
    # Two runs in one PowerShell session exercise the Add-Type guard in the script.
    $first = & $packageScript 2>&1 | Out-String
    $second = & $packageScript 2>&1 | Out-String
    Assert-True -What 'repeated runs in one session do not fail on the shell-refresh type' `
        -Condition ($first -notmatch 'already exists' -and $second -notmatch 'already exists')

    # ---------------------------------------------------------------- step 7
    Write-Step '7. open behaviour with an editor bound'
    $probeExe = Join-Path $package 'probe-editor.exe'
    Add-Type -TypeDefinition @'
using System;
using System.IO;
public static class MinkFiletypeProbe
{
    public static void Main(string[] args)
    {
        string log = Environment.GetEnvironmentVariable("MINK_PROBE_LOG");
        if (string.IsNullOrEmpty(log)) { return; }
        File.AppendAllText(log, "argv=" + string.Join("|", args)
            + Environment.NewLine + "cwd=" + Directory.GetCurrentDirectory() + Environment.NewLine);
    }
}
'@ -OutputAssembly $probeExe -OutputType ConsoleApplication
    Assert-True -What 'probe editor built' -Condition (Test-Path -LiteralPath $probeExe)

    $run = Invoke-RegisterScript -Arguments @('-Editor', $probeExe) -ScriptPath $packageScript
    Assert-Equal -What 'registration with -Editor exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-Equal -What 'open command is the editor applied to the file, quoted' `
        -Expected ('"{0}" "%1"' -f $probeExe) -Actual (Get-KeyValue -SubKey $OpenCommandSubKey)

    if (Test-Path -LiteralPath $probeLog) { Remove-Item -LiteralPath $probeLog -Force }
    $env:MINK_PROBE_LOG = $probeLog
    Start-Process -FilePath $helloMink
    $deadline = (Get-Date).AddSeconds(15)
    while (-not (Test-Path -LiteralPath $probeLog) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 200
    }
    $logText = if (Test-Path -LiteralPath $probeLog) { (Get-Content -LiteralPath $probeLog -Raw) } else { '' }
    Assert-True -What 'opening the file through the association reached the editor' `
        -Condition ($logText -match [regex]::Escape('hello.mink'))
    Write-Note ("probe log: " + ($logText -replace "`r?`n", ' ').Trim())
    Assert-True -What 'the source file was not compiled or executed' `
        -Condition (-not (Test-Path -LiteralPath (Join-Path $package 'hello.exe')) -and
        -not (Test-Path -LiteralPath (Join-Path $work 'hello.exe')))

    # ---------------------------------------------------------------- step 8
    Write-Step '8. rebinding without -Editor removes the stale command'
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 're-registration exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-True -What 'open command removed again' -Condition (-not (Test-KeyExists -SubKey $OpenCommandSubKey))
    Assert-Equal -What 'ProgID keeps only its own subkeys' -Expected 'DefaultIcon' `
        -Actual ((Get-KeySubNames -SubKey $ProgIdSubKey) -join ',')

    # ---------------------------------------------------------------- step 9
    Write-Step '9. unicode installation path (and a path with a space)'
    # The whole package tree is copied under a Unicode directory; the script is run with no
    # arguments, so it must resolve mink.exe and mink-file.ico from its own location. The
    # path is never passed on a command line, which also keeps the test independent of the
    # console code page.
    # The name is built from code points so this file stays pure ASCII: Windows
    # PowerShell 5.1 reads a BOM-less .ps1 in the ANSI code page, which would corrupt
    # a non-ASCII literal in the source.
    $unicodeName = 'mink-' + [string][char]0x0442 + [string][char]0x0435 + [string][char]0x0441 +
        [string][char]0x0442 + '-' + [string][char]0x65E5 + [string][char]0x672C + [string][char]0x8A9E
    $unicodeRoot = Join-Path $work $unicodeName
    New-Item -ItemType Directory -Path $unicodeRoot -Force | Out-Null
    Copy-Item -Path $package -Destination $unicodeRoot -Recurse -Force
    $unicodeBin = Join-Path $unicodeRoot (Join-Path (Split-Path -Leaf $package) 'bin')
    $unicodeIcon = (Resolve-Path -LiteralPath (Join-Path $unicodeBin 'mink-file.ico')).Path
    $unicodeScript = Join-Path $unicodeRoot (Join-Path (Split-Path -Leaf $package) 'windows\register-mink-filetype.ps1')
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $unicodeScript
    Assert-Equal -What 'registration from a unicode path exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-Equal -What 'unicode icon path round-trips through the registry' `
        -Expected ('"{0}",0' -f $unicodeIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-Equal -What 'the shell reports the unicode icon path' `
        -Expected ('"{0}",0' -f $unicodeIcon) `
        -Actual (Get-AssocString -Association $Extension -StringType $ASSOCSTR_DEFAULTICON)

    # ---------------------------------------------------------------- step 10
    Write-Step '10. uninstall from a clean extension removes everything MINK owns'
    $run = Invoke-RegisterScript -Arguments @('-Uninstall')
    Assert-Equal -What 'uninstall exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-True -What 'no MINK ProgID left behind' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))
    Assert-True -What 'no empty .mink key left behind' -Condition (-not (Test-KeyExists -SubKey $ExtensionSubKey))
    Assert-True -What 'MINK removed from "Open with"' `
        -Condition (-not ((Get-KeyValueNames -SubKey $OpenWithSubKey) -contains $ProgId))

    # ---------------------------------------------------------------- step 11
    Write-Step '11. previous machine state restored'
    Restore-MachineState
    $subKeys = @($ExtensionSubKey, $ProgIdSubKey, $FileExtsSubKey)
    $restored = $true
    for ($index = 0; $index -lt 3; $index++) {
        if (-not (Test-Snapshot -SubKey $subKeys[$index] -Snapshot $snapshots[$index])) {
            $restored = $false
            Write-Note "not restored: $($subKeys[$index])"
        }
    }
    Assert-True -What 'the registry is back to its starting state' -Condition $restored
}
finally {
    Restore-MachineState
    if (-not $KeepWorkDirectory -and (Test-Path -LiteralPath $work)) {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host ''
if ($script:failed -gt 0) {
    $result = 1
    Write-Host ("RESULT: FAIL - {0} passed, {1} failed" -f $script:passed, $script:failed) -ForegroundColor Red
}
else {
    Write-Host ("RESULT: PASS - {0} checks" -f $script:passed) -ForegroundColor Green
}
exit $result
