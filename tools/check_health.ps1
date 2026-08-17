<#
.SYNOPSIS
    Fails when a tracked metric got worse than the last commit. The ratchet.

.DESCRIPTION
    codebase_metrics.json has always been a report: 40+ numbers that nobody
    compares to yesterday's. This script is the missing half — it turns the
    report into a gate.

    Baseline is `git show HEAD:codebase_metrics.json`, current is the working
    tree file that gen_code_metrics.ps1 just rewrote. A metric that moved the
    wrong way blocks the commit.

    Which metrics are gated is NOT a list kept here. Duplicating it would
    recreate exactly the drift this repo keeps fighting (see the deleted project
    map in .cursorrules, and goal 2 of the evolution program). Instead the rule
    is derived from catalog.rs, the existing source of truth:

        gated  <=>  the metric has a direction (Lower/Higher)
                    AND the catalog author gave it warn/bad thresholds

    Thresholds are the author's statement that the number is worth coloring;
    anything merely informational (build timings, code.avg_lines) has none and
    is reported but never blocks. Adding a metric to the gate therefore means
    giving it limits in catalog.rs — one edit, one place.

    Ratcheting, not zeroing: the goal is "no worse than yesterday", the only
    workable mode on a codebase with 572 existing .unwrap() calls. Absolute
    thresholds still drive the colour on the metrics page; this script only
    cares about direction of travel.

.NOTES
    Standalone:      powershell -File tools/check_health.ps1
    Regressions only: exit code 1, with the offending metrics printed.

    Escape hatch — a regression that is a deliberate trade:
        $env:SKIP_HEALTH = '1'      (one shell)
        powershell -File tools/check_health.ps1 -Accept
    Accepting is the normal way to land a phase that trades one metric for
    another; it is not cheating, it is recording the trade in the commit.
#>

