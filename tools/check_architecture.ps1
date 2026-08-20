<#
.SYNOPSIS
    Checks the source tree against architecture.toml. The standard, executed.

.DESCRIPTION
    architecture.toml was written as goal 1 of the evolution program — one
    machine-readable source for the project's own rules. Until this script
    existed nothing read it, which is precisely how naming-conventions.md died:
    goal 2 found it describing frontend usecases as mod.rs + widget.rs +
    monitor.rs, files that never existed in any usecase. A standard nobody
    checks drifts, silently, and is then worse than no standard at all because
    it still looks authoritative.

    The rules are NOT written here. This file is an engine for a handful of
    check TYPES; every concrete rule is a [[rules]] entry in architecture.toml.
    Adding a rule is an edit to the TOML, not to this script — which is the
    point: a new convention can be adopted the day it is decided, and a case
    that legitimately breaks a rule is legalised by a `waivers` entry carrying
    its reason, rather than by switching the check off.

    Rule types:
      required_files          every directory matching `scope` contains `files`
      forbidden_filename      a retired name must not come back (one role, one name)
      dir_name_pattern        directories under `scope` match an index regex
      dir_matches_metadata_id directory name equals `id` inside its metadata.json
      snake_case_files        .rs file names are snake_case
      embedded_doc_path_sync  `source_path:` agrees with the neighbouring include_str!
      dir_manifest            the scripts in a directory are exactly the ones the manifest lists

    Severity `error` counts toward the gate; `warn` is reported only.

.NOTES
    Standalone:   powershell -File tools/check_architecture.ps1
                  powershell -File tools/check_architecture.ps1 -Verbose
    For tooling:  powershell -File tools/check_architecture.ps1 -Json
                  (prints one JSON object, always exits 0 — the caller decides)

    Exit code 1 when at least one `error` violation is found, unless -Json.
    gen_code_metrics.ps1 calls this with -Json and records the counts as
    arch.naming_violations / arch.waived_rules; the ratchet in check_health.ps1
    then gates them like any other metric, because catalog.rs gives them a
    direction and limits.
#>

