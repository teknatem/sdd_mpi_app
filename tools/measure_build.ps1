<#
.SYNOPSIS
    Measures what an edit actually costs, and writes build_timings.json.

.DESCRIPTION
    Build cost is the most-felt problem in this repo and the only one that was
    never measured: codebase_metrics.json tracks 30+ numbers and not one of them
    is a second. This script produces that missing baseline.

    Seven numbers, chosen because they are the ones a developer waits on:

        build.incr_backend_sec      backend `check` after editing ONE aggregate file
        build.incr_frontend_sec     frontend `check` after editing ONE aggregate file
        build.test_backend_sec      backend TEST BINARY built+linked after that edit
        build.wasm_frontend_sec     frontend WASM built after that edit
        build.contracts_ripple_sec  both crates re-checked after editing contracts
        build.full_backend_sec      backend from scratch (deps kept warm)
        build.full_frontend_sec     frontend (wasm) from scratch

    The two `check` numbers are cheap and the codegen ones are not; that spread
    is the finding, not a rounding detail. Measure both or the conclusion is wrong.

    Why `cargo check` and not `cargo build`:
      - `check` is the command the workflow in CLAUDE.md actually prescribes, so
        it is the wait that is really paid;
      - it produces no backend.exe, so a running backend cannot make the
        measurement fail with "Access is denied" the way `cargo run` does.

    Why "deps kept warm": `cargo clean` would also drop ~200 third-party crates
    and measure a cold dependency tree we are not trying to change. `cargo clean
    -p <crate>` drops only OUR artifacts, which is what the phases of the
    modernization plan can actually move.

    Why real edits instead of touching mtimes: rustc's incremental cache keys on
    content, so a bare mtime bump reports a rebuild that never happened. The
    script appends one probe comment line, measures, and restores the original
    bytes in a `finally` block.

.NOTES
    Run from the repo root, with nothing else compiling:
        powershell -File tools/measure_build.ps1

    Takes roughly 10-20 minutes. It is NOT part of the pre-commit hook for that
    reason: build_timings.json is committed, and gen_code_metrics.ps1 reads the
    committed numbers rather than re-measuring them.

    Re-run it when a phase of the modernization plan claims to have changed
    build cost — that claim is what this file exists to check.
#>

