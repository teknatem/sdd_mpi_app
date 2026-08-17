<#
.SYNOPSIS
    Regenerates codebase_metrics.json — the code half of the project metrics page.

.DESCRIPTION
    The backend cannot measure the source tree: in production it runs next to a
    database, not next to a repository, and walking 1700 files would break the
    page's promise of "no scans". So the numbers are frozen at commit time here
    and baked into the binary with include_str! (system/metrics/codebase.rs).

    Architecture and UI counters are NOT recomputed. They are parsed out of
    ARCHITECTURE.md and UI_REGISTRY.md, which the other two generators already
    produce and which already carry the counts in their headings and summary
    table. Duplicating that logic would give us a second source of truth that
    drifts. Hence the ordering in the pre-commit hook: architecture and UI
    registry first, this script last.

    Output shape is deliberately flat:

        { "generated_at", "git_head",
          "metrics": { "<catalog key>": <number> },
          "details": [ { code, label, value_label, rows: [{name, value, extra}] } ] }

    Keys must match crates/backend/src/system/metrics/catalog.rs — a key that is
    not in the catalog is collected and never shown, and a test in codebase.rs
    fails the build for exactly that reason.

.NOTES
    Run from the repo root:
        powershell -File tools/gen_code_metrics.ps1
    codebase_metrics.json is GENERATED and committed. Edit this script, not it.
#>

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

$metrics = [ordered]@{}
$details = @()

function Set-Metric([string]$key, $value) {
    if ($null -eq $value) { return }
    $metrics[$key] = [double]$value
}

# ---------------------------------------------------------------- source size
# Read through .NET rather than Get-Content: 1700 files at commit time is a
# noticeable wait otherwise.
$crates = @('backend', 'frontend', 'contracts')
$allFiles = @()
$totalLines = 0
$totalFiles = 0