param(
    # Emit a JSON object instead of a human report, and never fail.
    [switch]$Json,
    # List every waived match as well, to audit the waivers themselves.
    [switch]$ShowWaived
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

# ============================================================================
# A TOML reader, restricted to what architecture.toml actually uses
# ============================================================================
# PowerShell 5.1 has no TOML support and this repo has no vendored parser. The
# subset below is deliberate rather than lazy: tables, arrays of tables, string
# / bool / int scalars, string arrays (single- and multi-line), multi-line
# basic strings and one level of inline table. Anything richer is rejected
# loudly instead of being silently misread — a config parser that guesses is
# how you get a gate that passes for the wrong reason.

function ConvertFrom-TomlText([string]$text) {
    # A BOM would ride along into the first table header and break the match.
    if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }

    $rootTable = [ordered]@{}
    $current = $rootTable

    $lines = $text -split "`r?`n"
    $i = 0
    while ($i -lt $lines.Count) {
        $line = $lines[$i]
        $i++

        $trimmed = $line.Trim()
        if ($trimmed -eq '' -or $trimmed.StartsWith('#')) { continue }

        # [[array.of.tables]]
        if ($trimmed -match '^\[\[([^\]]+)\]\]$') {
            $path = $matches[1].Trim()
            $parent = $rootTable
            $parts = $path -split '\.'
            for ($p = 0; $p -lt $parts.Count - 1; $p++) {
                $key = $parts[$p].Trim()
                if (-not $parent.Contains($key)) { $parent[$key] = [ordered]@{} }
                $parent = $parent[$key]
            }
            $leaf = $parts[-1].Trim()
            if (-not $parent.Contains($leaf)) { $parent[$leaf] = @() }
            $entry = [ordered]@{}
            $parent[$leaf] = @($parent[$leaf]) + @($entry)
            $current = $entry
            continue
        }

        # [table.path]
        if ($trimmed -match '^\[([^\]]+)\]$') {
            $path = $matches[1].Trim()
            $parent = $rootTable
            foreach ($rawKey in ($path -split '\.')) {
                $key = $rawKey.Trim()
                if (-not $parent.Contains($key)) { $parent[$key] = [ordered]@{} }
                $parent = $parent[$key]
            }
            $current = $parent
            continue
        }

        # key = value
        if ($trimmed -match '^([A-Za-z0-9_\-]+)\s*=\s*(.*)$') {
            $key = $matches[1]
            $rest = $matches[2]

            # Multi-line basic string: key = """ ... """
            if ($rest.TrimStart().StartsWith('"""')) {
                $buf = $rest.Trim()
                if ($buf -eq '"""' -or -not ($buf.Substring(3).EndsWith('"""'))) {
                    while ($i -lt $lines.Count) {
                        $buf += "`n" + $lines[$i]
                        $i++
                        if ($lines[$i - 1].TrimEnd().EndsWith('"""')) { break }
                    }
                }
                $inner = $buf.Trim()
                $inner = $inner.Substring(3, [math]::Max(0, $inner.Length - 6))
                $current[$key] = $inner.Trim("`n")
                continue
            }

            # Array, possibly spanning lines until the closing bracket.
            # Comments are stripped PER LINE as the buffer grows: an array whose
            # elements each carry a trailing `# note` (see [hubs]) would otherwise
            # lose everything after the first one, closing bracket included.
            if ($rest.TrimStart().StartsWith('[')) {
                $buf = Strip-TomlComment $rest
                while ((Get-BracketBalance $buf) -gt 0 -and $i -lt $lines.Count) {
                    $buf += ' ' + (Strip-TomlComment $lines[$i])
                    $i++
                }
                $current[$key] = ConvertFrom-TomlArray $buf
                continue
            }

            # Inline table { a = "x", b = [1, 2] }
            if ($rest.TrimStart().StartsWith('{')) {
                $buf = Strip-TomlComment $rest
                while ((Get-BraceBalance $buf) -gt 0 -and $i -lt $lines.Count) {
                    $buf += ' ' + (Strip-TomlComment $lines[$i])
                    $i++
                }
                $current[$key] = ConvertFrom-TomlInlineTable $buf
                continue
            }

            $current[$key] = ConvertFrom-TomlScalar (Strip-TomlComment $rest)
            continue
        }

        throw "architecture.toml: line not understood by the reader: $trimmed"
    }

    return $rootTable
}

# Strips a trailing `# comment`, respecting quotes so a '#' inside a string survives.
function Strip-TomlComment([string]$s) {
    $inStr = $false
    for ($k = 0; $k -lt $s.Length; $k++) {
        $ch = $s[$k]
        if ($ch -eq '"') { $inStr = -not $inStr; continue }
        if ($ch -eq '#' -and -not $inStr) { return $s.Substring(0, $k) }
    }
    return $s
}

function Get-BracketBalance([string]$s) {
    $s = Strip-TomlComment $s
    $depth = 0; $inStr = $false
    foreach ($ch in $s.ToCharArray()) {
        if ($ch -eq '"') { $inStr = -not $inStr; continue }
        if ($inStr) { continue }
        if ($ch -eq '[') { $depth++ } elseif ($ch -eq ']') { $depth-- }
    }
    return $depth
}

function Get-BraceBalance([string]$s) {
    $s = Strip-TomlComment $s
    $depth = 0; $inStr = $false
    foreach ($ch in $s.ToCharArray()) {
        if ($ch -eq '"') { $inStr = -not $inStr; continue }
        if ($inStr) { continue }
        if ($ch -eq '{') { $depth++ } elseif ($ch -eq '}') { $depth-- }
    }
    return $depth
}

