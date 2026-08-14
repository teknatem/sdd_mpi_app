<#
.SYNOPSIS
    Regenerates UI_REGISTRY.md from the frontend CSS + Rust sources.

.DESCRIPTION
    The style registry is the factual half of the UI standard: it says WHAT
    exists, how much of it, and what is already dead. The normative half lives
    in memory-bank/architecture/ui-standard.md and says what is ALLOWED.
    (Same split as ARCHITECTURE.md vs the hand-written llm.md files.)

    Sources:
      - crates/frontend/static/**/*.css   -> class selectors, block roots, tokens
      - crates/frontend/src/**/*.rs       -> class usage (literals + format! prefixes)
      - crates/frontend/index.html        -> which stylesheets are actually linked
      - crates/frontend/static/ui-registry.allow.json -> curated allowlist

    Reported:
      1. Summary counters
      2. Block roots (the registry proper) with usage + allowlist status
      3. Blocks defined in more than one file (duplication)
      4. Tokens: undefined references (a bug list) and per-theme drift
      5. Hardcode: hex colours and raw px outside the token system
      6. Dead candidates: classes with no Rust reference, unlinked files

.NOTES
    Run from the repo root:
        powershell -File tools/gen_ui_registry.ps1
    UI_REGISTRY.md is GENERATED. Edit this script, not the output.
#>

