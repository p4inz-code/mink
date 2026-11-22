#Requires -Version 5.1
<#
.SYNOPSIS
    Adds a folder to, or removes it from, your own (per-user) PATH.

.DESCRIPTION
    `mink setup` and the MINK installer use this so that typing `mink` works in any
    new terminal. It changes the PATH of your account only: it reads and writes
    HKCU\Environment, never touches the machine-wide PATH, and needs no administrator
    rights. Windows tells running programs about the change, but a terminal that is
    already open keeps the old PATH until you open a new one.

    The folder is added once, at the end. Entries that are already there, with
    different capitalisation or a trailing backslash, are recognised as the same
    folder. Removing takes out only that folder and leaves the rest of PATH, including
    any `%VARIABLE%` entries, exactly as it was.

.PARAMETER Directory
    The folder to add or remove, normally the one that holds mink.exe.

.PARAMETER Add
    Add the folder.

.PARAMETER Remove
    Remove the folder.

.PARAMETER Status
    Print `on_path=true` or `on_path=false` for the folder and change nothing.

.EXAMPLE
    PS> .\mink-path.ps1 -Add -Directory "$env:LOCALAPPDATA\Programs\MINK\bin"

.NOTES
    Exit code 0 on success, 1 on error.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Directory,
    [switch]$Add,
    [switch]$Remove,
    [switch]$Status
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (($Add -and $Remove) -or -not ($Add -or $Remove -or $Status)) {
    Write-Host 'ERROR: give exactly one of -Add, -Remove or -Status.' -ForegroundColor Red
    exit 1
}
if (-not $Directory.Trim()) {
    Write-Host 'ERROR: -Directory is empty.' -ForegroundColor Red
    exit 1
}

function ConvertTo-ComparableDirectory {
    param([string]$Text)
    # Compare without expanding %VARIABLES%, so an entry is never rewritten by accident.
    return $Text.Trim().Trim('"').TrimEnd('\', '/').Replace('/', '\').ToLowerInvariant()
}

$wanted = ConvertTo-ComparableDirectory -Text $Directory
$environmentKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
if (-not $environmentKey) {
    Write-Host 'ERROR: could not open HKCU:\Environment.' -ForegroundColor Red
    exit 1
}
try {
    # Read the raw value so %VARIABLES% entries survive a rewrite, and keep REG_EXPAND_SZ
    # when the PATH already uses it.
    $raw = [string]$environmentKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
    if ($environmentKey.GetValueNames() -contains 'Path') { $kind = $environmentKey.GetValueKind('Path') }
    $entries = @($raw -split ';' | Where-Object { $_ -ne '' })
    $present = $false
    foreach ($entry in $entries) {
        if ((ConvertTo-ComparableDirectory -Text $entry) -eq $wanted) { $present = $true }
    }

    if ($Status) {
        Write-Host ('on_path={0}' -f ([string]$present).ToLowerInvariant())
        exit 0
    }

    $notify = $false
    if ($Add) {
        if ($present) {
            Write-Host "$Directory is already on your PATH."
        }
        else {
            $updated = ($entries + $Directory.Trim().TrimEnd('\')) -join ';'
            $environmentKey.SetValue('Path', $updated, $kind)
            $notify = $true
            Write-Host "Added $Directory to your PATH."
        }
    }
    else {
        if (-not $present) {
            Write-Host "$Directory was not on your PATH."
        }
        else {
            $kept = @($entries | Where-Object { (ConvertTo-ComparableDirectory -Text $_) -ne $wanted })
            $environmentKey.SetValue('Path', ($kept -join ';'), $kind)
            $notify = $true
            Write-Host "Removed $Directory from your PATH."
        }
    }
}
finally { $environmentKey.Dispose() }

if ($notify) {
    # Tell running programs (Explorer, new terminals) that the environment changed.
    if (-not ('Mink.EnvironmentBroadcast' -as [type])) {
        Add-Type -Namespace Mink -Name EnvironmentBroadcast -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam,
    uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
    }
    $result = [UIntPtr]::Zero
    # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, 2 seconds.
    [void][Mink.EnvironmentBroadcast]::SendMessageTimeout([IntPtr]0xFFFF, 0x1A, [UIntPtr]::Zero,
        'Environment', 0x2, 2000, [ref]$result)
}
exit 0