# TOML basic-string escapes. Without this every regex in the file arrives with
# its backslashes doubled — `"^a\\d{3}_"` would be matched literally and every
# single slice would be reported as misnamed.
function Expand-TomlEscapes([string]$s) {
    $sb = New-Object System.Text.StringBuilder
    for ($k = 0; $k -lt $s.Length; $k++) {
        if ($s[$k] -ne '\' -or $k -eq $s.Length - 1) { [void]$sb.Append($s[$k]); continue }
        $k++
        switch ($s[$k]) {
            'n'  { [void]$sb.Append("`n") }
            't'  { [void]$sb.Append("`t") }
            'r'  { [void]$sb.Append("`r") }
            '"'  { [void]$sb.Append('"') }
            '\'  { [void]$sb.Append('\') }
            default { [void]$sb.Append('\'); [void]$sb.Append($s[$k]) }
        }
    }
    return $sb.ToString()
}

function ConvertFrom-TomlScalar([string]$raw) {
    $v = $raw.Trim()
    if ($v -match '^"(.*)"$') { return Expand-TomlEscapes $matches[1] }
    if ($v -eq 'true') { return $true }
    if ($v -eq 'false') { return $false }
    if ($v -match '^-?\d+$') { return [int]$v }
    if ($v -match '^-?\d+\.\d+$') { return [double]$v }
    return $v
}

# Splits on top-level commas only, so nested arrays and quoted commas survive.
function Split-TomlItems([string]$body) {
    $items = @()
    $depth = 0; $inStr = $false; $buf = ''
    foreach ($ch in $body.ToCharArray()) {
        if ($ch -eq '"') { $inStr = -not $inStr; $buf += $ch; continue }
        if (-not $inStr) {
            if ($ch -eq '[' -or $ch -eq '{') { $depth++ }
            elseif ($ch -eq ']' -or $ch -eq '}') { $depth-- }
            elseif ($ch -eq ',' -and $depth -eq 0) {
                if ($buf.Trim() -ne '') { $items += $buf }
                $buf = ''
                continue
            }
        }
        $buf += $ch
    }
    if ($buf.Trim() -ne '') { $items += $buf }
    return $items
}

function ConvertFrom-TomlArray([string]$raw) {
    $v = $raw.Trim()
    $open = $v.IndexOf('['); $close = $v.LastIndexOf(']')
    if ($open -lt 0 -or $close -lt $open) { throw "architecture.toml: malformed array: $raw" }
    $body = $v.Substring($open + 1, $close - $open - 1)
    $out = @()
    foreach ($item in (Split-TomlItems $body)) {
        $t = $item.Trim()
        if ($t.StartsWith('{')) { $out += ,(ConvertFrom-TomlInlineTable $t) }
        elseif ($t.StartsWith('[')) { $out += ,(ConvertFrom-TomlArray $t) }
        else { $out += ConvertFrom-TomlScalar $t }
    }
    return ,$out
}

function ConvertFrom-TomlInlineTable([string]$raw) {
    $v = $raw.Trim()
    $open = $v.IndexOf('{'); $close = $v.LastIndexOf('}')
    if ($open -lt 0 -or $close -lt $open) { throw "architecture.toml: malformed inline table: $raw" }
    $body = $v.Substring($open + 1, $close - $open - 1)
    $tbl = [ordered]@{}
    foreach ($pair in (Split-TomlItems $body)) {
        if ($pair -match '^\s*([A-Za-z0-9_\-]+)\s*=\s*(.*)$') {
            $k = $matches[1]; $val = $matches[2].Trim()
            if ($val.StartsWith('[')) { $tbl[$k] = ConvertFrom-TomlArray $val }
            elseif ($val.StartsWith('{')) { $tbl[$k] = ConvertFrom-TomlInlineTable $val }
            else { $tbl[$k] = ConvertFrom-TomlScalar $val }
        }
    }
    return $tbl
}

# ============================================================================
# Path helpers
# ============================================================================
# Globs are matched against repo-relative, forward-slash paths. `*` stops at a
# separator, `**` crosses them — the shape everyone already expects from
# .gitignore and the pre-commit hook.

function Convert-GlobToRegex([string]$glob) {
    $escaped = [regex]::Escape($glob)
    $escaped = $escaped.Replace('/', '/')
    # Order matters: ** before *, or the first replacement eats the second.
    $escaped = $escaped.Replace('\*\*', '\x00DOUBLE\x00')
    $escaped = $escaped.Replace('\*', '[^/]*')
    $escaped = $escaped.Replace('\x00DOUBLE\x00', '.*')
    return '^' + $escaped + '$'
}

function Test-Glob([string]$path, [string]$glob) {
    return $path -match (Convert-GlobToRegex $glob)
}

function Test-AnyGlob([string]$path, $globs) {
    if ($null -eq $globs) { return $false }
    foreach ($g in @($globs)) {
        if ($g -and (Test-Glob $path $g)) { return $true }
    }
    return $false
}

function Get-RelPath([string]$abs) {
    return $abs.Substring($root.Length + 1).Replace('\', '/')
}

# Directories matching a glob such as crates/frontend/src/domain/*/ui/details.
function Resolve-ScopeDirs([string]$glob) {
    # Walk down from the first literal segment so we never enumerate the world.
    $parts = $glob -split '/'
    $literal = @()
    foreach ($p in $parts) {
        if ($p -match '[*?]') { break }
        $literal += $p
    }
    $base = if ($literal.Count -gt 0) { Join-Path $root ($literal -join '\') } else { $root }
    if (-not (Test-Path $base)) { return @() }

    $regex = Convert-GlobToRegex $glob
    $out = @()
    if ((Get-RelPath $base) -match $regex) { $out += $base }
    foreach ($d in [System.IO.Directory]::EnumerateDirectories($base, '*', 'AllDirectories')) {
        if ((Get-RelPath $d) -match $regex) { $out += $d }
    }
    return $out
}

# ============================================================================
# Load the standard
# ============================================================================
$tomlPath = Join-Path $root 'architecture.toml'
if (-not (Test-Path $tomlPath)) {
    Write-Warning "[arch] architecture.toml not found — nothing to check."
    if ($Json) { '{"errors":0,"warnings":0,"waived":0,"violations":[]}' }
    exit 0
}

$spec = ConvertFrom-TomlText ([System.IO.File]::ReadAllText($tomlPath, (New-Object System.Text.UTF8Encoding($false))))

$violations = @()
$waivedCount = 0

function Add-Violation($rule, [string]$path, [string]$message) {
    $waivers = if ($rule.Contains('waivers')) { $rule['waivers'] } else { @() }
    foreach ($w in @($waivers)) {
        # A waiver is an inline table { path = "...", why = "..." }. The `why`
        # is required by convention and printed with -ShowWaived, so a waiver
        # can never quietly become an unexplained hole.
        $wp = if ($w -is [System.Collections.IDictionary]) { $w['path'] } else { $w }
        if ($wp -and (Test-Glob $path $wp)) {
            $script:waivedCount++
            if ($ShowWaived) {
                $why = if ($w -is [System.Collections.IDictionary]) { $w['why'] } else { '(no reason given)' }
                Write-Host ("  waived  {0,-24} {1}  -- {2}" -f $rule['id'], $path, $why) -ForegroundColor DarkGray
            }
            return
        }
    }
    $severity = if ($rule.Contains('severity')) { $rule['severity'] } else { 'error' }
    $script:violations += [pscustomobject]@{
        Rule     = $rule['id']
        Severity = $severity
        Path     = $path
        Message  = $message
    }
}

# ============================================================================
# Rule engine
# ============================================================================

$rules = @()
if ($spec.Contains('rules')) { $rules = @($spec['rules']) }

foreach ($rule in $rules) {
    $type = $rule['type']
    $scope = $rule['scope']

    switch ($type) {

        # Every directory in scope carries the roles the standard declares for it.
        'required_files' {
            foreach ($dir in (Resolve-ScopeDirs $scope)) {
                $rel = Get-RelPath $dir
                if (Test-AnyGlob $rel $rule['exempt']) { continue }
                foreach ($f in @($rule['files'])) {
                    if (-not (Test-Path (Join-Path $dir $f))) {
                        Add-Violation $rule "$rel/$f" "missing required role file '$f'"
                    }
                }
            }
        }

        # A retired name must not come back. This is what keeps "one role, one
        # name" true a year from now instead of only on the day it was tidied.
        'forbidden_filename' {
            foreach ($dir in (Resolve-ScopeDirs $scope)) {
                $rel = Get-RelPath $dir
                if (Test-AnyGlob $rel $rule['exempt']) { continue }
                foreach ($f in @($rule['files'])) {
                    if (Test-Path (Join-Path $dir $f)) {
                        $use = if ($rule.Contains('use_instead')) { " — use '$($rule['use_instead'])'" } else { '' }
                        Add-Violation $rule "$rel/$f" "retired role name '$f'$use"
                    }
                }
            }
        }

        # Slice directories carry their family's index and a name after it.
        'dir_name_pattern' {
            $pattern = $rule['pattern']
            foreach ($parent in (Resolve-ScopeDirs $scope)) {
                foreach ($d in [System.IO.Directory]::EnumerateDirectories($parent)) {
                    $name = Split-Path $d -Leaf
                    $rel = Get-RelPath $d
                    if (Test-AnyGlob $rel $rule['exempt']) { continue }
                    if ($name -notmatch $pattern) {
                        Add-Violation $rule $rel "directory name does not match $pattern"
                    }
                }
            }
        }

        # The directory name and the id inside its own metadata.json are the
        # same fact written twice; this is the check that they agree. dv001 vs
        # dv001_revenue is exactly how they came apart before.
        'dir_matches_metadata_id' {
            foreach ($dir in (Resolve-ScopeDirs $scope)) {
                $rel = Get-RelPath $dir
                if (Test-AnyGlob $rel $rule['exempt']) { continue }
                $meta = Join-Path $dir 'metadata.json'
                if (-not (Test-Path $meta)) { continue }
                # NOT $json: PowerShell variable names are case-insensitive, so
                # that would collide with this script's own [switch]$Json.
                $metaObj = $null
                try {
                    $raw = [System.IO.File]::ReadAllText($meta, (New-Object System.Text.UTF8Encoding($false)))
                    $metaObj = $raw | ConvertFrom-Json
                } catch {
                    Add-Violation $rule "$rel/metadata.json" "metadata.json does not parse: $($_.Exception.Message)"
                }
                if ($null -eq $metaObj) { continue }
                $id = $metaObj.id
                $name = Split-Path $dir -Leaf
                if ($id -and $id -ne $name) {
                    Add-Violation $rule $rel "directory is '$name' but metadata.json id is '$id'"
                }
            }
        }

        # Rust module names are snake_case. Document corpora are exempt by glob,
        # because there the file name is an identifier of DATA (a skill id, a
        # knowledge-base article id), not a module name — see [conventions.doc_corpora].
        'snake_case_files' {
            foreach ($dir in (Resolve-ScopeDirs $scope)) {
                foreach ($f in [System.IO.Directory]::EnumerateFiles($dir, '*.rs', 'AllDirectories')) {
                    $rel = Get-RelPath $f
                    if (Test-AnyGlob $rel $rule['exempt']) { continue }
                    $stem = [System.IO.Path]::GetFileNameWithoutExtension($f)
                    if ($stem -cnotmatch '^[a-z0-9_]+$') {
                        Add-Violation $rule $rel "file name is not snake_case"
                    }
                }
            }
        }

        # The one check here that guards a string the compiler cannot.
        # EmbeddedKnowledgeSource writes each document's path twice: once as
        # include_str!(..), which rustc verifies, and once as source_path: "..",
        # which is just text. Rename a directory and the second one rots without
        # a single warning — the article keeps working but points at a file that
        # is no longer there.
        'embedded_doc_path_sync' {
            $file = Join-Path $root $rule['file']
            if (-not (Test-Path $file)) { break }
            $text = [System.IO.File]::ReadAllText($file, (New-Object System.Text.UTF8Encoding($false)))
            $prefix = $rule['include_prefix']   # e.g. crates/backend/src/shared/llm/
            $pairPattern = 'source_path:\s*(?:\r?\n\s*)?"([^"]+)"\s*,\s*raw:\s*include_str!\(\s*"([^"]+)"\s*\)'
            foreach ($m in [regex]::Matches($text, $pairPattern)) {
                $declared = $m.Groups[1].Value
                $included = $m.Groups[2].Value
                # Resolve include_str! (relative to the file) into a repo-relative path.
                $resolved = [System.IO.Path]::GetFullPath((Join-Path (Join-Path $root $prefix) $included))
                $resolvedRel = $resolved.Substring($root.Length + 1).Replace('\', '/')
                if ($declared -ne $resolvedRel) {
                    Add-Violation $rule $rule['file'] "source_path '$declared' disagrees with include_str! which resolves to '$resolvedRel'"
                }
                if (-not (Test-Path $resolved)) {
                    Add-Violation $rule $rule['file'] "embedded document does not exist: $resolvedRel"
                }
            }
        }

        # tools/ and scripts/ are two homes with different tenants, and the
        # border runs through WHO runs the script and WHEN: the development
        # process (generators, gates, the hook, the build loop) versus
        # operations on the environment and the data (database, release,
        # deploy, service). Neither can be read off a file name, so the census
        # of each directory is written out in the manifest — a new script
        # cannot be dropped in without being classified. See
        # [conventions.script_homes] for the border itself.
        'dir_manifest' {
            $dir = Join-Path $root $scope
            if (-not (Test-Path $dir)) {
                Add-Violation $rule $scope "manifest scope directory does not exist"
            } else {
                $exts = @($rule['extensions'])
                $declared = @($rule['files'])
                $hint = if ($rule.Contains('hint')) { " — $($rule['hint'])" } else { '' }
                $present = @()
                # Top level only: subdirectories are addressed by other means
                # (tools/hooks/ lives at core.hooksPath, not in this census).
                foreach ($f in [System.IO.Directory]::EnumerateFiles($dir)) {
                    $name = Split-Path $f -Leaf
                    $ext = [System.IO.Path]::GetExtension($name).ToLowerInvariant()
                    if ($exts -notcontains $ext) { continue }
                    $rel = Get-RelPath $f
                    if (Test-AnyGlob $rel $rule['exempt']) { continue }
                    $present += $name
                    if ($declared -notcontains $name) {
                        Add-Violation $rule $rel "script is not in the manifest of '$scope/'$hint"
                    }
                }
                foreach ($name in $declared) {
                    if ($present -notcontains $name) {
                        Add-Violation $rule "$scope/$name" "manifest lists a script that is not there — it was moved or deleted without updating architecture.toml"
                    }
                }
            }
        }

        default {
            throw "architecture.toml: rule '$($rule['id'])' has unknown type '$type'. Add the type to tools/check_architecture.ps1 or fix the spelling."
        }
    }
}

# ============================================================================
# Report
# ============================================================================
$errors = @($violations | Where-Object { $_.Severity -eq 'error' })
$warnings = @($violations | Where-Object { $_.Severity -ne 'error' })

if ($Json) {
    $payload = [ordered]@{
        errors     = $errors.Count
        warnings   = $warnings.Count
        waived     = $waivedCount
        rules      = $rules.Count
        violations = @($violations | ForEach-Object {
            [ordered]@{ rule = $_.Rule; severity = $_.Severity; path = $_.Path; message = $_.Message }
        })
    }
    $payload | ConvertTo-Json -Depth 6 -Compress
    exit 0
}

Write-Host "[arch] $($rules.Count) rule(s) from architecture.toml; $waivedCount waived match(es)." -ForegroundColor DarkGray

if ($warnings.Count -gt 0) {
    Write-Host "[arch] warnings:" -ForegroundColor Yellow
    foreach ($v in ($warnings | Sort-Object Rule, Path)) {
        Write-Host ("  {0,-24} {1}" -f $v.Rule, $v.Path) -ForegroundColor Yellow
        Write-Host ("  {0,-24}   {1}" -f '', $v.Message) -ForegroundColor DarkYellow
    }
}

if ($errors.Count -eq 0) {
    Write-Host "[arch] OK — the tree matches architecture.toml." -ForegroundColor Green
    exit 0
}

Write-Host ""
Write-Host "[arch] $($errors.Count) violation(s) of architecture.toml:" -ForegroundColor Red
foreach ($v in ($errors | Sort-Object Rule, Path)) {
    Write-Host ("  {0,-24} {1}" -f $v.Rule, $v.Path) -ForegroundColor Red
    Write-Host ("  {0,-24}   {1}" -f '', $v.Message) -ForegroundColor DarkGray
}
Write-Host ""
Write-Host "Either fix the tree, or — if the case is legitimate — add a waiver with its" -ForegroundColor DarkGray
Write-Host "reason to that rule in architecture.toml. Never delete the rule to pass." -ForegroundColor DarkGray
exit 1
