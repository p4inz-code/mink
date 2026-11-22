#Requires -Version 5.1
<#
.SYNOPSIS
    Writes site\src\site.data (the values the website is built from) from the real files.

.DESCRIPTION
    Reads the version from Cargo.toml, the SHA-256 and size of the setup file and the portable zip from
    dist\ (when they exist, otherwise it keeps what site.data already holds), and the benchmark numbers
    from testapps\bench\results.json. Nothing is typed by hand, so the page cannot drift from the release.

.EXAMPLE
    PS> .\tools\update_site_data.ps1
    PS> .\tools\build_site.ps1
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$dataPath = Join-Path $repo 'site\src\site.data'
$genDir = Join-Path $repo 'site\src\gen'
New-Item -ItemType Directory -Force -Path $genDir | Out-Null

# Keep what is already there unless a fresh value is available.
$old = @{}
if (Test-Path $dataPath) {
    foreach ($line in Get-Content $dataPath) {
        if ($line -match '^([A-Z0-9_]+)=(.*)$') { $old[$Matches[1]] = $Matches[2] }
    }
}
$v = (Select-String -Path (Join-Path $repo 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$old['VERSION'] = $v

function Set-FromRelease($shaKey, $sizeKey, $file) {
    $path = Join-Path $repo $file
    $sha = "$path.sha256"
    if ((Test-Path $path) -and (Test-Path $sha)) {
        $old[$shaKey] = ((Get-Content $sha -Raw).Trim() -split '\s+')[0].ToLower()
        $old[$sizeKey] = ('{0:N1} MB' -f ((Get-Item $path).Length / 1e6))
    }
}
Set-FromRelease 'SETUP_SHA' 'SETUP_SIZE' "dist\installer\mink-Setup-$v.exe"
Set-FromRelease 'ZIP_SHA' 'ZIP_SIZE' "dist\mink-$v-portable.zip"

# Benchmarks.
$resPath = Join-Path $repo 'testapps\bench\results.json'
if (Test-Path $resPath) {
    $res = Get-Content $resPath -Raw | ConvertFrom-Json
    $benches = @(
        @{ key = 'fib'; what = 'Recursive Fibonacci of 32 (function calls)' },
        @{ key = 'loop'; what = '30 million iterations of integer arithmetic' },
        @{ key = 'sieve'; what = 'Sieve of Eratosthenes to 2 million (array reads and writes)' },
        @{ key = 'collatz'; what = 'Longest Collatz chain under 100,000 (branches and division)' }
    )
    $langs = @('MINK', 'Python', 'Go', 'Rust')
    $html = New-Object System.Text.StringBuilder
    [void]$html.AppendLine('      <div class="bench">')
    $py = @(); $go = @()
    foreach ($b in $benches) {
        $ms = @{}
        foreach ($l in $langs) { $ms[$l] = [double]$res.median_ms."$l/$($b.key)" }
        $max = ($ms.Values | Measure-Object -Maximum).Maximum
        [void]$html.AppendLine('        <div class="row">')
        [void]$html.AppendLine("          <h3>$($b.what)</h3>")
        foreach ($l in $langs) {
            $w = [math]::Max(2, [math]::Round($ms[$l] / $max * 100))
            $cls = if ($l -eq 'MINK') { 'line mink' } else { 'line' }
            [void]$html.AppendLine("          <div class=""$cls""><span>$l</span><span class=""track""><span class=""fill"" style=""width:$w%""></span></span><span class=""ms"">$([math]::Round($ms[$l])) ms</span></div>")
        }
        [void]$html.AppendLine('        </div>')
        $py += $ms['Python'] / $ms['MINK']
        $go += $ms['MINK'] / $ms['Go']
    }
    [void]$html.AppendLine('      </div>')
    [System.IO.File]::WriteAllText((Join-Path $genDir 'bench_rows.html'), $html.ToString(), (New-Object System.Text.UTF8Encoding($false)))
    $minPy = [math]::Floor(($py | Measure-Object -Minimum).Minimum)
    $maxPy = [math]::Round(($py | Measure-Object -Maximum).Maximum)
    $old['RANGE_PY'] = "$minPy to $maxPy"
    $old['RANGE_GO'] = ('{0:N1} to {1:N1}' -f ($go | Measure-Object -Minimum).Minimum, ($go | Measure-Object -Maximum).Maximum)
    $old['RUNS'] = "$($res.info.runs)"
    $old['CPU'] = $res.info.cpu
    $old['MINK_VERSION'] = $res.info.mink
    $old['PY_VERSION'] = $res.info.python
    $old['GO_VERSION'] = $res.info.go
    $old['RUST_VERSION'] = $res.info.rust
}

$lines = @(
    '# Values for the MINK website. Written by tools\update_site_data.ps1; sitegen replaces {{NAME}} in the templates.',
    'HEADER=@@site/src/partials/header.html',
    'FOOTER=@@site/src/partials/footer.html',
    'BENCH_ROWS=@@site/src/gen/bench_rows.html'
)
foreach ($k in @('VERSION', 'SETUP_SHA', 'ZIP_SHA', 'SETUP_SIZE', 'ZIP_SIZE', 'RANGE_PY', 'RANGE_GO', 'RUNS', 'CPU', 'MINK_VERSION', 'PY_VERSION', 'GO_VERSION', 'RUST_VERSION')) {
    if ($old.ContainsKey($k)) { $lines += "$k=$($old[$k])" }
}
$lines += @(
    '# Code samples come from real files, escaped for HTML.',
    'CODE_HELLO=@site/src/code/hello.mink',
    'CODE_JSON=@examples/json_config/main.mink',
    'CODE_HASH=@examples/hash_and_compress/main.mink',
    'CODE_WEB=@examples/web_loopback/main.mink',
    'CODE_WIN=@site/src/code/winapi.mink'
)
[System.IO.File]::WriteAllText($dataPath, ($lines -join "`n") + "`n", (New-Object System.Text.UTF8Encoding($false)))
Write-Host "Wrote $dataPath for MINK $v"