param(
    # Skip the two "from scratch" numbers. They dominate the runtime; the
    # incremental ones are what change most often between phases.
    [switch]$IncrementalOnly
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root

# Probe files: one representative aggregate present in all three crates, so the
# three ripple measurements describe the same slice rather than three unrelated
# corners of the project.
$probes = [ordered]@{
    backend   = 'crates/backend/src/domain/a012_wb_sales/service.rs'
    frontend  = 'crates/frontend/src/domain/a012_wb_sales/ui/list/mod.rs'
    contracts = 'crates/contracts/src/domain/a012_wb_sales/aggregate.rs'
}

foreach ($name in $probes.Keys) {
    $path = Join-Path $root $probes[$name]
    if (-not (Test-Path $path)) {
        Pop-Location
        throw "Probe file missing: $($probes[$name]). Update `$probes in this script."
    }
}

$timings = [ordered]@{}

function Invoke-Timed {
    <#
        Runs a cargo command and returns elapsed seconds, or $null if it failed.
        A failed command must not be recorded: a compile error would otherwise
        be written down as a suspiciously fast build.
    #>
    param([string]$Label, [string[]]$CargoArgs)

    Write-Host ""
    Write-Host "  > cargo $($CargoArgs -join ' ')" -ForegroundColor DarkGray
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    # No `2>&1` here on purpose. In Windows PowerShell 5.1 redirecting a native
    # command's stderr wraps every line in a NativeCommandError, which under
    # $ErrorActionPreference='Stop' aborts the run — and cargo writes its
    # progress to stderr continuously. --quiet keeps that output small enough to
    # simply leave alone.
    & cargo @CargoArgs | Out-Null
    $sw.Stop()

    if ($LASTEXITCODE -ne 0) {
        Write-Host "    FAILED (exit $LASTEXITCODE) - not recorded" -ForegroundColor Red
        return $null
    }
    $secs = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    Write-Host ("    {0,8:N1} s  {1}" -f $secs, $Label) -ForegroundColor Green
    return $secs
}

function Measure-AfterEdit {
    <#
        Appends a probe comment to $ProbeRel, runs $Measure, restores the file.
        The restore lives in `finally` so an interrupted run does not leave a
        stray comment in a source file.
    #>
    param([string]$ProbeRel, [scriptblock]$Measure)

    $path = Join-Path $root $ProbeRel
    $original = [System.IO.File]::ReadAllBytes($path)
    # Cargo fingerprints path dependencies by mtime, not content. Restoring the
    # bytes with a fresh timestamp therefore leaves the file looking modified,
    # and the developer's NEXT build pays a full recompile for a measurement
    # they did not ask for. Put the original timestamp back too.
    $originalWrite = (Get-Item $path).LastWriteTimeUtc
    try {
        $probe = "`n// build-probe $([DateTime]::UtcNow.Ticks) - remove if you see this committed`n"
        [System.IO.File]::AppendAllText($path, $probe)
        return & $Measure
    } finally {
        [System.IO.File]::WriteAllBytes($path, $original)
        (Get-Item $path).LastWriteTimeUtc = $originalWrite
    }
}

$checkBackend  = @('check', '-p', 'backend', '--quiet')
$checkFrontend = @('check', '-p', 'frontend', '--target', 'wasm32-unknown-unknown', '--quiet')

Write-Host "Measuring build cost. Nothing else should be compiling." -ForegroundColor Cyan

# --- warm-up ----------------------------------------------------------------
# Everything below measures a delta from "already built". Without this, the
# first measurement silently absorbs whatever was stale when the script started.
Write-Host ""
Write-Host "[0/7] Warm-up (not recorded)" -ForegroundColor Cyan
# All four commands, not just the two `check`s: `test --no-run` and `build`
# produce different artifacts (codegen units, dev-dependencies, the linked test
# binary). Without warming those, the first measurement of each would be a
# from-scratch build wearing the label "incremental".
Invoke-Timed 'warm-up backend check'  $checkBackend  | Out-Null
Invoke-Timed 'warm-up frontend check' $checkFrontend | Out-Null
Invoke-Timed 'warm-up backend test build' @('test', '-p', 'backend', '--no-run', '--quiet') | Out-Null
Invoke-Timed 'warm-up frontend wasm build' @('build', '-p', 'frontend', '--target', 'wasm32-unknown-unknown', '--quiet') | Out-Null

# --- incremental ------------------------------------------------------------
Write-Host ""
Write-Host "[1/7] Incremental backend check (one aggregate file edited)" -ForegroundColor Cyan
$timings['build.incr_backend_sec'] = Measure-AfterEdit $probes.backend {
    Invoke-Timed 'incremental backend' $checkBackend
}

Write-Host ""
Write-Host "[2/7] Incremental frontend check (one aggregate file edited)" -ForegroundColor Cyan
$timings['build.incr_frontend_sec'] = Measure-AfterEdit $probes.frontend {
    Invoke-Timed 'incremental frontend' $checkFrontend
}

# --- codegen, not just type-checking ----------------------------------------
# The first version of this script measured only `cargo check`, and the answer
# came back suspiciously cheap: ~9 s to re-check 190k lines. That is real, but it
# is not the wait people actually complain about — `check` stops before codegen.
# CLAUDE.md quotes "backend test ~3 min", and the gap between that and 9 s is the
# whole point: the minutes live in codegen and linking.
#
# `--no-run` builds and links the test binary without executing it, isolating
# compile cost from test runtime. `build` does the same for the wasm pipeline.
# These two are what decides whether splitting the workspace into crates is worth
# it — the check numbers alone would have answered "no" for the wrong reason.
Write-Host ""
Write-Host "[3/7] Backend test binary after editing one aggregate file" -ForegroundColor Cyan
$timings['build.test_backend_sec'] = Measure-AfterEdit $probes.backend {
    Invoke-Timed 'incremental backend test build' @('test', '-p', 'backend', '--no-run', '--quiet')
}

Write-Host ""
Write-Host "[3b/7] Backend BINARY after editing one aggregate file" -ForegroundColor Cyan
# The number that actually describes "edit -> running application", and the one
# this script originally missed. `check`, `test --no-run` and `build --bin`
# produce THREE separate artifact sets (libbackend-*.rmeta,
# deps/backend-<hash>.exe, debug/backend.exe) with independent fingerprints —
# none of them warms the others. Measuring only the first two made the workflow
# look several times cheaper than it is for anyone who runs the app.
#
# Fails while backend.exe is running (the linker cannot overwrite it); a failed
# measurement is dropped rather than recorded, so stop the backend first —
# tools/restart_backend.ps1 does that.
$timings['build.bin_backend_sec'] = Measure-AfterEdit $probes.backend {
    Invoke-Timed 'incremental backend binary' @('build', '--bin', 'backend', '--quiet')
}

Write-Host ""
Write-Host "[4/7] Frontend wasm build after editing one aggregate file" -ForegroundColor Cyan
$timings['build.wasm_frontend_sec'] = Measure-AfterEdit $probes.frontend {
    Invoke-Timed 'incremental frontend wasm build' @('build', '-p', 'frontend', '--target', 'wasm32-unknown-unknown', '--quiet')
}

# --- the contracts ripple ---------------------------------------------------
# The number the plan expects to be worst: contracts has a fan-in of 424 backend
# files and 243 frontend files, so one edit there rebuilds effectively everything.
Write-Host ""
Write-Host "[5/7] Contracts ripple (one contracts file edited -> both crates)" -ForegroundColor Cyan
$timings['build.contracts_ripple_sec'] = Measure-AfterEdit $probes.contracts {
    $be = Invoke-Timed 'contracts -> backend'  $checkBackend
    $fe = Invoke-Timed 'contracts -> frontend' $checkFrontend
    if ($null -eq $be -or $null -eq $fe) { return $null }
    return [math]::Round($be + $fe, 1)
}

# --- from scratch -----------------------------------------------------------
if (-not $IncrementalOnly) {
    Write-Host ""
    Write-Host "[6/7] Full backend (our crates cleaned, dependencies kept)" -ForegroundColor Cyan
    & cargo clean -p backend | Out-Null
    $timings['build.full_backend_sec'] = Invoke-Timed 'full backend' $checkBackend

    Write-Host ""
    Write-Host "[7/7] Full frontend (our crates cleaned, dependencies kept)" -ForegroundColor Cyan
    & cargo clean -p frontend --target wasm32-unknown-unknown | Out-Null
    $timings['build.full_frontend_sec'] = Invoke-Timed 'full frontend' $checkFrontend
} else {
    Write-Host ""
    Write-Host "[6-7/7] Skipped (-IncrementalOnly)" -ForegroundColor DarkGray
}

# --- write ------------------------------------------------------------------
# Drop failed measurements rather than writing nulls: gen_code_metrics.ps1 and
# the ratchet both treat "absent" correctly and "null" as noise.
$clean = [ordered]@{}
foreach ($key in $timings.Keys) {
    if ($null -ne $timings[$key]) { $clean[$key] = [double]$timings[$key] }
}

# Merge over whatever was measured before, rather than replacing the file.
# -IncrementalOnly exists precisely so the cheap numbers can be refreshed often;
# if that run also erased the expensive from-scratch ones, using the flag would
# quietly cost you the very baseline you are comparing against.
$destPath = Join-Path $root 'build_timings.json'
if (Test-Path $destPath) {
    try {
        $previous = Get-Content $destPath -Raw | ConvertFrom-Json
        if ($previous.timings) {
            $merged = [ordered]@{}
            foreach ($prop in $previous.timings.PSObject.Properties) {
                $merged[$prop.Name] = [double]$prop.Value
            }
            # This run wins for the keys it actually measured.
            foreach ($key in $clean.Keys) { $merged[$key] = $clean[$key] }
            $clean = $merged
        }
    } catch {
        Write-Warning "Previous build_timings.json unreadable, writing fresh: $_"
    }
}

$head = & git rev-parse --short HEAD
if ($LASTEXITCODE -ne 0) { $head = $null }

$payload = [ordered]@{
    measured_at = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    git_head    = if ($head) { ($head | Select-Object -First 1).Trim() } else { $null }
    # Timings are only comparable to themselves on the same machine. Recording
    # which machine produced them keeps a laptop run from being read as a
    # regression against a desktop baseline.
    machine     = [ordered]@{
        name  = $env:COMPUTERNAME
        cpus  = [int]$env:NUMBER_OF_PROCESSORS
        rustc = ((& rustc -V) | Select-Object -First 1)
    }
    partial     = [bool]$IncrementalOnly
    timings     = $clean
}

# UTF-8 without BOM, same as codebase_metrics.json.
$json = $payload | ConvertTo-Json -Depth 5
$dest = Join-Path $root 'build_timings.json'
[System.IO.File]::WriteAllText($dest, $json, (New-Object System.Text.UTF8Encoding($false)))

Pop-Location

Write-Host ""
Write-Host "build_timings.json written: $dest" -ForegroundColor Cyan
foreach ($key in $clean.Keys) {
    Write-Host ("  {0,-32} {1,8:N1} s" -f $key, $clean[$key])
}
Write-Host ""
Write-Host "Now run: powershell -File tools/gen_code_metrics.ps1" -ForegroundColor DarkGray