foreach ($crate in $crates) {
    $srcRoot = Join-Path $root "crates/$crate/src"
    if (-not (Test-Path $srcRoot)) { continue }

    $crateLines = 0
    $crateFiles = 0
    $crateTests = 0

    foreach ($file in [System.IO.Directory]::EnumerateFiles($srcRoot, '*.rs', 'AllDirectories')) {
        $text = [System.IO.File]::ReadAllText($file)
        # Same convention as `wc -l`: count line breaks, not lines.
        $lines = $text.Split("`n").Length - 1
        $crateLines += $lines
        $crateFiles++
        $crateTests += ([regex]::Matches($text, '#\[test\]|#\[tokio::test\]')).Count

        $allFiles += [pscustomobject]@{
            Rel     = $file.Substring($root.Length + 1).Replace('\', '/')
            Crate   = $crate
            Lines   = $lines
            Unwrap  = ([regex]::Matches($text, '\.unwrap\(\)')).Count
            Todo    = ([regex]::Matches($text, 'TODO|FIXME')).Count
        }
    }

    Set-Metric "code.lines.$crate" $crateLines
    $totalLines += $crateLines
    $totalFiles += $crateFiles

    # Test density, not raw counts: the only number comparable across crates
    # of very different size.
    if ($crateLines -gt 0) {
        Set-Metric "tests.density.$crate" ([math]::Round($crateTests * 1000.0 / $crateLines, 3))
    }
    $script:testsTotal += $crateTests
}

Set-Metric 'code.lines.total' $totalLines
Set-Metric 'code.files.total' $totalFiles
if ($totalFiles -gt 0) {
    Set-Metric 'code.avg_lines' ([math]::Round($totalLines / $totalFiles, 0))
}
Set-Metric 'code.files_over_1000' (@($allFiles | Where-Object { $_.Lines -gt 1000 }).Count)

# 2000 is the line above which a file stops being "large" and starts being a hub:
# every file over it in this repo either aggregates the whole project (routes.rs,
# registry.rs) or bundles several roles into one. Tracked separately from the
# 1000-line count because these are the ones the plan actually splits.
Set-Metric 'code.files_over_2000' (@($allFiles | Where-Object { $_.Lines -gt 2000 }).Count)
if ($allFiles.Count -gt 0) {
    Set-Metric 'code.max_file_lines' (($allFiles | Measure-Object Lines -Maximum).Maximum)
}

# Crate count is the unit of incremental compilation, so it belongs next to the
# build timings: splitting the workspace is the plan's answer to build cost, and
# this is the number that moves when it happens.
$crateManifests = @(Get-ChildItem (Join-Path $root 'crates') -Filter 'Cargo.toml' -File -Depth 1 -ErrorAction SilentlyContinue)
if ($crateManifests.Count -gt 0) {
    Set-Metric 'arch.crates' $crateManifests.Count
}

$topFiles = @($allFiles | Sort-Object Lines -Descending | Select-Object -First 10)
if ($totalLines -gt 0 -and $topFiles.Count -gt 0) {
    $topLines = ($topFiles | Measure-Object Lines -Sum).Sum
    Set-Metric 'code.top10_share' ([math]::Round($topLines * 100.0 / $totalLines, 2))
}

Set-Metric 'tests.total' $script:testsTotal
Set-Metric 'smells.unwrap' (($allFiles | Measure-Object Unwrap -Sum).Sum)
Set-Metric 'smells.todo_fixme' (($allFiles | Measure-Object Todo -Sum).Sum)

$details += [ordered]@{
    code        = 'code.top_files'
    label       = 'Самые большие файлы'
    value_label = 'Строк'
    rows        = @($topFiles | ForEach-Object {
        [ordered]@{ name = $_.Rel; value = [double]$_.Lines }
    })
}

# ----------------------------------------------------------- orphan .rs files
# Files under crates/*/src that no `mod` declaration reaches. Rust compiles only
# what is declared, so these are dead: cargo check never sees them, cargo fmt
# never formats them, and yet they are counted in code.lines.total and read by
# anyone browsing the tree — the file-level version of the empty directories in
# goal 7 of the evolution program, and a worse one, because a ghost with 235
# lines of plausible code in it looks alive.
#
# Approximate but sound in the direction that matters: a file is considered
# reached if SOME file in the same crate declares `mod <name>;`. That can only
# under-report (two modules with the same leaf name), never invent a ghost.
$orphanFiles = @()
foreach ($crate in $crates) {
    $srcRoot = Join-Path $root "crates/$crate/src"
    if (-not (Test-Path $srcRoot)) { continue }

    $declared = New-Object 'System.Collections.Generic.HashSet[string]'
    $files = @([System.IO.Directory]::EnumerateFiles($srcRoot, '*.rs', 'AllDirectories'))
    foreach ($file in $files) {
        $text = [System.IO.File]::ReadAllText($file)
        foreach ($m in [regex]::Matches($text, '(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;')) {
            [void]$declared.Add($m.Groups[1].Value)
        }
    }

    foreach ($file in $files) {
        $name = [System.IO.Path]::GetFileName($file)
        # Crate roots are reached by Cargo, not by a `mod` statement.
        if ($name -eq 'lib.rs' -or $name -eq 'main.rs') { continue }
        # `foo/mod.rs` is declared as `mod foo;`, `foo.rs` as `mod foo;`.
        $modName = if ($name -eq 'mod.rs') {
            Split-Path (Split-Path $file -Parent) -Leaf
        } else {
            [System.IO.Path]::GetFileNameWithoutExtension($name)
        }
        if (-not $declared.Contains($modName)) {
            $orphanFiles += $file.Substring($root.Length + 1).Replace('\', '/')
        }
    }
}
Set-Metric 'code.orphan_files' $orphanFiles.Count
if ($orphanFiles.Count -gt 0) {
    $details += [ordered]@{
        code        = 'code.orphans'
        label       = 'Файлы вне дерева модулей'
        value_label = 'Строк'
        rows        = @($orphanFiles | ForEach-Object {
            $abs = Join-Path $root $_
            $n = ([System.IO.File]::ReadAllText($abs).Split("`n").Length - 1)
            [ordered]@{ name = $_; value = [double]$n }
        })
    }
}

# ------------------------------------------------------- API and FE boundaries
# Targeted second pass rather than folding this into the loop above: it touches
# ~90 handler files and the frontend tree, not all 1762, and keeping the regexes
# next to the thing they describe is worth one extra read.
#
# What these three measure is a single leak in the same wall. `contracts` exists
# to make the FE<->BE contract compile-checked; every handler that answers with
# a bare `serde_json::Value`, every error collapsed to a naked `StatusCode`, and
# every frontend file that speaks HTTP on its own bypasses it.

$handlersDir = Join-Path $root 'crates/backend/src/api/handlers'
if (Test-Path $handlersDir) {
    $untyped = 0
    $statusOnly = 0
    foreach ($file in [System.IO.Directory]::EnumerateFiles($handlersDir, '*.rs', 'AllDirectories')) {
        $text = [System.IO.File]::ReadAllText($file)
        # `-> Json<Value>` and `-> Result<Json<Value>, _>` alike: the response
        # body is untyped either way.
        $untyped += ([regex]::Matches($text, '->\s*(?:Result<\s*)?Json<serde_json::Value>')).Count
        # Any Err type that is just a status code. `[^;{]` bounds the match to a
        # signature — a declaration cannot span a `;` or the opening brace.
        # `,?` covers the rustfmt'ed multi-line form, where the error type ends
        # with a trailing comma before `>`; without it those slip past the gate.
        $statusOnly += ([regex]::Matches($text, '->\s*Result<[^;{]*?,\s*(?:axum::http::)?StatusCode\s*,?\s*>')).Count
    }
    Set-Metric 'api.untyped_handlers' $untyped
    Set-Metric 'api.status_only_errors' $statusOnly
}

# Files still taking the database from the global singleton instead of AppState.
# Counted per file for the same reason as raw_fetch_files: the cost is one more
# place that cannot be pointed at a different database — which is what kept the
# project untestable. Phase 3 opened the alternative; this counts the remainder.
$backendSrc = Join-Path $root 'crates/backend/src'
if (Test-Path $backendSrc) {
    $globalDb = 0
    foreach ($file in [System.IO.Directory]::EnumerateFiles($backendSrc, '*.rs', 'AllDirectories')) {
        if ([System.IO.File]::ReadAllText($file) -match 'get_connection\(\)') { $globalDb++ }
    }
    Set-Metric 'api.global_db_files' $globalDb
}

# Files that build their own HTTP call instead of going through shared/api_utils.
# Counted per file, not per call: the cost is that auth, 401 handling and error
# parsing are re-decided in each of them.
$feSrc = Join-Path $root 'crates/frontend/src'
if (Test-Path $feSrc) {
    $rawFetch = 0
    foreach ($file in [System.IO.Directory]::EnumerateFiles($feSrc, '*.rs', 'AllDirectories')) {
        $text = [System.IO.File]::ReadAllText($file)
        if ($text -match 'fetch_with_request|gloo_net') { $rawFetch++ }
    }
    Set-Metric 'fe.raw_fetch_files' $rawFetch
}

# Integration tests = the canonical Rust location, crates/backend/tests/. Kept
# distinct from tests.total because the two answer different questions: how much
# is tested at all, versus whether anything is tested through a real database.
# Starts at 0 by construction — a global `static DB_CONN` leaves no way to open
# a test database. It is the metric phase 3 of the modernization plan exists for.
$integrationDir = Join-Path $root 'crates/backend/tests'
$integrationTests = 0
if (Test-Path $integrationDir) {
    foreach ($file in [System.IO.Directory]::EnumerateFiles($integrationDir, '*.rs', 'AllDirectories')) {
        $text = [System.IO.File]::ReadAllText($file)
        $integrationTests += ([regex]::Matches($text, '#\[test\]|#\[tokio::test\]')).Count
    }
}
Set-Metric 'tests.integration' $integrationTests

# ------------------------------------------------------------- build timings
# Written by tools/measure_build.ps1, which is far too slow for the pre-commit
# hook. Read from the committed file so the page shows the last real measurement
# instead of nothing; absent file simply means these metrics stay unset.
$timingsPath = Join-Path $root 'build_timings.json'
if (Test-Path $timingsPath) {
    try {
        $bt = Get-Content $timingsPath -Raw | ConvertFrom-Json
        if ($bt.timings) {
            foreach ($prop in $bt.timings.PSObject.Properties) {
                Set-Metric $prop.Name $prop.Value
            }
        }
    } catch {
        Write-Warning "build_timings.json is unreadable, build metrics skipped: $_"
    }
}

# ------------------------------------------------------- architecture findings
# Output of the external code_grid_dashboard analyzer. The directory is
# gitignored, so on a fresh clone this metric is simply absent rather than zero —
# "not measured" and "nothing found" must not look alike.
$diagPath = Join-Path $root '.vsa_designer/diagnostics.json'
if (Test-Path $diagPath) {
    try {
        $diag = Get-Content $diagPath -Raw | ConvertFrom-Json
        Set-Metric 'arch.vsa_findings' (@($diag.findings).Count)
    } catch {
        Write-Warning "diagnostics.json is unreadable, analyzer findings skipped: $_"
    }
}

# ------------------------------------------------------------------ migrations
$migrations = Join-Path $root 'migrations'
if (Test-Path $migrations) {
    Set-Metric 'arch.migrations' (@(Get-ChildItem $migrations -Filter *.sql -File).Count)
}

# ------------------------------------------------------------------------ docs
$memoryBank = Join-Path $root 'memory-bank'
if (Test-Path $memoryBank) {
    Set-Metric 'docs.memory_bank' (@(Get-ChildItem $memoryBank -Filter *.md -File -Recurse).Count)
    $adrDir = Join-Path $memoryBank 'decisions'
    if (Test-Path $adrDir) {
        Set-Metric 'docs.adr' (@(Get-ChildItem $adrDir -Filter 'ADR-*.md' -File).Count)
    }
}

# --------------------------------------------------- ARCHITECTURE.md counters
# Every section has the same shape: a `##` heading followed by a markdown table
# whose rows start with a backticked code. Count rows, do not parse contents.
$archPath = Join-Path $root 'ARCHITECTURE.md'
if (Test-Path $archPath) {
    $archLines = [System.IO.File]::ReadAllLines($archPath)

    # Heading -> (metric key, does it count towards doc coverage)
    $sections = [ordered]@{
        'Aggregates'      = @{ Key = 'arch.aggregates';   Docs = $true }
        'Projections'     = @{ Key = 'arch.projections';  Docs = $true }
        'Use-cases'       = @{ Key = 'arch.usecases';     Docs = $true }
        'Data schemes'    = @{ Key = 'arch.data_schemes'; Docs = $false }
        'Scheduled tasks' = @{ Key = 'arch.tasks';        Docs = $false }
        'DataView'        = @{ Key = 'arch.data_views';   Docs = $true }
    }

    $current = $null
    $rowCount = @{}
    $docsTotal = 0
    $docsCovered = 0

    foreach ($line in $archLines) {
        if ($line -match '^##\s+(.+?)(\s+\(.*\))?\s*$') {
            $title = $Matches[1].Trim()
            $current = $null
            foreach ($name in $sections.Keys) {
                if ($title -like "$name*") { $current = $name; break }
            }
            # Headings that already carry the count in parentheses.
            if ($title -like 'UI scopes*' -and $line -match '\((\d+)\)') {
                Set-Metric 'arch.ui_scopes' $Matches[1]
            }
            if ($title -like 'API routes*' -and $line -match '\((\d+)\)') {
                Set-Metric 'arch.routes' $Matches[1]
            }
            continue
        }

        if ($null -eq $current) { continue }
        if ($line -notmatch '^\|\s*`') { continue }

        if (-not $rowCount.ContainsKey($current)) { $rowCount[$current] = 0 }
        $rowCount[$current]++

        if ($sections[$current].Docs) {
            $docsTotal++
            if ($line -match '✓') { $docsCovered++ }
        }
    }

    foreach ($name in $sections.Keys) {
        if ($rowCount.ContainsKey($name)) {
            Set-Metric $sections[$name].Key $rowCount[$name]
        }
    }
    if ($docsTotal -gt 0) {
        Set-Metric 'arch.docs_coverage' ([math]::Round($docsCovered * 100.0 / $docsTotal, 1))
    }
}

# ----------------------------------------------------- UI_REGISTRY.md counters
$uiPath = Join-Path $root 'UI_REGISTRY.md'
if (Test-Path $uiPath) {
    $uiLines = [System.IO.File]::ReadAllLines($uiPath)

    # Summary labels -> catalog keys, matched as substrings: the wording in the
    # registry generator has changed before, the metric codes have not.
    $summaryMap = [ordered]@{
        'Block roots'                    = 'ui.block_roots'
        'Classes with no Rust reference' = 'ui.dead_classes'
        'Inline'                         = 'ui.inline_styles'
        'Hardcoded hex'                  = 'ui.hardcoded_hex'
        'Raw px'                         = 'ui.raw_px'
        'NO fallback'                    = 'ui.broken_tokens'
    }

    $inSummary = $false
    $unregistered = 0
    foreach ($line in $uiLines) {
        if ($line -match '^##\s') {
            $inSummary = $line -match '^##\s+Summary'
            if ($line -match '^##\s+Blocks defined in more than one file\s*\((\d+)\)') {
                Set-Metric 'ui.duplicate_blocks' $Matches[1]
            }
            continue
        }

        if ($inSummary -and $line -match '^\|\s*(.+?)\s*\|\s*(\d+)\s*\|\s*$') {
            $label = $Matches[1]
            $value = $Matches[2]
            foreach ($needle in $summaryMap.Keys) {
                if ($label -like "*$needle*") {
                    Set-Metric $summaryMap[$needle] $value
                    break
                }
            }
            continue
        }

        # Unregistered blocks are counted from the registry table — the Summary
        # section does not carry that number.
        if ($line -match '^\|\s*`[^`]+`\s*\|.*\|\s*unregistered\s*\|\s*$') {
            $unregistered++
        }
    }
    Set-Metric 'ui.unregistered_blocks' $unregistered
}

# ------------------------------------------------------------------------- git
function Invoke-Git([string[]]$gitArgs) {
    try {
        $output = & git @gitArgs 2>$null
        if ($LASTEXITCODE -ne 0) { return $null }
        return $output
    } catch {
        return $null
    }
}

$head = Invoke-Git @('rev-parse', '--short', 'HEAD')
$gitHead = if ($head) { ($head | Select-Object -First 1).Trim() } else { $null }

$commitsTotal = Invoke-Git @('rev-list', '--count', 'HEAD')
if ($commitsTotal) { Set-Metric 'git.commits_total' ($commitsTotal | Select-Object -First 1) }

$commits30 = Invoke-Git @('rev-list', '--count', '--since=30 days ago', 'HEAD')
if ($commits30) { Set-Metric 'git.commits_30d' ($commits30 | Select-Object -First 1) }

# --numstat prints one line per file: added, deleted, path. Binary files come
# with "-" instead of numbers and must be skipped, or the sum turns into a string.
$numstat = Invoke-Git @('log', '--since=30 days ago', '--numstat', '--format=')
if ($numstat) {
    $added = 0
    $deleted = 0
    foreach ($line in $numstat) {
        if ($line -match '^(\d+)\s+(\d+)\s+') {
            $added += [int]$Matches[1]
            $deleted += [int]$Matches[2]
        }
    }
    Set-Metric 'git.added_30d' $added
    Set-Metric 'git.deleted_30d' $deleted
}

# ----------------------------------------------------------------------- write
$payload = [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    git_head     = $gitHead
    metrics      = $metrics
    details      = $details
}

# UTF-8 WITHOUT BOM: the file goes into include_str!, and a BOM inside the
# string literal makes serde_json fail on the very first character.
$json = $payload | ConvertTo-Json -Depth 6
$dest = Join-Path $root 'codebase_metrics.json'
[System.IO.File]::WriteAllText($dest, $json, (New-Object System.Text.UTF8Encoding($false)))

Write-Host "codebase_metrics.json regenerated: $dest"
Write-Host "  metrics=$($metrics.Count) lines=$totalLines files=$totalFiles"
