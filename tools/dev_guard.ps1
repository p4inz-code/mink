<#
.SYNOPSIS
  Runs a build or test command with a hard cap on how hard it pushes the machine.

.DESCRIPTION
  MINK builds and test runs can saturate every core for minutes. This wrapper
  keeps the machine responsive and cool:

    * the command runs at BelowNormal priority;
    * Cargo and the test harness are limited to -Jobs worker threads
      (default 6, so a 32-thread CPU stays under about a fifth busy from
      the build itself);
    * total CPU load is sampled every few seconds, and when it stays above
      -MaxCpu percent for two samples in a row the command's process tree is
      suspended until the load falls back under the limit;
    * the highest load seen and the number of pauses are printed at the end.

  Nothing here reads temperatures: Windows does not expose them without
  administrator rights. The cap is on CPU load, which is what drives heat.

.EXAMPLE
  powershell -File tools\dev_guard.ps1 -Run "cargo test --no-fail-fast"
#>
param(
    [int]$Jobs = 6,
    [int]$MaxCpu = 70,
    [int]$SampleSeconds = 2,
    [Parameter(Mandatory = $true)]
    [string]$Run
)

$ErrorActionPreference = 'Stop'

$env:CARGO_BUILD_JOBS = "$Jobs"
$env:RUST_TEST_THREADS = "$Jobs"

Add-Type -Namespace MinkGuard -Name Native -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("ntdll.dll")] public static extern int NtSuspendProcess(System.IntPtr h);
[System.Runtime.InteropServices.DllImport("ntdll.dll")] public static extern int NtResumeProcess(System.IntPtr h);
'@

function Get-Tree([int]$RootId) {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    $ids = [System.Collections.Generic.List[int]]::new()
    $ids.Add($RootId)
    for ($i = 0; $i -lt $ids.Count; $i++) {
        foreach ($p in $all) { if ($p.ParentProcessId -eq $ids[$i] -and -not $ids.Contains($p.ProcessId)) { $ids.Add($p.ProcessId) } }
    }
    $ids
}

function Set-Tree([int]$RootId, [bool]$Suspend) {
    foreach ($id in (Get-Tree $RootId)) {
        $proc = Get-Process -Id $id -ErrorAction SilentlyContinue
        if (-not $proc) { continue }
        if ($Suspend) { [void][MinkGuard.Native]::NtSuspendProcess($proc.Handle) }
        else { [void][MinkGuard.Native]::NtResumeProcess($proc.Handle) }
    }
}

$exe = "cmd.exe"
$arguments = @("/c", $Run)
$proc = Start-Process -FilePath $exe -ArgumentList $arguments -NoNewWindow -PassThru
$null = $proc.Handle  # keeps the handle so ExitCode is readable
try { $proc.PriorityClass = "BelowNormal" } catch { }

$peak = 0; $pauses = 0; $high = 0; $suspended = $false
while (-not $proc.HasExited) {
    Start-Sleep -Seconds $SampleSeconds
    # The performance counter is cheap; querying WMI for load would itself spike the CPU.
    # A counter read can fail transiently (the performance library reports
    # 0x800007D6 now and then); a missed sample counts as 0 and the next one
    # recovers.
    $cpu = 0
    try {
        $sample = Get-Counter '\Processor(_Total)\% Processor Time' -ErrorAction Stop
        $cpu = [int]$sample.CounterSamples[0].CookedValue
    } catch { }
    if ($cpu -gt $peak) { $peak = $cpu }
    if ($cpu -gt $MaxCpu) { $high++ } else { $high = 0 }
    if (-not $suspended -and $high -ge 2) {
        Set-Tree $proc.Id $true; $suspended = $true; $pauses++
        Write-Host "dev_guard: CPU $cpu% above $MaxCpu%, paused the command"
    } elseif ($suspended -and $cpu -le ($MaxCpu - 15)) {
        Set-Tree $proc.Id $false; $suspended = $false; $high = 0
        Write-Host "dev_guard: CPU $cpu%, resumed"
    }
}
if ($suspended) { Set-Tree $proc.Id $false }
$proc.WaitForExit()
Write-Host "dev_guard: finished with exit code $($proc.ExitCode); peak CPU $peak%; pauses $pauses"
exit $proc.ExitCode