param(
    # Write crates/frontend/static/ui-registry.allow.json from the CURRENT state,
    # sanctioning every block that exists today. Run once, after a cleanup pass -
    # bootstrapping earlier would freeze the mess into the allowlist.
    [switch]$BootstrapAllowlist
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$out  = New-Object System.Text.StringBuilder
$BT   = [char]96

function W([string]$s = '') { [void]$out.AppendLine($s) }
function Q([string]$s)      { return "$BT$s$BT" }

function Cell([string]$s) {
    if ([string]::IsNullOrWhiteSpace($s)) { return '' }
    return ($s -replace '\|', '\|')
}

$staticDir = Join-Path $root 'crates/frontend/static'
$srcDir    = Join-Path $root 'crates/frontend/src'
$assetsDir = Join-Path $root 'crates/frontend/assets'

if (-not (Test-Path $staticDir)) { throw "Not found: $staticDir" }

# --------------------------------------------------------------- Helpers ---------------------------------------------------------------

function Strip-Comments([string]$css) {
    return [regex]::Replace($css, '/\*.*?\*/', ' ', 'Singleline')
}

# Declaration bodies never contain braces, so a single innermost-block pass
# removes exactly the declarations and leaves every selector standing -
# including selectors nested inside @media / @layer / @supports.
function Get-SelectorText([string]$css) {
    return [regex]::Replace($css, '\{[^{}]*\}', ' ; ')
}

# "foo-bar__el--mod" -> "foo-bar"
function Get-BlockRoot([string]$cls) {
    $r = ($cls -split '__')[0]
    $r = ($r -split '--')[0]
    return $r
}

function Get-Layer([string]$relPath) {
    $p = $relPath -replace '\\', '/'
    if ($p -match '^static/themes/core/')                { return 'core' }
    if ($p -match '^static/themes/(dark|light|forest)/') { return 'theme' }
    if ($p -match '^static/pages/')                      { return 'page' }
    if ($p -match '^assets/')                            { return 'asset' }
    return 'feature'
}

# --------------------------------------------------------------- 1. Collect CSS facts ---------------------------------------------------------------

# assets/**.css is deliberately OUT of scope: those sheets are include_str!'d into
# BI-card HTML and iframe srcdoc (shared/bi_card/renderer.rs, plugins/frame/srcdoc.rs),
# so they live in their own sandboxed cascade. App-level BEM rules do not apply to
# them, and their generic names (.value, .line, .name, .ring) would drown the registry.
$cssFiles = @()
$cssFiles += Get-ChildItem $staticDir -Recurse -File -Filter '*.css'

$classDefs   = @{}   # class      -> [string[]] relative files
$blockFiles  = @{}   # blockRoot  -> hashtable of files (used as a set)
$blockClass  = @{}   # blockRoot  -> hashtable of classes
$tokenDefs   = @{}   # --token    -> [string[]] files
$tokenUses   = @{}   # --token    -> [string[]] files
$tokenNoFallback = @{}   # --token  -> used at least once without a fallback
$themeTokens = @{}   # theme name -> hashtable of tokens
$fileStats   = @{}   # relPath    -> stats object

foreach ($f in $cssFiles) {
    $rel = $f.FullName.Substring($root.Length + 1) -replace '\\', '/'
    $rel = $rel -replace '^crates/frontend/', ''
    $layer = Get-Layer $rel

    $raw   = Get-Content $f.FullName -Raw -Encoding UTF8
    if ($null -eq $raw) { $raw = '' }
    $clean = Strip-Comments $raw
    $sel   = Get-SelectorText $clean

    # --- classes (from selector text only) ---
    $classesHere = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($m in [regex]::Matches($sel, '\.(-?[A-Za-z_][A-Za-z0-9_-]*)')) {
        [void]$classesHere.Add($m.Groups[1].Value)
    }
    foreach ($c in $classesHere) {
        if (-not $classDefs.ContainsKey($c)) { $classDefs[$c] = @() }
        $classDefs[$c] += $rel

        $b = Get-BlockRoot $c
        if (-not $blockFiles.ContainsKey($b)) {
            $blockFiles[$b] = @{}
            $blockClass[$b] = @{}
        }
        $blockFiles[$b][$rel] = $true
        $blockClass[$b][$c]   = $true
    }

    # --- tokens (need the declaration bodies, so use $clean) ---
    foreach ($m in [regex]::Matches($clean, '(--[A-Za-z0-9_-]+)\s*:')) {
        $t = $m.Groups[1].Value
        if (-not $tokenDefs.ContainsKey($t)) { $tokenDefs[$t] = @() }
        if ($tokenDefs[$t] -notcontains $rel) { $tokenDefs[$t] += $rel }

        if ($layer -eq 'theme') {
            $theme = ''
            if ($rel -match '^static/themes/([a-z]+)/') { $theme = $Matches[1] }
            if ($theme -ne '') {
                if (-not $themeTokens.ContainsKey($theme)) { $themeTokens[$theme] = @{} }
                $themeTokens[$theme][$t] = $true
            }
        }
    }
    foreach ($m in [regex]::Matches($clean, 'var\(\s*(--[A-Za-z0-9_-]+)\s*(,?)')) {
        $t = $m.Groups[1].Value
        if (-not $tokenUses.ContainsKey($t)) { $tokenUses[$t] = @() }
        if ($tokenUses[$t] -notcontains $rel) { $tokenUses[$t] += $rel }
        # var(--x, fallback) degrades to the fallback; var(--x) with no
        # definition renders as an invalid value and the property is dropped.
        # Only the latter is a rendering bug.
        if ($m.Groups[2].Value -ne ',') { $tokenNoFallback[$t] = $true }
    }

    # --- hardcode counters (skip token definition lines and theme files) ---
    $hex = 0; $px = 0
    foreach ($line in ($clean -split "`n")) {
        if ($line -match '^\s*--[A-Za-z0-9_-]+\s*:') { continue }
        $hex += ([regex]::Matches($line, '#[0-9a-fA-F]{3,8}\b')).Count
        if ($line -match '\b(padding|margin|gap|font-size|top|left|right|bottom)\b') {
            $px += ([regex]::Matches($line, '\b\d+px\b')).Count
        }
    }

    $fileStats[$rel] = [pscustomobject]@{
        Rel     = $rel
        Layer   = $layer
        Lines   = ($raw -split "`n").Count
        Classes = $classesHere.Count
        Hex     = $hex
        Px      = $px
        Full    = $f.FullName
    }
}

# --------------------------------------------------------------- 2. Collect Rust usage ---------------------------------------------------------------

$rsLiterals = New-Object 'System.Collections.Generic.HashSet[string]'
$dynPrefix  = New-Object 'System.Collections.Generic.HashSet[string]'
# Custom properties the Rust side sets at runtime (style:--x=.. / "--x: {}").
# They are legitimately absent from the CSS - the value comes from a signal, so
# they must not be reported as broken references.
$runtimeVars = New-Object 'System.Collections.Generic.HashSet[string]'
$inlineStyle = 0
$rsFiles = Get-ChildItem $srcDir -Recurse -File -Filter '*.rs'

foreach ($f in $rsFiles) {
    $txt = Get-Content $f.FullName -Raw -Encoding UTF8
    if ($null -eq $txt) { continue }

    $inlineStyle += ([regex]::Matches($txt, '\bstyle\s*=\s*"')).Count
    $inlineStyle += ([regex]::Matches($txt, '\battr:style\s*=')).Count

    foreach ($m in [regex]::Matches($txt, '(--[A-Za-z0-9_-]+)\s*(?::|=)')) {
        [void]$runtimeVars.Add($m.Groups[1].Value)
    }

    foreach ($m in [regex]::Matches($txt, '"([^"\\]*(?:\\.[^"\\]*)*)"')) {
        $lit = $m.Groups[1].Value
        if ($lit -eq '') { continue }

        # Whole tokens that look like class names.
        foreach ($tk in [regex]::Matches($lit, '-?[A-Za-z_][A-Za-z0-9_-]*')) {
            [void]$rsLiterals.Add($tk.Value)
        }
        # format!("badge badge--{}", kind) -> remember the "badge--" stem so a
        # dynamically built class is never reported as dead.
        $brace = $lit.IndexOf('{')
        if ($brace -gt 0) {
            $head = $lit.Substring(0, $brace)
            if ($head -match '(-?[A-Za-z_][A-Za-z0-9_-]*)$') {
                $stem = $Matches[1]
                if ($stem.Length -ge 3) { [void]$dynPrefix.Add($stem) }
            }
        }
    }
}

function Test-ClassUsed([string]$cls) {
    if ($rsLiterals.Contains($cls)) { return $true }
    foreach ($p in $dynPrefix) {
        if ($cls.Length -gt $p.Length -and $cls.StartsWith($p)) { return $true }
    }
    return $false
}

# Usage per class, cached once (the dynamic-prefix scan is the expensive part).
$classUsed = @{}
foreach ($c in $classDefs.Keys) { $classUsed[$c] = Test-ClassUsed $c }

# --------------------------------------------------------------- 3. Allowlist ---------------------------------------------------------------

$allowPath = Join-Path $staticDir 'ui-registry.allow.json'
$allowed   = @{}
$allowMeta = @{}
$gateOn    = $false
if (Test-Path $allowPath) {
    $gateOn = $true
    $json = Get-Content $allowPath -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($e in $json.blocks) {
        $allowed[$e.block] = $true
        $allowMeta[$e.block] = $e
    }
}

# --------------------------------------------------------------- 4. Linked stylesheets ---------------------------------------------------------------

$linked = @{}
$indexHtml = Join-Path $root 'crates/frontend/index.html'
if (Test-Path $indexHtml) {
    $html = Get-Content $indexHtml -Raw -Encoding UTF8
    foreach ($m in [regex]::Matches($html, 'href\s*=\s*"([^"]+\.css)"')) {
        $linked[($m.Groups[1].Value -replace '^/', '')] = $true
    }
}
# @import chains inside CSS
foreach ($f in $cssFiles) {
    $rel = $f.FullName.Substring($root.Length + 1) -replace '\\', '/'
    $rel = $rel -replace '^crates/frontend/', ''
    $dir = Split-Path $rel -Parent
    $txt = Get-Content $f.FullName -Raw -Encoding UTF8
    if ($null -eq $txt) { continue }
    foreach ($m in [regex]::Matches($txt, '@import\s+url\(\s*["'']?([^"'')]+)')) {
        $target = $m.Groups[1].Value.Trim()
        # A leading '/' is root-relative to the served root, not to this file's dir
        # (e.g. base.css does @import url('/static/fonts/roboto.css')).
        $joined = if ($target.StartsWith('/')) { $target.TrimStart('/') }
                  else { ($dir + '/' + $target) }
        $joined = $joined -replace '\\', '/'
        # normalise ./ and ../
        $parts = @()
        foreach ($seg in ($joined -split '/')) {
            if ($seg -eq '.' -or $seg -eq '') { continue }
            if ($seg -eq '..') { if ($parts.Count -gt 0) { $parts = $parts[0..($parts.Count - 2)] }; continue }
            $parts += $seg
        }
        $linked[($parts -join '/')] = $true
    }
}
# include_str! / srcdoc injection from Rust
$rsAll = New-Object System.Text.StringBuilder
foreach ($f in $rsFiles) {
    [void]$rsAll.Append((Get-Content $f.FullName -Raw -Encoding UTF8))
}
$rsBlob = $rsAll.ToString()
# HTML assets loaded inside iframes reference their own CSS
$htmlFiles = @()
if (Test-Path $assetsDir) { $htmlFiles += Get-ChildItem $assetsDir -Recurse -File -Filter '*.html' }
$htmlBlob = ''
foreach ($h in $htmlFiles) { $htmlBlob += (Get-Content $h.FullName -Raw -Encoding UTF8) }

foreach ($rel in $fileStats.Keys) {
    $leaf = Split-Path $rel -Leaf
    if ($rsBlob -match [regex]::Escape($leaf))   { $linked[$rel] = $true }
    if ($htmlBlob -match [regex]::Escape($leaf)) { $linked[$rel] = $true }
    # Theme sheets are swapped at runtime by rewriting the <link> href from a
    # format! template (shared/theme/theme_select.rs), so the filename never
    # appears literally in Rust. Adding a theme = a row in THEMES + its file.
    if ($rel -match '^static/themes/[a-z]+/[a-z]+\.css$' -and $rel -notmatch '/core/') {
        $linked[$rel] = $true
    }
}

# --------------------------------------------------------------- 5. Emit ---------------------------------------------------------------

$totalClasses = $classDefs.Count
$totalBlocks  = $blockFiles.Count
$deadClasses  = @($classDefs.Keys | Where-Object { -not $classUsed[$_] })
$totalLines   = ($fileStats.Values | Measure-Object -Property Lines -Sum).Sum
$totalHex     = ($fileStats.Values | Where-Object { $_.Layer -ne 'theme' } | Measure-Object -Property Hex -Sum).Sum
$totalPx      = ($fileStats.Values | Measure-Object -Property Px -Sum).Sum

$undefTokens    = @()   # no definition AND no fallback -> broken rendering
$softTokens     = @()   # no definition but always has a fallback -> degraded
$thawTokens     = @()
$runtimeTokens  = @()
foreach ($t in ($tokenUses.Keys | Sort-Object)) {
    if ($tokenDefs.ContainsKey($t)) { continue }
    # Thaw/Fluent runtime tokens are injected from WASM at runtime: camelCase
    # names or the --thaw- prefix. They are not ours and resolve fine.
    if ($t -match '^--thaw-' -or $t -cmatch '[A-Z]') { $thawTokens += $t }
    # Set by our own Rust code on the element (style:--x=..): value comes from a
    # signal, so a CSS definition would be wrong, not missing.
    elseif ($runtimeVars.Contains($t)) { $runtimeTokens += $t }
    elseif ($tokenNoFallback.ContainsKey($t)) { $undefTokens += $t }
    else { $softTokens += $t }
}

W '# UI REGISTRY'
W ''
W '> **GENERATED file - do not edit by hand.** Source of truth is the CSS + Rust code.'
W "> Regenerate: $(Q 'powershell -File tools/gen_ui_registry.ps1')"
W '> Factual half of the UI standard (what exists). The normative half - what is'
W "> allowed - lives in $(Q 'memory-bank/architecture/ui-standard.md')."
W ''

W '## Summary'
W ''
W '| Metric | Value |'
W '|--------|-------|'
W "| CSS files | $($fileStats.Count) |"
W "| CSS lines | $totalLines |"
W "| Distinct classes | $totalClasses |"
W "| Block roots | $totalBlocks |"
W "| Classes with no Rust reference | $($deadClasses.Count) |"
W "| Inline ``style=`` in .rs | $inlineStyle |"
W "| Hardcoded hex outside themes | $totalHex |"
W "| Raw px in spacing/size props | $totalPx |"
W "| Tokens defined | $($tokenDefs.Count) |"
W "| Tokens undefined with NO fallback (broken) | $($undefTokens.Count) |"
W "| Tokens undefined but with a fallback (dormant) | $($softTokens.Count) |"
W "| Tokens set by Rust at runtime | $($runtimeTokens.Count) |"
W "| Tokens used but undefined (Thaw runtime) | $($thawTokens.Count) |"
if ($gateOn) { W "| Allowlist entries | $($allowed.Count) |" }
else         { W '| Allowlist | not created yet (gate off) |' }
W ''

# --- Block roots ---
W "## Block roots ($totalBlocks)"
W ''
W 'One row per top-level BEM block. `Used` counts the block''s classes that appear'
W 'in Rust (literal or `format!` stem). `Status` is the allowlist verdict.'
W ''
W '| Block | Layer | Files | Classes | Used | Status |'
W '|-------|-------|-------|---------|------|--------|'
foreach ($b in ($blockFiles.Keys | Sort-Object)) {
    $files   = @($blockFiles[$b].Keys | Sort-Object)
    $classes = @($blockClass[$b].Keys)
    $used    = @($classes | Where-Object { $classUsed[$_] }).Count
    $layers  = @($files | ForEach-Object { Get-Layer $_ } | Select-Object -Unique | Sort-Object)
    $status  = '-'
    if ($gateOn) {
        if ($allowed.ContainsKey($b)) { $status = 'allowed' }
        else                          { $status = '**unregistered**' }
    }
    if ($used -eq 0) { $status = "$status / dead" }
    $fileCell = if ($files.Count -le 2) { ($files -join '<br>') } else { "$($files[0])<br>+$($files.Count - 1) more" }
    W "| $(Q $b) | $($layers -join ',') | $(Cell $fileCell) | $($classes.Count) | $used | $status |"
}
W ''

# --- Duplicates ---
$dupes = @($blockFiles.Keys | Where-Object { $blockFiles[$_].Count -gt 1 } |
           Sort-Object { -$blockFiles[$_].Count })
W "## Blocks defined in more than one file ($($dupes.Count))"
W ''
W 'A block owned by several files has no owner. Each of these is either one'
W 'concept that must collapse onto a single definition, or a name collision.'
W ''
W '| Block | Files | Defined in |'
W '|-------|-------|------------|'
foreach ($b in $dupes) {
    $files = @($blockFiles[$b].Keys | Sort-Object)
    W "| $(Q $b) | $($files.Count) | $(Cell ($files -join '<br>')) |"
}
W ''

# --- Tokens ---
W '## Tokens'
W ''
W "### Used but never defined - ours ($($undefTokens.Count))"
W ''
if ($undefTokens.Count -eq 0) {
    W 'None. '
} else {
    W 'These render as invalid values unless the `var()` call carries a fallback.'
    W 'This is a bug list, not a style preference.'
    W ''
    W '| Token | Referenced from |'
    W '|-------|-----------------|'
    foreach ($t in $undefTokens) {
        $files = @($tokenUses[$t] | Sort-Object)
        $cell = if ($files.Count -le 3) { ($files -join '<br>') } else { "$($files[0])<br>+$($files.Count - 1) more" }
        W "| $(Q $t) | $(Cell $cell) |"
    }
}
W ''
W "### Undefined but always carries a fallback ($($softTokens.Count))"
W ''
W 'These resolve to their `var(--x, fallback)` default, so nothing renders broken -'
W 'but the intended value never arrives. Usually a dangling hook: the CSS expects'
W 'someone to set the variable and nobody does.'
W ''
if ($softTokens.Count -gt 0) {
    W '| Token | Referenced from |'
    W '|-------|-----------------|'
    foreach ($t in $softTokens) {
        $files = @($tokenUses[$t] | Sort-Object)
        W "| $(Q $t) | $(Cell ($files -join '<br>')) |"
    }
}
W ''
W "### Set by Rust at runtime ($($runtimeTokens.Count))"
W ''
W 'Written onto the element from a signal (`style:--x=..`), so they are absent from'
W 'the CSS by design. Not bugs.'
W ''
if ($runtimeTokens.Count -gt 0) { W (($runtimeTokens | ForEach-Object { Q $_ }) -join ', ') }
W ''
W "### Thaw runtime tokens ($($thawTokens.Count))"
W ''
W 'Injected by the Thaw component library at runtime - undefined in our CSS by design.'
W ''
if ($thawTokens.Count -gt 0) { W (($thawTokens | ForEach-Object { Q $_ }) -join ', ') }
W ''

# Theme drift
$themeNames = @($themeTokens.Keys | Sort-Object)
if ($themeNames.Count -gt 1) {
    W '### Theme drift'
    W ''
    W 'A token defined by some themes but not others resolves to whatever the'
    W 'previous theme left behind. Tokens carrying a base value in'
    W '`themes/core/variables.css` are excluded - there a theme file is an'
    W 'override, and not overriding is a legitimate choice (e.g. the strict dark'
    W 'theme deliberately skips the fancy `--glass-filter` / `--scrim-*` system).'
    W ''
    W '| Theme | Tokens | Missing vs union |'
    W '|-------|--------|------------------|'
    # A base value in core makes the token resolvable in every theme.
    $coreDefined = @{}
    foreach ($t in $tokenDefs.Keys) {
        foreach ($src in $tokenDefs[$t]) {
            if ((Get-Layer $src) -eq 'core') { $coreDefined[$t] = $true; break }
        }
    }
    $union = @{}
    foreach ($th in $themeNames) {
        foreach ($t in $themeTokens[$th].Keys) {
            if (-not $coreDefined.ContainsKey($t)) { $union[$t] = $true }
        }
    }
    $missingBy = @{}
    foreach ($th in $themeNames) {
        $missingBy[$th] = @($union.Keys | Where-Object { -not $themeTokens[$th].ContainsKey($_) } | Sort-Object)
        W "| $th | $($themeTokens[$th].Count) | $($missingBy[$th].Count) |"
    }
    W ''
    foreach ($th in $themeNames) {
        if ($missingBy[$th].Count -eq 0) { continue }
        W ("**Missing in $th ($($missingBy[$th].Count)):** " + (($missingBy[$th] | ForEach-Object { Q $_ }) -join ', '))
        W ''
    }
}

# --- Hardcode ---
W '## Hardcode by file'
W ''
W 'Colours belong in `static/themes/*/`; spacing and sizes belong in tokens.'
W 'Theme files are excluded from the hex count - defining colours is their job.'
W ''
W '| File | Layer | Lines | Classes | Hex | Raw px |'
W '|------|-------|-------|---------|-----|--------|'
foreach ($s in ($fileStats.Values | Sort-Object { -($_.Hex + $_.Px) })) {
    if ($s.Hex -eq 0 -and $s.Px -eq 0) { continue }
    $hexShown = if ($s.Layer -eq 'theme') { '-' } else { $s.Hex }
    W "| $(Cell $s.Rel) | $($s.Layer) | $($s.Lines) | $($s.Classes) | $hexShown | $($s.Px) |"
}
W ''

# --- Dead candidates ---
W "## Dead candidates"
W ''
W "### Unlinked stylesheets"
W ''
W 'Not linked from `index.html`, not reached by any `@import`, and their filename'
W 'appears in no `.rs` or asset `.html`. Nothing loads these.'
W ''
$unlinked = @($fileStats.Values | Where-Object {
    -not $linked.ContainsKey($_.Rel) -and $_.Layer -ne 'core'
} | Sort-Object { -$_.Lines })
if ($unlinked.Count -eq 0) {
    W 'None.'
} else {
    W '| File | Lines | Classes |'
    W '|------|-------|---------|'
    foreach ($s in $unlinked) { W "| $(Cell $s.Rel) | $($s.Lines) | $($s.Classes) |" }
}
W ''
W "### Classes with no Rust reference ($($deadClasses.Count))"
W ''
W 'Conservative: a class counts as used if it appears as a whole token in any Rust'
W 'string literal, or if some `format!` stem is a prefix of it. Still verify before'
W 'deleting - a class may be referenced from an asset `.html` or a plugin bundle.'
W ''
W '| Block | Dead classes |'
W '|-------|--------------|'
$deadByBlock = @{}
foreach ($c in $deadClasses) {
    $b = Get-BlockRoot $c
    if (-not $deadByBlock.ContainsKey($b)) { $deadByBlock[$b] = @() }
    $deadByBlock[$b] += $c
}
foreach ($b in ($deadByBlock.Keys | Sort-Object { -$deadByBlock[$_].Count })) {
    $list = @($deadByBlock[$b] | Sort-Object)
    W "| $(Q $b) | $(Cell (($list | ForEach-Object { Q $_ }) -join ' ')) |"
}
W ''

if ($BootstrapAllowlist) {
    if (Test-Path $allowPath) {
        Write-Host "Allowlist already exists, not overwriting: $allowPath"
    } else {
        $lines = New-Object System.Text.StringBuilder
        [void]$lines.AppendLine('{')
        [void]$lines.AppendLine('  "$comment": [')
        [void]$lines.AppendLine('    "Sanctioned CSS block roots. A block missing here makes pre-commit warn.",')
        [void]$lines.AppendLine('    "Approving a new block = adding an entry with a real reason.",')
        [void]$lines.AppendLine('    "Before adding one, check UI_REGISTRY.md: the concept usually already exists.",')
        [void]$lines.AppendLine('    "Bootstrapped from the state after the 2026-08-14 cleanup - an entry here means",')
        [void]$lines.AppendLine('    "\"existed at bootstrap\", not \"reviewed and blessed\"."')
        [void]$lines.AppendLine('  ],')
        [void]$lines.AppendLine('  "blocks": [')
        $sorted = @($blockFiles.Keys | Sort-Object)
        for ($i = 0; $i -lt $sorted.Count; $i++) {
            $b = $sorted[$i]
            $files = @($blockFiles[$b].Keys | Sort-Object)
            $layer = (Get-Layer $files[0])
            $comma = if ($i -lt $sorted.Count - 1) { ',' } else { '' }
            [void]$lines.AppendLine("    { ""block"": ""$b"", ""layer"": ""$layer"", ""reason"": ""bootstrap"" }$comma")
        }
        [void]$lines.AppendLine('  ]')
        [void]$lines.AppendLine('}')
        [System.IO.File]::WriteAllText($allowPath, $lines.ToString(), (New-Object System.Text.UTF8Encoding($false)))
        Write-Host "Allowlist bootstrapped with $($sorted.Count) blocks: $allowPath"
        Write-Host "Re-run without -BootstrapAllowlist to refresh UI_REGISTRY.md statuses."
    }
}

$dest = Join-Path $root 'UI_REGISTRY.md'
$enc  = New-Object System.Text.UTF8Encoding($true)
[System.IO.File]::WriteAllText($dest, $out.ToString(), $enc)
Write-Host "UI_REGISTRY.md regenerated: $dest"
Write-Host "  classes=$totalClasses blocks=$totalBlocks dead=$($deadClasses.Count) undef-tokens=$($undefTokens.Count)"