param(
    # Report regressions but exit 0. For a commit that deliberately trades one
    # metric for another.
    [switch]$Accept,
    # Compare against a different commit than HEAD.
    [string]$Baseline = 'HEAD'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

if ($env:SKIP_HEALTH -eq '1') {
    Write-Host "[health] SKIP_HEALTH=1 — ratchet skipped." -ForegroundColor DarkGray
    exit 0
}

# ---------------------------------------------------- directions from catalog
# Parsed out of Rust rather than restated here. The catalog is formatted by
# rustfmt, so every entry is `def("key", "label", "group", "unit", N, Dir)`
# with arbitrary whitespace, and the optional wrappers with_limits/with_hint sit
# around it. `warn:`/`bad:` never appear literally — they are set by
# with_limits(...), so presence of limits is detected by that call wrapping the
# def for this key.
$catalogPath = Join-Path $root 'crates/backend/src/system/metrics/catalog.rs'
if (-not (Test-Path $catalogPath)) {
    Write-Warning "[health] catalog.rs not found — nothing to gate."
    exit 0
}
$catalog = [System.IO.File]::ReadAllText($catalogPath)

# Cut the test module off first. catalog.rs builds throwaway MetricDefs there to
# exercise status() — `def("x", "x", "code", "", 0, Lower)` and friends — and
# they parse exactly like real entries. Harmless while the fixture names are
# nonsense, actively wrong the day a fixture reuses a real key with different
# limits and shadows it.
$testModule = [regex]::Match($catalog, '(?m)^#\[cfg\(test\)\]')
if ($testModule.Success) {
    $catalog = $catalog.Substring(0, $testModule.Index)
}

# Each def(...) with its direction.
$defPattern = 'def\(\s*"([^"]+)"\s*,\s*"[^"]*"\s*,\s*"[^"]*"\s*,\s*"[^"]*"\s*,\s*(\d+)\s*,\s*(Lower|Higher|Neutral)\s*,?\s*\)'
$directions = @{}
$precisions = @{}
foreach ($m in [regex]::Matches($catalog, $defPattern)) {
    $key = $m.Groups[1].Value
    $precisions[$key] = [int]$m.Groups[2].Value
    $directions[$key] = $m.Groups[3].Value
}

# A key is "limited" when a with_limits( ... ) call encloses its def. Detected by
# finding the def's offset and checking that the nearest preceding with_limits
# opens before it and has not already closed — cheaply approximated by looking
# for `with_limits(` within the 200 characters before the def, which is what
# rustfmt produces for every limited entry in this file.
$limited = @{}
foreach ($m in [regex]::Matches($catalog, $defPattern)) {
    $key = $m.Groups[1].Value
    $start = [math]::Max(0, $m.Index - 200)
    $before = $catalog.Substring($start, $m.Index - $start)
    if ($before -match 'with_limits\(\s*$|with_limits\(\s*\r?\n\s*$') {
        $limited[$key] = $true
    }
}

if ($directions.Count -eq 0) {
    Write-Warning "[health] could not parse catalog.rs — ratchet disabled rather than guessing."
    exit 0
}

# ------------------------------------------------------------------- baseline
Push-Location $root
$baselineRaw = & git show "${Baseline}:codebase_metrics.json" 2>$null
$baselineOk = ($LASTEXITCODE -eq 0)
Pop-Location

if (-not $baselineOk) {
    Write-Host "[health] no committed baseline at $Baseline — nothing to compare. First run is always clean." -ForegroundColor DarkGray
    exit 0
}

$currentPath = Join-Path $root 'codebase_metrics.json'
if (-not (Test-Path $currentPath)) {
    Write-Warning "[health] codebase_metrics.json missing — run tools/gen_code_metrics.ps1 first."
    exit 0
}

try {
    $base = ($baselineRaw -join "`n") | ConvertFrom-Json
    $curr = Get-Content $currentPath -Raw | ConvertFrom-Json
} catch {
    Write-Warning "[health] metrics JSON unreadable, ratchet skipped: $_"
    exit 0
}

# ------------------------------------------------------------------- compare
$regressions = @()
$improvements = @()

foreach ($prop in $curr.metrics.PSObject.Properties) {
    $key = $prop.Name
    $now = [double]$prop.Value

    # Only gate what the catalog both directs and limits.
    if (-not $directions.ContainsKey($key)) { continue }
    $dir = $directions[$key]
    if ($dir -eq 'Neutral') { continue }
    if (-not $limited.ContainsKey($key)) { continue }

    $baseProp = $base.metrics.PSObject.Properties[$key]
    # A brand-new metric has no yesterday. It sets the baseline, it does not fail.
    if ($null -eq $baseProp) { continue }
    $was = [double]$baseProp.Value

    # Compare at the precision the metrics page itself displays. A density that
    # reads 3.21 before and after has not regressed in any sense a human can see,
    # and blocking on the 0.003 underneath it is how a gate earns its way onto
    # the ignore list. Counts have precision 0, so for them any move of one still
    # counts — a single new .unwrap() is caught, a rounding wobble is not.
    $p = if ($precisions.ContainsKey($key)) { $precisions[$key] } else { 0 }
    $wasR = [math]::Round($was, $p)
    $nowR = [math]::Round($now, $p)
    if ($wasR -eq $nowR) { continue }

    $delta = [math]::Round($nowR - $wasR, $p)
    $worse = if ($dir -eq 'Lower') { $nowR -gt $wasR } else { $nowR -lt $wasR }
    $row = [pscustomobject]@{ Key = $key; Was = $wasR; Now = $nowR; Delta = $delta; Dir = $dir }
    if ($worse) { $regressions += $row } else { $improvements += $row }
}

# --------------------------------------------------------------------- report
function Format-Row($r) {
    $sign = if ($r.Delta -gt 0) { '+' } else { '' }
    return ("  {0,-28} {1,10} -> {2,-10} ({3}{4})" -f $r.Key, $r.Was, $r.Now, $sign, $r.Delta)
}

if ($improvements.Count -gt 0) {
    Write-Host "[health] improved:" -ForegroundColor Green
    foreach ($r in ($improvements | Sort-Object Key)) { Write-Host (Format-Row $r) -ForegroundColor Green }
}

if ($regressions.Count -eq 0) {
    Write-Host "[health] OK — no tracked metric got worse since $Baseline." -ForegroundColor Green
    exit 0
}

Write-Host ""
Write-Host "[health] REGRESSION — these moved the wrong way since ${Baseline}:" -ForegroundColor Red
foreach ($r in ($regressions | Sort-Object Key)) { Write-Host (Format-Row $r) -ForegroundColor Red }
Write-Host ""

if ($Accept) {
    Write-Host "[health] -Accept given: recorded, not blocking." -ForegroundColor Yellow
    exit 0
}

Write-Host "If this is a deliberate trade, re-run the commit with:" -ForegroundColor DarkGray
Write-Host "    `$env:SKIP_HEALTH = '1'" -ForegroundColor DarkGray
Write-Host "or explain it in the commit message and use tools/check_health.ps1 -Accept." -ForegroundColor DarkGray
exit 1
